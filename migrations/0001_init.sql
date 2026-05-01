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
