use serde_json::json;
use tfk_mcp::{daemon_request_for, degraded_response, parse_command_line, StdioCommand};

#[test]
fn parses_health_json_line_command() {
    let command = parse_command_line(r#"{"command":"health"}"#).unwrap();

    assert_eq!(command, StdioCommand::Health);
}

#[test]
fn lens_command_maps_to_daemon_lens_request() {
    let command = parse_command_line(r#"{"command":"lens","query":"时间场"}"#).unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/lens");
    assert_eq!(body["query"], "时间场");
    assert_eq!(body["horizon"], json!([]));
    assert_eq!(body["perspective"], json!([]));
}

#[test]
fn health_command_maps_to_daemon_health_request() {
    let request = daemon_request_for(&StdioCommand::Health).unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/healthz");
    assert!(request.body.is_empty());
}

#[test]
fn degraded_response_is_clear_when_daemon_is_unavailable() {
    let response = degraded_response("health", "failed to connect to /tmp/tfk.sock");

    assert_eq!(response["ok"], false);
    assert_eq!(response["command"], "health");
    assert_eq!(response["degraded"], true);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("daemon unavailable"));
}
