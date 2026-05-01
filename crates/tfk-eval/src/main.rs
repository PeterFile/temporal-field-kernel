use std::path::PathBuf;

use clap::{Parser, Subcommand};
use tfk_eval::replay_fixture;

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
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command.unwrap_or(Command::ListFixtures) {
        Command::ListFixtures => println!("fixtures/temporalbench"),
        Command::Replay { fixture, query } => {
            let summary = replay_fixture(&fixture, &query)?;
            println!("{}", serde_json::to_string(&summary)?);
        }
    }
    Ok(())
}
