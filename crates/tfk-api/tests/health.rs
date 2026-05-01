use tfk_api::health_envelope;

#[test]
fn health_envelope_reports_ok() {
    let envelope = health_envelope("req-health", "trace-health");

    assert!(envelope.ok);
    assert_eq!(envelope.request_id, "req-health");
    assert_eq!(envelope.trace_id, "trace-health");
    assert_eq!(envelope.data.unwrap()["status"], "ok");
}
