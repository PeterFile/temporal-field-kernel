use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tfk_eval::{
    replay_action_loop_fixture, replay_fixture, replay_forecast_fixture,
    replay_lens_linked_raw_event_fixture,
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
    ActionLoop {
        #[arg(long)]
        fixture: PathBuf,
    },
    LensLinkedRawEvent {
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
        Command::ActionLoop { fixture } => {
            let summary = replay_action_loop_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
        Command::LensLinkedRawEvent { fixture } => {
            let summary = replay_lens_linked_raw_event_fixture(&fixture)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
    }
    Ok(())
}
