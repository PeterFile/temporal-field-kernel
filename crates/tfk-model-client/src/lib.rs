use serde::{Deserialize, Serialize};

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
}

pub trait PredictionClient {
    fn predict(&self, request_id: &str) -> Result<PredictionResponse, ModelClientError>;
}

#[derive(Debug, Default)]
pub struct DisabledPredictionClient;

impl PredictionClient for DisabledPredictionClient {
    fn predict(&self, _request_id: &str) -> Result<PredictionResponse, ModelClientError> {
        Err(ModelClientError::NotConfigured)
    }
}
