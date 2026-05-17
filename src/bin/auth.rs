use clap::Parser;
use eve_trade_hub_analyzer::{Config, telemetry};

/// Run the EVE SSO login flow and persist an encrypted refresh token.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Open the authorize URL in the default browser instead of just printing it.
    #[arg(long, default_value_t = false)]
    open: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("auth flow not yet implemented (Phase 3b)");
    Ok(())
}
