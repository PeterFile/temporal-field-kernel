# Architecture

```text
Popular Agent / CLI / MCP Client
        ↓
Adapter: CLI wrapper / MCP thin wrapper / local API client
        ↓
tfkd Rust daemon
        ↓
append-only JSONL raw archive + SQLite WAL projection store
        ↓
continuation graph + rules + lens/preflight/delta APIs
```

Principles:

- JSONL is the raw event archive.
- SQLite is the rebuildable projection/query store.
- Python predictor is advisory and optional.
- MCP is thin and contains no business logic.
- UDS is the default local API transport; HTTP localhost is optional.
