# API Design

Canonical local endpoints:

```text
POST /v1/observe
POST /v1/continuations
GET  /v1/continuations
GET  /v1/continuations/:id
POST /v1/lens
POST /v1/forecast
POST /v1/preflight
POST /v1/commit
GET  /v1/commitments
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

Accepts `RawEventInput` and appends it to the JSONL archive plus SQLite/FTS projection for historical influence.

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

### POST /v1/continuations

Accepts `ContinuationInput` and records a minimal continuation graph node in SQLite.
When omitted, `continuation_type` defaults to `narrative` for backward compatibility.

Minimal body:

```json
{
  "title": "项目状态机不是目标",
  "summary": "继续跟踪这个判断",
  "continuation_type": "narrative",
  "status": "active",
  "parent_id": null,
  "raw_event_id": null
}
```

Returns `ApiEnvelope<StoredContinuation>`.

### GET /v1/continuations

Returns `ApiEnvelope<Vec<StoredContinuation>>` ordered by creation time.

### GET /v1/continuations/:id

Returns `ApiEnvelope<StoredContinuation>` for one stored continuation, or a 404 envelope.

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

Smoke:

```bash
tfk preflight --uncertainty 0.9 --irreversibility 0.8 --externality 0.7
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

Current behavior is intentionally minimal: search continuation title/summary and active commitments first, then return a temporal `LensCard` grounded by matching `continuation` provenance. Matching active commitments are surfaced as `commitment_constraints`. If no continuation or active commitment matches, it falls back to raw events with `raw_event` provenance. This is a lens over living pasts, not a generic memory recall API.

### POST /v1/commit

Accepts `CommitRequest`, preserves the existing behavior of returning the created obligation `StoredContinuation`, and also persists a linked structured commitment row.

### GET /v1/commitments

Returns `ApiEnvelope<Vec<StoredCommitment>>` for active commitments whose linked continuation is still active.
