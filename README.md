# Temporal Field Kernel

External Agent time-field kernel that turns history into influence on futures, tasks into continuations, actions into path choices, and context into temporal lenses.

Current scope: no UI in core. The MVP is a Rust kernel daemon, Rust CLI, SQLite WAL projection store, append-only JSONL raw event archive, Datalog-like rules, optional Python prediction sidecar, local UDS/HTTP API, and MCP thin wrapper.

## Workspace

```text
crates/
  tfk-protocol      shared wire types
  tfk-core          continuation/preflight/lens logic
  tfk-store         SQLite + JSONL temporal projection/archive
  tfk-rules         embedded Datalog-like rule engine
  tfk-vector        optional vector index contract + sqlite-vec capability probe
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
cargo run -q -p tfk-daemon -- serve --uds /tmp/tfk.sock --data-dir /tmp/tfk-data --forecast-advisory-json fixtures/temporalbench/forecast_advisory/basic_forecast.json
cargo run -q -p tfk-daemon -- serve --uds /tmp/tfk.sock --data-dir /tmp/tfk-data --forecast-sidecar-command python3 --forecast-sidecar-arg python/tfk_predictor/tfk_predictor/server.py
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock observe --session s1 --adapter cli "不要做项目状态机"
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation create --summary "继续跟踪这个判断" "项目状态机不是目标"
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation list
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock continuation get cont_...
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock relation create --from-id cont_left --to-id cont_right --kind blocks --reason "right waits for left"
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock relation list
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock commitment list
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock commit create --speaker agent --statement "ship PR1" --scope current_project --deadline 2026-05-07 --revocable true
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock forecast --json-file fixtures/temporalbench/forecast_advisory/basic_forecast.json
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock advisory-forecast-signal list
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock advisory-forecast-signal get signal_...
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock assimilate --json-file /tmp/delta.json
cargo run -q -p tfk-cli -- --uds /tmp/tfk.sock lens "项目状态机"
```

`--forecast-advisory-json` is opt-in. It loads local static advisory forecast signals from either a bare `AdvisoryForecastSignal[]` JSON file or an object containing `advisory_signals`. `--forecast-sidecar-command` is also opt-in and runs a local stdio sidecar command with repeated `--forecast-sidecar-arg` values; the bundled Python sidecar emits `advisory_signals` and degrades to a deterministic heuristic when `river` is not installed. Degraded forecast sidecar status is exposed as `/v1/forecast` envelope warnings without changing deterministic forecast results. When both flags are omitted, forecast behavior is unchanged; the deterministic scorer still runs in all cases.

Continuation relation create/list is a smoke path for explicit graph edges between existing continuation IDs. Repeating `relation create` with the same `--from-id`, `--to-id`, and `--kind` is idempotent: it returns the existing edge and keeps the original reason.

The daemon keeps the raw event archive and rebuildable projection in:

```text
<data-dir>/tfk.db
<data-dir>/archive/events-000001.jsonl
```
