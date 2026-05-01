use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "tfk-eval", about = "TemporalBench fixture runner")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    ListFixtures,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command.unwrap_or(Command::ListFixtures) {
        Command::ListFixtures => println!("fixtures/temporalbench"),
    }
    Ok(())
}
