use schemars::schema_for;
use tfk_protocol::{
    ApiEnvelope, CandidateAction, CommitRequest, ContinuationInput, ContinuationRelationEdge,
    ContinuationRelationKind, ContinuationStatus, ContinuationType, EventModality, EventSource,
    EvidenceStatus, ForecastRequest, LensCard, RawEventInput, StoredCommitment, StoredContinuation,
    TemporalDeltaInput,
};

#[test]
fn raw_event_input_serializes_with_stable_lowercase_enums() {
    let event = RawEventInput::new_text(
        "session-1",
        "adapter-cli",
        EventSource::User,
        "继续头脑风暴，不要做项目状态机",
    );

    let json = serde_json::to_value(&event).unwrap();

    assert_eq!(json["source"], "user");
    assert_eq!(json["modality"], "text");
    assert_eq!(json["evidence_status"], "observed");
    assert_eq!(event.modality, EventModality::Text);
    assert_eq!(event.evidence_status, EvidenceStatus::Observed);
}

#[test]
fn api_envelope_wraps_success_payload_with_trace_ids() {
    let envelope = ApiEnvelope::ok("req-1", "trace-1", serde_json::json!({"healthy": true}));

    assert!(envelope.ok);
    assert_eq!(envelope.request_id, "req-1");
    assert_eq!(envelope.trace_id, "trace-1");
    assert_eq!(envelope.data.unwrap()["healthy"], true);
    assert!(envelope.warnings.is_empty());
}

#[test]
fn continuation_wire_types_are_stable_json_and_schema_compatible() {
    let input = ContinuationInput {
        title: "项目状态机不是目标".to_string(),
        summary: "保留成可恢复的后续工作，而不是一次性 raw event".to_string(),
        continuation_type: ContinuationType::Obligation,
        status: ContinuationStatus::Active,
        parent_id: Some("cont_parent".to_string()),
        raw_event_id: Some("evt_source".to_string()),
    };

    let json = serde_json::to_value(&input).unwrap();

    assert_eq!(json["status"], "active");
    assert_eq!(json["continuation_type"], "obligation");
    assert_eq!(json["parent_id"], "cont_parent");
    assert_eq!(json["raw_event_id"], "evt_source");

    let stored = StoredContinuation {
        id: "cont_1".to_string(),
        title: input.title,
        summary: input.summary,
        continuation_type: input.continuation_type,
        status: input.status,
        parent_id: input.parent_id,
        raw_event_id: input.raw_event_id,
        created_at: "2026-05-02T00:00:00Z".to_string(),
        updated_at: "2026-05-02T00:00:00Z".to_string(),
    };
    let stored_json = serde_json::to_value(&stored).unwrap();

    assert_eq!(stored_json["id"], "cont_1");
    assert_eq!(stored_json["status"], "active");
    assert_eq!(stored_json["continuation_type"], "obligation");
    let _input_schema = schema_for!(ContinuationInput);
    let _stored_schema = schema_for!(StoredContinuation);
}

#[test]
fn continuation_input_defaults_legacy_missing_type_to_narrative() {
    let input: ContinuationInput = serde_json::from_value(serde_json::json!({
        "title": "项目状态机不是目标",
        "summary": "继续跟踪这个判断",
        "status": "active",
        "parent_id": null,
        "raw_event_id": null
    }))
    .unwrap();

    assert_eq!(input.continuation_type, ContinuationType::Narrative);
}

#[test]
fn lens_card_accepts_legacy_wire_shape_without_time_field_details() {
    let card: LensCard = serde_json::from_value(serde_json::json!({
        "stance": "grounded_recall",
        "why_now": "1 matching raw event",
        "avoid": ["do not infer closure from raw recall alone"],
        "open_questions": ["which recalled event should become an active continuation?"]
    }))
    .unwrap();

    assert_eq!(card.stance, "grounded_recall");
    assert!(card.active_continuations.is_empty());
    assert!(card.commitment_constraints.is_empty());
    assert!(card.boundaries.is_empty());
    assert!(card.preferred_action.is_none());
    assert!(card.temporal_debt.is_none());
}

#[test]
fn commitment_wire_types_roundtrip_and_lens_field_is_optional() {
    let commitment = StoredCommitment {
        id: "commit_1".to_string(),
        continuation_id: "cont_1".to_string(),
        speaker: "agent".to_string(),
        statement: "I will send the draft tomorrow".to_string(),
        scope: Some("current_project".to_string()),
        deadline: Some("2026-05-02".to_string()),
        revocable: true,
        status: ContinuationStatus::Active,
        created_at: "2026-05-02T00:00:00Z".to_string(),
    };
    let json = serde_json::to_value(&commitment).unwrap();

    assert_eq!(json["id"], "commit_1");
    assert_eq!(json["continuation_id"], "cont_1");
    assert_eq!(json["status"], "active");
    assert_eq!(
        serde_json::from_value::<StoredCommitment>(json).unwrap(),
        commitment
    );

    let card: LensCard = serde_json::from_value(serde_json::json!({
        "stance": "act",
        "why_now": "active commitment constrains the next action",
        "active_continuations": [],
        "commitment_constraints": [commitment],
        "avoid": ["do not violate explicit commitments"],
        "open_questions": []
    }))
    .unwrap();

    assert_eq!(card.commitment_constraints.len(), 1);
    assert_eq!(card.commitment_constraints[0].continuation_id, "cont_1");
    let _commitment_schema = schema_for!(StoredCommitment);
    let _lens_schema = schema_for!(LensCard);
}

#[test]
fn relation_forecast_commit_and_delta_wire_types_roundtrip() {
    let relation = ContinuationRelationEdge {
        from_id: "cont_speed".to_string(),
        to_id: "cont_verify".to_string(),
        kind: ContinuationRelationKind::Conflicts,
        reason: Some("speed conflicts with verification".to_string()),
    };
    let relation_json = serde_json::to_value(&relation).unwrap();
    assert_eq!(relation_json["kind"], "conflicts");
    assert_eq!(
        serde_json::from_value::<ContinuationRelationEdge>(relation_json).unwrap(),
        relation
    );

    let forecast = ForecastRequest {
        actions: vec![CandidateAction {
            name: "verify first".to_string(),
            continuation_id: Some("cont_verify".to_string()),
            progress: 0.7,
            closure: 0.5,
            option_value_preserved: 0.8,
            risk: 0.1,
            irreversibility: 0.1,
            confusion: 0.1,
            friction: 0.2,
            temporal_debt_added: 0.0,
            uncertainty: 0.2,
            externality: 0.3,
        }],
        relations: vec![relation],
    };
    let forecast_json = serde_json::to_value(&forecast).unwrap();
    assert_eq!(forecast_json["actions"][0]["option_value_preserved"], 0.8);
    let _: ForecastRequest = serde_json::from_value(forecast_json).unwrap();

    let commit = CommitRequest {
        speaker: "agent".to_string(),
        statement: "I will send the draft tomorrow".to_string(),
        scope: Some("current_project".to_string()),
        deadline: Some("2026-05-02".to_string()),
        revocable: true,
    };
    let commit_json = serde_json::to_value(&commit).unwrap();
    assert_eq!(commit_json["revocable"], true);
    let _: CommitRequest = serde_json::from_value(commit_json).unwrap();

    let delta = TemporalDeltaInput {
        action_id: "a42".to_string(),
        changes: vec![tfk_protocol::ContinuationStatusDelta {
            continuation_id: "cont_verify".to_string(),
            delta: tfk_protocol::ContinuationDelta::Close,
        }],
        claims_made: Vec::new(),
        evidence: Vec::new(),
    };
    let delta_json = serde_json::to_value(&delta).unwrap();
    assert_eq!(delta_json["changes"][0]["delta"], "close");
    let _: TemporalDeltaInput = serde_json::from_value(delta_json).unwrap();
}
