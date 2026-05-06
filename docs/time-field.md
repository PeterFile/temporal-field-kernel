# Time-Field Contract

Temporal Field Kernel is an external Agent time-field kernel. It makes long-term human continuity, consequences, timing, commitments, narrative continuity, living pasts, and forming futures explicit for any agent that calls it.

It is not an agent memory platform. It is the layer that turns history into influence on futures, tasks into continuations, actions into path choices, and context into temporal lenses.

## Non-Goals

- Not a memory platform.
- Not a task manager.
- Not a workflow engine.
- No UI in core.

## Primitives

- **Historical influence**: past observations and decisions are preserved as evidence that can shape future interpretation and choices.
- **Continuation**: an unfinished thread of intent, obligation, question, or narrative that future work may carry forward.
- **Continuation relation**: an explicit edge between continuations, idempotent by `(from_id, to_id, kind)`, that preserves how one living past supports, blocks, conflicts with, depends on, or subsumes another.
- **Path choice**: an action framed as selecting one possible future over others, with consequence and reversibility made explicit.
- **Temporal lens**: a query-time view that projects relevant pasts, commitments, and plausible futures into current context.
- **Commitment**: an explicit promise, constraint, or declared direction that should influence later path choices.
- **Living past**: past events that remain active because they still constrain, explain, or motivate present and future work.
- **Forming future**: a projected or implied future state that is not fixed yet but can be influenced by current choices.

## Crate Mapping

- `tfk-protocol`: wire contracts for observations, continuations, continuation relations, lenses, preflight signals, forecasts, commits, and envelopes.
- `tfk-core`: deterministic continuation, temporal lens, preflight, and scoring logic.
- `tfk-store`: append-only raw archive plus SQLite projections for historical influence, continuations, idempotent continuation relations, and lens grounding.
- `tfk-rules`: embedded Datalog-like fixed-point rules for deriving influence and consequence signals.
- `tfk-api`: local daemon API surface for observing, querying lenses, checking preflight risk, and recording continuations and relation edges.
- `tfk-daemon`: local kernel process that owns the archive, projections, API, and optional model client wiring.
- `tfk-cli`: operator and adapter entrypoint for feeding observations and reading temporal lens output.
- `tfk-mcp`: thin MCP wrapper over the daemon API; it contains no core time-field logic.
- `tfk-model-client`: optional Python predictor client for advisory forming-future signals.
- `tfk-eval`: TemporalBench runner for replaying fixtures against time-field behavior.

## Current Boundary

The current implementation is contract-level and local-first:

- raw event observation and append-only archive
- rebuildable SQLite projection with search
- minimal continuation graph with explicit idempotent continuation relations
- continuation-aware temporal lens cards with raw event fallback
- deterministic preflight scoring for path-choice confirmation
- thin local API, CLI, and MCP wrapper

The kernel does not schedule work, own user workflows, provide a UI, or act as an agent memory product.

## Next Slices

- Strengthen continuation influence in `/v1/lens` so active living pasts shape current context.
- Add commitment capture and retrieval as first-class path-choice constraints.
- Add advisory forecast signals for forming futures through the optional model client.
- Expand TemporalBench fixtures around historical influence, commitments, and consequence-sensitive path choices.
