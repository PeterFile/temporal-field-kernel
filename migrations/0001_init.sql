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
  type TEXT NOT NULL,
  origin_event_id TEXT NOT NULL REFERENCES raw_events(id),
  status TEXT NOT NULL,
  tension TEXT NOT NULL,
  scope TEXT NOT NULL,
  path_predicate TEXT NOT NULL,
  pressure_curve TEXT NOT NULL,
  violation_cost TEXT NOT NULL,
  closure_condition TEXT,
  repair_policy TEXT,
  confidence REAL NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
