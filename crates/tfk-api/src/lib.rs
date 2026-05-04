use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tfk_core::{
    ForecastScorer, PreflightScorer, PreflightSignals, TimeFieldContinuation, TimeFieldLensEngine,
};
use tfk_protocol::{
    ApiEnvelope, CommitRequest, ContinuationInput, ContinuationStatus, ContinuationType,
    ForecastRequest, ForecastResult, LensCard, LensRequest, ProvenanceRef, RawEventInput,
    StoredContinuation, TemporalDeltaInput,
};
use tfk_store::{Store, StoreError, StoredRawEvent, StoredTemporalDelta};

#[derive(Clone)]
pub struct ApiState {
    store: Arc<Mutex<Store>>,
    preflight_scorer: PreflightScorer,
    forecast_scorer: ForecastScorer,
}

impl ApiState {
    pub fn new(store: Store) -> Self {
        Self {
            store: Arc::new(Mutex::new(store)),
            preflight_scorer: PreflightScorer::with_threshold(0.5),
            forecast_scorer: ForecastScorer,
        }
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
        .route(
            "/v1/continuations",
            post(create_continuation_handler).get(list_continuations_handler),
        )
        .route("/v1/continuations/:id", get(get_continuation_handler))
        .route("/v1/preflight", post(preflight_handler))
        .route("/v1/lens", post(lens_handler))
        .route("/v1/forecast", post(forecast_handler))
        .route("/v1/commit", post(commit_handler))
        .route("/v1/assimilate", post(assimilate_handler))
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

async fn preflight_handler(
    State(state): State<ApiState>,
    Json(signals): Json<PreflightSignals>,
) -> Json<ApiEnvelope<tfk_core::PreflightResult>> {
    Json(ApiEnvelope::ok(
        "local-preflight",
        "local-preflight",
        state.preflight_scorer.score(signals),
    ))
}

async fn forecast_handler(
    State(state): State<ApiState>,
    Json(request): Json<ForecastRequest>,
) -> Json<ApiEnvelope<ForecastResult>> {
    Json(ApiEnvelope::ok(
        "local-forecast",
        "local-forecast",
        state.forecast_scorer.score(&request),
    ))
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
        title: request.statement,
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

    Ok(Json(ApiEnvelope::ok(
        "local-commit",
        "local-commit",
        stored,
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

async fn lens_handler(
    State(state): State<ApiState>,
    Json(request): Json<LensRequest>,
) -> Result<Json<ApiEnvelope<LensCard>>, ApiError> {
    let (continuations, events) = {
        let store = state
            .store
            .lock()
            .map_err(|_| internal_error("store lock poisoned"))?;
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
        if !continuations.is_empty() {
            (continuations, Vec::new())
        } else {
            let hits = store
                .search_raw_events(&request.query)
                .map_err(|error| internal_error(error.to_string()))?;
            let mut events = Vec::new();
            for id in hits {
                if let Some(event) = store
                    .get_raw_event(&id)
                    .map_err(|error| internal_error(error.to_string()))?
                {
                    events.push(event);
                }
            }
            (continuations, events)
        }
    };

    let card = if continuations.is_empty() {
        lens_card(&request, 0, events.len())
    } else {
        let time_field_continuations: Vec<_> = continuations
            .iter()
            .map(|continuation| TimeFieldContinuation {
                id: continuation.id.clone(),
                title: continuation.title.clone(),
                summary: continuation.summary.clone(),
                continuation_type: continuation.continuation_type,
                status: continuation.status,
            })
            .collect();
        TimeFieldLensEngine.generate(&request, &time_field_continuations, 0)
    };
    let mut envelope = ApiEnvelope::ok("local-lens", "local-lens", card);
    envelope.provenance = if continuations.is_empty() {
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

    Ok(Json(envelope))
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
