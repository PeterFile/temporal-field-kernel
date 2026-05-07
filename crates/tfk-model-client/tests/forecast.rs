use tfk_model_client::{
    DisabledPredictionClient, ForecastPredictionClient, ModelClientError, StaticForecastClient,
    StdioForecastClient,
};
use tfk_protocol::{AdvisoryForecastSignal, CandidateAction, ForecastRequest};

#[test]
fn disabled_forecast_client_is_safe_noop() {
    let client = DisabledPredictionClient;

    let signals = client.forecast(&empty_request()).unwrap();

    assert!(signals.is_empty());
}

#[test]
fn static_forecast_client_returns_configured_signals() {
    let client = StaticForecastClient::new(vec![AdvisoryForecastSignal {
        name: "forming_future_risk".to_string(),
        model: "static-test".to_string(),
        confidence: 0.8,
        action_name: Some("verify first".to_string()),
        reason: None,
    }]);

    let signals = client.forecast(&empty_request()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "forming_future_risk");
}

#[test]
fn static_forecast_client_loads_bare_advisory_signal_array() {
    let path = write_json(
        r#"[
          {
            "name": "forming_future_risk",
            "model": "static-test",
            "confidence": 0.8,
            "action_name": "verify first"
          }
        ]"#,
    );

    let client = StaticForecastClient::from_json_file(&path).unwrap();
    let signals = client.forecast(&empty_request()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "forming_future_risk");
}

#[test]
fn static_forecast_client_loads_wrapped_advisory_signals() {
    let path = write_json(
        r#"{
          "advisory_signals": [
            {
              "name": "forming_future_risk",
              "model": "static-test",
              "confidence": 0.8
            }
          ]
        }"#,
    );

    let client = StaticForecastClient::from_json_file(&path).unwrap();
    let signals = client.forecast(&empty_request()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].model, "static-test");
}

#[test]
fn static_forecast_client_loads_temporalbench_forecast_fixture_shape() {
    let client = StaticForecastClient::from_json_file(workspace_fixture(
        "fixtures/temporalbench/forecast_advisory/basic_forecast.json",
    ))
    .unwrap();
    let signals = client.forecast(&empty_request()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "forming_future_risk");
}

#[test]
fn static_forecast_client_rejects_missing_advisory_signals() {
    let path = write_json(r#"{"request":{"actions":[]}}"#);

    let error = StaticForecastClient::from_json_file(&path).unwrap_err();

    assert!(matches!(error, ModelClientError::PredictionFailed(_)));
    assert!(error.to_string().contains("missing advisory_signals"));
}

#[test]
fn static_forecast_client_rejects_malformed_json() {
    let path = write_json(r#"{"advisory_signals":["#);

    let error = StaticForecastClient::from_json_file(&path).unwrap_err();

    assert!(matches!(error, ModelClientError::PredictionFailed(_)));
    assert!(error.to_string().contains("failed to parse forecast JSON"));
}

#[test]
fn failing_forecast_client_error_is_explicit() {
    struct FailingClient;

    impl ForecastPredictionClient for FailingClient {
        fn forecast(
            &self,
            _request: &ForecastRequest,
        ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
            Err(ModelClientError::PredictionFailed("boom".to_string()))
        }
    }

    let error = FailingClient.forecast(&empty_request()).unwrap_err();

    assert!(error.to_string().contains("boom"));
}

#[test]
fn stdio_forecast_client_parses_advisory_signal_response() {
    let script = write_python_script(
        r#"
import json
import sys

json.loads(sys.stdin.readline())
print(json.dumps({
    "degraded": False,
    "reason": "extra response fields are tolerated",
    "predictions": [{"name": "ignored"}],
    "advisory_signals": [{
        "name": "forming_future_risk",
        "model": "stdio-test",
        "confidence": 0.91,
        "action_name": "ship slice2",
        "reason": "sidecar detected future risk"
    }]
}))
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let signals = client.forecast(&request_with_actions()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "forming_future_risk");
    assert_eq!(signals[0].model, "stdio-test");
    assert_eq!(signals[0].confidence, 0.91);
    assert_eq!(signals[0].action_name.as_deref(), Some("ship slice2"));
    assert_eq!(
        signals[0].reason.as_deref(),
        Some("sidecar detected future risk")
    );
}

#[test]
fn stdio_forecast_client_sends_wrapped_request_with_actions() {
    let script = write_python_script(
        r#"
import json
import sys

payload = json.loads(sys.stdin.readline())
assert payload["request_id"] == "local-forecast", payload
request = payload["request"]
assert request["actions"][0]["name"] == "ship slice2", request
assert request["actions"][0]["continuation_id"] == "cont-1", request
assert request["actions"][0]["progress"] == 0.25, request
assert request.get("relations", []) == [], request
print(json.dumps({
    "advisory_signals": [{
        "name": "wrapper_seen",
        "model": "stdio-test",
        "confidence": 0.75,
        "action_name": request["actions"][0]["name"]
    }]
}))
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let signals = client.forecast(&request_with_actions()).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].name, "wrapper_seen");
    assert_eq!(signals[0].action_name.as_deref(), Some("ship slice2"));
}

#[test]
fn stdio_forecast_client_rejects_missing_advisory_signals() {
    let script = write_python_script(
        r#"
import json
import sys

json.loads(sys.stdin.readline())
print(json.dumps({
    "degraded": True,
    "reason": "model unavailable",
    "predictions": []
}))
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let error = client.forecast(&empty_request()).unwrap_err();

    assert!(matches!(error, ModelClientError::PredictionFailed(_)));
    assert!(error.to_string().contains("missing advisory_signals"));
}

#[test]
fn stdio_forecast_client_parses_degraded_status_without_failing() {
    let script = write_python_script(
        r#"
import json
import sys

json.loads(sys.stdin.readline())
print(json.dumps({
    "degraded": True,
    "reason": "model unavailable",
    "advisory_signals": []
}))
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let status = client.forecast_with_status(&empty_request()).unwrap();

    assert!(status.advisory_signals.is_empty());
    assert!(status.degraded);
    assert_eq!(status.reason.as_deref(), Some("model unavailable"));
}

#[test]
fn stdio_forecast_client_defaults_missing_degraded_to_false() {
    let script = write_python_script(
        r#"
import json
import sys

json.loads(sys.stdin.readline())
print(json.dumps({
    "advisory_signals": [],
    "extra_field": "ignored"
}))
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let status = client.forecast_with_status(&empty_request()).unwrap();

    assert!(status.advisory_signals.is_empty());
    assert!(!status.degraded);
    assert_eq!(status.reason, None);
}

#[test]
fn stdio_forecast_client_rejects_nonzero_child_exit() {
    let script = write_python_script(
        r#"
import json
import sys

json.loads(sys.stdin.readline())
print("sidecar boom", file=sys.stderr)
sys.exit(7)
"#,
    );
    let client = StdioForecastClient::new(python_program(), [script.as_os_str()]);

    let error = client.forecast(&empty_request()).unwrap_err();

    assert!(matches!(error, ModelClientError::PredictionFailed(_)));
    assert!(error.to_string().contains("exited"));
    assert!(error.to_string().contains("sidecar boom"));
}

fn empty_request() -> ForecastRequest {
    ForecastRequest {
        actions: Vec::new(),
        relations: Vec::new(),
    }
}

fn request_with_actions() -> ForecastRequest {
    ForecastRequest {
        actions: vec![CandidateAction {
            name: "ship slice2".to_string(),
            continuation_id: Some("cont-1".to_string()),
            progress: 0.25,
            closure: 0.5,
            option_value_preserved: 0.75,
            risk: 0.1,
            irreversibility: 0.2,
            confusion: 0.3,
            friction: 0.4,
            temporal_debt_added: 0.05,
            uncertainty: 0.6,
            externality: 0.7,
        }],
        relations: Vec::new(),
    }
}

fn write_json(content: &str) -> std::path::PathBuf {
    let path = temp_dir().join("forecast.json");
    std::fs::write(&path, content).unwrap();
    path
}

fn write_python_script(content: &str) -> std::path::PathBuf {
    let path = temp_dir().join("sidecar.py");
    std::fs::write(&path, content.trim_start()).unwrap();
    path
}

fn python_program() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string())
}

fn temp_dir() -> std::path::PathBuf {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    loop {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate =
            std::env::temp_dir().join(format!("tfk-model-client-{}-{}", std::process::id(), id));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create temp dir {}: {error}", candidate.display()),
        }
    }
}

fn workspace_fixture(relative_path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path)
}
