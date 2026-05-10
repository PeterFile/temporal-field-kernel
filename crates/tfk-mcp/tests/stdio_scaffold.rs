use serde_json::json;
use tfk_mcp::{
    daemon_request_for, degraded_response, dispatch_to_daemon, parse_command_line, StdioCommand,
};

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
fn preflight_command_maps_to_daemon_preflight_request() {
    let command = parse_command_line(
        r#"{"command":"preflight","uncertainty":0.9,"irreversibility":0.8,"externality":0.7,"option_value_loss":0.1}"#,
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/preflight");
    assert_eq!(body["uncertainty"], 0.9);
    assert_eq!(body["irreversibility"], 0.8);
    assert_eq!(body["externality"], 0.7);
    assert_eq!(body["option_value_loss"], 0.1);
}

#[test]
fn preflight_command_defaults_option_value_loss() {
    let command = parse_command_line(
        r#"{"command":"preflight","uncertainty":0.9,"irreversibility":0.8,"externality":0.7}"#,
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(body["option_value_loss"], 0.0);
}

#[test]
fn forecast_command_maps_to_daemon_forecast_request() {
    let request_json = json!({
        "actions": [{
            "name": "verify first",
            "continuation_id": "cont_verify",
            "progress": 0.7,
            "closure": 0.5,
            "option_value_preserved": 0.8,
            "risk": 0.1,
            "irreversibility": 0.1,
            "confusion": 0.1,
            "friction": 0.2,
            "temporal_debt_added": 0.0,
            "uncertainty": 0.2,
            "externality": 0.3
        }],
        "relations": []
    });
    let command = parse_command_line(
        &json!({
            "command": "forecast",
            "request": request_json,
        })
        .to_string(),
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/forecast");
    assert_eq!(body["actions"][0]["name"], "verify first");
    assert_eq!(body["actions"][0]["option_value_preserved"], 0.8);
}

#[test]
fn commit_command_maps_to_daemon_commit_request() {
    let command = parse_command_line(
        r#"{"command":"commit","request":{"speaker":"agent","statement":"I will send the draft","scope":"current_project","deadline":"2026-05-02","revocable":true}}"#,
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/commit");
    assert_eq!(body["speaker"], "agent");
    assert_eq!(body["statement"], "I will send the draft");
    assert_eq!(body["revocable"], true);
}

#[test]
fn assimilate_command_maps_to_daemon_assimilate_request() {
    let command = parse_command_line(
        r#"{"command":"assimilate","request":{"action_id":"a42","changes":[{"continuation_id":"cont_verify","delta":"close"}],"claims_made":[],"evidence":[]}}"#,
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/assimilate");
    assert_eq!(body["action_id"], "a42");
    assert_eq!(body["changes"][0]["continuation_id"], "cont_verify");
    assert_eq!(body["changes"][0]["delta"], "close");
}

#[test]
fn observe_command_maps_to_daemon_observe_request() {
    let command = parse_command_line(
        &json!({
            "command": "observe",
            "request": {
                "session_id": "s1",
                "adapter_id": "cli",
                "source": "user",
                "modality": "text",
                "content": "remember this",
                "evidence_status": "observed"
            }
        })
        .to_string(),
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/observe");
    assert_eq!(body["session_id"], "s1");
    assert_eq!(body["adapter_id"], "cli");
    assert_eq!(body["source"], "user");
    assert_eq!(body["modality"], "text");
    assert_eq!(body["content"], "remember this");
    assert_eq!(body["evidence_status"], "observed");
}

#[test]
fn continuation_create_command_maps_to_daemon_continuations_post() {
    let command = parse_command_line(
        &json!({
            "command": "continuation_create",
            "request": {
                "title": "rollback verifier",
                "summary": "release gate stays closed until evidence is checked",
                "continuation_type": "risk",
                "status": "active",
                "parent_id": null,
                "raw_event_id": null
            }
        })
        .to_string(),
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/continuations");
    assert_eq!(body["title"], "rollback verifier");
    assert_eq!(
        body["summary"],
        "release gate stays closed until evidence is checked"
    );
    assert_eq!(body["continuation_type"], "risk");
    assert_eq!(body["status"], "active");
}

#[test]
fn continuation_list_command_maps_to_daemon_continuations_get() {
    let command = parse_command_line(r#"{"command":"continuation_list"}"#).unwrap();
    let request = daemon_request_for(&command).unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/continuations");
    assert!(request.body.is_empty());
}

#[test]
fn continuation_get_command_maps_to_daemon_continuation_get() {
    let command =
        parse_command_line(r#"{"command":"continuation_get","id":"cont_ABC-123"}"#).unwrap();
    let request = daemon_request_for(&command).unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/continuations/cont_ABC-123");
    assert!(request.body.is_empty());
}

#[test]
fn continuation_get_rejects_unsafe_path_ids() {
    for bad_id in ["", "cont/a", "cont\rheader", "cont\nheader", "cont?query"] {
        let command = parse_command_line(
            &json!({
                "command": "continuation_get",
                "id": bad_id,
            })
            .to_string(),
        )
        .unwrap();
        let error = daemon_request_for(&command).unwrap_err().to_string();

        assert!(
            error.contains("continuation id"),
            "unexpected error for {bad_id:?}: {error}"
        );
    }
}

#[test]
fn relation_create_command_maps_to_daemon_relation_post() {
    let command = parse_command_line(
        &json!({
            "command": "relation_create",
            "request": {
                "from_id": "cont_a",
                "to_id": "cont_b",
                "kind": "depends_on",
                "reason": "needs handoff"
            }
        })
        .to_string(),
    )
    .unwrap();
    let request = daemon_request_for(&command).unwrap();
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/v1/continuation-relations");
    assert_eq!(body["from_id"], "cont_a");
    assert_eq!(body["to_id"], "cont_b");
    assert_eq!(body["kind"], "depends_on");
    assert_eq!(body["reason"], "needs handoff");
}

#[test]
fn relation_list_command_maps_to_daemon_relations_get() {
    let command = parse_command_line(r#"{"command":"relation_list"}"#).unwrap();
    let request = daemon_request_for(&command).unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/continuation-relations");
    assert!(request.body.is_empty());
}

#[test]
fn commitment_list_command_maps_to_daemon_commitments_get() {
    let command = parse_command_line(r#"{"command":"commitment_list"}"#).unwrap();
    let request = daemon_request_for(&command).unwrap();

    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/v1/commitments");
    assert!(request.body.is_empty());
}

#[tokio::test]
async fn malformed_action_loop_request_returns_command_error_without_daemon() {
    let command =
        parse_command_line(r#"{"command":"forecast","request":{"actions":[{"name":"bad"}]}}"#)
            .unwrap();
    let response =
        dispatch_to_daemon(std::path::Path::new("/tmp/tfk-missing.sock"), &command).await;

    assert_eq!(response["ok"], false);
    assert_eq!(response["command"], "forecast");
    assert_eq!(response["degraded"], false);
    assert!(response["error"].as_str().unwrap().contains("request"));
}

#[tokio::test]
async fn malformed_observe_request_returns_command_error_without_daemon() {
    let command = parse_command_line(
        &json!({
            "command": "observe",
            "request": { "session_id": "s1" }
        })
        .to_string(),
    )
    .unwrap();
    let response =
        dispatch_to_daemon(std::path::Path::new("/tmp/tfk-missing.sock"), &command).await;

    assert_eq!(response["ok"], false);
    assert_eq!(response["command"], "observe");
    assert_eq!(response["degraded"], false);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("observe request did not match protocol schema"));
}

#[test]
fn preflight_degraded_response_uses_command_name() {
    let response = degraded_response("preflight", "failed to connect to /tmp/tfk.sock");

    assert_eq!(response["ok"], false);
    assert_eq!(response["command"], "preflight");
    assert_eq!(response["degraded"], true);
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
