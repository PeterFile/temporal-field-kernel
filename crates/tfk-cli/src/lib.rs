use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub fn build_http_request(method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAccept: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(body);
    request
}

pub fn extract_http_response_body(response: &[u8]) -> anyhow::Result<Vec<u8>> {
    let header_end =
        find_header_end(response).context("daemon response did not contain headers")?;
    let headers = std::str::from_utf8(&response[..header_end])
        .context("daemon response headers were not valid UTF-8")?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .context("daemon response did not contain an HTTP status")?;

    if !(200..300).contains(&status) {
        bail!("daemon returned HTTP status {status}");
    }

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

pub async fn request_json_over_uds(
    socket_path: &Path,
    path: &str,
    body: &[u8],
) -> anyhow::Result<Vec<u8>> {
    request_over_uds(socket_path, "POST", path, body).await
}

pub async fn request_over_uds(
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
