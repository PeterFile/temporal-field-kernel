use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "tfk-mcp", about = "Temporal Field Kernel MCP thin wrapper")]
struct Cli {
    /// Path to tfkd Unix domain socket. The wrapper currently only validates CLI shape.
    #[arg(long, default_value = "auto")]
    socket: String,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    println!("tfk-mcp scaffold: socket={}", cli.socket);
    Ok(())
}
