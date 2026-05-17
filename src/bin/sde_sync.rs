use clap::Parser;
use eve_trade_hub_analyzer::{Config, telemetry};

/// Download the latest Fuzzwork SDE CSVs and load type/group/market-group tables.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Force a reload even if eve_sde_meta.version matches the upstream.
    #[arg(long, default_value_t = false)]
    force: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("sde-sync not yet implemented (Phase 1)");
    Ok(())
}
