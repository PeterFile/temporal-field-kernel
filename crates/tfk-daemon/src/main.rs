use std::io::ErrorKind;
use std::net::SocketAddr;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use clap::{Parser, Subcommand};
use hyper::service::service_fn;
use hyper::{body::Incoming, Request};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HyperBuilder;
use tfk_store::Store;
use tower::ServiceExt;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "tfkd", about = "Temporal Field Kernel local daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print daemon diagnostics.
    Doctor,
    /// Serve the local API. UDS is the default local-first transport; HTTP is optional.
    Serve {
        #[arg(long)]
        uds: Option<PathBuf>,
        #[arg(long)]
        http: Option<String>,
        /// Directory containing tfk.db and the append-only archive directory.
        #[arg(long)]
        data_dir: Option<PathBuf>,
        /// Load advisory-only static forecast signals from local JSON.
        #[arg(long)]
        forecast_advisory_json: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    match Cli::parse().command {
        Command::Doctor => {
            println!("tfkd: ok");
            println!("transport: uds default, http-localhost optional");
            println!("default_socket: {}", default_socket_path().display());
            println!("default_data_dir: {}", default_data_dir().display());
        }
        Command::Serve {
            uds,
            http,
            data_dir,
            forecast_advisory_json,
        } => {
            let data_dir = data_dir.unwrap_or_else(default_data_dir);
            let store = open_store(&data_dir)?;
            let state = api_state_for_store(store, forecast_advisory_json.as_deref())?;
            if let Some(http) = http {
                serve_http(http, state).await?;
            } else {
                let socket_path = uds.unwrap_or_else(default_socket_path);
                serve_uds(socket_path, state).await?;
            }
        }
    }
    Ok(())
}

fn api_state_for_store(
    store: Store,
    forecast_advisory_json: Option<&Path>,
) -> anyhow::Result<tfk_api::ApiState> {
    let state = tfk_api::ApiState::new(store);
    let Some(path) = forecast_advisory_json else {
        return Ok(state);
    };
    let client = tfk_model_client::StaticForecastClient::from_json_file(path)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to load forecast advisory JSON {}", path.display()))?;
    Ok(state.with_forecast_client(client))
}

async fn serve_http(http: String, state: tfk_api::ApiState) -> anyhow::Result<()> {
    let addr = parse_loopback_http_addr(&http)?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;
    tracing::info!(%addr, "serving tfkd HTTP API");
    axum::serve(listener, tfk_api::router_with_state(state)).await?;
    Ok(())
}

fn parse_loopback_http_addr(http: &str) -> anyhow::Result<SocketAddr> {
    let addr = http
        .parse::<SocketAddr>()
        .with_context(|| format!("invalid HTTP bind address {http}"))?;
    if !addr.ip().is_loopback() {
        bail!("refusing non-loopback HTTP bind address {addr}; tfkd HTTP is local-only");
    }
    Ok(addr)
}

async fn serve_uds(socket_path: PathBuf, state: tfk_api::ApiState) -> anyhow::Result<()> {
    if let Some(parent) = socket_path.parent() {
        ensure_private_dir(parent)?;
    }
    remove_stale_socket_if_present(&socket_path)?;
    let listener = tokio::net::UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind {}", socket_path.display()))?;
    restrict_socket_permissions(&socket_path)?;
    tracing::info!(path = %socket_path.display(), "serving tfkd UDS API");

    let app = tfk_api::router_with_state(state);
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();
        tokio::spawn(async move {
            let io = TokioIo::new(stream);
            let service =
                service_fn(move |request: Request<Incoming>| app.clone().oneshot(request));
            if let Err(error) = HyperBuilder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                tracing::warn!(%error, "UDS connection failed");
            }
        });
    }
}

fn open_store(data_dir: &Path) -> anyhow::Result<Store> {
    Store::open(data_dir.join("tfk.db"), data_dir.join("archive"))
        .with_context(|| format!("failed to open store under {}", data_dir.display()))
}

fn remove_stale_socket_if_present(socket_path: &Path) -> anyhow::Result<()> {
    let metadata = match std::fs::symlink_metadata(socket_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", socket_path.display()))
        }
    };

    if !metadata.file_type().is_socket() {
        bail!(
            "refusing to remove non-socket path {}; delete it manually or choose another --uds path",
            socket_path.display()
        );
    }

    std::fs::remove_file(socket_path)
        .with_context(|| format!("failed to remove stale socket {}", socket_path.display()))
}

fn ensure_private_dir(path: &Path) -> anyhow::Result<()> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            let mode = metadata.permissions().mode() & 0o7777;
            let writable_by_group_or_other = mode & 0o022 != 0;
            let has_sticky_bit = mode & 0o1000 != 0;
            if writable_by_group_or_other && !has_sticky_bit {
                bail!(
                    "refusing unsafe socket directory {} with mode {mode:o}; use an owner-only directory or a sticky runtime directory",
                    path.display()
                );
            }
            Ok(())
        }
        Ok(_) => bail!("socket parent is not a directory: {}", path.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            std::fs::create_dir_all(path)
                .with_context(|| format!("failed to create directory {}", path.display()))?;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
                .with_context(|| format!("failed to restrict directory {}", path.display()))
        }
        Err(error) => {
            Err(error).with_context(|| format!("failed to inspect directory {}", path.display()))
        }
    }
}

fn restrict_socket_permissions(socket_path: &Path) -> anyhow::Result<()> {
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to restrict socket {}", socket_path.display()))
}

fn default_socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("tfk")
        .join("tfkd.sock")
}

fn default_data_dir() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home).join("tfk");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("tfk");
    }
    std::env::temp_dir().join("tfk-data")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::ErrorKind;
    use std::os::unix::fs::{symlink, PermissionsExt as _};
    use std::os::unix::net::UnixListener as StdUnixListener;

    use super::*;

    #[test]
    fn http_bind_accepts_loopback_address() {
        let addr = parse_loopback_http_addr("127.0.0.1:7331").unwrap();

        assert!(addr.ip().is_loopback());
        assert_eq!(addr.port(), 7331);
    }

    #[test]
    fn http_bind_rejects_non_loopback_address() {
        let error = parse_loopback_http_addr("0.0.0.0:7331").unwrap_err();

        assert!(error
            .to_string()
            .contains("refusing non-loopback HTTP bind"));
    }

    #[test]
    fn stale_socket_cleanup_rejects_regular_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tfkd.sock");
        fs::write(&path, "not a socket").unwrap();

        let error = remove_stale_socket_if_present(&path).unwrap_err();

        assert!(path.exists());
        assert!(error
            .to_string()
            .contains("refusing to remove non-socket path"));
    }

    #[test]
    fn stale_socket_cleanup_removes_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tfkd.sock");
        let Some(listener) = bind_test_socket_or_skip(&path) else {
            return;
        };
        drop(listener);

        remove_stale_socket_if_present(&path).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn private_dir_uses_owner_only_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime");

        ensure_private_dir(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn existing_parent_permissions_are_not_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();

        ensure_private_dir(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755);
    }

    #[test]
    fn existing_group_or_world_writable_parent_without_sticky_bit_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unsafe-runtime");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o777)).unwrap();

        let error = ensure_private_dir(&path).unwrap_err();

        assert!(error
            .to_string()
            .contains("refusing unsafe socket directory"));
    }

    #[test]
    fn sticky_world_writable_parent_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sticky-runtime");
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o1777)).unwrap();

        ensure_private_dir(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o1777);
    }

    #[test]
    fn symlink_to_sticky_world_writable_parent_is_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("sticky-runtime");
        let link = dir.path().join("runtime-link");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o1777)).unwrap();
        symlink(&target, &link).unwrap();

        ensure_private_dir(&link).unwrap();
    }

    #[test]
    fn socket_restriction_uses_owner_only_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tfkd.sock");
        let Some(listener) = bind_test_socket_or_skip(&path) else {
            return;
        };
        drop(listener);

        restrict_socket_permissions(&path).unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn serve_parses_static_forecast_advisory_json_flag() {
        let cli = Cli::parse_from(["tfkd", "serve", "--forecast-advisory-json", "forecast.json"]);

        let Command::Serve {
            forecast_advisory_json,
            ..
        } = cli.command
        else {
            panic!("expected serve command");
        };

        assert_eq!(forecast_advisory_json, Some(PathBuf::from("forecast.json")));
    }

    fn bind_test_socket_or_skip(path: &Path) -> Option<StdUnixListener> {
        match StdUnixListener::bind(path) {
            Ok(listener) => Some(listener),
            Err(error) if error.kind() == ErrorKind::PermissionDenied => None,
            Err(error) => panic!("failed to bind test socket {}: {error}", path.display()),
        }
    }
}
