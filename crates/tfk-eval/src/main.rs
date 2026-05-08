use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tfk_eval::{
    replay_action_loop_fixture, replay_commitment_forecast_fixture, replay_fixture,
    replay_forecast_fixture, replay_lens_advisory_signal_fixture,
    replay_lens_linked_raw_event_fixture, replay_relation_boundary_fixture,
    replay_relation_ranking_fixture, replay_rules_lens_influence_fixture,
    replay_semantic_lens_influence_fixture,
};

#[derive(Debug, Parser)]
#[command(name = "tfk-eval", about = "TemporalBench fixture runner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    ListFixtures,
    Replay {
        #[arg(long)]
        fixture: PathBuf,
        #[arg(long)]
        query: String,
    },
    Forecast {
        #[arg(long)]
        fixture: PathBuf,
    },
    CommitmentForecast {
        #[arg(long)]
        fixture: PathBuf,
    },
    ActionLoop {
        #[arg(long)]
        fixture: PathBuf,
    },
    LensLinkedRawEvent {
        #[arg(long)]
        fixture: PathBuf,
    },
    LensAdvisorySignal {
        #[arg(long)]
        fixture: PathBuf,
    },
    RelationBoundary {
        #[arg(long)]
        fixture: PathBuf,
    },
    RelationRanking {
        #[arg(long)]
        fixture: PathBuf,
    },
    SemanticLensInfluence {
        #[arg(long)]
        fixture: PathBuf,
    },
    RulesLensInfluence {
        #[arg(long)]
        fixture: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command.unwrap_or(Command::ListFixtures) {
        Command::ListFixtures => println!("fixtures/temporalbench"),
        Command::Replay { fixture, query } => {
            let summary = replay_fixture(&fixture, &query)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::Forecast { fixture } => {
            let summary = replay_forecast_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::CommitmentForecast { fixture } => {
            let summary = replay_commitment_forecast_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::ActionLoop { fixture } => {
            let summary = replay_action_loop_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::LensLinkedRawEvent { fixture } => {
            let summary = replay_lens_linked_raw_event_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::LensAdvisorySignal { fixture } => {
            let summary = replay_lens_advisory_signal_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::RelationBoundary { fixture } => {
            let summary = replay_relation_boundary_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::RelationRanking { fixture } => {
            let summary = replay_relation_ranking_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::SemanticLensInfluence { fixture } => {
            let summary = replay_semantic_lens_influence_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::RulesLensInfluence { fixture } => {
            let summary = replay_rules_lens_influence_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
    }
    Ok(())
}
