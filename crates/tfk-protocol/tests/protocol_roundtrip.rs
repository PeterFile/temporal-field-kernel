use tfk_protocol::{ApiEnvelope, EventModality, EventSource, EvidenceStatus, RawEventInput};

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
