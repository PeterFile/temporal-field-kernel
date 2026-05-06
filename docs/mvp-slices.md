# MVP Slices

## v0.1

- Rust workspace
- tfkd / tfk binaries
- SQLite WAL temporal projection
- append-only JSONL archive
- raw event index
- FTS5 search
- preflight scoring
- projection-backed `/v1/observe`, `/v1/preflight`, and minimal `/v1/lens`
- `tfk observe` and `tfk lens` UDS calls into `tfkd`
- lens/preflight protocol types

## v0.2

- minimal continuation graph protocol/projection/API/CLI
- continuation-aware temporal lens output with raw event fallback
- MCP thin wrapper
- CLI wrapper session capture
- embedded Datalog-like fixed-point rules
- TemporalBench fixture replay

## v0.3

- sqlite-vec adapter (optional `tfk-vector` contract with Noop fallback; runtime must probe `vec0` before creating virtual tables)
- optional Python sidecar with river
- advisory forming-future prediction table
