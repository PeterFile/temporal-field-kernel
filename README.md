# Temporal Field Kernel

Local-first temporal sidecar for popular agents.

Current scope: no UI. The MVP is a Rust kernel daemon, Rust CLI, SQLite WAL projection store, append-only JSONL raw event archive, Datalog-like rules, optional Python prediction sidecar, local UDS/HTTP API, and MCP thin wrapper.

## Workspace

```text
crates/
  tfk-protocol      shared wire types
  tfk-core          continuation/preflight/lens logic
  tfk-store         SQLite + JSONL storage
  tfk-rules         embedded Datalog-like rule engine
  tfk-api           local API router
  tfk-daemon        tfkd binary
  tfk-cli           tfk CLI
  tfk-mcp           MCP thin wrapper
  tfk-model-client  Python sidecar client
  tfk-eval          TemporalBench runner
```

## Local smoke

```bash
cargo run -q -p tfk-daemon -- serve --uds /tmp/tfk.sock --data-dir /tmp/tfk-data
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock observe --session s1 --adapter cli "不要做项目状态机"
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation create --summary "继续跟踪这个判断" "项目状态机不是目标"
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation list
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation get cont_...
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock lens "项目状态机"
```

The daemon stores raw events in:

```text
<data-dir>/tfk.db
<data-dir>/archive/events-000001.jsonl
```
