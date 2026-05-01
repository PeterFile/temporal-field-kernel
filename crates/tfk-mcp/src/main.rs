use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::Parser;
use serde_json::json;

#[derive(Debug, Parser)]
#[command(
    name = "tfk-mcp",
    about = "Temporal Field Kernel MCP JSON-line stdio scaffold"
)]
struct Cli {
    /// Path to tfkd Unix domain socket, or auto for the local default.
    #[arg(long, env = "TFK_SOCKET", default_value = "auto")]
    socket: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket_path = resolve_socket_path(&cli.socket);
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let response = match tfk_mcp::parse_command_line(&line) {
            Ok(command) => tfk_mcp::dispatch_to_daemon(&socket_path, &command).await,
            Err(error) => json!({
                "ok": false,
                "command": null,
                "degraded": false,
                "error": error.to_string(),
            }),
        };
        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

fn resolve_socket_path(socket: &str) -> PathBuf {
    if socket != "auto" {
        return PathBuf::from(socket);
    }

    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("tfk")
        .join("tfkd.sock")
}
