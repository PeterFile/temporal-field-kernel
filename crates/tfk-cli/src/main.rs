use std::path::PathBuf;

use anyhow::bail;
use clap::{Parser, Subcommand};
use tfk_protocol::{
    ContinuationInput, ContinuationStatus, ContinuationType, EventSource, LensRequest,
    RawEventInput,
};

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
    /// Create or list continuations through the local tfkd daemon.
    Continuation {
        #[command(subcommand)]
        command: ContinuationCommand,
    },
    /// List active commitments through the local tfkd daemon.
    Commitment {
        #[command(subcommand)]
        command: CommitmentCommand,
    },
}

#[derive(Debug, Subcommand)]
enum ContinuationCommand {
    /// Create an active continuation.
    Create {
        #[arg(long)]
        summary: String,
        #[arg(long)]
        parent_id: Option<String>,
        #[arg(long)]
        raw_event_id: Option<String>,
        #[arg(
            long = "kind",
            alias = "continuation-type",
            default_value = "narrative"
        )]
        continuation_type: ContinuationType,
        title: String,
    },
    /// List stored continuations.
    List,
    /// Get one stored continuation.
    Get { id: String },
}

#[derive(Debug, Subcommand)]
enum CommitmentCommand {
    /// List active structured commitments.
    List,
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
        Command::Continuation { command } => match command {
            ContinuationCommand::Create {
                title,
                summary,
                parent_id,
                raw_event_id,
                continuation_type,
            } => {
                let input = ContinuationInput {
                    title,
                    summary,
                    continuation_type,
                    status: ContinuationStatus::Active,
                    parent_id,
                    raw_event_id,
                };
                let body = serde_json::to_vec(&input)?;
                let response =
                    tfk_cli::request_json_over_uds(&socket_path, "/v1/continuations", &body)
                        .await?;
                print_json(&response)?;
            }
            ContinuationCommand::List => {
                let response =
                    tfk_cli::request_over_uds(&socket_path, "GET", "/v1/continuations", b"")
                        .await?;
                print_json(&response)?;
            }
            ContinuationCommand::Get { id } => {
                let path = continuation_get_path(&id)?;
                let response = tfk_cli::request_over_uds(&socket_path, "GET", &path, b"").await?;
                print_json(&response)?;
            }
        },
        Command::Commitment { command } => match command {
            CommitmentCommand::List => {
                let response =
                    tfk_cli::request_over_uds(&socket_path, "GET", "/v1/commitments", b"").await?;
                print_json(&response)?;
            }
        },
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

fn continuation_get_path(id: &str) -> anyhow::Result<String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        bail!("invalid continuation id");
    }
    Ok(format!("/v1/continuations/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_continuation_create_command() {
        let cli = Cli::parse_from([
            "tfk",
            "--uds",
            "/tmp/tfk.sock",
            "continuation",
            "create",
            "--summary",
            "继续跟踪",
            "--parent-id",
            "cont_parent",
            "--raw-event-id",
            "evt_source",
            "项目状态机不是目标",
        ]);

        match cli.command {
            Command::Continuation {
                command:
                    ContinuationCommand::Create {
                        title,
                        summary,
                        parent_id,
                        raw_event_id,
                        continuation_type,
                    },
            } => {
                assert_eq!(title, "项目状态机不是目标");
                assert_eq!(summary, "继续跟踪");
                assert_eq!(parent_id.as_deref(), Some("cont_parent"));
                assert_eq!(raw_event_id.as_deref(), Some("evt_source"));
                assert_eq!(continuation_type, ContinuationType::Narrative);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_continuation_list_command() {
        let cli = Cli::parse_from(["tfk", "continuation", "list"]);

        assert!(matches!(
            cli.command,
            Command::Continuation {
                command: ContinuationCommand::List
            }
        ));
    }

    #[test]
    fn parses_continuation_create_kind() {
        let cli = Cli::parse_from([
            "tfk",
            "continuation",
            "create",
            "--summary",
            "继续跟踪",
            "--kind",
            "obligation",
            "项目状态机不是目标",
        ]);

        match cli.command {
            Command::Continuation {
                command:
                    ContinuationCommand::Create {
                        continuation_type, ..
                    },
            } => assert_eq!(continuation_type, ContinuationType::Obligation),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_continuation_create_continuation_type_alias() {
        let cli = Cli::parse_from([
            "tfk",
            "continuation",
            "create",
            "--summary",
            "继续跟踪",
            "--continuation-type",
            "risk",
            "项目状态机不是目标",
        ]);

        match cli.command {
            Command::Continuation {
                command:
                    ContinuationCommand::Create {
                        continuation_type, ..
                    },
            } => assert_eq!(continuation_type, ContinuationType::Risk),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_continuation_create_kind() {
        let error = Cli::try_parse_from([
            "tfk",
            "continuation",
            "create",
            "--summary",
            "继续跟踪",
            "--kind",
            "memory",
            "项目状态机不是目标",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn parses_continuation_get_command() {
        let cli = Cli::parse_from(["tfk", "continuation", "get", "cont_abc123"]);

        match cli.command {
            Command::Continuation {
                command: ContinuationCommand::Get { id },
            } => assert_eq!(id, "cont_abc123"),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn continuation_get_path_rejects_request_line_injection() {
        let error = continuation_get_path("cont_1\r\nX-Bad: true").unwrap_err();

        assert!(error.to_string().contains("invalid continuation id"));
    }

    #[test]
    fn parses_commitment_list_command() {
        let cli = Cli::parse_from(["tfk", "commitment", "list"]);

        assert!(matches!(
            cli.command,
            Command::Commitment {
                command: CommitmentCommand::List
            }
        ));
    }
}
