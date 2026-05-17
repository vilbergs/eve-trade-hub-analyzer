use clap::Parser;
use eve_trade_hub_analyzer::{Config, telemetry};

/// Roll a day's snapshots into market_daily_agg and fetch ESI history for Jita.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// The day to roll up (YYYY-MM-DD). Defaults to yesterday UTC.
    #[arg(long)]
    day: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("rollup not yet implemented (Phase 6)");
    Ok(())
}
