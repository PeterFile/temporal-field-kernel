use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tfk_protocol::{EventSource, LensRequest, RawEventInput};

#[derive(Debug, Parser)]
#[command(name = "tfk", about = "Temporal Field Kernel CLI")]
struct Cli {
    /// tfkd Unix domain socket path. Defaults to $XDG_RUNTIME_DIR/tfk/tfkd.sock or /tmp/tfk/tfkd.sock.
    #[arg(long, env = "TFK_SOCKET")]
    uds: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print local diagnostics.
    Doctor,
    /// Observe a raw text event through the local tfkd daemon.
    Observe {
        #[arg(long, default_value = "manual")]
        session: String,
        #[arg(long, default_value = "cli")]
        adapter: String,
        content: String,
    },
    /// Request a minimal lens card from the local tfkd daemon.
    Lens { query: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket_path = cli.uds.unwrap_or_else(default_socket_path);

    match cli.command {
        Command::Doctor => {
            println!("tfk: ok");
            println!("socket: {}", socket_path.display());
        }
        Command::Observe {
            session,
            adapter,
            content,
        } => {
            let event = RawEventInput::new_text(session, adapter, EventSource::User, content);
            let body = serde_json::to_vec(&event)?;
            let response =
                tfk_cli::request_json_over_uds(&socket_path, "/v1/observe", &body).await?;
            print_json(&response)?;
        }
        Command::Lens { query } => {
            let request = LensRequest {
                query,
                horizon: Vec::new(),
                perspective: Vec::new(),
            };
            let body = serde_json::to_vec(&request)?;
            let response = tfk_cli::request_json_over_uds(&socket_path, "/v1/lens", &body).await?;
            print_json(&response)?;
        }
    }
    Ok(())
}

fn print_json(body: &[u8]) -> anyhow::Result<()> {
    let value: serde_json::Value = serde_json::from_slice(body)?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn default_socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("tfk")
        .join("tfkd.sock")
}
