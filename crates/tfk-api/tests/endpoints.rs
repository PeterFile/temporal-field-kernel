use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use tempfile::tempdir;
use tfk_model_client::{ForecastPredictionClient, ModelClientError, StaticForecastClient};
use tfk_protocol::{
    AdvisoryForecastSignal, ApiEnvelope, CandidateAction, CommitRequest, ContinuationDelta,
    ContinuationInput, ContinuationRelationEdge, ContinuationRelationKind, ContinuationStatus,
    ContinuationStatusDelta, ContinuationType, EventSource, ForecastRequest, ForecastResult,
    LensCard, LensRequest, PreflightResult, PreflightSignals, RawEventInput, StoredCommitment,
    StoredContinuation, TemporalDeltaInput,
};
use tfk_store::{Store, StoredRawEvent};
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
}

#[tokio::test]
async fn forecast_endpoint_appends_injected_advisory_signals() {
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

    let reopened = Store::open(&db_path, &archive_dir).unwrap();
    let stored = reopened.list_advisory_forecast_signals().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].name, "forming_future_risk");
    assert_eq!(stored[0].confidence, 0.8);
    assert_eq!(stored[0].model, "static-test");
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
    assert!(unrelated_envelope
        .data
        .unwrap()
        .commitment_constraints
        .is_empty());
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

async fn create_continuation_via_api(app: &axum::Router, title: &str) -> StoredContinuation {
    let response = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/v1/continuations",
            &ContinuationInput {
                title: title.to_string(),
                summary: "relation test summary".to_string(),
                continuation_type: ContinuationType::Obligation,
                status: ContinuationStatus::Active,
                parent_id: None,
                raw_event_id: None,
            },
        ))
        .await
        .unwrap();
    let envelope: ApiEnvelope<StoredContinuation> = read_json(response).await;
    envelope.data.unwrap()
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
