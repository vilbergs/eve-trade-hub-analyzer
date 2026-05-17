use clap::{Parser, Subcommand, ValueEnum};
use eve_trade_hub_analyzer::{Config, telemetry};

/// Emit a stock-health or seeding report against the stored market data.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Per-type view of what's missing, low, or stale at the hub.
    StockHealth {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        #[arg(long)]
        group: Option<i64>,
        #[arg(long)]
        category: Option<i64>,
    },
    /// Ranked list of types to import from Jita.
    Seeding {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        #[arg(long, default_value_t = 0.0)]
        min_profit_per_day: f64,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Format {
    Table,
    Csv,
    Json,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let _args = Args::parse();
    let _config = Config::from_env()?;
    tracing::warn!("report not yet implemented (Phase 7)");
    Ok(())
}
