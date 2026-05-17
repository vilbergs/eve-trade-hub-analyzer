use clap::Parser;
use eve_trade_hub_analyzer::{Config, telemetry};

/// Long-running daemon: snapshot hub + Jita market orders on an interval.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Run a single poll cycle and exit (useful for testing).
    #[arg(long, default_value_t = false)]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("poll daemon not yet implemented (Phase 5)");
    Ok(())
}
