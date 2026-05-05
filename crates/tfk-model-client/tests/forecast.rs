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
