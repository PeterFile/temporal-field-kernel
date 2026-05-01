CREATE VIRTUAL TABLE IF NOT EXISTS raw_events_fts
USING fts5(event_id UNINDEXED, content, tokenize='unicode61');
