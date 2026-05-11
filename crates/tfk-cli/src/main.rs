use std::path::{Path, PathBuf};

use anyhow::bail;
use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use tfk_protocol::{
    CommitRequest, ContinuationInput, ContinuationRelationEdge, ContinuationRelationKind,
    ContinuationStatus, ContinuationType, EventSource, EvidenceStatus, ForecastRequest,
    LensRequest, PreflightSignals, RawEventInput, TemporalDeltaInput,
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
        #[arg(long, default_value = "user", value_parser = parse_event_source)]
        source: EventSource,
        #[arg(long, default_value = "observed", value_parser = parse_evidence_status)]
        evidence_status: EvidenceStatus,
        #[arg(long)]
        act_type: Option<String>,
        #[arg(long)]
        time_utc: Option<String>,
        content: String,
    },
    /// Request a minimal lens card from the local tfkd daemon.
    Lens { query: String },
    /// Check deterministic path-choice risk through the local tfkd daemon.
    Preflight {
        #[arg(long)]
        uncertainty: f64,
        #[arg(long)]
        irreversibility: f64,
        #[arg(long)]
        externality: f64,
        #[arg(long, default_value_t = 0.0)]
        option_value_loss: f64,
    },
    /// Create or list continuations through the local tfkd daemon.
    Continuation {
        #[command(subcommand)]
        command: ContinuationCommand,
    },
    /// Create or list continuation relations through the local tfkd daemon.
    Relation {
        #[command(subcommand)]
        command: RelationCommand,
    },
    /// List active commitments through the local tfkd daemon.
    Commitment {
        #[command(subcommand)]
        command: CommitmentCommand,
    },
    /// Create structured commitments through the local tfkd daemon.
    Commit {
        #[command(subcommand)]
        command: CommitCommand,
    },
    /// Request action-loop forecast scoring through the local tfkd daemon.
    Forecast {
        #[arg(long)]
        json_file: PathBuf,
    },
    /// List or get advisory forecast signals through the local tfkd daemon.
    AdvisoryForecastSignal {
        #[command(subcommand)]
        command: AdvisoryForecastSignalCommand,
    },
    /// Apply an action-loop temporal delta through the local tfkd daemon.
    Assimilate {
        #[arg(long)]
        json_file: PathBuf,
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
enum RelationCommand {
    /// Create a persisted continuation relation.
    Create {
        #[arg(long)]
        from_id: String,
        #[arg(long)]
        to_id: String,
        #[arg(long, value_parser = parse_relation_kind)]
        kind: ContinuationRelationKind,
        #[arg(long)]
        reason: Option<String>,
    },
    /// List persisted continuation relations.
    List,
}

#[derive(Debug, Subcommand)]
enum CommitmentCommand {
    /// List active structured commitments.
    List,
}

#[derive(Debug, Subcommand)]
enum CommitCommand {
    /// Create a structured commitment.
    Create {
        #[arg(long)]
        speaker: String,
        #[arg(long)]
        statement: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        deadline: Option<String>,
        #[arg(
            long,
            required = true,
            action = clap::ArgAction::Set,
            value_parser = clap::value_parser!(bool)
        )]
        revocable: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AdvisoryForecastSignalCommand {
    /// List advisory forecast signals.
    List,
    /// Get one advisory forecast signal.
    Get { id: String },
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
            source,
            evidence_status,
            act_type,
            time_utc,
            content,
        } => {
            let body = observe_request_body(
                session,
                adapter,
                source,
                content,
                act_type,
                evidence_status,
                time_utc,
            )?;
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
        Command::Preflight {
            uncertainty,
            irreversibility,
            externality,
            option_value_loss,
        } => {
            let body = preflight_request_body(
                uncertainty,
                irreversibility,
                externality,
                option_value_loss,
            )?;
            let response =
                tfk_cli::request_json_over_uds(&socket_path, "/v1/preflight", &body).await?;
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
        Command::Relation { command } => match command {
            RelationCommand::Create {
                from_id,
                to_id,
                kind,
                reason,
            } => {
                let body = relation_create_request_body(from_id, to_id, kind, reason)?;
                let response =
                    tfk_cli::request_json_over_uds(&socket_path, relation_endpoint(), &body)
                        .await?;
                print_json(&response)?;
            }
            RelationCommand::List => {
                let response =
                    tfk_cli::request_over_uds(&socket_path, "GET", relation_endpoint(), b"")
                        .await?;
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
        Command::Commit { command } => match command {
            CommitCommand::Create {
                speaker,
                statement,
                scope,
                deadline,
                revocable,
            } => {
                let body = commit_request_body(speaker, statement, scope, deadline, revocable)?;
                let response =
                    tfk_cli::request_json_over_uds(&socket_path, commit_create_endpoint(), &body)
                        .await?;
                print_json(&response)?;
            }
        },
        Command::Forecast { json_file } => {
            let body = json_file_body::<ForecastRequest>(&json_file)?;
            let response =
                tfk_cli::request_json_over_uds(&socket_path, forecast_endpoint(), &body).await?;
            print_json(&response)?;
        }
        Command::AdvisoryForecastSignal { command } => match command {
            AdvisoryForecastSignalCommand::List => {
                let response = tfk_cli::request_over_uds(
                    &socket_path,
                    "GET",
                    advisory_forecast_signals_endpoint(),
                    b"",
                )
                .await?;
                print_json(&response)?;
            }
            AdvisoryForecastSignalCommand::Get { id } => {
                let path = advisory_forecast_signal_get_path(&id)?;
                let response = tfk_cli::request_over_uds(&socket_path, "GET", &path, b"").await?;
                print_json(&response)?;
            }
        },
        Command::Assimilate { json_file } => {
            let body = json_file_body::<TemporalDeltaInput>(&json_file)?;
            let response =
                tfk_cli::request_json_over_uds(&socket_path, assimilate_endpoint(), &body).await?;
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

fn continuation_get_path(id: &str) -> anyhow::Result<String> {
    resource_get_path("/v1/continuations", id, "continuation id")
}

fn advisory_forecast_signal_get_path(id: &str) -> anyhow::Result<String> {
    resource_get_path(
        advisory_forecast_signals_endpoint(),
        id,
        "advisory forecast signal id",
    )
}

fn resource_get_path(base_path: &str, id: &str, label: &str) -> anyhow::Result<String> {
    if !is_safe_path_id(id) {
        bail!("invalid {label}");
    }
    Ok(format!("{base_path}/{id}"))
}

fn is_safe_path_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn preflight_request_body(
    uncertainty: f64,
    irreversibility: f64,
    externality: f64,
    option_value_loss: f64,
) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&PreflightSignals {
        uncertainty,
        irreversibility,
        externality,
        option_value_loss,
    })?)
}

fn observe_request_body(
    session: String,
    adapter: String,
    source: EventSource,
    content: String,
    act_type: Option<String>,
    evidence_status: EvidenceStatus,
    time_utc: Option<String>,
) -> anyhow::Result<Vec<u8>> {
    let mut event = RawEventInput::new_text(session, adapter, source, content);
    event.act_type = act_type;
    event.evidence_status = evidence_status;
    event.time_utc = time_utc;
    Ok(serde_json::to_vec(&event)?)
}

fn commit_request_body(
    speaker: String,
    statement: String,
    scope: Option<String>,
    deadline: Option<String>,
    revocable: bool,
) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&CommitRequest {
        speaker,
        statement,
        scope,
        deadline,
        revocable,
    })?)
}

fn relation_create_request_body(
    from_id: String,
    to_id: String,
    kind: ContinuationRelationKind,
    reason: Option<String>,
) -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&ContinuationRelationEdge {
        from_id,
        to_id,
        kind,
        reason,
    })?)
}

fn parse_relation_kind(value: &str) -> Result<ContinuationRelationKind, String> {
    match value {
        "blocks" => Ok(ContinuationRelationKind::Blocks),
        "conflicts" => Ok(ContinuationRelationKind::Conflicts),
        "supports" => Ok(ContinuationRelationKind::Supports),
        "depends_on" => Ok(ContinuationRelationKind::DependsOn),
        "subsumes" => Ok(ContinuationRelationKind::Subsumes),
        _ => Err("expected one of: blocks, conflicts, supports, depends_on, subsumes".to_string()),
    }
}

fn parse_event_source(value: &str) -> Result<EventSource, String> {
    match value {
        "user" => Ok(EventSource::User),
        "agent" => Ok(EventSource::Agent),
        "tool" => Ok(EventSource::Tool),
        "world" => Ok(EventSource::World),
        "system" => Ok(EventSource::System),
        _ => Err("expected one of: user, agent, tool, world, system".to_string()),
    }
}

fn parse_evidence_status(value: &str) -> Result<EvidenceStatus, String> {
    match value {
        "observed" => Ok(EvidenceStatus::Observed),
        "user_asserted" => Ok(EvidenceStatus::UserAsserted),
        "inferred" => Ok(EvidenceStatus::Inferred),
        "predicted" => Ok(EvidenceStatus::Predicted),
        "normative" => Ok(EvidenceStatus::Normative),
        _ => Err(
            "expected one of: observed, user_asserted, inferred, predicted, normative".to_string(),
        ),
    }
}

fn json_file_body<T>(path: &Path) -> anyhow::Result<Vec<u8>>
where
    T: DeserializeOwned + serde::Serialize,
{
    let bytes = std::fs::read(path)?;
    let request = match serde_json::from_slice::<T>(&bytes) {
        Ok(request) => request,
        Err(direct_error) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            match value.get("request") {
                Some(request_value) => serde_json::from_value(request_value.clone())?,
                None => return Err(direct_error.into()),
            }
        }
    };
    Ok(serde_json::to_vec(&request)?)
}

fn commit_create_endpoint() -> &'static str {
    "/v1/commit"
}

fn relation_endpoint() -> &'static str {
    "/v1/continuation-relations"
}

fn forecast_endpoint() -> &'static str {
    "/v1/forecast"
}

fn advisory_forecast_signals_endpoint() -> &'static str {
    "/v1/advisory-forecast-signals"
}

fn assimilate_endpoint() -> &'static str {
    "/v1/assimilate"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observe_defaults() {
        let cli = Cli::parse_from(["tfk", "observe", "hello temporal field"]);

        match cli.command {
            Command::Observe {
                session,
                adapter,
                source,
                evidence_status,
                act_type,
                time_utc,
                content,
            } => {
                assert_eq!(session, "manual");
                assert_eq!(adapter, "cli");
                assert_eq!(source, EventSource::User);
                assert_eq!(evidence_status, EvidenceStatus::Observed);
                assert_eq!(act_type, None);
                assert_eq!(time_utc, None);
                assert_eq!(content, "hello temporal field");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn observe_default_body_serializes_user_observed() {
        let body = observe_request_body(
            "manual".to_string(),
            "cli".to_string(),
            EventSource::User,
            "hello temporal field".to_string(),
            None,
            EvidenceStatus::Observed,
            None,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["session_id"], "manual");
        assert_eq!(json["adapter_id"], "cli");
        assert_eq!(json["source"], "user");
        assert_eq!(json["evidence_status"], "observed");
        assert_eq!(json["content"], "hello temporal field");
        assert!(json["act_type"].is_null());
        assert!(json["time_utc"].is_null());
    }

    #[test]
    fn parses_observe_agent_flags() {
        let cli = Cli::parse_from([
            "tfk",
            "observe",
            "--session",
            "session-agent",
            "--adapter",
            "agent-cli",
            "--source",
            "agent",
            "--evidence-status",
            "inferred",
            "--act-type",
            "tool_call",
            "--time-utc",
            "2026-05-10T00:00:00Z",
            "agent inferred a state change",
        ]);

        match cli.command {
            Command::Observe {
                session,
                adapter,
                source,
                evidence_status,
                act_type,
                time_utc,
                content,
            } => {
                assert_eq!(session, "session-agent");
                assert_eq!(adapter, "agent-cli");
                assert_eq!(source, EventSource::Agent);
                assert_eq!(evidence_status, EvidenceStatus::Inferred);
                assert_eq!(act_type.as_deref(), Some("tool_call"));
                assert_eq!(time_utc.as_deref(), Some("2026-05-10T00:00:00Z"));
                assert_eq!(content, "agent inferred a state change");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn observe_agent_body_serializes_source_evidence_act_type_and_time() {
        let body = observe_request_body(
            "session-agent".to_string(),
            "agent-cli".to_string(),
            EventSource::Agent,
            "agent inferred a state change".to_string(),
            Some("tool_call".to_string()),
            EvidenceStatus::Inferred,
            Some("2026-05-10T00:00:00Z".to_string()),
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["session_id"], "session-agent");
        assert_eq!(json["adapter_id"], "agent-cli");
        assert_eq!(json["source"], "agent");
        assert_eq!(json["evidence_status"], "inferred");
        assert_eq!(json["act_type"], "tool_call");
        assert_eq!(json["time_utc"], "2026-05-10T00:00:00Z");
        assert_eq!(json["content"], "agent inferred a state change");
    }

    #[test]
    fn observe_parsers_accept_supported_source_and_evidence_values() {
        assert_eq!(parse_event_source("user").unwrap(), EventSource::User);
        assert_eq!(parse_event_source("agent").unwrap(), EventSource::Agent);
        assert_eq!(parse_event_source("tool").unwrap(), EventSource::Tool);
        assert_eq!(parse_event_source("world").unwrap(), EventSource::World);
        assert_eq!(parse_event_source("system").unwrap(), EventSource::System);

        assert_eq!(
            parse_evidence_status("observed").unwrap(),
            EvidenceStatus::Observed
        );
        assert_eq!(
            parse_evidence_status("user_asserted").unwrap(),
            EvidenceStatus::UserAsserted
        );
        assert_eq!(
            parse_evidence_status("inferred").unwrap(),
            EvidenceStatus::Inferred
        );
        assert_eq!(
            parse_evidence_status("predicted").unwrap(),
            EvidenceStatus::Predicted
        );
        assert_eq!(
            parse_evidence_status("normative").unwrap(),
            EvidenceStatus::Normative
        );
    }

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
    fn parses_relation_create_command() {
        let cli = Cli::parse_from([
            "tfk",
            "relation",
            "create",
            "--from-id",
            "cont_a",
            "--to-id",
            "cont_b",
            "--kind",
            "depends_on",
            "--reason",
            "a needs b",
        ]);

        match cli.command {
            Command::Relation {
                command:
                    RelationCommand::Create {
                        from_id,
                        to_id,
                        kind,
                        reason,
                    },
            } => {
                assert_eq!(from_id, "cont_a");
                assert_eq!(to_id, "cont_b");
                assert_eq!(kind, ContinuationRelationKind::DependsOn);
                assert_eq!(reason.as_deref(), Some("a needs b"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn relation_create_request_body_serializes_snake_case_kind_and_reason() {
        let body = relation_create_request_body(
            "cont_a".to_string(),
            "cont_b".to_string(),
            ContinuationRelationKind::DependsOn,
            Some("a needs b".to_string()),
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["from_id"], "cont_a");
        assert_eq!(json["to_id"], "cont_b");
        assert_eq!(json["kind"], "depends_on");
        assert_eq!(json["reason"], "a needs b");
    }

    #[test]
    fn parses_relation_list_command() {
        let cli = Cli::parse_from(["tfk", "relation", "list"]);

        assert!(matches!(
            cli.command,
            Command::Relation {
                command: RelationCommand::List
            }
        ));
    }

    #[test]
    fn rejects_invalid_relation_kind() {
        let error = Cli::try_parse_from([
            "tfk",
            "relation",
            "create",
            "--from-id",
            "cont_a",
            "--to-id",
            "cont_b",
            "--kind",
            "maybe",
        ])
        .unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn parses_preflight_command() {
        let cli = Cli::parse_from([
            "tfk",
            "preflight",
            "--uncertainty",
            "0.9",
            "--irreversibility",
            "0.8",
            "--externality",
            "0.7",
            "--option-value-loss",
            "0.1",
        ]);

        match cli.command {
            Command::Preflight {
                uncertainty,
                irreversibility,
                externality,
                option_value_loss,
            } => {
                assert_eq!(uncertainty, 0.9);
                assert_eq!(irreversibility, 0.8);
                assert_eq!(externality, 0.7);
                assert_eq!(option_value_loss, 0.1);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn preflight_request_body_defaults_option_value_loss() {
        let body = preflight_request_body(0.9, 0.8, 0.7, 0.0).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["uncertainty"], 0.9);
        assert_eq!(json["irreversibility"], 0.8);
        assert_eq!(json["externality"], 0.7);
        assert_eq!(json["option_value_loss"], 0.0);
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

    #[test]
    fn parses_commit_create_command_with_explicit_revocable() {
        let cli = Cli::parse_from([
            "tfk",
            "commit",
            "create",
            "--speaker",
            "agent",
            "--statement",
            "ship PR1",
            "--scope",
            "current_project",
            "--deadline",
            "2026-05-07",
            "--revocable",
            "true",
        ]);

        match cli.command {
            Command::Commit {
                command:
                    CommitCommand::Create {
                        speaker,
                        statement,
                        scope,
                        deadline,
                        revocable,
                    },
            } => {
                assert_eq!(speaker, "agent");
                assert_eq!(statement, "ship PR1");
                assert_eq!(scope.as_deref(), Some("current_project"));
                assert_eq!(deadline.as_deref(), Some("2026-05-07"));
                assert!(revocable);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_commit_create_without_explicit_revocable() {
        let error = Cli::try_parse_from([
            "tfk",
            "commit",
            "create",
            "--speaker",
            "agent",
            "--statement",
            "ship PR1",
        ])
        .unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn commit_create_body_uses_protocol_shape() {
        let body = commit_request_body(
            "agent".to_string(),
            "ship PR1".to_string(),
            Some("current_project".to_string()),
            None,
            false,
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["speaker"], "agent");
        assert_eq!(json["statement"], "ship PR1");
        assert_eq!(json["scope"], "current_project");
        assert!(json.get("deadline").is_none());
        assert_eq!(json["revocable"], false);
    }

    #[test]
    fn parses_forecast_json_file_command() {
        let cli = Cli::parse_from(["tfk", "forecast", "--json-file", "forecast.json"]);

        match cli.command {
            Command::Forecast { json_file } => {
                assert_eq!(json_file, PathBuf::from("forecast.json"))
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_assimilate_json_file_command() {
        let cli = Cli::parse_from(["tfk", "assimilate", "--json-file", "/tmp/delta.json"]);

        match cli.command {
            Command::Assimilate { json_file } => {
                assert_eq!(json_file, PathBuf::from("/tmp/delta.json"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_advisory_forecast_signal_list_command() {
        let cli = Cli::parse_from(["tfk", "advisory-forecast-signal", "list"]);

        assert!(matches!(
            cli.command,
            Command::AdvisoryForecastSignal {
                command: AdvisoryForecastSignalCommand::List
            }
        ));
    }

    #[test]
    fn parses_advisory_forecast_signal_get_command() {
        let cli = Cli::parse_from(["tfk", "advisory-forecast-signal", "get", "sig_abc-123"]);

        match cli.command {
            Command::AdvisoryForecastSignal {
                command: AdvisoryForecastSignalCommand::Get { id },
            } => assert_eq!(id, "sig_abc-123"),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn advisory_forecast_signal_get_path_allows_safe_ascii_id() {
        let path = advisory_forecast_signal_get_path("Signal_abc-123").unwrap();

        assert_eq!(path, "/v1/advisory-forecast-signals/Signal_abc-123");
    }

    #[test]
    fn advisory_forecast_signal_get_path_rejects_request_line_and_path_injection() {
        for id in [
            "",
            "sig\r\nX-Bad: true",
            "sig\nX-Bad: true",
            "sig/../other",
            "sig%2Fother",
        ] {
            let error = advisory_forecast_signal_get_path(id).unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains("invalid advisory forecast signal id"),
                "unexpected error for {id:?}: {error}"
            );
        }
    }

    #[test]
    fn advisory_forecast_signal_endpoint_paths_are_stable() {
        assert_eq!(
            advisory_forecast_signals_endpoint(),
            "/v1/advisory-forecast-signals"
        );
        assert_eq!(
            advisory_forecast_signal_get_path("sig_123").unwrap(),
            "/v1/advisory-forecast-signals/sig_123"
        );
    }

    #[test]
    fn json_file_body_validates_naked_forecast_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forecast.json");
        std::fs::write(
            &path,
            r#"{"actions":[{"name":"a","progress":1.0,"closure":0.0,"option_value_preserved":0.0,"risk":0.0,"irreversibility":0.0,"confusion":0.0,"friction":0.0,"temporal_debt_added":0.0,"uncertainty":0.0,"externality":0.0}]}"#,
        )
        .unwrap();

        let body = json_file_body::<ForecastRequest>(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["actions"][0]["name"], "a");
        assert!(json.get("relations").is_none());
    }

    #[test]
    fn json_file_body_validates_wrapped_forecast_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forecast-fixture.json");
        std::fs::write(
            &path,
            r#"{"request":{"actions":[{"name":"wrapped","progress":1.0,"closure":0.0,"option_value_preserved":0.0,"risk":0.0,"irreversibility":0.0,"confusion":0.0,"friction":0.0,"temporal_debt_added":0.0,"uncertainty":0.0,"externality":0.0}]},"fixture":"temporalbench"}"#,
        )
        .unwrap();

        let body = json_file_body::<ForecastRequest>(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["actions"][0]["name"], "wrapped");
        assert!(json.get("request").is_none());
        assert!(json.get("fixture").is_none());
    }

    #[test]
    fn json_file_body_validates_assimilate_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delta.json");
        std::fs::write(
            &path,
            r#"{"action_id":"act_1","changes":[{"continuation_id":"cont_1","delta":"advance"}]}"#,
        )
        .unwrap();

        let body = json_file_body::<TemporalDeltaInput>(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["action_id"], "act_1");
        assert_eq!(json["changes"][0]["delta"], "advance");
        assert!(json.get("claims_made").is_none());
        assert!(json.get("evidence").is_none());
    }

    #[test]
    fn json_file_body_validates_wrapped_assimilate_request() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("delta-fixture.json");
        std::fs::write(
            &path,
            r#"{"request":{"action_id":"act_wrapped","changes":[{"continuation_id":"cont_1","delta":"close"}]},"expected_status":"closed"}"#,
        )
        .unwrap();

        let body = json_file_body::<TemporalDeltaInput>(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["action_id"], "act_wrapped");
        assert_eq!(json["changes"][0]["delta"], "close");
        assert!(json.get("request").is_none());
        assert!(json.get("expected_status").is_none());
    }

    #[test]
    fn json_file_body_prefers_naked_request_when_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forecast-with-metadata.json");
        std::fs::write(
            &path,
            r#"{"actions":[{"name":"naked","progress":1.0,"closure":0.0,"option_value_preserved":0.0,"risk":0.0,"irreversibility":0.0,"confusion":0.0,"friction":0.0,"temporal_debt_added":0.0,"uncertainty":0.0,"externality":0.0}],"request":{"metadata":"not a wrapper"}}"#,
        )
        .unwrap();

        let body = json_file_body::<ForecastRequest>(&path).unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        assert_eq!(json["actions"][0]["name"], "naked");
        assert!(json.get("request").is_none());
    }

    #[test]
    fn action_loop_endpoint_paths_are_stable() {
        assert_eq!(commit_create_endpoint(), "/v1/commit");
        assert_eq!(forecast_endpoint(), "/v1/forecast");
        assert_eq!(assimilate_endpoint(), "/v1/assimilate");
    }
}
