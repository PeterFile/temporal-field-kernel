use tfk_model_client::{
    DisabledPredictionClient, ForecastPredictionClient, ModelClientError, StaticForecastClient,
};
use tfk_protocol::{AdvisoryForecastSignal, ForecastRequest};

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

fn empty_request() -> ForecastRequest {
    ForecastRequest {
        actions: Vec::new(),
        relations: Vec::new(),
    }
}

fn write_json(content: &str) -> std::path::PathBuf {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let dir = loop {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let candidate =
            std::env::temp_dir().join(format!("tfk-model-client-{}-{}", std::process::id(), id));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create temp dir {}: {error}", candidate.display()),
        }
    };
    let path = dir.join("forecast.json");
    std::fs::write(&path, content).unwrap();
    path
}

fn workspace_fixture(relative_path: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative_path)
}
