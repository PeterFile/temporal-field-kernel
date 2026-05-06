use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
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

    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self, ModelClientError> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|error| {
            ModelClientError::PredictionFailed(format!(
                "failed to read forecast JSON {}: {error}",
                path.display()
            ))
        })?;
        Self::from_json_str(&content)
    }

    pub fn from_json_str(content: &str) -> Result<Self, ModelClientError> {
        let value: Value = serde_json::from_str(content).map_err(|error| {
            ModelClientError::PredictionFailed(format!("failed to parse forecast JSON: {error}"))
        })?;
        let signals = match value {
            Value::Array(_) => serde_json::from_value(value).map_err(invalid_signal_error)?,
            Value::Object(mut object) => {
                let signals = object.remove("advisory_signals").ok_or_else(|| {
                    ModelClientError::PredictionFailed(
                        "missing advisory_signals in forecast JSON".to_string(),
                    )
                })?;
                serde_json::from_value(signals).map_err(invalid_signal_error)?
            }
            _ => {
                return Err(ModelClientError::PredictionFailed(
                    "forecast JSON must be an advisory signal array or object".to_string(),
                ));
            }
        };
        Ok(Self::new(signals))
    }
}

fn invalid_signal_error(error: serde_json::Error) -> ModelClientError {
    ModelClientError::PredictionFailed(format!(
        "invalid advisory_signals in forecast JSON: {error}"
    ))
}

impl ForecastPredictionClient for StaticForecastClient {
    fn forecast(
        &self,
        _request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
        Ok(self.signals.clone())
    }
}
