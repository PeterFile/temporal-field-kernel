# API Design

Canonical local endpoints:

```text
POST /v1/observe
POST /v1/lens
POST /v1/forecast
POST /v1/preflight
POST /v1/commit
POST /v1/assimilate
GET  /healthz
```

Transport order:

1. Unix domain socket by default.
2. HTTP localhost only when explicitly enabled.
3. MCP remains a thin wrapper over the daemon API.

## Implemented in v0.1 scaffold

### GET /healthz

Returns an `ApiEnvelope` with `{ "status": "ok" }`.

### POST /v1/observe

Accepts `RawEventInput` and appends it to the JSONL archive plus SQLite/FTS projection.

Minimal body:

```json
{
  "session_id": "s1",
  "adapter_id": "cli",
  "source": "user",
  "modality": "text",
  "content": "不要做项目状态机",
  "act_type": null,
  "evidence_status": "observed",
  "time_utc": null
}
```

Returns `ApiEnvelope<StoredRawEvent>`.

### POST /v1/preflight

Accepts `PreflightSignals`:

```json
{
  "uncertainty": 0.9,
  "irreversibility": 0.8,
  "externality": 0.7,
  "option_value_loss": 0.0
}
```

Returns the deterministic scorer result. Current rule:

```text
requires_confirmation = uncertainty * irreversibility * externality > 0.5
```

### POST /v1/lens

Accepts `LensRequest`:

```json
{
  "query": "项目状态机",
  "horizon": [],
  "perspective": []
}
```

Current behavior is intentionally minimal: search raw events and return a `LensCard` grounded by matching `raw_event` provenance. It is not a continuation graph yet.
