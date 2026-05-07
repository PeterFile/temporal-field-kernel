use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

#[derive(Debug, Clone, PartialEq)]
pub struct ForecastPredictionStatus {
    pub advisory_signals: Vec<AdvisoryForecastSignal>,
    pub degraded: bool,
    pub reason: Option<String>,
}

impl ForecastPredictionStatus {
    pub fn ready(advisory_signals: Vec<AdvisoryForecastSignal>) -> Self {
        Self {
            advisory_signals,
            degraded: false,
            reason: None,
        }
    }
}

pub trait ForecastPredictionClient: Send + Sync {
    fn forecast(
        &self,
        request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError>;

    fn forecast_with_status(
        &self,
        request: &ForecastRequest,
    ) -> Result<ForecastPredictionStatus, ModelClientError> {
        self.forecast(request).map(ForecastPredictionStatus::ready)
    }
}

#[derive(Debug, Clone)]
pub struct StdioForecastClient {
    program: PathBuf,
    args: Vec<OsString>,
}

impl StdioForecastClient {
    pub fn new<P, I, A>(program: P, args: I) -> Self
    where
        P: Into<PathBuf>,
        I: IntoIterator<Item = A>,
        A: Into<OsString>,
    {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

impl ForecastPredictionClient for StdioForecastClient {
    fn forecast(
        &self,
        request: &ForecastRequest,
    ) -> Result<Vec<AdvisoryForecastSignal>, ModelClientError> {
        self.forecast_with_status(request)
            .map(|status| status.advisory_signals)
    }

    fn forecast_with_status(
        &self,
        request: &ForecastRequest,
    ) -> Result<ForecastPredictionStatus, ModelClientError> {
        let command = describe_sidecar_command(&self.program, &self.args);
        let payload = StdioForecastRequest {
            request_id: "local-forecast",
            request,
        };
        let mut request_line = serde_json::to_vec(&payload).map_err(|error| {
            ModelClientError::PredictionFailed(format!(
                "failed to serialize forecast sidecar request for {command}: {error}"
            ))
        })?;
        request_line.push(b'\n');

        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                ModelClientError::PredictionFailed(format!(
                    "failed to spawn forecast sidecar {command}: {error}"
                ))
            })?;

        let write_result = match child.stdin.take() {
            Some(mut stdin) => stdin.write_all(&request_line),
            None => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "forecast sidecar stdin was not available",
            )),
        };
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ModelClientError::PredictionFailed(format!(
                "failed to write forecast sidecar request to {command}: {error}"
            )));
        }

        let output = child.wait_with_output().map_err(|error| {
            ModelClientError::PredictionFailed(format!(
                "failed to wait for forecast sidecar {command}: {error}"
            ))
        })?;

        if !output.status.success() {
            return Err(ModelClientError::PredictionFailed(format!(
                "forecast sidecar {command} exited with {}; stderr: {}; stdout: {}",
                output.status,
                stream_excerpt(&output.stderr),
                stream_excerpt(&output.stdout)
            )));
        }

        let stdout = std::str::from_utf8(&output.stdout).map_err(|error| {
            ModelClientError::PredictionFailed(format!(
                "forecast sidecar {command} returned non-UTF-8 JSON response: {error}"
            ))
        })?;
        let response_text = stdout.trim();
        if response_text.is_empty() {
            return Err(ModelClientError::PredictionFailed(format!(
                "forecast sidecar {command} returned empty stdout; expected JSON response with advisory_signals"
            )));
        }

        let response: StdioForecastResponse = serde_json::from_str(response_text).map_err(|error| {
            ModelClientError::PredictionFailed(format!(
                "failed to parse forecast sidecar response JSON from {command}: {error}; stdout: {}",
                stream_excerpt(&output.stdout)
            ))
        })?;

        let advisory_signals = response.advisory_signals.ok_or_else(|| {
            ModelClientError::PredictionFailed(format!(
                "missing advisory_signals in forecast sidecar response from {command}"
            ))
        })?;

        Ok(ForecastPredictionStatus {
            advisory_signals,
            degraded: response.degraded,
            reason: response.reason,
        })
    }
}

#[derive(Serialize)]
struct StdioForecastRequest<'a> {
    request_id: &'static str,
    request: &'a ForecastRequest,
}

#[derive(Deserialize)]
struct StdioForecastResponse {
    advisory_signals: Option<Vec<AdvisoryForecastSignal>>,
    #[serde(default)]
    degraded: bool,
    reason: Option<String>,
}

fn describe_sidecar_command(program: &Path, args: &[OsString]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(program.as_os_str().to_string_lossy().into_owned());
    parts.extend(args.iter().map(|arg| arg.to_string_lossy().into_owned()));
    parts.join(" ")
}

fn stream_excerpt(bytes: &[u8]) -> String {
    const MAX_LEN: usize = 512;

    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    if text.is_empty() {
        return "<empty>".to_string();
    }

    let mut excerpt: String = text.chars().take(MAX_LEN).collect();
    if text.chars().count() > MAX_LEN {
        excerpt.push_str("...");
    }
    excerpt
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
