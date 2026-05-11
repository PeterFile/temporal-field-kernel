//! JSON-line stdio scaffold for MCP-facing clients.
//!
//! This is intentionally not a full MCP protocol implementation. It only parses a
//! tiny local command shape and forwards supported calls to the tfkd UDS HTTP API.

use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use tfk_protocol::{
    CommitRequest, ContinuationInput, ContinuationRelationEdge, ForecastRequest, LensRequest,
    PreflightSignals, RawEventInput, TemporalDeltaInput,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum StdioCommand {
    Health,
    Observe {
        request: Value,
    },
    RawEventSearch {
        query: String,
    },
    RawEventGet {
        id: String,
    },
    Lens {
        query: String,
    },
    Preflight {
        uncertainty: f64,
        irreversibility: f64,
        externality: f64,
        #[serde(default)]
        option_value_loss: Option<f64>,
    },
    Forecast {
        request: Value,
    },
    Commit {
        request: Value,
    },
    Assimilate {
        request: Value,
    },
    ContinuationCreate {
        request: Value,
    },
    ContinuationList,
    ContinuationGet {
        id: String,
    },
    RelationCreate {
        request: Value,
    },
    RelationList,
    CommitmentList,
}

impl StdioCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Health => "health",
            Self::Observe { .. } => "observe",
            Self::RawEventSearch { .. } => "raw_event_search",
            Self::RawEventGet { .. } => "raw_event_get",
            Self::Lens { .. } => "lens",
            Self::Preflight { .. } => "preflight",
            Self::Forecast { .. } => "forecast",
            Self::Commit { .. } => "commit",
            Self::Assimilate { .. } => "assimilate",
            Self::ContinuationCreate { .. } => "continuation_create",
            Self::ContinuationList => "continuation_list",
            Self::ContinuationGet { .. } => "continuation_get",
            Self::RelationCreate { .. } => "relation_create",
            Self::RelationList => "relation_list",
            Self::CommitmentList => "commitment_list",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonRequest {
    pub method: &'static str,
    pub path: String,
    pub body: Vec<u8>,
}

pub fn parse_command_line(line: &str) -> anyhow::Result<StdioCommand> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        bail!("empty tfk-mcp command line");
    }

    serde_json::from_str(trimmed).context("invalid tfk-mcp command JSON")
}

pub fn daemon_request_for(command: &StdioCommand) -> anyhow::Result<DaemonRequest> {
    match command {
        StdioCommand::Health => Ok(DaemonRequest {
            method: "GET",
            path: "/healthz".to_string(),
            body: Vec::new(),
        }),
        StdioCommand::Observe { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/observe".to_string(),
            body: typed_request_body::<RawEventInput>(request, "observe")?,
        }),
        StdioCommand::RawEventSearch { query } => Ok(DaemonRequest {
            method: "GET",
            path: raw_event_search_path(query),
            body: Vec::new(),
        }),
        StdioCommand::RawEventGet { id } => {
            let id = safe_path_id(id, "raw event id")?;

            Ok(DaemonRequest {
                method: "GET",
                path: format!("/v1/raw-events/{id}"),
                body: Vec::new(),
            })
        }
        StdioCommand::Lens { query } => {
            if query.trim().is_empty() {
                bail!("lens query must not be empty");
            }
            let request = LensRequest {
                query: query.clone(),
                horizon: Vec::new(),
                perspective: Vec::new(),
            };

            Ok(DaemonRequest {
                method: "POST",
                path: "/v1/lens".to_string(),
                body: serde_json::to_vec(&request)?,
            })
        }
        StdioCommand::Preflight {
            uncertainty,
            irreversibility,
            externality,
            option_value_loss,
        } => {
            let request = PreflightSignals {
                uncertainty: *uncertainty,
                irreversibility: *irreversibility,
                externality: *externality,
                option_value_loss: option_value_loss.unwrap_or(0.0),
            };

            Ok(DaemonRequest {
                method: "POST",
                path: "/v1/preflight".to_string(),
                body: serde_json::to_vec(&request)?,
            })
        }
        StdioCommand::Forecast { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/forecast".to_string(),
            body: typed_request_body::<ForecastRequest>(request, "forecast")?,
        }),
        StdioCommand::Commit { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/commit".to_string(),
            body: typed_request_body::<CommitRequest>(request, "commit")?,
        }),
        StdioCommand::Assimilate { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/assimilate".to_string(),
            body: typed_request_body::<TemporalDeltaInput>(request, "assimilate")?,
        }),
        StdioCommand::ContinuationCreate { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/continuations".to_string(),
            body: typed_request_body::<ContinuationInput>(request, "continuation_create")?,
        }),
        StdioCommand::ContinuationList => Ok(DaemonRequest {
            method: "GET",
            path: "/v1/continuations".to_string(),
            body: Vec::new(),
        }),
        StdioCommand::ContinuationGet { id } => {
            let id = safe_path_id(id, "continuation id")?;

            Ok(DaemonRequest {
                method: "GET",
                path: format!("/v1/continuations/{id}"),
                body: Vec::new(),
            })
        }
        StdioCommand::RelationCreate { request } => Ok(DaemonRequest {
            method: "POST",
            path: "/v1/continuation-relations".to_string(),
            body: typed_request_body::<ContinuationRelationEdge>(request, "relation_create")?,
        }),
        StdioCommand::RelationList => Ok(DaemonRequest {
            method: "GET",
            path: "/v1/continuation-relations".to_string(),
            body: Vec::new(),
        }),
        StdioCommand::CommitmentList => Ok(DaemonRequest {
            method: "GET",
            path: "/v1/commitments".to_string(),
            body: Vec::new(),
        }),
    }
}

fn safe_path_id<'a>(id: &'a str, label: &str) -> anyhow::Result<&'a str> {
    if id.is_empty() {
        bail!("{label} must not be empty");
    }

    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("{label} must contain only ASCII letters, digits, '_' or '-'");
    }

    Ok(id)
}

fn raw_event_search_path(query: &str) -> String {
    format!("/v1/raw-events?query={}", percent_encode_query(query))
}

fn percent_encode_query(query: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    let mut encoded = String::new();
    for byte in query.bytes() {
        if is_unreserved_query_byte(byte) {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0F) as usize] as char);
        }
    }
    encoded
}

fn is_unreserved_query_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn typed_request_body<T>(request: &Value, command: &str) -> anyhow::Result<Vec<u8>>
where
    T: DeserializeOwned + Serialize,
{
    let typed = serde_json::from_value::<T>(request.clone())
        .with_context(|| format!("{command} request did not match protocol schema"))?;
    serde_json::to_vec(&typed).context("failed to serialize daemon request")
}

pub async fn dispatch_to_daemon(socket_path: &Path, command: &StdioCommand) -> Value {
    let command_name = command.name();
    let request = match daemon_request_for(command) {
        Ok(request) => request,
        Err(error) => return command_error_response(command_name, error),
    };

    match request_json_over_uds(socket_path, request.method, &request.path, &request.body).await {
        Ok(body) => match serde_json::from_slice::<Value>(&body) {
            Ok(data) => json!({
                "ok": true,
                "command": command_name,
                "degraded": false,
                "data": data,
            }),
            Err(error) => command_error_response(command_name, error),
        },
        Err(error) => degraded_response(command_name, error),
    }
}

pub fn degraded_response(command: &str, error: impl ToString) -> Value {
    json!({
        "ok": false,
        "command": command,
        "degraded": true,
        "error": format!("daemon unavailable: {}", error.to_string()),
    })
}

pub async fn request_json_over_uds(
    socket_path: &Path,
    method: &str,
    path: &str,
    body: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("failed to connect to {}", socket_path.display()))?;
    let request = build_http_request(method, path, body);
    stream
        .write_all(&request)
        .await
        .with_context(|| format!("failed to write request to {}", socket_path.display()))?;

    let mut response = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut buf))
            .await
            .context("timed out waiting for daemon response")?
            .with_context(|| format!("failed to read response from {}", socket_path.display()))?;
        if read == 0 {
            break;
        }
        response.extend_from_slice(&buf[..read]);
        if let Some(expected_len) = expected_response_len(&response)? {
            if response.len() >= expected_len {
                break;
            }
        }
    }

    extract_http_response_body(&response)
}

fn command_error_response(command: &str, error: impl ToString) -> Value {
    json!({
        "ok": false,
        "command": command,
        "degraded": false,
        "error": error.to_string(),
    })
}

fn build_http_request(method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    request
}

fn extract_http_response_body(response: &[u8]) -> anyhow::Result<Vec<u8>> {
    let header_end =
        find_header_end(response).context("daemon response did not contain headers")?;
    let headers = std::str::from_utf8(&response[..header_end])
        .context("daemon response headers were not valid UTF-8")?;
    let _status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .context("daemon response did not contain an HTTP status")?;

    let body_start = header_end + 4;
    let body = &response[body_start..];
    if let Some(content_length) = content_length(headers)? {
        if body.len() < content_length {
            bail!(
                "daemon response body was shorter than Content-Length: got {}, expected {content_length}",
                body.len()
            );
        }
        return Ok(body[..content_length].to_vec());
    }

    Ok(body.to_vec())
}

fn find_header_end(response: &[u8]) -> Option<usize> {
    response.windows(4).position(|window| window == b"\r\n\r\n")
}

fn expected_response_len(response: &[u8]) -> anyhow::Result<Option<usize>> {
    let Some(header_end) = find_header_end(response) else {
        return Ok(None);
    };
    let headers = std::str::from_utf8(&response[..header_end])
        .context("daemon response headers were not valid UTF-8")?;
    Ok(content_length(headers)?.map(|len| header_end + 4 + len))
}

fn content_length(headers: &str) -> anyhow::Result<Option<usize>> {
    for line in headers.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value
                .trim()
                .parse::<usize>()
                .map(Some)
                .map_err(|error| anyhow!("invalid Content-Length header: {error}"));
        }
    }
    Ok(None)
}
