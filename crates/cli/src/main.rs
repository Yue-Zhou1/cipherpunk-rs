use anyhow::Result;
use clap::Parser;

use audit_agent_cli::{Cli, run_cli};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    run_cli(cli).await
}
