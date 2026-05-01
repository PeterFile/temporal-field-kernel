use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tfk_protocol::{ContinuationInput, ContinuationStatus, RawEventInput, StoredContinuation};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("time format error: {0}")]
    TimeFormat(#[from] time::error::Format),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub struct Store {
    conn: Connection,
    archive_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredRawEvent {
    pub id: String,
    pub session_id: String,
    pub adapter_id: String,
    pub source: String,
    pub modality: String,
    pub act_type: Option<String>,
    pub time_utc: String,
    pub content: String,
    pub archive_path: String,
    pub archive_offset: i64,
    pub archive_len: i64,
    pub content_hash: String,
    pub evidence_status: String,
    pub created_at: String,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>, archive_dir: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref();
        let archive_dir = archive_dir.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            ensure_private_dir(parent)?;
        }
        ensure_private_dir(&archive_dir)?;

        ensure_private_file(db_path)?;

        let conn = Connection::open(db_path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_millis(5_000))?;
        run_migrations(&conn)?;
        restrict_file_if_exists(db_path)?;
        restrict_file_if_exists(&wal_path(db_path))?;
        restrict_file_if_exists(&shm_path(db_path))?;

        Ok(Self { conn, archive_dir })
    }

    pub fn append_raw_event(&self, input: &RawEventInput) -> Result<StoredRawEvent> {
        let id = format!("evt_{}", Uuid::new_v4().simple());
        let now = now_rfc3339()?;
        let time_utc = input.time_utc.clone().unwrap_or_else(|| now.clone());
        let archive_path = self.archive_dir.join("events-000001.jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .mode(0o600)
            .open(&archive_path)?;
        restrict_file_if_exists(&archive_path)?;
        let archive_offset = file.seek(SeekFrom::End(0))? as i64;

        let archive_record = serde_json::json!({
            "id": id,
            "session_id": input.session_id,
            "adapter_id": input.adapter_id,
            "source": input.source,
            "modality": input.modality,
            "act_type": input.act_type,
            "time_utc": time_utc,
            "content": input.content,
            "evidence_status": input.evidence_status,
            "created_at": now,
        });
        let mut line = serde_json::to_vec(&archive_record)?;
        line.push(b'\n');
        file.write_all(&line)?;
        file.flush()?;
        let archive_len = line.len() as i64;
        let content_hash = hex::encode(Sha256::digest(&line));
        let archive_path_string = archive_path.to_string_lossy().to_string();
        let source = serde_json::to_value(input.source)?
            .as_str()
            .unwrap()
            .to_string();
        let modality = serde_json::to_value(input.modality)?
            .as_str()
            .unwrap()
            .to_string();
        let evidence_status = serde_json::to_value(input.evidence_status)?
            .as_str()
            .unwrap()
            .to_string();

        self.conn.execute(
            "INSERT INTO raw_events (
                id, session_id, adapter_id, source, modality, act_type, time_utc, content,
                archive_path, archive_offset, archive_len, content_hash, evidence_status, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                id,
                input.session_id,
                input.adapter_id,
                source,
                modality,
                input.act_type,
                time_utc,
                input.content,
                archive_path_string,
                archive_offset,
                archive_len,
                content_hash,
                evidence_status,
                now,
            ],
        )?;
        self.conn.execute(
            "INSERT INTO raw_events_fts(event_id, content) VALUES (?1, ?2)",
            params![id, input.content],
        )?;

        Ok(self
            .get_raw_event(&id)?
            .expect("inserted raw event must be readable"))
    }

    pub fn get_raw_event(&self, id: &str) -> Result<Option<StoredRawEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, adapter_id, source, modality, act_type, time_utc, content,
                    archive_path, archive_offset, archive_len, content_hash, evidence_status, created_at
             FROM raw_events WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(StoredRawEvent {
            id: row.get(0)?,
            session_id: row.get(1)?,
            adapter_id: row.get(2)?,
            source: row.get(3)?,
            modality: row.get(4)?,
            act_type: row.get(5)?,
            time_utc: row.get(6)?,
            content: row.get(7)?,
            archive_path: row.get(8)?,
            archive_offset: row.get(9)?,
            archive_len: row.get(10)?,
            content_hash: row.get(11)?,
            evidence_status: row.get(12)?,
            created_at: row.get(13)?,
        }))
    }

    pub fn create_continuation(&self, input: &ContinuationInput) -> Result<StoredContinuation> {
        let id = format!("cont_{}", Uuid::new_v4().simple());
        let now = now_rfc3339()?;
        let status = continuation_status_to_string(input.status)?;

        self.conn.execute(
            "INSERT INTO continuations (
                id, title, summary, status, parent_id, raw_event_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                input.title,
                input.summary,
                status,
                input.parent_id,
                input.raw_event_id,
                now,
                now,
            ],
        )?;

        Ok(self
            .get_continuation(&id)?
            .expect("inserted continuation must be readable"))
    }

    pub fn list_continuations(&self) -> Result<Vec<StoredContinuation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, summary, status, parent_id, raw_event_id, created_at, updated_at
             FROM continuations
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], row_to_continuation_fields)?;
        let mut continuations = Vec::new();
        for row in rows {
            continuations.push(row?.into_stored()?);
        }
        Ok(continuations)
    }

    pub fn get_continuation(&self, id: &str) -> Result<Option<StoredContinuation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, summary, status, parent_id, raw_event_id, created_at, updated_at
             FROM continuations WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        row_to_continuation_fields(row)?.into_stored().map(Some)
    }

    pub fn search_continuations(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let pattern = like_literal_pattern(query);
        let mut stmt = self.conn.prepare(
            "SELECT id FROM continuations
             WHERE title LIKE ?1 ESCAPE '\\'
                OR summary LIKE ?1 ESCAPE '\\'
             ORDER BY updated_at, id
             LIMIT 20",
        )?;
        let rows = stmt.query_map(params![pattern], |row| row.get::<_, String>(0))?;
        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }

    pub fn search_raw_events(&self, query: &str) -> Result<Vec<String>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut hits = match self.search_raw_events_fts(query) {
            Ok(hits) => hits,
            Err(StoreError::Sqlite(_)) => Vec::new(),
            Err(error) => return Err(error),
        };
        if hits.is_empty() {
            hits = self.search_raw_events_like(query)?;
        }
        Ok(hits)
    }

    fn search_raw_events_fts(&self, query: &str) -> Result<Vec<String>> {
        let fts_query = fts_literal_query(query);
        let pattern = like_literal_pattern(query);
        let mut stmt = self.conn.prepare(
            "SELECT raw_events.id
             FROM raw_events_fts
             JOIN raw_events ON raw_events.id = raw_events_fts.event_id
             WHERE raw_events_fts MATCH ?1
               AND raw_events.content LIKE ?2 ESCAPE '\\'
             LIMIT 20",
        )?;
        let rows = stmt.query_map(params![fts_query, pattern], |row| row.get::<_, String>(0))?;
        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }

    fn search_raw_events_like(&self, query: &str) -> Result<Vec<String>> {
        let pattern = like_literal_pattern(query);
        let mut stmt = self.conn.prepare(
            "SELECT id FROM raw_events
             WHERE content LIKE ?1 ESCAPE '\\'
             ORDER BY created_at LIMIT 20",
        )?;
        let rows = stmt.query_map(params![pattern], |row| row.get::<_, String>(0))?;
        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }
}

struct ContinuationFields {
    id: String,
    title: String,
    summary: String,
    status: String,
    parent_id: Option<String>,
    raw_event_id: Option<String>,
    created_at: String,
    updated_at: String,
}

impl ContinuationFields {
    fn into_stored(self) -> Result<StoredContinuation> {
        Ok(StoredContinuation {
            id: self.id,
            title: self.title,
            summary: self.summary,
            status: continuation_status_from_string(&self.status)?,
            parent_id: self.parent_id,
            raw_event_id: self.raw_event_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn row_to_continuation_fields(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContinuationFields> {
    Ok(ContinuationFields {
        id: row.get(0)?,
        title: row.get(1)?,
        summary: row.get(2)?,
        status: row.get(3)?,
        parent_id: row.get(4)?,
        raw_event_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn continuation_status_to_string(status: ContinuationStatus) -> Result<String> {
    Ok(serde_json::to_value(status)?.as_str().unwrap().to_string())
}

fn continuation_status_from_string(status: &str) -> Result<ContinuationStatus> {
    Ok(serde_json::from_value(serde_json::Value::String(
        status.to_string(),
    ))?)
}

fn fts_literal_query(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() {
                return Err(io_error(
                    ErrorKind::InvalidInput,
                    format!("store path is not a directory: {}", path.display()),
                ));
            }
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(io_error(
                    ErrorKind::PermissionDenied,
                    format!(
                        "refusing non-private directory {} with mode {mode:o}; expected owner-only permissions",
                        path.display()
                    ),
                ));
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn ensure_private_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => restrict_file_if_exists(path),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            match OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .mode(0o600)
                .open(path)
            {
                Ok(_) => Ok(()),
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    restrict_file_if_exists(path)
                }
                Err(error) => Err(error.into()),
            }
        }
        Err(error) => Err(error.into()),
    }
}

fn restrict_file_if_exists(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_file() {
                return Err(io_error(
                    ErrorKind::InvalidInput,
                    format!("store path is not a regular file: {}", path.display()),
                ));
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn wal_path(db_path: &Path) -> PathBuf {
    sidecar_path(db_path, "-wal")
}

fn shm_path(db_path: &Path) -> PathBuf {
    sidecar_path(db_path, "-shm")
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

fn like_literal_pattern(query: &str) -> String {
    let mut pattern = String::with_capacity(query.len() + 2);
    pattern.push('%');
    for ch in query.chars() {
        match ch {
            '\\' => pattern.push_str("\\\\"),
            '%' => pattern.push_str("\\%"),
            '_' => pattern.push_str("\\_"),
            _ => pattern.push(ch),
        }
    }
    pattern.push('%');
    pattern
}

fn io_error(kind: ErrorKind, message: String) -> StoreError {
    StoreError::Io(std::io::Error::new(kind, message))
}

fn run_migrations(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        CREATE TABLE IF NOT EXISTS raw_events (
          id TEXT PRIMARY KEY,
          session_id TEXT NOT NULL,
          adapter_id TEXT NOT NULL,
          source TEXT NOT NULL,
          modality TEXT NOT NULL,
          act_type TEXT,
          time_utc TEXT NOT NULL,
          content TEXT NOT NULL,
          archive_path TEXT NOT NULL,
          archive_offset INTEGER NOT NULL,
          archive_len INTEGER NOT NULL,
          content_hash TEXT NOT NULL,
          evidence_status TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS raw_events_fts
        USING fts5(event_id UNINDEXED, content, tokenize='unicode61');
        CREATE TABLE IF NOT EXISTS continuations (
          id TEXT PRIMARY KEY,
          title TEXT NOT NULL,
          summary TEXT NOT NULL,
          status TEXT NOT NULL,
          parent_id TEXT,
          raw_event_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )?;
    Ok(())
}

fn now_rfc3339() -> Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}
