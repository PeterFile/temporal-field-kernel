use schemars::schema_for;
use tfk_protocol::{
    ApiEnvelope, ContinuationInput, ContinuationStatus, EventModality, EventSource, EvidenceStatus,
    RawEventInput, StoredContinuation,
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
        status: ContinuationStatus::Active,
        parent_id: Some("cont_parent".to_string()),
        raw_event_id: Some("evt_source".to_string()),
    };

    let json = serde_json::to_value(&input).unwrap();

    assert_eq!(json["status"], "active");
    assert_eq!(json["parent_id"], "cont_parent");
    assert_eq!(json["raw_event_id"], "evt_source");

    let stored = StoredContinuation {
        id: "cont_1".to_string(),
        title: input.title,
        summary: input.summary,
        status: input.status,
        parent_id: input.parent_id,
        raw_event_id: input.raw_event_id,
        created_at: "2026-05-02T00:00:00Z".to_string(),
        updated_at: "2026-05-02T00:00:00Z".to_string(),
    };
    let stored_json = serde_json::to_value(&stored).unwrap();

    assert_eq!(stored_json["id"], "cont_1");
    assert_eq!(stored_json["status"], "active");
    let _input_schema = schema_for!(ContinuationInput);
    let _stored_schema = schema_for!(StoredContinuation);
}
