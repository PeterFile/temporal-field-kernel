use std::io::ErrorKind;
use std::io::{Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

#[test]
fn uds_smoke_health_observe_and_raw_event_search() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = smoke_tempdir()?;
    let tmp_path = tmp.path().to_path_buf();
    if !uds_bind_supported(tmp.path())? {
        eprintln!(
            "skipping UDS smoke: sandbox denied AF_UNIX bind under {}",
            tmp.path().display()
        );
        tmp.close()?;
        assert!(!tmp_path.exists());
        return Ok(());
    }

    let socket_path = tmp.path().join("s");
    let data_dir = tmp.path().join("d");
    let mut daemon = TfkdProcess::spawn(&socket_path, &data_dir)?;

    wait_for_uds(&socket_path, &mut daemon.child)?;

    let health = uds_request(&socket_path, "GET", "/healthz", None)?;
    assert_eq!(health.status, 200);
    assert_eq!(health.json["ok"], true);
    assert_eq!(health.json["data"]["status"], "ok");

    let observe_body = json!({
        "session_id": "uds-smoke-session",
        "adapter_id": "uds-smoke",
        "source": "user",
        "modality": "text",
        "content": "uds smoke needle evidence",
        "act_type": null,
        "evidence_status": "observed",
        "time_utc": null
    });
    let observed = uds_request(&socket_path, "POST", "/v1/observe", Some(&observe_body))?;
    assert_eq!(observed.status, 200);
    assert_eq!(observed.json["ok"], true);
    assert_eq!(observed.json["data"]["session_id"], "uds-smoke-session");
    assert_eq!(
        observed.json["data"]["content"],
        "uds smoke needle evidence"
    );

    let searched = uds_request(&socket_path, "GET", "/v1/raw-events?query=needle", None)?;
    assert_eq!(searched.status, 200);
    assert_eq!(searched.json["ok"], true);
    let events = searched.json["data"].as_array().expect("data is an array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["id"], observed.json["data"]["id"]);

    daemon.kill_and_wait();
    drop(daemon);
    tmp.close()?;
    assert!(!tmp_path.exists());
    Ok(())
}

fn smoke_tempdir() -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir
        .parent()
        .and_then(Path::parent)
        .ok_or("tfk-daemon manifest is not under workspace/crates")?;
    let base = workspace.join("target").join("u");
    std::fs::create_dir_all(&base)?;
    Ok(tempfile::Builder::new().prefix("r").tempdir_in(base)?)
}

fn uds_bind_supported(path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let probe = path.join("p");
    match UnixListener::bind(&probe) {
        Ok(listener) => {
            drop(listener);
            std::fs::remove_file(probe)?;
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::PermissionDenied => Ok(false),
        Err(error) => Err(error.into()),
    }
}

struct TfkdProcess {
    child: Child,
}

impl TfkdProcess {
    fn spawn(socket_path: &Path, data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let child = Command::new(env!("CARGO_BIN_EXE_tfkd"))
            .arg("serve")
            .arg("--uds")
            .arg(socket_path)
            .arg("--data-dir")
            .arg(data_dir)
            .spawn()?;
        Ok(Self { child })
    }

    fn kill_and_wait(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for TfkdProcess {
    fn drop(&mut self) {
        self.kill_and_wait();
    }
}

struct UdsResponse {
    status: u16,
    json: Value,
}

fn wait_for_uds(socket_path: &Path, child: &mut Child) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match std::fs::symlink_metadata(socket_path) {
            Ok(metadata) if metadata.file_type().is_socket() => return Ok(()),
            Ok(_) => {
                return Err(format!("UDS path is not a socket: {}", socket_path.display()).into())
            }
            Err(last_error) if last_error.kind() == ErrorKind::NotFound => {
                if let Some(status) = child.try_wait()? {
                    return Err(format!("tfkd exited before UDS readiness: {status}").into());
                }
                if Instant::now() >= deadline {
                    return Err(format!("timed out waiting for UDS: {last_error}").into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn uds_request(
    socket_path: &Path,
    method: &str,
    target: &str,
    json_body: Option<&Value>,
) -> Result<UdsResponse, Box<dyn std::error::Error>> {
    let body = match json_body {
        Some(value) => serde_json::to_vec(value)?,
        None => Vec::new(),
    };
    let content_type = if json_body.is_some() {
        "Content-Type: application/json\r\n"
    } else {
        ""
    };
    let request = format!(
        "{method} {target} HTTP/1.1\r\nHost: localhost\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );

    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(request.as_bytes())?;
    stream.write_all(&body)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    parse_response(&response)
}

fn parse_response(response: &[u8]) -> Result<UdsResponse, Box<dyn std::error::Error>> {
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("missing HTTP header terminator")?;
    let headers = std::str::from_utf8(&response[..header_end])?;
    let mut lines = headers.lines();
    let status_line = lines.next().ok_or("missing HTTP status line")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or("missing HTTP status code")?
        .parse::<u16>()?;
    let content_length = lines
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>())
        })
        .transpose()?
        .ok_or("missing Content-Length")?;
    let body_start = header_end + 4;
    let body_end = body_start + content_length;
    if response.len() < body_end {
        return Err("short HTTP response body".into());
    }
    let json = serde_json::from_slice(&response[body_start..body_end])?;
    Ok(UdsResponse { status, json })
}
