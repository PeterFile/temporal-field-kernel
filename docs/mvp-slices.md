# MVP Slices

## v0.1

- Rust workspace
- tfkd / tfk binaries
- SQLite WAL store
- append-only JSONL archive
- raw event index
- FTS5 search
- preflight scoring
- store-backed `/v1/observe`, `/v1/preflight`, and minimal `/v1/lens`
- `tfk observe` and `tfk lens` UDS calls into `tfkd`
- lens/preflight protocol types

## v0.2

- MCP thin wrapper
- CLI wrapper session capture
- embedded Datalog-like rules
- TemporalBench fixture replay

## v0.3

- sqlite-vec adapter
- optional Python sidecar with river
- model prediction table
