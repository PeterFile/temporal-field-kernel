use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tfk_core::{
    lens_rule_facts, ForecastScorer, PreflightScorer, RuleFact, TimeFieldContinuation,
    TimeFieldLensEngine, TimeFieldVectorInfluence,
};
use tfk_model_client::ForecastPredictionClient;
use tfk_protocol::{
    ApiEnvelope, CommitRequest, ContinuationInput, ContinuationRelationEdge, ContinuationStatus,
    ContinuationType, ForecastRequest, ForecastResult, LensAdvisoryForecastSignal, LensCard,
    LensRequest, PreflightResult, PreflightSignals, ProvenanceRef, RawEventInput, StoredCommitment,
    StoredContinuation, TemporalDeltaInput,
};
use tfk_store::{
    Store, StoreError, StoredAdvisoryForecastSignal, StoredRawEvent, StoredTemporalDelta,
};

#[derive(Clone)]
pub struct ApiState {
    store: Arc<Mutex<Store>>,
    preflight_scorer: PreflightScorer,
    forecast_scorer: ForecastScorer,
    forecast_client: Option<Arc<dyn ForecastPredictionClient>>,
}

impl ApiState {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            preflight_scorer: PreflightScorer::with_threshold(0.5),
            forecast_scorer: ForecastScorer,
            forecast_client: None,
        }
    }

    pub fn with_forecast_client(mut self, client: impl ForecastPredictionClient + 'static) -> Self {
        self.forecast_client = Some(Arc::new(client));
        self
    }
}

pub fn router() -> Router {
    Router::new().route("/healthz", get(health_handler))
}

pub fn router_with_store(store: Store) -> Router {
    router_with_state(ApiState::new(store))
}

pub fn router_with_state(state: ApiState) -> Router {
    Router::new()
        .route("/healthz", get(health_handler))
        .route("/v1/observe", post(observe_handler))
        .route("/v1/raw-events", get(search_raw_events_handler))
        .route("/v1/raw-events/:id", get(get_raw_event_handler))
        .route(
            "/v1/continuations",
            post(create_continuation_handler).get(list_continuations_handler),
        )
        .route("/v1/continuations/:id", get(get_continuation_handler))
        .route(
            "/v1/continuation-relations",
            post(create_continuation_relation_handler).get(list_continuation_relations_handler),
        )
        .route("/v1/preflight", post(preflight_handler))
        .route("/v1/lens", post(lens_handler))
        .route("/v1/forecast", post(forecast_handler))
        .route(
            "/v1/advisory-forecast-signals",
            get(list_advisory_forecast_signals_handler),
        )
        .route(
            "/v1/advisory-forecast-signals/:id",
            get(get_advisory_forecast_signal_handler),
        )
        .route("/v1/commit", post(commit_handler))
        .route("/v1/commitments", get(list_commitments_handler))
        .route("/v1/assimilate", post(assimilate_handler))
        .route("/v1/temporal-deltas", get(list_temporal_deltas_handler))
        .route("/v1/temporal-deltas/:id", get(get_temporal_delta_handler))
        .with_state(state)
}

pub fn health_envelope(
    request_id: impl Into<String>,
    trace_id: impl Into<String>,
) -> ApiEnvelope<Value> {
    ApiEnvelope::ok(request_id, trace_id, json!({ "status": "ok" }))
}

async fn health_handler() -> Json<ApiEnvelope<Value>> {
    Json(health_envelope("local-health", "local-health"))
}

async fn observe_handler(
    State(state): State<ApiState>,
    Json(input): Json<RawEventInput>,
) -> Result<Json<ApiEnvelope<StoredRawEvent>>, ApiError> {
    let stored = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .append_raw_event(&input)
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-observe",
        "local-observe",
        stored,
    )))
}

#[derive(Debug, Default, serde::Deserialize)]
struct RawEventSearchParams {
    query: Option<String>,
}

async fn search_raw_events_handler(
    State(state): State<ApiState>,
    Query(params): Query<RawEventSearchParams>,
) -> Result<Json<ApiEnvelope<Vec<StoredRawEvent>>>, ApiError> {
    let query = params.query.unwrap_or_default();
    let events = {
        let store = state
            .store
            .lock()
            .map_err(|_| internal_error("store lock poisoned"))?;
        let ids = store
            .search_raw_events(&query)
            .map_err(|error| internal_error(error.to_string()))?;
        let mut events = Vec::new();
        for id in ids {
            if let Some(event) = store
                .get_raw_event(&id)
                .map_err(|error| internal_error(error.to_string()))?
            {
                events.push(event);
            }
        }
        events
    };

    Ok(Json(ApiEnvelope::ok(
        "local-raw-event-search",
        "local-raw-event-search",
        events,
    )))
}

async fn get_raw_event_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ApiEnvelope<StoredRawEvent>>, ApiError> {
    let event = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .get_raw_event(&id)
        .map_err(|error| internal_error(error.to_string()))?
        .ok_or_else(|| not_found_error(format!("raw event not found: {id}")))?;

    Ok(Json(ApiEnvelope::ok(
        "local-raw-event-get",
        "local-raw-event-get",
        event,
    )))
}

async fn create_continuation_handler(
    State(state): State<ApiState>,
    Json(input): Json<ContinuationInput>,
) -> Result<Json<ApiEnvelope<StoredContinuation>>, ApiError> {
    let stored = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .create_continuation(&input)
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-continuation-create",
        "local-continuation-create",
        stored,
    )))
}

async fn list_continuations_handler(
    State(state): State<ApiState>,
) -> Result<Json<ApiEnvelope<Vec<StoredContinuation>>>, ApiError> {
    let continuations = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .list_continuations()
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-continuation-list",
        "local-continuation-list",
        continuations,
    )))
}

async fn get_continuation_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ApiEnvelope<StoredContinuation>>, ApiError> {
    let continuation = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .get_continuation(&id)
        .map_err(|error| internal_error(error.to_string()))?
        .ok_or_else(|| not_found_error(format!("continuation not found: {id}")))?;

    Ok(Json(ApiEnvelope::ok(
        "local-continuation-get",
        "local-continuation-get",
        continuation,
    )))
}

async fn create_continuation_relation_handler(
    State(state): State<ApiState>,
    Json(input): Json<ContinuationRelationEdge>,
) -> Result<Json<ApiEnvelope<ContinuationRelationEdge>>, ApiError> {
    let stored = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .create_continuation_relation(&input)
        .map_err(api_error_for_relation_create)?;

    Ok(Json(ApiEnvelope::ok(
        "local-continuation-relation-create",
        "local-continuation-relation-create",
        stored,
    )))
}

async fn list_continuation_relations_handler(
    State(state): State<ApiState>,
) -> Result<Json<ApiEnvelope<Vec<ContinuationRelationEdge>>>, ApiError> {
    let relations = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .list_continuation_relations()
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-continuation-relation-list",
        "local-continuation-relation-list",
        relations,
    )))
}

async fn preflight_handler(
    State(state): State<ApiState>,
    Json(signals): Json<PreflightSignals>,
) -> Json<ApiEnvelope<PreflightResult>> {
    Json(ApiEnvelope::ok(
        "local-preflight",
        "local-preflight",
        state.preflight_scorer.score(signals),
    ))
}

async fn forecast_handler(
    State(state): State<ApiState>,
    Json(request): Json<ForecastRequest>,
) -> Result<Json<ApiEnvelope<ForecastResult>>, ApiError> {
    let mut warnings = Vec::new();
    let mut continuation_ids = Vec::new();
    for continuation_id in request
        .actions
        .iter()
        .filter_map(|action| action.continuation_id.clone())
    {
        if !continuation_ids.contains(&continuation_id) {
            continuation_ids.push(continuation_id);
        }
    }
    let commitment_constraints = if continuation_ids.is_empty() {
        Vec::new()
    } else {
        state
            .store
            .lock()
            .map_err(|_| internal_error("store lock poisoned"))?
            .active_commitments_for_continuations(&continuation_ids)
            .map_err(|error| internal_error(error.to_string()))?
    };
    let mut result = state
        .forecast_scorer
        .score_with_commitments(&request, &commitment_constraints);
    let mut provenance: Vec<_> = commitment_constraints
        .iter()
        .map(|commitment| ProvenanceRef {
            kind: "commitment".to_string(),
            id: commitment.id.clone(),
        })
        .collect();

    if let Some(client) = &state.forecast_client {
        match client.forecast_with_status(&request) {
            Ok(status) => {
                if status.degraded {
                    warnings.push(degraded_forecast_warning(status.reason.as_deref()));
                }
                let signals = status.advisory_signals;
                if !signals.is_empty() {
                    let persist_result = state
                        .store
                        .lock()
                        .map_err(|_| "store lock poisoned".to_string())
                        .and_then(|store| {
                            store
                                .record_advisory_forecast_signals(&signals)
                                .map_err(|error| error.to_string())
                        });
                    match persist_result {
                        Ok(stored_signals) => {
                            provenance.extend(stored_signals.into_iter().map(|signal| ProvenanceRef {
                                kind: "advisory_forecast_signal".to_string(),
                                id: signal.id,
                            }));
                        }
                        Err(error) => warnings.push(format!(
                            "forecast advisory signals were not persisted; deterministic result returned: {error}"
                        )),
                    }
                }
                result.advisory_signals.extend(signals);
            }
            Err(error) => warnings.push(format!(
                "forecast advisory model failed; deterministic result returned: {error}"
            )),
        }
    }

    let mut envelope = ApiEnvelope::ok("local-forecast", "local-forecast", result);
    envelope.warnings = warnings;
    envelope.provenance = provenance;

    Ok(Json(envelope))
}

async fn list_advisory_forecast_signals_handler(
    State(state): State<ApiState>,
) -> Result<Json<ApiEnvelope<Vec<StoredAdvisoryForecastSignal>>>, ApiError> {
    let signals = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .list_advisory_forecast_signals()
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-advisory-forecast-signal-list",
        "local-advisory-forecast-signal-list",
        signals,
    )))
}

async fn get_advisory_forecast_signal_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ApiEnvelope<StoredAdvisoryForecastSignal>>, ApiError> {
    let signal = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .get_advisory_forecast_signal(&id)
        .map_err(|error| internal_error(error.to_string()))?
        .ok_or_else(|| not_found_error(format!("advisory forecast signal not found: {id}")))?;

    Ok(Json(ApiEnvelope::ok(
        "local-advisory-forecast-signal-get",
        "local-advisory-forecast-signal-get",
        signal,
    )))
}

fn degraded_forecast_warning(reason: Option<&str>) -> String {
    let reason = reason.map(str::trim).filter(|reason| !reason.is_empty());
    match reason {
        Some(reason) => {
            format!("forecast advisory model degraded; deterministic result returned: {reason}")
        }
        None => "forecast advisory model degraded; deterministic result returned".to_string(),
    }
}

async fn commit_handler(
    State(state): State<ApiState>,
    Json(request): Json<CommitRequest>,
) -> Result<Json<ApiEnvelope<StoredContinuation>>, ApiError> {
    let summary = format!(
        "speaker={}; statement={}; scope={}; deadline={}; revocable={}",
        request.speaker,
        request.statement,
        request.scope.as_deref().unwrap_or("unspecified"),
        request.deadline.as_deref().unwrap_or("unspecified"),
        request.revocable
    );
    let input = ContinuationInput {
        title: request.statement.clone(),
        summary,
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    };
    let stored = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .create_continuation(&input)
        .map_err(|error| internal_error(error.to_string()))?;
    state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .create_commitment(&request, &stored.id)
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-commit",
        "local-commit",
        stored,
    )))
}

async fn list_commitments_handler(
    State(state): State<ApiState>,
) -> Result<Json<ApiEnvelope<Vec<StoredCommitment>>>, ApiError> {
    let commitments = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .list_active_commitments()
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-commitment-list",
        "local-commitment-list",
        commitments,
    )))
}

async fn assimilate_handler(
    State(state): State<ApiState>,
    Json(input): Json<TemporalDeltaInput>,
) -> Result<Json<ApiEnvelope<StoredTemporalDelta>>, ApiError> {
    let delta = {
        let mut store = state
            .store
            .lock()
            .map_err(|_| internal_error("store lock poisoned"))?;
        store
            .assimilate_delta(&input)
            .map_err(api_error_for_store)?
    };

    Ok(Json(ApiEnvelope::ok(
        "local-assimilate",
        "local-assimilate",
        delta,
    )))
}

async fn list_temporal_deltas_handler(
    State(state): State<ApiState>,
) -> Result<Json<ApiEnvelope<Vec<StoredTemporalDelta>>>, ApiError> {
    let deltas = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .list_temporal_deltas()
        .map_err(|error| internal_error(error.to_string()))?;

    Ok(Json(ApiEnvelope::ok(
        "local-temporal-delta-list",
        "local-temporal-delta-list",
        deltas,
    )))
}

async fn get_temporal_delta_handler(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<Json<ApiEnvelope<StoredTemporalDelta>>, ApiError> {
    let delta = state
        .store
        .lock()
        .map_err(|_| internal_error("store lock poisoned"))?
        .get_temporal_delta(&id)
        .map_err(|error| internal_error(error.to_string()))?
        .ok_or_else(|| not_found_error(format!("temporal delta not found: {id}")))?;

    Ok(Json(ApiEnvelope::ok(
        "local-temporal-delta-get",
        "local-temporal-delta-get",
        delta,
    )))
}

async fn lens_handler(
    State(state): State<ApiState>,
    Json(request): Json<LensRequest>,
) -> Result<Json<ApiEnvelope<LensCard>>, ApiError> {
    let (
        continuations,
        events,
        commitment_constraints,
        relations,
        promoted_raw_event_ids,
        vector_influences,
        advisory_signals,
    ) = {
        let store = state
            .store
            .lock()
            .map_err(|_| internal_error("store lock poisoned"))?;
        let advisory_signals = store
            .search_advisory_forecast_signals(&request.query)
            .map_err(|error| internal_error(error.to_string()))?;
        let continuation_hits = store
            .search_continuations(&request.query)
            .map_err(|error| internal_error(error.to_string()))?;
        let mut continuations = Vec::new();
        for id in continuation_hits {
            if let Some(continuation) = store
                .get_continuation(&id)
                .map_err(|error| internal_error(error.to_string()))?
            {
                continuations.push(continuation);
            }
        }
        let vector_hits = store
            .search_vector_continuations_for_lens(&request.query)
            .map_err(|error| internal_error(error.to_string()))?;
        let vector_influences: Vec<_> = vector_hits
            .iter()
            .map(|hit| TimeFieldVectorInfluence {
                continuation_id: hit.continuation.id.clone(),
                strength: vector_strength_from_distance(hit.distance),
            })
            .collect();
        for hit in vector_hits {
            if !continuations
                .iter()
                .any(|stored| stored.id == hit.continuation.id)
            {
                continuations.push(hit.continuation);
            }
        }
        let continuation_ids: Vec<_> = continuations
            .iter()
            .map(|continuation| continuation.id.clone())
            .collect();
        let mut commitment_constraints = store
            .active_commitments_for_continuations(&continuation_ids)
            .map_err(|error| internal_error(error.to_string()))?;
        for commitment in store
            .search_active_commitments(&request.query)
            .map_err(|error| internal_error(error.to_string()))?
        {
            if !commitment_constraints
                .iter()
                .any(|stored| stored.id == commitment.id)
            {
                if let Some(continuation) = store
                    .get_continuation(&commitment.continuation_id)
                    .map_err(|error| internal_error(error.to_string()))?
                {
                    if !continuations
                        .iter()
                        .any(|stored| stored.id == continuation.id)
                    {
                        continuations.push(continuation);
                    }
                }
                commitment_constraints.push(commitment);
            }
        }
        if !continuations.is_empty() {
            let continuation_ids: Vec<_> = continuations
                .iter()
                .map(|continuation| continuation.id.clone())
                .collect();
            let relations = store
                .active_continuation_relations_for_continuation_ids(&continuation_ids)
                .map_err(|error| internal_error(error.to_string()))?;
            (
                continuations,
                Vec::new(),
                commitment_constraints,
                relations,
                Vec::new(),
                vector_influences,
                advisory_signals,
            )
        } else {
            let hits = store
                .search_raw_events(&request.query)
                .map_err(|error| internal_error(error.to_string()))?;
            let linked_continuations = store
                .active_continuations_for_raw_event_ids(&hits)
                .map_err(|error| internal_error(error.to_string()))?;
            if linked_continuations.is_empty() {
                let mut events = Vec::new();
                for id in hits {
                    if let Some(event) = store
                        .get_raw_event(&id)
                        .map_err(|error| internal_error(error.to_string()))?
                    {
                        events.push(event);
                    }
                }
                (
                    continuations,
                    events,
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    advisory_signals,
                )
            } else {
                let continuation_ids: Vec<_> = linked_continuations
                    .iter()
                    .map(|continuation| continuation.id.clone())
                    .collect();
                let commitment_constraints = store
                    .active_commitments_for_continuations(&continuation_ids)
                    .map_err(|error| internal_error(error.to_string()))?;
                let relations = store
                    .active_continuation_relations_for_continuation_ids(&continuation_ids)
                    .map_err(|error| internal_error(error.to_string()))?;
                (
                    linked_continuations,
                    Vec::new(),
                    commitment_constraints,
                    relations,
                    hits,
                    Vec::new(),
                    advisory_signals,
                )
            }
        }
    };

    let (mut card, rule_facts) = if continuations.is_empty() {
        (lens_card(&request, 0, events.len()), Vec::new())
    } else {
        let time_field_continuations: Vec<_> = continuations
            .iter()
            .map(|continuation| TimeFieldContinuation {
                id: continuation.id.clone(),
                title: continuation.title.clone(),
                summary: if continuation
                    .raw_event_id
                    .as_ref()
                    .is_some_and(|raw_event_id| promoted_raw_event_ids.contains(raw_event_id))
                {
                    // The raw-event search already proved the query matches evidence linked to
                    // this continuation. Seed the in-memory core projection with that query so
                    // TimeFieldLensEngine can score the linked continuation without mutating the
                    // stored continuation or widening protocol/schema.
                    format!("{} {}", continuation.summary, request.query)
                } else {
                    continuation.summary.clone()
                },
                continuation_type: continuation.continuation_type,
                status: continuation.status,
            })
            .collect();
        let rule_facts = lens_rule_facts(&request, &time_field_continuations);
        let card = TimeFieldLensEngine
            .generate_with_relations_and_rule_facts_and_vector_influences(
                &request,
                &time_field_continuations,
                &relations,
                &rule_facts,
                &vector_influences,
                0,
            );
        (card, rule_facts)
    };
    let mut commitment_provenance = Vec::new();
    if !commitment_constraints.is_empty() {
        for commitment in &commitment_constraints {
            if !commitment_provenance
                .iter()
                .any(|provenance: &ProvenanceRef| provenance.id.as_str() == commitment.id.as_str())
            {
                commitment_provenance.push(ProvenanceRef {
                    kind: "commitment".to_string(),
                    id: commitment.id.clone(),
                });
            }
        }
        card.commitment_constraints = commitment_constraints;
        card.avoid
            .push("do not violate explicit commitment constraints".to_string());
    }
    let advisory_forecast_signals: Vec<_> = advisory_signals
        .iter()
        .map(lens_advisory_forecast_signal)
        .collect();
    let advisory_signal_provenance: Vec<_> = advisory_signals
        .iter()
        .map(|signal| ProvenanceRef {
            kind: "advisory_forecast_signal".to_string(),
            id: signal.id.clone(),
        })
        .collect();
    card.advisory_forecast_signals = advisory_forecast_signals;

    let mut envelope = ApiEnvelope::ok("local-lens", "local-lens", card);
    let mut provenance: Vec<ProvenanceRef> = if continuations.is_empty() {
        events
            .into_iter()
            .map(|event| ProvenanceRef {
                kind: "raw_event".to_string(),
                id: event.id,
            })
            .collect()
    } else {
        continuations
            .into_iter()
            .map(|continuation| ProvenanceRef {
                kind: "continuation".to_string(),
                id: continuation.id,
            })
            .collect()
    };
    provenance.extend(commitment_provenance);
    provenance.extend(advisory_signal_provenance);
    provenance.extend(rule_fact_provenance(&rule_facts));
    envelope.provenance = provenance;

    Ok(Json(envelope))
}

fn vector_strength_from_distance(distance: f64) -> f64 {
    if !distance.is_finite() {
        return 0.0;
    }
    1.0 / (1.0 + distance.max(0.0))
}

fn rule_fact_provenance(rule_facts: &[RuleFact]) -> Vec<ProvenanceRef> {
    rule_facts
        .iter()
        .filter(|fact| matches!(fact.predicate.as_str(), "needs_review" | "path_choice"))
        .map(|fact| ProvenanceRef {
            kind: "rule_fact".to_string(),
            id: rule_fact_id(fact),
        })
        .collect()
}

fn rule_fact_id(fact: &RuleFact) -> String {
    format!("{}({})", fact.predicate, fact.args.join(","))
}

fn lens_advisory_forecast_signal(
    signal: &StoredAdvisoryForecastSignal,
) -> LensAdvisoryForecastSignal {
    LensAdvisoryForecastSignal {
        id: signal.id.clone(),
        name: signal.name.clone(),
        confidence: signal.confidence,
        model: signal.model.clone(),
        action_name: signal.action_name.clone(),
        reason: signal.reason.clone(),
        created_at: signal.created_at.clone(),
    }
}

fn lens_card(request: &LensRequest, continuation_count: usize, raw_event_count: usize) -> LensCard {
    if continuation_count > 0 {
        return LensCard {
            stance: "continuation_recall".to_string(),
            why_now: format!(
                "{continuation_count} matching continuation(s) found for query: {}",
                request.query
            ),
            active_continuations: Vec::new(),
            commitment_constraints: Vec::new(),
            advisory_forecast_signals: Vec::new(),
            boundaries: Vec::new(),
            avoid: vec![
                "do not infer closure from recall alone".to_string(),
                "do not expand this into vector search or full Datalog".to_string(),
            ],
            preferred_action: None,
            open_questions: vec!["what is the next concrete observation or action?".to_string()],
            temporal_debt: None,
        };
    }

    if raw_event_count == 0 {
        return LensCard {
            stance: "scaffold".to_string(),
            why_now: format!("no matching raw events found for query: {}", request.query),
            active_continuations: Vec::new(),
            commitment_constraints: Vec::new(),
            advisory_forecast_signals: Vec::new(),
            boundaries: Vec::new(),
            avoid: vec![
                "do not invent continuity without stored evidence".to_string(),
                "do not treat this scaffold as a full continuation graph".to_string(),
            ],
            preferred_action: None,
            open_questions: vec!["what evidence should be observed next?".to_string()],
            temporal_debt: None,
        };
    }

    LensCard {
        stance: "grounded_recall".to_string(),
        why_now: format!(
            "{raw_event_count} matching raw event(s) found for query: {}",
            request.query
        ),
        active_continuations: Vec::new(),
        commitment_constraints: Vec::new(),
        advisory_forecast_signals: Vec::new(),
        boundaries: Vec::new(),
        avoid: vec![
            "do not infer closure from raw recall alone".to_string(),
            "do not treat this as a full continuation graph yet".to_string(),
        ],
        preferred_action: None,
        open_questions: vec![
            "which recalled event should become an active continuation?".to_string()
        ],
        temporal_debt: None,
    }
}

type ApiError = (StatusCode, Json<ApiEnvelope<Value>>);

fn api_error_for_store(error: StoreError) -> ApiError {
    match error {
        StoreError::InvalidTemporalDelta(message) => invalid_request_error(message),
        other => internal_error(other.to_string()),
    }
}

fn api_error_for_relation_create(error: StoreError) -> ApiError {
    match error {
        StoreError::Sqlite(_) => invalid_request_error("invalid continuation relation"),
        other => internal_error(other.to_string()),
    }
}

fn internal_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiEnvelope {
            request_id: "local-error".to_string(),
            trace_id: "local-error".to_string(),
            ok: false,
            data: None,
            warnings: vec![message.into()],
            provenance: Vec::new(),
        }),
    )
}

fn invalid_request_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiEnvelope {
            request_id: "local-error".to_string(),
            trace_id: "local-error".to_string(),
            ok: false,
            data: None,
            warnings: vec![message.into()],
            provenance: Vec::new(),
        }),
    )
}

fn not_found_error(message: impl Into<String>) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ApiEnvelope {
            request_id: "local-error".to_string(),
            trace_id: "local-error".to_string(),
            ok: false,
            data: None,
            warnings: vec![message.into()],
            provenance: Vec::new(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };
    use tempfile::tempdir;
    use tfk_protocol::CandidateAction;
    use tower::ServiceExt;

    #[tokio::test]
    async fn forecast_endpoint_fails_closed_when_commitment_store_lock_is_poisoned() {
        let tmp = tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let store = Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap();
        let shared_store = Arc::new(Mutex::new(store));

        let poison_target = Arc::clone(&shared_store);
        let _ = std::thread::spawn(move || {
            let _guard = poison_target.lock().unwrap();
            panic!("poison commitment store lock");
        })
        .join();
        assert!(shared_store.lock().is_err());

        let app = router_with_state(ApiState {
            store: shared_store,
            preflight_scorer: PreflightScorer::with_threshold(0.5),
            forecast_scorer: ForecastScorer,
            forecast_client: None,
        });
        let request = ForecastRequest {
            actions: vec![CandidateAction {
                name: "ship irreversible release".to_string(),
                continuation_id: Some("commitment-bound-continuation".to_string()),
                progress: 0.9,
                closure: 0.8,
                option_value_preserved: 0.1,
                risk: 0.9,
                irreversibility: 0.9,
                confusion: 0.2,
                friction: 0.2,
                temporal_debt_added: 0.8,
                uncertainty: 0.2,
                externality: 0.5,
            }],
            relations: Vec::new(),
        };

        let response = app
            .oneshot(json_request("POST", "/v1/forecast", &request))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let envelope: ApiEnvelope<Value> = read_json(response).await;
        assert!(!envelope.ok);
        assert!(envelope.data.is_none());
        assert!(envelope
            .warnings
            .iter()
            .any(|warning| warning.contains("store lock poisoned")));
    }

    fn json_request<T: serde::Serialize>(method: &str, uri: &str, body: &T) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(body).unwrap()))
            .unwrap()
    }

    async fn read_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
