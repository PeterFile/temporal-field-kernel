use std::sync::{Arc, Mutex};

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tempfile::tempdir;
use tfk_model_client::{
    ForecastPredictionClient, ForecastPredictionStatus, ModelClientError, StaticForecastClient,
};
use tfk_protocol::{
    AdvisoryForecastSignal, ApiEnvelope, CandidateAction, CommitRequest, ContinuationDelta,
    ContinuationInput, ContinuationRelationEdge, ContinuationRelationKind, ContinuationStatus,
    ContinuationStatusDelta, ContinuationType, EventSource, ForecastRequest, ForecastResult,
    LensCard, LensRequest, PreflightResult, PreflightSignals, RawEventInput, StoredCommitment,
    StoredContinuation, TemporalDeltaInput,
};
use tfk_store::{Store, StoredAdvisoryForecastSignal, StoredRawEvent, StoredTemporalDelta};
use tfk_vector::{
    VectorDocument, VectorDocumentKind, VectorHit, VectorIndex, VectorIndexOutcome,
    VectorIndexStatus,
};
use tower::ServiceExt;

#[tokio::test]
async fn observe_endpoint_persists_raw_event_and_lens_can_recall_it() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let input = RawEventInput::new_text("s1", "cli", EventSource::User, "不要做项目状态机");
    let observe_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &input))
        .await
        .unwrap();

    assert_eq!(observe_response.status(), StatusCode::OK);
    let observe_envelope: ApiEnvelope<StoredRawEvent> = read_json(observe_response).await;
    assert!(observe_envelope.ok);
    let stored = observe_envelope.data.unwrap();
    assert_eq!(stored.session_id, "s1");
    assert_eq!(stored.adapter_id, "cli");
    assert_eq!(stored.content, "不要做项目状态机");
    assert!(stored.archive_len > 0);

    let lens = LensRequest {
        query: "项目状态机".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    assert_eq!(lens_envelope.provenance[0].kind, "raw_event");
    assert_eq!(lens_envelope.provenance[0].id, stored.id);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "grounded_recall");
    assert!(card.why_now.contains("1 matching raw event"));
}

#[tokio::test]
async fn raw_event_get_returns_observed_event() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let input = RawEventInput::new_text("s1", "cli", EventSource::User, "raw get observed event");
    let observe_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &input))
        .await
        .unwrap();
    assert_eq!(observe_response.status(), StatusCode::OK);
    let observe_envelope: ApiEnvelope<StoredRawEvent> = read_json(observe_response).await;
    assert!(observe_envelope.ok);
    let stored = observe_envelope.data.unwrap();

    let get_response = app
        .oneshot(empty_request(
            "GET",
            &format!("/v1/raw-events/{}", stored.id),
        ))
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);
    let get_envelope: ApiEnvelope<StoredRawEvent> = read_json(get_response).await;
    assert!(get_envelope.ok);
    assert_eq!(get_envelope.data.unwrap(), stored);
}

#[tokio::test]
async fn raw_event_get_missing_returns_404_envelope() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let response = app
        .oneshot(empty_request("GET", "/v1/raw-events/missing-event"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope.data.is_none());
    assert!(envelope.provenance.is_empty());
    assert!(envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("raw event not found")));
}

#[tokio::test]
async fn raw_event_search_returns_matching_events() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let matching_input =
        RawEventInput::new_text("s1", "cli", EventSource::User, "needle raw event evidence");
    let matching_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &matching_input))
        .await
        .unwrap();
    assert_eq!(matching_response.status(), StatusCode::OK);
    let matching_envelope: ApiEnvelope<StoredRawEvent> = read_json(matching_response).await;
    let matching = matching_envelope.data.unwrap();

    let unrelated_input = RawEventInput::new_text(
        "s1",
        "cli",
        EventSource::User,
        "unrelated raw event evidence",
    );
    let unrelated_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &unrelated_input))
        .await
        .unwrap();
    assert_eq!(unrelated_response.status(), StatusCode::OK);

    let search_response = app
        .oneshot(empty_request("GET", "/v1/raw-events?query=needle"))
        .await
        .unwrap();

    assert_eq!(search_response.status(), StatusCode::OK);
    let search_envelope: ApiEnvelope<Vec<StoredRawEvent>> = read_json(search_response).await;
    assert!(search_envelope.ok);
    assert_eq!(search_envelope.data.unwrap(), vec![matching]);
}

#[tokio::test]
async fn raw_event_search_empty_query_returns_empty_vec() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let input = RawEventInput::new_text(
        "s1",
        "cli",
        EventSource::User,
        "empty query must not dump this raw event",
    );
    let observe_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &input))
        .await
        .unwrap();
    assert_eq!(observe_response.status(), StatusCode::OK);

    for uri in ["/v1/raw-events?query=", "/v1/raw-events"] {
        let response = app
            .clone()
            .oneshot(empty_request("GET", uri))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let envelope: ApiEnvelope<Vec<StoredRawEvent>> = read_json(response).await;
        assert!(envelope.ok);
        assert!(envelope.data.unwrap().is_empty());
    }
}

#[tokio::test]
async fn lens_endpoint_uses_semantic_candidate_expansion_for_distributed_tokens() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let target_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuations",
            &ContinuationInput {
                title: "rollback verifier".to_string(),
                summary: "release gate stays closed until evidence is checked".to_string(),
                continuation_type: ContinuationType::Risk,
                status: ContinuationStatus::Active,
                parent_id: None,
                raw_event_id: None,
            },
        ))
        .await
        .unwrap();
    assert_eq!(target_response.status(), StatusCode::OK);
    let target: ApiEnvelope<StoredContinuation> = read_json(target_response).await;
    let target = target.data.unwrap();

    for input in [
        ContinuationInput {
            title: "rollback only".to_string(),
            summary: "unrelated deployment note".to_string(),
            continuation_type: ContinuationType::Narrative,
            status: ContinuationStatus::Active,
            parent_id: None,
            raw_event_id: None,
        },
        ContinuationInput {
            title: "gate only".to_string(),
            summary: "unrelated release note".to_string(),
            continuation_type: ContinuationType::Narrative,
            status: ContinuationStatus::Active,
            parent_id: None,
            raw_event_id: None,
        },
    ] {
        let response = app
            .clone()
            .oneshot(json_request("POST", "/v1/continuations", &input))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let lens = LensRequest {
        query: "rollback gate".to_string(),
        horizon: vec!["next-action".to_string()],
        perspective: vec!["safety".to_string()],
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "verify");
    assert_eq!(card.active_continuations.len(), 1);
    assert_eq!(card.active_continuations[0].id, target.id);
    assert!(lens_envelope
        .provenance
        .iter()
        .any(|provenance| { provenance.kind == "continuation" && provenance.id == target.id }));
    assert_eq!(
        lens_envelope
            .provenance
            .iter()
            .filter(|provenance| provenance.kind == "continuation")
            .count(),
        1
    );
}

#[tokio::test]
async fn lens_endpoint_ranks_exact_phrase_before_semantic_overlap() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let semantic = create_typed_continuation_via_api(
        &app,
        "rollback verifier",
        "release gate stays closed until evidence is checked",
        ContinuationType::Risk,
    )
    .await;
    let exact = create_typed_continuation_via_api(
        &app,
        "rollback gate",
        "exact phrase should win candidate ordering",
        ContinuationType::Risk,
    )
    .await;

    let lens = LensRequest {
        query: "rollback gate".to_string(),
        horizon: vec!["next-action".to_string()],
        perspective: vec!["safety".to_string()],
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    let active_ids: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.id.as_str())
        .collect();
    assert_eq!(active_ids, vec![exact.id.as_str(), semantic.id.as_str()]);
}

#[tokio::test]
async fn lens_endpoint_treats_wildcard_query_as_literal_and_preserves_exact_hit() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let exact = create_typed_continuation_via_api(
        &app,
        "100%_literal",
        "exact wildcard literal should still activate",
        ContinuationType::Risk,
    )
    .await;
    for title in ["100 literal", "100-literal", "100xxliteral"] {
        create_typed_continuation_via_api(
            &app,
            title,
            "wildcard literal query must not activate token-overlap distractors",
            ContinuationType::Narrative,
        )
        .await;
    }

    let lens = LensRequest {
        query: "100%_literal".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    assert_eq!(
        card.active_continuations
            .iter()
            .map(|continuation| continuation.id.as_str())
            .collect::<Vec<_>>(),
        vec![exact.id.as_str()]
    );
    assert!(lens_envelope
        .provenance
        .iter()
        .any(|provenance| provenance.kind == "continuation" && provenance.id == exact.id));
}

#[tokio::test]
async fn lens_endpoint_treats_backslash_query_as_literal_not_semantic_tokens() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let exact = create_typed_continuation_via_api(
        &app,
        r"100\literal",
        "exact backslash literal should still activate",
        ContinuationType::Risk,
    )
    .await;
    for title in ["100 literal", "100-literal", "100xxliteral"] {
        create_typed_continuation_via_api(
            &app,
            title,
            "backslash literal query must not activate token-overlap distractors",
            ContinuationType::Narrative,
        )
        .await;
    }

    let lens = LensRequest {
        query: r"100\literal".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    assert_eq!(
        card.active_continuations
            .iter()
            .map(|continuation| continuation.id.as_str())
            .collect::<Vec<_>>(),
        vec![exact.id.as_str()]
    );
    assert!(lens_envelope
        .provenance
        .iter()
        .any(|provenance| provenance.kind == "continuation" && provenance.id == exact.id));
}

#[tokio::test]
async fn lens_endpoint_noop_vector_index_preserves_default_no_match_behavior() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    create_typed_continuation_via_api(
        &app,
        "opaque codename",
        "no shared words here",
        ContinuationType::Obligation,
    )
    .await;

    let response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "needleless vector query".to_string(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            },
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<LensCard> = read_json(response).await;
    assert!(envelope.ok);
    assert!(envelope.provenance.is_empty());
    let card = envelope.data.unwrap();
    assert_eq!(card.stance, "scaffold");
    assert!(card.active_continuations.is_empty());
}

#[tokio::test]
async fn lens_endpoint_surfaces_injected_vector_continuation_hit_without_text_match() {
    let tmp = tempdir().unwrap();
    let index = Arc::new(InjectedTextVectorIndex::default());
    let store = open_test_store_with_vector(tmp.path(), index.clone());
    let app = tfk_api::router_with_store(store);
    let vector_target = create_typed_continuation_via_api(
        &app,
        "opaque codename",
        "no shared words here",
        ContinuationType::Obligation,
    )
    .await;
    create_typed_continuation_via_api(
        &app,
        "unrelated candidate",
        "also has no query match",
        ContinuationType::Risk,
    )
    .await;
    index.set_hits(vec![VectorHit {
        source_id: vector_target.id.clone(),
        kind: VectorDocumentKind::Continuation,
        distance: 0.0,
    }]);

    let response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "needleless vector query".to_string(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            },
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<LensCard> = read_json(response).await;
    assert!(envelope.ok);
    assert!(envelope.provenance.iter().any(|provenance| {
        provenance.kind == "continuation" && provenance.id == vector_target.id
    }));
    let card = envelope.data.unwrap();
    assert_eq!(card.stance, "act");
    assert_eq!(card.active_continuations.len(), 1);
    assert_eq!(card.active_continuations[0].id, vector_target.id);
}

#[tokio::test]
async fn lens_endpoint_ignores_stale_vector_hits_to_closed_and_retired_continuations() {
    let tmp = tempdir().unwrap();
    let index = Arc::new(InjectedTextVectorIndex::default());
    let store = open_test_store_with_vector(tmp.path(), index.clone());
    let app = tfk_api::router_with_store(store);
    let closed = create_continuation_with_status_via_api(
        &app,
        "closed opaque codename",
        "closed stale hit",
        ContinuationType::Obligation,
        ContinuationStatus::Closed,
    )
    .await;
    let retired = create_continuation_with_status_via_api(
        &app,
        "retired opaque codename",
        "retired stale hit",
        ContinuationType::Risk,
        ContinuationStatus::Retired,
    )
    .await;
    index.set_hits(vec![
        VectorHit {
            source_id: closed.id.clone(),
            kind: VectorDocumentKind::Continuation,
            distance: 0.0,
        },
        VectorHit {
            source_id: retired.id.clone(),
            kind: VectorDocumentKind::Continuation,
            distance: 0.1,
        },
    ]);

    let response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "needleless vector query".to_string(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            },
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<LensCard> = read_json(response).await;
    assert!(envelope.ok);
    assert!(!envelope
        .provenance
        .iter()
        .any(|provenance| provenance.id == closed.id || provenance.id == retired.id));
    let card = envelope.data.unwrap();
    assert_eq!(card.stance, "scaffold");
    assert!(card.active_continuations.is_empty());
}

#[tokio::test]
async fn preflight_endpoint_returns_confirmation_decision() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let signals = PreflightSignals {
        uncertainty: 0.9,
        irreversibility: 0.8,
        externality: 0.7,
        option_value_loss: 0.0,
    };
    let response = app
        .oneshot(json_request("POST", "/v1/preflight", &signals))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<PreflightResult> = read_json(response).await;
    assert!(envelope.ok);
    let result = envelope.data.unwrap();
    assert!(result.requires_confirmation);
    assert_eq!(result.threshold, 0.5);
    assert!(result.risk_product > result.threshold);
}

#[tokio::test]
async fn forecast_endpoint_returns_ranked_candidate_actions() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let forecast = ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "ship now".to_string(),
                continuation_id: None,
                progress: 0.9,
                closure: 0.2,
                option_value_preserved: 0.1,
                risk: 0.8,
                irreversibility: 0.8,
                confusion: 0.4,
                friction: 0.3,
                temporal_debt_added: 0.6,
                uncertainty: 0.8,
                externality: 0.8,
            },
            CandidateAction {
                name: "verify then ship".to_string(),
                continuation_id: None,
                progress: 0.7,
                closure: 0.6,
                option_value_preserved: 0.8,
                risk: 0.1,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.2,
                temporal_debt_added: 0.0,
                uncertainty: 0.2,
                externality: 0.3,
            },
        ],
        relations: Vec::new(),
    };

    let response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    let result = envelope.data.unwrap();
    assert_eq!(result.ranked_actions[0].name, "verify then ship");
    assert!(result.ranked_actions[1].requires_confirmation);
    assert!(result.advisory_signals.is_empty());
    assert!(envelope.warnings.is_empty());
    assert!(envelope.provenance.is_empty());
}

#[tokio::test]
async fn forecast_endpoint_scores_persisted_commitment_constraints() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let commit = CommitRequest {
        speaker: "agent".to_string(),
        statement: "do not ship irreversible release until rollback evidence is verified"
            .to_string(),
        scope: Some("api-test/forecast-commitment".to_string()),
        deadline: Some("2026-05-07".to_string()),
        revocable: false,
    };

    let commit_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/commit", &commit))
        .await
        .unwrap();
    assert_eq!(commit_response.status(), StatusCode::OK);
    let commit_envelope: ApiEnvelope<StoredContinuation> = read_json(commit_response).await;
    let continuation = commit_envelope.data.unwrap();

    let list_response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/commitments"))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredCommitment>> = read_json(list_response).await;
    let commitments = list_envelope.data.unwrap();
    let stored_commitment = commitments
        .iter()
        .find(|commitment| commitment.continuation_id == continuation.id)
        .expect("created commitment should be listed");

    let forecast = ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "ship irreversible release".to_string(),
                continuation_id: Some(continuation.id.clone()),
                progress: 1.0,
                closure: 0.9,
                option_value_preserved: 0.6,
                risk: 0.2,
                irreversibility: 0.6,
                confusion: 0.1,
                friction: 0.1,
                temporal_debt_added: 0.1,
                uncertainty: 0.4,
                externality: 0.4,
            },
            CandidateAction {
                name: "verify rollback evidence".to_string(),
                continuation_id: Some(continuation.id.clone()),
                progress: 0.6,
                closure: 0.2,
                option_value_preserved: 0.8,
                risk: 0.05,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.2,
                temporal_debt_added: 0.0,
                uncertainty: 0.2,
                externality: 0.1,
            },
        ],
        relations: Vec::new(),
    };

    let forecast_response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast))
        .await
        .unwrap();
    assert_eq!(forecast_response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(forecast_response).await;
    assert!(envelope.ok);
    assert!(envelope.warnings.is_empty());
    assert!(envelope.provenance.iter().any(|provenance| {
        provenance.kind == "commitment" && provenance.id == stored_commitment.id
    }));
    let result = envelope.data.unwrap();
    assert_eq!(result.ranked_actions[0].name, "verify rollback evidence");
    let risky_action = result
        .ranked_actions
        .iter()
        .find(|action| action.name == "ship irreversible release")
        .expect("risky action should be ranked");
    assert!(risky_action.requires_confirmation);
    assert!(risky_action.ask_before_act);
    assert!(risky_action
        .reason
        .contains("commitment_constraint_penalty"));
}

#[tokio::test]
async fn advisory_forecast_signal_endpoints_list_get_and_forecast_provenance_roundtrip() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let client = StaticForecastClient::new(vec![AdvisoryForecastSignal {
        name: "forming_future_risk".to_string(),
        model: "static-test".to_string(),
        confidence: 0.8,
        action_name: Some("verify then ship".to_string()),
        reason: Some("unresolved risk".to_string()),
    }]);
    let app =
        tfk_api::router_with_state(tfk_api::ApiState::new(store).with_forecast_client(client));

    let response = app
        .clone()
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    let result = envelope.data.unwrap();
    assert_eq!(result.ranked_actions[0].name, "verify then ship");
    assert_eq!(result.advisory_signals.len(), 1);
    assert_eq!(result.advisory_signals[0].name, "forming_future_risk");
    assert!(envelope.warnings.is_empty());
    assert_eq!(envelope.provenance.len(), 1);
    assert_eq!(envelope.provenance[0].kind, "advisory_forecast_signal");
    assert!(envelope.provenance[0].id.starts_with("advisory_signal_"));

    let list_response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/advisory-forecast-signals"))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredAdvisoryForecastSignal>> =
        read_json(list_response).await;
    assert!(list_envelope.ok);
    let listed = list_envelope.data.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, envelope.provenance[0].id);
    assert_eq!(listed[0].name, "forming_future_risk");
    assert_eq!(listed[0].confidence, 0.8);
    assert_eq!(listed[0].model, "static-test");

    let get_response = app
        .oneshot(empty_request(
            "GET",
            &format!(
                "/v1/advisory-forecast-signals/{}",
                envelope.provenance[0].id
            ),
        ))
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let get_envelope: ApiEnvelope<StoredAdvisoryForecastSignal> = read_json(get_response).await;
    assert!(get_envelope.ok);
    assert_eq!(get_envelope.data.unwrap(), listed[0]);

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    let stored = reopened.list_advisory_forecast_signals().unwrap();
    assert_eq!(stored, listed);
}

#[tokio::test]
async fn advisory_forecast_signal_get_missing_returns_404_envelope() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let response = app
        .oneshot(empty_request(
            "GET",
            "/v1/advisory-forecast-signals/missing-signal",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope.data.is_none());
    assert!(envelope.provenance.is_empty());
    assert!(envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("advisory forecast signal not found")));
}

#[tokio::test]
async fn advisory_forecast_signal_empty_client_leaves_provenance_empty() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let client = StaticForecastClient::new(Vec::new());
    let app =
        tfk_api::router_with_state(tfk_api::ApiState::new(store).with_forecast_client(client));

    let response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    assert!(envelope.data.unwrap().advisory_signals.is_empty());
    assert!(envelope.warnings.is_empty());
    assert!(envelope.provenance.is_empty());

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    assert!(reopened
        .list_advisory_forecast_signals()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn lens_endpoint_projects_matching_persisted_advisory_forecast_signals() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let client = StaticForecastClient::new(vec![
        AdvisoryForecastSignal {
            name: "rollback_evidence_gap".to_string(),
            model: "static-test".to_string(),
            confidence: 0.93,
            action_name: Some("verify rollback evidence".to_string()),
            reason: Some("rollback evidence is missing before irreversible release".to_string()),
        },
        AdvisoryForecastSignal {
            name: "unrelated_calendar_pressure".to_string(),
            model: "static-test".to_string(),
            confidence: 0.42,
            action_name: Some("schedule review".to_string()),
            reason: Some("calendar drift".to_string()),
        },
    ]);
    let app =
        tfk_api::router_with_state(tfk_api::ApiState::new(store).with_forecast_client(client));

    let forecast_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();
    assert_eq!(forecast_response.status(), StatusCode::OK);
    let forecast_envelope: ApiEnvelope<ForecastResult> = read_json(forecast_response).await;
    assert_eq!(forecast_envelope.provenance.len(), 2);

    let list_response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/advisory-forecast-signals"))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredAdvisoryForecastSignal>> =
        read_json(list_response).await;
    let stored_signals = list_envelope.data.unwrap();
    let matching_signal = stored_signals
        .iter()
        .find(|signal| signal.name == "rollback_evidence_gap")
        .unwrap();

    let lens = LensRequest {
        query: "rollback evidence".to_string(),
        horizon: Vec::new(),
        perspective: vec!["risk".to_string()],
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.advisory_forecast_signals.len(), 1);
    assert_eq!(card.advisory_forecast_signals[0].id, matching_signal.id);
    assert_eq!(
        card.advisory_forecast_signals[0].name,
        "rollback_evidence_gap"
    );
    assert_eq!(card.advisory_forecast_signals[0].confidence, 0.93);
    assert_eq!(card.advisory_forecast_signals[0].model, "static-test");
    assert_eq!(
        card.advisory_forecast_signals[0].action_name.as_deref(),
        Some("verify rollback evidence")
    );
    assert_eq!(
        card.advisory_forecast_signals[0].reason.as_deref(),
        Some("rollback evidence is missing before irreversible release")
    );
    assert!(card
        .advisory_forecast_signals
        .iter()
        .all(|signal| signal.name != "unrelated_calendar_pressure"));
    let advisory_provenance: Vec<_> = lens_envelope
        .provenance
        .iter()
        .filter(|provenance| provenance.kind == "advisory_forecast_signal")
        .collect();
    assert_eq!(advisory_provenance.len(), 1);
    assert_eq!(advisory_provenance[0].id, matching_signal.id);
}

#[tokio::test]
async fn forecast_endpoint_keeps_deterministic_result_when_model_fails() {
    struct FailingClient;

    impl ForecastPredictionClient for FailingClient {
        fn forecast(
            &self,
            _request: &ForecastRequest,
        ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
            Err(ModelClientError::PredictionFailed(
                "sidecar down".to_string(),
            ))
        }
    }

    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_state(
        tfk_api::ApiState::new(store).with_forecast_client(FailingClient),
    );

    let response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    let result = envelope.data.unwrap();
    assert_eq!(result.ranked_actions[0].name, "verify then ship");
    assert!(result.advisory_signals.is_empty());
    assert!(envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("forecast advisory model failed")));
}

#[tokio::test]
async fn forecast_endpoint_warns_when_advisory_model_degraded() {
    struct DegradedClient;

    impl ForecastPredictionClient for DegradedClient {
        fn forecast(
            &self,
            _request: &ForecastRequest,
        ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
            Ok(Vec::new())
        }

        fn forecast_with_status(
            &self,
            _request: &ForecastRequest,
        ) -> Result<ForecastPredictionStatus, ModelClientError> {
            Ok(ForecastPredictionStatus {
                advisory_signals: Vec::new(),
                degraded: true,
                reason: Some("model unavailable".to_string()),
            })
        }
    }

    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_state(
        tfk_api::ApiState::new(store).with_forecast_client(DegradedClient),
    );

    let response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    assert!(envelope.ok);
    let result = envelope.data.unwrap();
    assert_eq!(result.ranked_actions[0].name, "verify then ship");
    assert!(result.advisory_signals.is_empty());
    assert!(envelope.warnings.iter().any(|warning| {
        warning.contains("forecast advisory model degraded")
            && warning.contains("model unavailable")
    }));
    assert!(!envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("forecast advisory model failed")));
}

#[tokio::test]
async fn forecast_endpoint_without_client_does_not_store_advisory_signals() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let app = tfk_api::router_with_store(store);

    let response = app
        .oneshot(json_request("POST", "/v1/forecast", &forecast_request()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ForecastResult> = read_json(response).await;
    assert!(envelope.data.unwrap().advisory_signals.is_empty());
    assert!(envelope.provenance.is_empty());

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    assert!(reopened
        .list_advisory_forecast_signals()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn continuation_endpoints_create_list_and_read_from_store() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let input = ContinuationInput {
        title: "项目状态机不是目标".to_string(),
        summary: "把这个判断变成可恢复的 continuation".to_string(),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    };

    let create_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    let create_envelope: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    assert!(create_envelope.ok);
    let created = create_envelope.data.unwrap();
    assert!(created.id.starts_with("cont_"));
    assert_eq!(created.title, input.title);
    assert_eq!(created.continuation_type, ContinuationType::Obligation);
    assert_eq!(created.status, ContinuationStatus::Active);

    let list_response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/continuations"))
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredContinuation>> = read_json(list_response).await;
    assert!(list_envelope.ok);
    assert_eq!(list_envelope.data.unwrap(), vec![created.clone()]);

    let get_response = app
        .oneshot(empty_request(
            "GET",
            &format!("/v1/continuations/{}", created.id),
        ))
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);
    let get_envelope: ApiEnvelope<StoredContinuation> = read_json(get_response).await;
    assert!(get_envelope.ok);
    assert_eq!(get_envelope.data.unwrap(), created);
}

#[tokio::test]
async fn continuation_endpoint_accepts_legacy_body_without_type() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let input = serde_json::json!({
        "title": "项目状态机不是目标",
        "summary": "继续跟踪这个判断",
        "status": "active",
        "parent_id": null,
        "raw_event_id": null
    });

    let create_response = app
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    let create_envelope: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    assert!(create_envelope.ok);
    let created = create_envelope.data.unwrap();
    assert_eq!(created.continuation_type, ContinuationType::Narrative);

    let json = serde_json::to_value(created).unwrap();
    assert_eq!(json["continuation_type"], "narrative");
}

#[tokio::test]
async fn continuation_relations_endpoint_roundtrips() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let left = create_continuation_via_api(&app, "shared query left").await;
    let right = create_continuation_via_api(&app, "shared query right").await;
    let relation = ContinuationRelationEdge {
        from_id: left.id,
        to_id: right.id,
        kind: ContinuationRelationKind::Blocks,
        reason: Some("right waits for left".to_string()),
    };

    let create_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &relation,
        ))
        .await
        .unwrap();

    assert_eq!(create_response.status(), StatusCode::OK);
    let create_envelope: ApiEnvelope<ContinuationRelationEdge> = read_json(create_response).await;
    assert!(create_envelope.ok);
    assert_eq!(create_envelope.data.unwrap(), relation);

    let list_response = app
        .oneshot(empty_request("GET", "/v1/continuation-relations"))
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<ContinuationRelationEdge>> = read_json(list_response).await;
    assert!(list_envelope.ok);
    assert_eq!(list_envelope.data.unwrap(), vec![relation]);
}

#[tokio::test]
async fn continuation_relations_endpoint_is_idempotent_for_same_triple() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let left = create_continuation_via_api(&app, "idempotent relation left").await;
    let right = create_continuation_via_api(&app, "idempotent relation right").await;
    let first_relation = ContinuationRelationEdge {
        from_id: left.id.clone(),
        to_id: right.id.clone(),
        kind: ContinuationRelationKind::DependsOn,
        reason: Some("original reason".to_string()),
    };
    let changed_relation = ContinuationRelationEdge {
        from_id: left.id,
        to_id: right.id,
        kind: ContinuationRelationKind::DependsOn,
        reason: Some("changed reason should not replace existing".to_string()),
    };

    let first_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &first_relation,
        ))
        .await
        .unwrap();

    assert_eq!(first_response.status(), StatusCode::OK);
    let first_envelope: ApiEnvelope<ContinuationRelationEdge> = read_json(first_response).await;
    assert!(first_envelope.ok);
    let first_stored = first_envelope.data.unwrap();
    assert_eq!(first_stored, first_relation);

    let second_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &changed_relation,
        ))
        .await
        .unwrap();

    assert_eq!(second_response.status(), StatusCode::OK);
    let second_envelope: ApiEnvelope<ContinuationRelationEdge> = read_json(second_response).await;
    assert!(second_envelope.ok);
    assert_eq!(second_envelope.data.unwrap(), first_stored);

    let list_response = app
        .oneshot(empty_request("GET", "/v1/continuation-relations"))
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<ContinuationRelationEdge>> = read_json(list_response).await;
    assert!(list_envelope.ok);
    assert_eq!(list_envelope.data.unwrap(), vec![first_stored]);
}

#[tokio::test]
async fn continuation_relations_endpoint_rejects_unknown_endpoint() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let relation = ContinuationRelationEdge {
        from_id: "missing-left".to_string(),
        to_id: "missing-right".to_string(),
        kind: ContinuationRelationKind::Conflicts,
        reason: Some("invalid endpoints".to_string()),
    };

    let response = app
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &relation,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("invalid continuation relation")));
}

#[tokio::test]
async fn lens_recalls_matching_continuation_before_raw_event_fallback() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let input = ContinuationInput {
        title: "项目状态机不是目标".to_string(),
        summary: "summary also participates in continuation recall".to_string(),
        continuation_type: ContinuationType::Narrative,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    };
    let create_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();
    let created: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    let created = created.data.unwrap();
    let raw = RawEventInput::new_text(
        "s1",
        "cli",
        EventSource::User,
        "raw event also says 项目状态机",
    );
    app.clone()
        .oneshot(json_request("POST", "/v1/observe", &raw))
        .await
        .unwrap();

    let lens = LensRequest {
        query: "项目状态机".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    assert_eq!(lens_envelope.provenance[0].kind, "continuation");
    assert_eq!(lens_envelope.provenance[0].id, created.id);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "continuation_recall");
    assert!(card.why_now.contains("1 matching continuation"));
}

#[tokio::test]
async fn lens_uses_persisted_active_continuation_relations() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let left = create_continuation_via_api(&app, "shared relation query left").await;
    let right = create_continuation_via_api(&app, "shared relation query right").await;
    let relation = ContinuationRelationEdge {
        from_id: left.id.clone(),
        to_id: right.id.clone(),
        kind: ContinuationRelationKind::Blocks,
        reason: Some("stored block reason".to_string()),
    };
    app.clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &relation,
        ))
        .await
        .unwrap();
    let lens = LensRequest {
        query: "shared relation query".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };

    let blocked_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(blocked_response.status(), StatusCode::OK);
    let blocked_envelope: ApiEnvelope<LensCard> = read_json(blocked_response).await;
    let blocked_card = blocked_envelope.data.unwrap();
    assert_eq!(blocked_card.stance, "verify");
    assert!(blocked_card.preferred_action.is_none());
    assert!(blocked_card.boundaries.iter().any(|boundary| {
        boundary.kind == "relation_block"
            && boundary.reason.as_deref() == Some("stored block reason")
    }));
    assert!(blocked_card
        .avoid
        .iter()
        .any(|item| item.contains("blocked") || item.contains("collapse related")));

    app.clone()
        .oneshot(json_request(
            "POST",
            "/v1/assimilate",
            &TemporalDeltaInput {
                action_id: "close-relation-endpoint".to_string(),
                changes: vec![ContinuationStatusDelta {
                    continuation_id: left.id,
                    delta: ContinuationDelta::Close,
                }],
                claims_made: Vec::new(),
                evidence: Vec::new(),
            },
        ))
        .await
        .unwrap();

    let closed_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(closed_response.status(), StatusCode::OK);
    let closed_envelope: ApiEnvelope<LensCard> = read_json(closed_response).await;
    let closed_card = closed_envelope.data.unwrap();
    assert!(!closed_card
        .boundaries
        .iter()
        .any(|boundary| boundary.kind == "relation_block" || boundary.kind == "relation_conflict"));
}

#[tokio::test]
async fn lens_applies_rules_derived_review_pressure_from_continuation_markers() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let baseline = create_typed_continuation_via_api(
        &app,
        "rules influence baseline risk",
        "ordinary unresolved risk without explicit rule markers",
        ContinuationType::Risk,
    )
    .await;
    let review = create_typed_continuation_via_api(
        &app,
        "rules influence review target",
        "risk_level=high; time_horizon=near; verify this before action",
        ContinuationType::Epistemic,
    )
    .await;

    let response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "rules influence".to_string(),
                horizon: vec!["next-action".to_string()],
                perspective: vec!["research".to_string()],
            },
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<LensCard> = read_json(response).await;
    assert!(envelope.ok);
    assert!(envelope
        .provenance
        .iter()
        .any(|provenance| { provenance.kind == "continuation" && provenance.id == baseline.id }));
    assert!(envelope
        .provenance
        .iter()
        .any(|provenance| { provenance.kind == "continuation" && provenance.id == review.id }));
    let rule_fact_provenance: Vec<_> = envelope
        .provenance
        .iter()
        .filter(|provenance| provenance.kind == "rule_fact")
        .collect();
    assert!(rule_fact_provenance.iter().any(|provenance| {
        provenance.id.starts_with("path_choice(")
            && provenance.id.contains(&review.id)
            && provenance.id.contains("review_now")
    }));
    assert!(rule_fact_provenance.iter().all(|provenance| {
        (provenance.id.starts_with("needs_review(") || provenance.id.starts_with("path_choice("))
            && provenance.id.contains(&review.id)
            && !provenance.id.contains(&baseline.id)
    }));

    let card = envelope.data.unwrap();
    assert_eq!(card.stance, "verify");
    assert_eq!(card.active_continuations.len(), 2);
    assert_eq!(card.active_continuations[0].id, review.id);
    assert_eq!(card.active_continuations[1].id, baseline.id);
    assert!(card.active_continuations[0].activation > card.active_continuations[1].activation);
    assert_eq!(
        card.active_continuations[0].recommended_delta,
        ContinuationDelta::Verify
    );
    assert!(card.active_continuations[0]
        .risk_if_ignored
        .contains("rules-derived"));
}

#[tokio::test]
async fn lens_orders_active_continuations_by_persisted_relation_kind_ranking() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let baseline = create_obligation_via_api(
        &app,
        "relation ranking baseline action",
        "relation ranking baseline action keeps the unmodified activation floor",
    )
    .await;
    let supported = create_obligation_via_api(
        &app,
        "relation ranking supported action",
        "relation ranking supported action should rise above the baseline",
    )
    .await;
    let prerequisite = create_obligation_via_api(
        &app,
        "relation ranking prerequisite action",
        "relation ranking prerequisite action receives support and dependency pressure",
    )
    .await;
    let dependent = create_obligation_via_api(&app, "relation ranking dependent action", "").await;
    let umbrella = create_obligation_via_api(
        &app,
        "relation ranking umbrella action",
        "relation ranking umbrella action should outrank the supported-only path",
    )
    .await;
    let child = create_obligation_via_api(
        &app,
        "relation ranking child action",
        "relation ranking child action is intentionally lower than the baseline",
    )
    .await;

    create_relation_via_api(
        &app,
        ContinuationRelationEdge {
            from_id: baseline.id.clone(),
            to_id: supported.id.clone(),
            kind: ContinuationRelationKind::Supports,
            reason: Some("support-only relation should lift the target continuation".to_string()),
        },
    )
    .await;
    create_relation_via_api(
        &app,
        ContinuationRelationEdge {
            from_id: supported.id.clone(),
            to_id: prerequisite.id.clone(),
            kind: ContinuationRelationKind::Supports,
            reason: Some("support composes with dependency pressure on the target".to_string()),
        },
    )
    .await;
    create_relation_via_api(
        &app,
        ContinuationRelationEdge {
            from_id: dependent.id.clone(),
            to_id: prerequisite.id.clone(),
            kind: ContinuationRelationKind::DependsOn,
            reason: Some(
                "dependency should prioritize the prerequisite over the dependent".to_string(),
            ),
        },
    )
    .await;
    create_relation_via_api(
        &app,
        ContinuationRelationEdge {
            from_id: umbrella.id.clone(),
            to_id: child.id.clone(),
            kind: ContinuationRelationKind::Subsumes,
            reason: Some("subsuming parent should outrank the child it absorbs".to_string()),
        },
    )
    .await;

    let lens_response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "relation ranking".to_string(),
                horizon: Vec::new(),
                perspective: vec!["action".to_string()],
            },
        ))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "act");
    assert!(card.boundaries.is_empty());
    assert_eq!(card.active_continuations.len(), 6);
    let ordered_titles: Vec<_> = card
        .active_continuations
        .iter()
        .map(|continuation| continuation.title.as_str())
        .collect();
    assert_eq!(
        ordered_titles,
        vec![
            "relation ranking prerequisite action",
            "relation ranking umbrella action",
            "relation ranking supported action",
            "relation ranking baseline action",
            "relation ranking child action",
            "relation ranking dependent action",
        ]
    );
    assert_eq!(card.active_continuations[0].id, prerequisite.id);
    assert!(card.active_continuations[0].activation > card.active_continuations[1].activation);
    assert!(card.active_continuations[1].activation > card.active_continuations[2].activation);
    assert!(card.active_continuations[2].activation > card.active_continuations[3].activation);
    assert!(card.active_continuations[3].activation > card.active_continuations[4].activation);
    assert!(card.active_continuations[4].activation > card.active_continuations[5].activation);
}

#[tokio::test]
async fn lens_projects_active_continuation_as_time_field_action_constraint() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let input = ContinuationInput {
        title: "项目状态机不是目标".to_string(),
        summary: "后续方案不能以 task state / project memory 为主轴".to_string(),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    };
    let create_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();
    let created: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    let created = created.data.unwrap();

    let lens = LensRequest {
        query: "项目状态机".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };
    let lens_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(lens_response.status(), StatusCode::OK);
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    assert!(lens_envelope.ok);
    assert_eq!(lens_envelope.provenance[0].kind, "continuation");
    assert_eq!(lens_envelope.provenance[0].id, created.id);
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.stance, "act");
    assert_eq!(card.active_continuations.len(), 1);
    assert_eq!(card.active_continuations[0].id, created.id);
    assert!(card.active_continuations[0].pressure > 0.8);
    assert!(card.preferred_action.is_some());
}

#[tokio::test]
async fn lens_promotes_raw_event_hit_to_linked_active_continuation() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let raw = RawEventInput::new_text(
        "s1",
        "cli",
        EventSource::User,
        "linked-only-query appears only in raw content",
    );
    let observe_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/observe", &raw))
        .await
        .unwrap();
    let observe_envelope: ApiEnvelope<StoredRawEvent> = read_json(observe_response).await;
    let stored_event = observe_envelope.data.unwrap();
    let input = ContinuationInput {
        title: "continuation title without token".to_string(),
        summary: "summary without the searched token".to_string(),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: Some(stored_event.id.clone()),
    };
    let create_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();
    let created: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    let created = created.data.unwrap();
    let lens = LensRequest {
        query: "linked-only-query".to_string(),
        horizon: Vec::new(),
        perspective: Vec::new(),
    };

    let active_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(active_response.status(), StatusCode::OK);
    let active_envelope: ApiEnvelope<LensCard> = read_json(active_response).await;
    assert_eq!(active_envelope.provenance[0].kind, "continuation");
    assert_eq!(active_envelope.provenance[0].id, created.id);
    let active_card = active_envelope.data.unwrap();
    assert_eq!(active_card.stance, "act");
    assert_eq!(active_card.active_continuations.len(), 1);
    assert_eq!(active_card.active_continuations[0].id, created.id);

    let delta = TemporalDeltaInput {
        action_id: "close-linked-continuation".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: created.id.clone(),
            delta: ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };
    app.clone()
        .oneshot(json_request("POST", "/v1/assimilate", &delta))
        .await
        .unwrap();

    let closed_response = app
        .oneshot(json_request("POST", "/v1/lens", &lens))
        .await
        .unwrap();

    assert_eq!(closed_response.status(), StatusCode::OK);
    let closed_envelope: ApiEnvelope<LensCard> = read_json(closed_response).await;
    assert_eq!(closed_envelope.provenance[0].kind, "raw_event");
    assert_eq!(closed_envelope.provenance[0].id, stored_event.id);
    let closed_card = closed_envelope.data.unwrap();
    assert_eq!(closed_card.stance, "grounded_recall");
    assert!(closed_card.active_continuations.is_empty());
}

#[tokio::test]
async fn commit_endpoint_creates_active_obligation_continuation() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let request = CommitRequest {
        speaker: "agent".to_string(),
        statement: "I will send the draft tomorrow".to_string(),
        scope: Some("current_project".to_string()),
        deadline: Some("2026-05-02".to_string()),
        revocable: true,
    };

    let response = app
        .oneshot(json_request("POST", "/v1/commit", &request))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<StoredContinuation> = read_json(response).await;
    let continuation = envelope.data.unwrap();
    assert_eq!(continuation.continuation_type, ContinuationType::Obligation);
    assert_eq!(continuation.status, ContinuationStatus::Active);
    assert!(continuation.summary.contains("scope=current_project"));
    assert!(continuation.summary.contains("deadline=2026-05-02"));
    assert!(continuation.summary.contains("revocable=true"));
}

#[tokio::test]
async fn commit_endpoint_persists_retrievable_structured_active_commitment() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let request = CommitRequest {
        speaker: "agent".to_string(),
        statement: "I will send the draft tomorrow".to_string(),
        scope: Some("current_project".to_string()),
        deadline: Some("2026-05-02".to_string()),
        revocable: true,
    };

    let commit_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/commit", &request))
        .await
        .unwrap();
    let commit_envelope: ApiEnvelope<StoredContinuation> = read_json(commit_response).await;
    let continuation = commit_envelope.data.unwrap();

    let list_response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/commitments"))
        .await
        .unwrap();

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredCommitment>> = read_json(list_response).await;
    let commitments = list_envelope.data.unwrap();
    assert_eq!(commitments.len(), 1);
    let commitment = &commitments[0];
    assert!(commitment.id.starts_with("commit_"));
    assert_eq!(commitment.continuation_id, continuation.id);
    assert_eq!(commitment.speaker, request.speaker);
    assert_eq!(commitment.statement, request.statement);
    assert_eq!(commitment.scope, request.scope);
    assert_eq!(commitment.deadline, request.deadline);
    assert_eq!(commitment.revocable, request.revocable);
    assert_eq!(commitment.status, ContinuationStatus::Active);

    let lens_response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "draft".to_string(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            },
        ))
        .await
        .unwrap();
    let lens_envelope: ApiEnvelope<LensCard> = read_json(lens_response).await;
    let expected_commitment_provenance = tfk_protocol::ProvenanceRef {
        kind: "commitment".to_string(),
        id: commitment.id.clone(),
    };
    assert!(lens_envelope
        .provenance
        .contains(&expected_commitment_provenance));
    assert_eq!(
        lens_envelope
            .provenance
            .iter()
            .filter(|provenance| {
                provenance.kind == "commitment" && provenance.id.as_str() == commitment.id.as_str()
            })
            .count(),
        1
    );
    let card = lens_envelope.data.unwrap();
    assert_eq!(card.commitment_constraints, commitments);
    assert!(card
        .avoid
        .iter()
        .any(|item| item.contains("explicit commitment")));

    let unrelated_response = app
        .oneshot(json_request(
            "POST",
            "/v1/lens",
            &LensRequest {
                query: "unrelated".to_string(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            },
        ))
        .await
        .unwrap();
    let unrelated_envelope: ApiEnvelope<LensCard> = read_json(unrelated_response).await;
    assert!(!unrelated_envelope
        .provenance
        .contains(&expected_commitment_provenance));
    assert!(!unrelated_envelope
        .provenance
        .iter()
        .any(|provenance| provenance.kind == "commitment"));
    assert!(unrelated_envelope
        .data
        .unwrap()
        .commitment_constraints
        .is_empty());
}

#[tokio::test]
async fn assimilate_endpoint_applies_commitment_lifecycle_for_close_and_revocable_retire() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let app = tfk_api::router_with_store(store);

    let close_continuation = create_commitment_via_api(
        &app,
        CommitRequest {
            speaker: "agent".to_string(),
            statement: "I will keep release gate closed until audit passes".to_string(),
            scope: Some("release_gate".to_string()),
            deadline: None,
            revocable: false,
        },
    )
    .await;
    let close_commitment = get_commitment_for_continuation(&app, &close_continuation.id).await;

    let retire_continuation = create_commitment_via_api(
        &app,
        CommitRequest {
            speaker: "agent".to_string(),
            statement: "I will run the revocable staging check".to_string(),
            scope: Some("staging_check".to_string()),
            deadline: None,
            revocable: true,
        },
    )
    .await;
    let retire_commitment = get_commitment_for_continuation(&app, &retire_continuation.id).await;

    let close_delta = TemporalDeltaInput {
        action_id: "close-commitment-continuation".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: close_continuation.id.clone(),
            delta: ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };
    let close_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/assimilate", &close_delta))
        .await
        .unwrap();
    assert_eq!(close_response.status(), StatusCode::OK);

    let get_closed_response = app
        .clone()
        .oneshot(empty_request(
            "GET",
            &format!("/v1/continuations/{}", close_continuation.id),
        ))
        .await
        .unwrap();
    let get_closed_envelope: ApiEnvelope<StoredContinuation> = read_json(get_closed_response).await;
    assert_eq!(
        get_closed_envelope.data.unwrap().status,
        ContinuationStatus::Closed
    );
    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    assert_eq!(
        reopened
            .get_commitment(&close_commitment.id)
            .unwrap()
            .unwrap()
            .status,
        ContinuationStatus::Closed
    );

    let retire_delta = TemporalDeltaInput {
        action_id: "retire-revocable-commitment-continuation".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: retire_continuation.id.clone(),
            delta: ContinuationDelta::Retire,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };
    let retire_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/assimilate", &retire_delta))
        .await
        .unwrap();
    assert_eq!(retire_response.status(), StatusCode::OK);

    let get_retired_response = app
        .clone()
        .oneshot(empty_request(
            "GET",
            &format!("/v1/continuations/{}", retire_continuation.id),
        ))
        .await
        .unwrap();
    let get_retired_envelope: ApiEnvelope<StoredContinuation> =
        read_json(get_retired_response).await;
    assert_eq!(
        get_retired_envelope.data.unwrap().status,
        ContinuationStatus::Retired
    );
    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    assert_eq!(
        reopened
            .get_commitment(&retire_commitment.id)
            .unwrap()
            .unwrap()
            .status,
        ContinuationStatus::Retired
    );

    let list_response = app
        .oneshot(empty_request("GET", "/v1/commitments"))
        .await
        .unwrap();
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_envelope: ApiEnvelope<Vec<StoredCommitment>> = read_json(list_response).await;
    let commitments = list_envelope.data.unwrap();
    assert!(!commitments
        .iter()
        .any(|commitment| commitment.id == close_commitment.id));
    assert!(!commitments
        .iter()
        .any(|commitment| commitment.id == retire_commitment.id));
}

#[tokio::test]
async fn assimilate_endpoint_rejects_non_revocable_retire_and_rolls_back() {
    let tmp = tempdir().unwrap();
    let data_dir = tmp.path().join("data");
    let db_path = data_dir.join("tfk.db");
    let archive_dir = data_dir.join("archive");
    let store = Store::open(&db_path, &archive_dir).unwrap();
    let app = tfk_api::router_with_store(store);

    let continuation = create_commitment_via_api(
        &app,
        CommitRequest {
            speaker: "agent".to_string(),
            statement: "I will keep the audit gate non-revocable".to_string(),
            scope: Some("audit_gate".to_string()),
            deadline: None,
            revocable: false,
        },
    )
    .await;
    let commitment = get_commitment_for_continuation(&app, &continuation.id).await;
    let delta = TemporalDeltaInput {
        action_id: "reject-non-revocable-retire".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: continuation.id.clone(),
            delta: ContinuationDelta::Retire,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let response = app
        .clone()
        .oneshot(json_request("POST", "/v1/assimilate", &delta))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope.warnings.iter().any(|warning| {
        warning.contains("non-revocable")
            && warning.contains("retire")
            && warning.contains("commitment")
    }));

    let get_response = app
        .clone()
        .oneshot(empty_request(
            "GET",
            &format!("/v1/continuations/{}", continuation.id),
        ))
        .await
        .unwrap();
    let get_envelope: ApiEnvelope<StoredContinuation> = read_json(get_response).await;
    assert_eq!(
        get_envelope.data.unwrap().status,
        ContinuationStatus::Active
    );

    let commitments_response = app
        .oneshot(empty_request("GET", "/v1/commitments"))
        .await
        .unwrap();
    let commitments_envelope: ApiEnvelope<Vec<StoredCommitment>> =
        read_json(commitments_response).await;
    assert!(commitments_envelope
        .data
        .unwrap()
        .iter()
        .any(|listed| listed.id == commitment.id));

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    assert_eq!(
        reopened
            .get_commitment(&commitment.id)
            .unwrap()
            .unwrap()
            .status,
        ContinuationStatus::Active
    );
    assert!(reopened.list_temporal_deltas().unwrap().is_empty());
}

#[tokio::test]
async fn assimilate_endpoint_persists_delta_and_updates_continuation_status() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let input = ContinuationInput {
        title: "verify release".to_string(),
        summary: "risk must be checked".to_string(),
        continuation_type: ContinuationType::Risk,
        status: ContinuationStatus::Active,
        parent_id: None,
        raw_event_id: None,
    };
    let create_response = app
        .clone()
        .oneshot(json_request("POST", "/v1/continuations", &input))
        .await
        .unwrap();
    let created: ApiEnvelope<StoredContinuation> = read_json(create_response).await;
    let created = created.data.unwrap();
    let delta = TemporalDeltaInput {
        action_id: "a42".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: created.id.clone(),
            delta: ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let response = app
        .clone()
        .oneshot(json_request("POST", "/v1/assimilate", &delta))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<tfk_store::StoredTemporalDelta> = read_json(response).await;
    assert_eq!(envelope.data.unwrap().action_id, "a42");

    let get_response = app
        .oneshot(empty_request(
            "GET",
            &format!("/v1/continuations/{}", created.id),
        ))
        .await
        .unwrap();
    let get_envelope: ApiEnvelope<StoredContinuation> = read_json(get_response).await;
    assert_eq!(
        get_envelope.data.unwrap().status,
        ContinuationStatus::Closed
    );
}

#[tokio::test]
async fn temporal_delta_list_endpoint_returns_persisted_delta() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let stored_delta = create_temporal_delta_via_api(&app, "list temporal delta").await;

    let response = app
        .oneshot(empty_request("GET", "/v1/temporal-deltas"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<Vec<StoredTemporalDelta>> = read_json(response).await;
    assert!(envelope.ok);
    assert_eq!(envelope.data.unwrap(), vec![stored_delta]);
}

#[tokio::test]
async fn temporal_delta_get_endpoint_returns_persisted_delta() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let stored_delta = create_temporal_delta_via_api(&app, "get temporal delta").await;

    let response = app
        .oneshot(empty_request(
            "GET",
            &format!("/v1/temporal-deltas/{}", stored_delta.id),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<StoredTemporalDelta> = read_json(response).await;
    assert!(envelope.ok);
    assert_eq!(envelope.data.unwrap(), stored_delta);
}

#[tokio::test]
async fn temporal_delta_get_missing_returns_404_envelope() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);

    let response = app
        .oneshot(empty_request("GET", "/v1/temporal-deltas/missing-delta"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope.data.is_none());
    assert!(envelope.provenance.is_empty());
    assert!(envelope.warnings.iter().any(|warning| {
        warning.contains("temporal delta not found") && warning.contains("missing-delta")
    }));
}

#[tokio::test]
async fn assimilate_endpoint_rejects_missing_status_target() {
    let tmp = tempdir().unwrap();
    let store = open_test_store(tmp.path());
    let app = tfk_api::router_with_store(store);
    let delta = TemporalDeltaInput {
        action_id: "a-missing".to_string(),
        changes: vec![ContinuationStatusDelta {
            continuation_id: "missing-continuation".to_string(),
            delta: ContinuationDelta::Advance,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let response = app
        .oneshot(json_request("POST", "/v1/assimilate", &delta))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let envelope: ApiEnvelope<serde_json::Value> = read_json(response).await;
    assert!(!envelope.ok);
    assert!(envelope
        .warnings
        .iter()
        .any(|warning| warning.contains("missing-continuation")));
}

fn open_test_store(root: &std::path::Path) -> Store {
    let data_dir = root.join("data");
    Store::open(data_dir.join("tfk.db"), data_dir.join("archive")).unwrap()
}

fn open_test_store_with_vector(root: &std::path::Path, index: Arc<dyn VectorIndex>) -> Store {
    let data_dir = root.join("data");
    Store::open_with_vector_index(data_dir.join("tfk.db"), data_dir.join("archive"), index).unwrap()
}

async fn create_commitment_via_api(
    app: &axum::Router,
    request: CommitRequest,
) -> StoredContinuation {
    let response = app
        .clone()
        .oneshot(json_request("POST", "/v1/commit", &request))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<StoredContinuation> = read_json(response).await;
    assert!(envelope.ok);
    envelope.data.unwrap()
}

async fn get_commitment_for_continuation(
    app: &axum::Router,
    continuation_id: &str,
) -> StoredCommitment {
    let response = app
        .clone()
        .oneshot(empty_request("GET", "/v1/commitments"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<Vec<StoredCommitment>> = read_json(response).await;
    envelope
        .data
        .unwrap()
        .into_iter()
        .find(|commitment| commitment.continuation_id == continuation_id)
        .expect("created commitment should be listed")
}

async fn create_temporal_delta_via_api(app: &axum::Router, title: &str) -> StoredTemporalDelta {
    let continuation = create_continuation_via_api(app, title).await;
    let delta = TemporalDeltaInput {
        action_id: format!("{title} action"),
        changes: vec![ContinuationStatusDelta {
            continuation_id: continuation.id,
            delta: ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };

    let response = app
        .clone()
        .oneshot(json_request("POST", "/v1/assimilate", &delta))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<StoredTemporalDelta> = read_json(response).await;
    assert!(envelope.ok);
    envelope.data.unwrap()
}

async fn create_continuation_via_api(app: &axum::Router, title: &str) -> StoredContinuation {
    create_obligation_via_api(app, title, "relation test summary").await
}

async fn create_typed_continuation_via_api(
    app: &axum::Router,
    title: &str,
    summary: &str,
    continuation_type: ContinuationType,
) -> StoredContinuation {
    create_continuation_with_status_via_api(
        app,
        title,
        summary,
        continuation_type,
        ContinuationStatus::Active,
    )
    .await
}

async fn create_continuation_with_status_via_api(
    app: &axum::Router,
    title: &str,
    summary: &str,
    continuation_type: ContinuationType,
    status: ContinuationStatus,
) -> StoredContinuation {
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuations",
            &ContinuationInput {
                title: title.to_string(),
                summary: summary.to_string(),
                continuation_type,
                status,
                parent_id: None,
                raw_event_id: None,
            },
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<StoredContinuation> = read_json(response).await;
    assert!(envelope.ok);
    envelope.data.unwrap()
}

async fn create_obligation_via_api(
    app: &axum::Router,
    title: &str,
    summary: &str,
) -> StoredContinuation {
    create_typed_continuation_via_api(app, title, summary, ContinuationType::Obligation).await
}

async fn create_relation_via_api(
    app: &axum::Router,
    relation: ContinuationRelationEdge,
) -> ContinuationRelationEdge {
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuation-relations",
            &relation,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let envelope: ApiEnvelope<ContinuationRelationEdge> = read_json(response).await;
    assert!(envelope.ok);
    envelope.data.unwrap()
}

#[derive(Debug, Default)]
struct InjectedTextVectorIndex {
    hits: Mutex<Vec<VectorHit>>,
}

impl InjectedTextVectorIndex {
    fn set_hits(&self, hits: Vec<VectorHit>) {
        *self.hits.lock().unwrap() = hits;
    }
}

impl VectorIndex for InjectedTextVectorIndex {
    fn status(&self) -> VectorIndexStatus {
        VectorIndexStatus::available("injected-text")
    }

    fn upsert(&self, _document: &VectorDocument) -> tfk_vector::Result<VectorIndexOutcome> {
        Ok(VectorIndexOutcome::Indexed)
    }

    fn search(
        &self,
        _query_embedding: &[f32],
        _limit: usize,
    ) -> tfk_vector::Result<Vec<VectorHit>> {
        Ok(Vec::new())
    }

    fn search_text(&self, _query: &str, limit: usize) -> tfk_vector::Result<Vec<VectorHit>> {
        Ok(self
            .hits
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }
}

fn forecast_request() -> ForecastRequest {
    ForecastRequest {
        actions: vec![
            CandidateAction {
                name: "ship now".to_string(),
                continuation_id: None,
                progress: 0.9,
                closure: 0.2,
                option_value_preserved: 0.1,
                risk: 0.8,
                irreversibility: 0.8,
                confusion: 0.4,
                friction: 0.3,
                temporal_debt_added: 0.6,
                uncertainty: 0.8,
                externality: 0.8,
            },
            CandidateAction {
                name: "verify then ship".to_string(),
                continuation_id: None,
                progress: 0.7,
                closure: 0.6,
                option_value_preserved: 0.8,
                risk: 0.1,
                irreversibility: 0.1,
                confusion: 0.1,
                friction: 0.2,
                temporal_debt_added: 0.0,
                uncertainty: 0.2,
                externality: 0.3,
            },
        ],
        relations: Vec::new(),
    }
}

fn json_request<T: serde::Serialize>(method: &str, uri: &str, body: &T) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

fn empty_request(method: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn read_json<T: serde::de::DeserializeOwned>(response: axum::response::Response) -> T {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
