use clap::Parser;
use eve_trade_hub_analyzer::{Config, telemetry};

/// List Upwell structures the linked character can dock at.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("discover not yet implemented (Phase 4)");
    Ok(())
}
