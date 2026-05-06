use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Seek, SeekFrom, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tfk_protocol::{
    AdvisoryForecastSignal, CommitRequest, ContinuationDelta, ContinuationInput,
    ContinuationRelationEdge, ContinuationRelationKind, ContinuationStatus, ContinuationType,
    RawEventInput, StoredCommitment, StoredContinuation, TemporalDeltaInput,
};
use tfk_vector::{
    NoopVectorIndex, VectorDocument, VectorDocumentKind, VectorIndex, VectorIndexStatus,
};
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
    #[error("invalid temporal delta: {0}")]
    InvalidTemporalDelta(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub struct Store {
    conn: Connection,
    archive_dir: PathBuf,
    vector_index: Arc<dyn VectorIndex>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredTemporalDelta {
    pub id: String,
    pub action_id: String,
    pub changes_json: String,
    pub claims_json: String,
    pub evidence_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredAdvisoryForecastSignal {
    pub id: String,
    pub name: String,
    pub confidence: f64,
    pub model: String,
    pub action_name: Option<String>,
    pub reason: Option<String>,
    pub created_at: String,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>, archive_dir: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_vector_index(db_path, archive_dir, Arc::new(NoopVectorIndex::default()))
    }

    pub fn open_with_vector_index(
        db_path: impl AsRef<Path>,
        archive_dir: impl AsRef<Path>,
        vector_index: Arc<dyn VectorIndex>,
    ) -> Result<Self> {
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

        Ok(Self {
            conn,
            archive_dir,
            vector_index,
        })
    }

    pub fn vector_index_status(&self) -> VectorIndexStatus {
        self.vector_index.status()
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

        let stored = self
            .get_raw_event(&id)?
            .expect("inserted raw event must be readable");
        let document = VectorDocument::raw_event(&stored.id, &stored.content);
        let _ = self.vector_index.upsert(&document);
        Ok(stored)
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
        let continuation_type = continuation_type_to_string(input.continuation_type)?;

        self.conn.execute(
            "INSERT INTO continuations (
                id, title, summary, continuation_type, status, parent_id, raw_event_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                input.title,
                input.summary,
                continuation_type,
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

    pub fn create_commitment(
        &self,
        request: &CommitRequest,
        continuation_id: &str,
    ) -> Result<StoredCommitment> {
        let id = format!("commit_{}", Uuid::new_v4().simple());
        let now = now_rfc3339()?;
        let status = continuation_status_to_string(ContinuationStatus::Active)?;

        self.conn.execute(
            "INSERT INTO commitments (
                id, continuation_id, speaker, statement, scope, deadline, revocable, status, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id,
                continuation_id,
                &request.speaker,
                &request.statement,
                &request.scope,
                &request.deadline,
                request.revocable,
                status,
                now,
            ],
        )?;

        Ok(self
            .get_commitment(&id)?
            .expect("inserted commitment must be readable"))
    }

    pub fn get_commitment(&self, id: &str) -> Result<Option<StoredCommitment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, continuation_id, speaker, statement, scope, deadline, revocable, status, created_at
             FROM commitments WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        row_to_commitment(row)?.into_stored().map(Some)
    }

    pub fn list_active_commitments(&self) -> Result<Vec<StoredCommitment>> {
        let mut stmt = self.conn.prepare(
            "SELECT commitments.id, commitments.continuation_id, commitments.speaker,
                    commitments.statement, commitments.scope, commitments.deadline,
                    commitments.revocable, commitments.status, commitments.created_at
             FROM commitments
             JOIN continuations ON continuations.id = commitments.continuation_id
             WHERE commitments.status = 'active'
               AND continuations.status = 'active'
             ORDER BY commitments.created_at, commitments.id",
        )?;
        let rows = stmt.query_map([], row_to_commitment)?;
        self.collect_commitments(rows)
    }

    pub fn search_active_commitments(&self, query: &str) -> Result<Vec<StoredCommitment>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let pattern = like_literal_pattern(query);
        let mut stmt = self.conn.prepare(
            "SELECT commitments.id, commitments.continuation_id, commitments.speaker,
                    commitments.statement, commitments.scope, commitments.deadline,
                    commitments.revocable, commitments.status, commitments.created_at
             FROM commitments
             JOIN continuations ON continuations.id = commitments.continuation_id
             WHERE commitments.status = 'active'
               AND continuations.status = 'active'
               AND (
                    commitments.speaker LIKE ?1 ESCAPE '\\'
                 OR commitments.statement LIKE ?1 ESCAPE '\\'
                 OR commitments.scope LIKE ?1 ESCAPE '\\'
                 OR commitments.deadline LIKE ?1 ESCAPE '\\'
                 OR continuations.title LIKE ?1 ESCAPE '\\'
                 OR continuations.summary LIKE ?1 ESCAPE '\\'
               )
             ORDER BY commitments.created_at, commitments.id
             LIMIT 20",
        )?;
        let rows = stmt.query_map(params![pattern], row_to_commitment)?;
        self.collect_commitments(rows)
    }

    pub fn active_commitments_for_continuations(
        &self,
        continuation_ids: &[String],
    ) -> Result<Vec<StoredCommitment>> {
        let mut commitments = Vec::new();
        for continuation_id in continuation_ids {
            let mut stmt = self.conn.prepare(
                "SELECT commitments.id, commitments.continuation_id, commitments.speaker,
                        commitments.statement, commitments.scope, commitments.deadline,
                        commitments.revocable, commitments.status, commitments.created_at
                 FROM commitments
                 JOIN continuations ON continuations.id = commitments.continuation_id
                 WHERE commitments.status = 'active'
                   AND continuations.status = 'active'
                   AND commitments.continuation_id = ?1
                 ORDER BY commitments.created_at, commitments.id",
            )?;
            commitments.extend(self.collect_commitments(
                stmt.query_map(params![continuation_id], row_to_commitment)?,
            )?);
        }
        Ok(commitments)
    }

    fn collect_commitments<I>(&self, rows: I) -> Result<Vec<StoredCommitment>>
    where
        I: IntoIterator<Item = rusqlite::Result<CommitmentFields>>,
    {
        let mut commitments = Vec::new();
        for row in rows {
            commitments.push(row?.into_stored()?);
        }
        Ok(commitments)
    }

    pub fn list_continuations(&self) -> Result<Vec<StoredContinuation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, summary, continuation_type, status, parent_id, raw_event_id, created_at, updated_at
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
            "SELECT id, title, summary, continuation_type, status, parent_id, raw_event_id, created_at, updated_at
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

    pub fn active_continuations_for_raw_event_ids(
        &self,
        raw_event_ids: &[String],
    ) -> Result<Vec<StoredContinuation>> {
        let mut continuations = Vec::new();
        for raw_event_id in raw_event_ids {
            let mut stmt = self.conn.prepare(
                "SELECT id, title, summary, continuation_type, status, parent_id, raw_event_id, created_at, updated_at
                 FROM continuations
                 WHERE status = 'active'
                   AND raw_event_id = ?1
                 ORDER BY updated_at, id",
            )?;
            let rows = stmt.query_map(params![raw_event_id], row_to_continuation_fields)?;
            for row in rows {
                continuations.push(row?.into_stored()?);
            }
        }
        Ok(continuations)
    }

    pub fn create_continuation_relation(
        &self,
        relation: &ContinuationRelationEdge,
    ) -> Result<ContinuationRelationEdge> {
        let kind = continuation_relation_kind_to_string(relation.kind)?;
        let now = now_rfc3339()?;
        self.conn.execute(
            "INSERT INTO continuation_relations (
                from_id, to_id, kind, reason, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &relation.from_id,
                &relation.to_id,
                kind,
                &relation.reason,
                now,
            ],
        )?;
        Ok(relation.clone())
    }

    pub fn list_continuation_relations(&self) -> Result<Vec<ContinuationRelationEdge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_id, to_id, kind, reason
             FROM continuation_relations
             ORDER BY created_at, from_id, to_id, kind",
        )?;
        let rows = stmt.query_map([], row_to_continuation_relation_fields)?;
        collect_continuation_relations(rows)
    }

    pub fn active_continuation_relations_for_continuation_ids(
        &self,
        continuation_ids: &[String],
    ) -> Result<Vec<ContinuationRelationEdge>> {
        let mut relations = Vec::new();
        for continuation_id in continuation_ids {
            let mut stmt = self.conn.prepare(
                "SELECT relation.from_id, relation.to_id, relation.kind, relation.reason
                 FROM continuation_relations relation
                 JOIN continuations from_continuation ON from_continuation.id = relation.from_id
                 JOIN continuations to_continuation ON to_continuation.id = relation.to_id
                 WHERE (relation.from_id = ?1 OR relation.to_id = ?1)
                   AND from_continuation.status = 'active'
                   AND to_continuation.status = 'active'
                 ORDER BY relation.created_at, relation.from_id, relation.to_id, relation.kind",
            )?;
            for relation in collect_continuation_relations(stmt.query_map(
                params![continuation_id],
                row_to_continuation_relation_fields,
            )?)? {
                if !relations.contains(&relation) {
                    relations.push(relation);
                }
            }
        }
        Ok(relations)
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

    pub fn assimilate_delta(&mut self, input: &TemporalDeltaInput) -> Result<StoredTemporalDelta> {
        let id = format!("delta_{}", Uuid::new_v4().simple());
        let now = now_rfc3339()?;
        let changes_json = serde_json::to_string(&input.changes)?;
        let claims_json = serde_json::to_string(&input.claims_made)?;
        let evidence_json = serde_json::to_string(&input.evidence)?;

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO temporal_deltas (
                id, action_id, changes_json, claims_json, evidence_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &id,
                &input.action_id,
                &changes_json,
                &claims_json,
                &evidence_json,
                &now,
            ],
        )?;
        for change in &input.changes {
            let Some(status) = status_for_delta(change.delta) else {
                continue;
            };
            let status = continuation_status_to_string(status)?;
            let updated = tx.execute(
                "UPDATE continuations
                 SET status = ?1, updated_at = ?2
                 WHERE id = ?3",
                params![status, &now, &change.continuation_id],
            )?;
            if updated != 1 {
                return Err(StoreError::InvalidTemporalDelta(format!(
                    "status delta {:?} references unknown continuation {}",
                    change.delta, change.continuation_id
                )));
            }
        }
        tx.commit()?;

        Ok(self
            .get_temporal_delta(&id)?
            .expect("inserted temporal delta must be readable"))
    }

    pub fn list_temporal_deltas(&self) -> Result<Vec<StoredTemporalDelta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action_id, changes_json, claims_json, evidence_json, created_at
             FROM temporal_deltas
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], row_to_temporal_delta)?;
        let mut deltas = Vec::new();
        for row in rows {
            deltas.push(row?);
        }
        Ok(deltas)
    }

    pub fn get_temporal_delta(&self, id: &str) -> Result<Option<StoredTemporalDelta>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, action_id, changes_json, claims_json, evidence_json, created_at
             FROM temporal_deltas WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(row_to_temporal_delta(row)?))
    }

    pub fn record_advisory_forecast_signals(
        &self,
        signals: &[AdvisoryForecastSignal],
    ) -> Result<Vec<StoredAdvisoryForecastSignal>> {
        let mut stored = Vec::with_capacity(signals.len());
        for signal in signals {
            let id = format!("advisory_signal_{}", Uuid::new_v4().simple());
            let now = now_rfc3339()?;
            self.conn.execute(
                "INSERT INTO advisory_forecast_signals (
                    id, name, confidence, model, action_name, reason, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &id,
                    &signal.name,
                    signal.confidence,
                    &signal.model,
                    &signal.action_name,
                    &signal.reason,
                    &now,
                ],
            )?;
            let stored_signal = self
                .get_advisory_forecast_signal(&id)?
                .expect("inserted advisory forecast signal must be readable");
            let document = VectorDocument {
                source_id: stored_signal.id.clone(),
                kind: VectorDocumentKind::AdvisoryForecastSignal,
                text: advisory_forecast_signal_vector_text(&stored_signal),
                embedding: None,
            };
            let _ = self.vector_index.upsert(&document);
            stored.push(stored_signal);
        }
        Ok(stored)
    }

    pub fn list_advisory_forecast_signals(&self) -> Result<Vec<StoredAdvisoryForecastSignal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, confidence, model, action_name, reason, created_at
             FROM advisory_forecast_signals
             ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], row_to_advisory_forecast_signal)?;
        let mut signals = Vec::new();
        for row in rows {
            signals.push(row?);
        }
        Ok(signals)
    }

    pub fn get_advisory_forecast_signal(
        &self,
        id: &str,
    ) -> Result<Option<StoredAdvisoryForecastSignal>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, confidence, model, action_name, reason, created_at
             FROM advisory_forecast_signals WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(row_to_advisory_forecast_signal(row)?))
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

fn advisory_forecast_signal_vector_text(signal: &StoredAdvisoryForecastSignal) -> String {
    format!(
        "name: {}\nmodel: {}\nconfidence: {}\naction_name: {}\nreason: {}",
        signal.name,
        signal.model,
        signal.confidence,
        signal.action_name.as_deref().unwrap_or_default(),
        signal.reason.as_deref().unwrap_or_default()
    )
}

fn row_to_advisory_forecast_signal(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredAdvisoryForecastSignal> {
    Ok(StoredAdvisoryForecastSignal {
        id: row.get(0)?,
        name: row.get(1)?,
        confidence: row.get(2)?,
        model: row.get(3)?,
        action_name: row.get(4)?,
        reason: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_to_temporal_delta(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredTemporalDelta> {
    Ok(StoredTemporalDelta {
        id: row.get(0)?,
        action_id: row.get(1)?,
        changes_json: row.get(2)?,
        claims_json: row.get(3)?,
        evidence_json: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn status_for_delta(delta: ContinuationDelta) -> Option<ContinuationStatus> {
    match delta {
        ContinuationDelta::Activate
        | ContinuationDelta::Advance
        | ContinuationDelta::Repair
        | ContinuationDelta::Verify
        | ContinuationDelta::Renegotiate => Some(ContinuationStatus::Active),
        ContinuationDelta::Create | ContinuationDelta::Split => None,
        ContinuationDelta::Stabilize => Some(ContinuationStatus::Stabilized),
        ContinuationDelta::Defer => Some(ContinuationStatus::Deferred),
        ContinuationDelta::Close => Some(ContinuationStatus::Closed),
        ContinuationDelta::Retire => Some(ContinuationStatus::Retired),
    }
}

struct ContinuationFields {
    id: String,
    title: String,
    summary: String,
    continuation_type: String,
    status: String,
    parent_id: Option<String>,
    raw_event_id: Option<String>,
    created_at: String,
    updated_at: String,
}

struct CommitmentFields {
    id: String,
    continuation_id: String,
    speaker: String,
    statement: String,
    scope: Option<String>,
    deadline: Option<String>,
    revocable: bool,
    status: String,
    created_at: String,
}

struct ContinuationRelationFields {
    from_id: String,
    to_id: String,
    kind: String,
    reason: Option<String>,
}

impl ContinuationRelationFields {
    fn into_edge(self) -> Result<ContinuationRelationEdge> {
        Ok(ContinuationRelationEdge {
            from_id: self.from_id,
            to_id: self.to_id,
            kind: continuation_relation_kind_from_string(&self.kind)?,
            reason: self.reason,
        })
    }
}

impl CommitmentFields {
    fn into_stored(self) -> Result<StoredCommitment> {
        Ok(StoredCommitment {
            id: self.id,
            continuation_id: self.continuation_id,
            speaker: self.speaker,
            statement: self.statement,
            scope: self.scope,
            deadline: self.deadline,
            revocable: self.revocable,
            status: continuation_status_from_string(&self.status)?,
            created_at: self.created_at,
        })
    }
}

fn row_to_commitment(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommitmentFields> {
    Ok(CommitmentFields {
        id: row.get(0)?,
        continuation_id: row.get(1)?,
        speaker: row.get(2)?,
        statement: row.get(3)?,
        scope: row.get(4)?,
        deadline: row.get(5)?,
        revocable: row.get(6)?,
        status: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn row_to_continuation_relation_fields(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ContinuationRelationFields> {
    Ok(ContinuationRelationFields {
        from_id: row.get(0)?,
        to_id: row.get(1)?,
        kind: row.get(2)?,
        reason: row.get(3)?,
    })
}

fn collect_continuation_relations<I>(rows: I) -> Result<Vec<ContinuationRelationEdge>>
where
    I: IntoIterator<Item = rusqlite::Result<ContinuationRelationFields>>,
{
    let mut relations = Vec::new();
    for row in rows {
        relations.push(row?.into_edge()?);
    }
    Ok(relations)
}

impl ContinuationFields {
    fn into_stored(self) -> Result<StoredContinuation> {
        Ok(StoredContinuation {
            id: self.id,
            title: self.title,
            summary: self.summary,
            continuation_type: continuation_type_from_string(&self.continuation_type)?,
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
        continuation_type: row.get(3)?,
        status: row.get(4)?,
        parent_id: row.get(5)?,
        raw_event_id: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn continuation_type_to_string(continuation_type: ContinuationType) -> Result<String> {
    Ok(serde_json::to_value(continuation_type)?
        .as_str()
        .unwrap()
        .to_string())
}

fn continuation_type_from_string(continuation_type: &str) -> Result<ContinuationType> {
    Ok(serde_json::from_value(serde_json::Value::String(
        continuation_type.to_string(),
    ))?)
}

fn continuation_status_to_string(status: ContinuationStatus) -> Result<String> {
    Ok(serde_json::to_value(status)?.as_str().unwrap().to_string())
}

fn continuation_status_from_string(status: &str) -> Result<ContinuationStatus> {
    Ok(serde_json::from_value(serde_json::Value::String(
        status.to_string(),
    ))?)
}

fn continuation_relation_kind_to_string(kind: ContinuationRelationKind) -> Result<String> {
    Ok(serde_json::to_value(kind)?.as_str().unwrap().to_string())
}

fn continuation_relation_kind_from_string(kind: &str) -> Result<ContinuationRelationKind> {
    Ok(serde_json::from_value(serde_json::Value::String(
        kind.to_string(),
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
          continuation_type TEXT NOT NULL DEFAULT 'narrative',
          status TEXT NOT NULL,
          parent_id TEXT,
          raw_event_id TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS commitments (
          id TEXT PRIMARY KEY,
          continuation_id TEXT NOT NULL REFERENCES continuations(id),
          speaker TEXT NOT NULL,
          statement TEXT NOT NULL,
          scope TEXT,
          deadline TEXT,
          revocable INTEGER NOT NULL,
          status TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS continuation_relations (
          from_id TEXT NOT NULL REFERENCES continuations(id),
          to_id TEXT NOT NULL REFERENCES continuations(id),
          kind TEXT NOT NULL,
          reason TEXT,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS temporal_deltas (
          id TEXT PRIMARY KEY,
          action_id TEXT NOT NULL,
          changes_json TEXT NOT NULL,
          claims_json TEXT NOT NULL,
          evidence_json TEXT NOT NULL,
          created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS advisory_forecast_signals (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          confidence REAL NOT NULL,
          model TEXT NOT NULL,
          action_name TEXT,
          reason TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )?;
    if !continuations_column_exists(conn, "continuation_type")? {
        conn.execute(
            "ALTER TABLE continuations
             ADD COLUMN continuation_type TEXT NOT NULL DEFAULT 'narrative'",
            [],
        )?;
    }
    Ok(())
}

fn continuations_column_exists(conn: &Connection, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA table_info(continuations)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn now_rfc3339() -> Result<String> {
    Ok(OffsetDateTime::now_utc().format(&Rfc3339)?)
}
