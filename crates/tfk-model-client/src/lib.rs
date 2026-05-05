use serde::{Deserialize, Serialize};
use tfk_protocol::{AdvisoryForecastSignal, ForecastRequest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictionSignal {
    pub name: String,
    pub value: f64,
    pub confidence: f64,
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub request_id: String,
    pub predictions: Vec<PredictionSignal>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelClientError {
    #[error("python predictor sidecar is not configured")]
    NotConfigured,
    #[error("forecast prediction failed: {0}")]
    PredictionFailed(String),
}

pub trait PredictionClient {
    fn predict(&self, request_id: &str) -> Result<PredictionResponse, ModelClientError>;
}

pub trait ForecastPredictionClient: Send + Sync {
    fn forecast(
        &self,
        request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError>;
}

#[derive(Debug, Default)]
pub struct DisabledPredictionClient;

impl PredictionClient for DisabledPredictionClient {
    fn predict(&self, _request_id: &str) -> Result<PredictionResponse, ModelClientError> {
        Err(ModelClientError::NotConfigured)
    }
}

impl ForecastPredictionClient for DisabledPredictionClient {
    fn forecast(
        &self,
        _request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone)]
pub struct StaticForecastClient {
    signals: Vec<AdvisoryForecastSignal>,
}

impl StaticForecastClient {
    pub fn new(signals: Vec<AdvisoryForecastSignal>) -> Self {
        Self { signals }
    }
}

impl ForecastPredictionClient for StaticForecastClient {
    fn forecast(
        &self,
        _request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
        Ok(self.signals.clone())
    }
}
