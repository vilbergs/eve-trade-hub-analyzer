use clap::{Parser, Subcommand};
use eve_trade_hub_analyzer::analysis::output::{Format, render};
use eve_trade_hub_analyzer::analysis::{seeding, stock_health, stock_health_history};
use eve_trade_hub_analyzer::{Config, db, telemetry};

/// Emit a stock-health or seeding report against the stored market data.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Per (station, type) view of what's missing, low, or stale.
    StockHealth {
        #[arg(long, default_value_t = 100)]
        limit: i64,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Restrict to a single station_id; otherwise group across all
        /// tracked stations.
        #[arg(long)]
        station: Option<i64>,
    },
    /// Ranked list of types to import from Jita (Phase 7b).
    Seeding {
        #[arg(long, default_value_t = 50)]
        limit: i64,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        #[arg(long)]
        station: Option<i64>,
        #[arg(long, default_value_t = 0.0)]
        min_profit_per_day: f64,
    },
    /// Days-of-supply history for a single type over time.
    StockHealthHistory {
        /// The type_id to query.
        #[arg(long)]
        type_id: i64,
        /// How many days of history to show (default 30).
        #[arg(long, default_value_t = 30)]
        days: i64,
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
        /// Restrict to a single station_id.
        #[arg(long)]
        station: Option<i64>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let mut out = std::io::stdout().lock();

    match args.cmd {
        Cmd::StockHealth {
            limit,
            format,
            station,
        } => {
            let rows = stock_health::run(&pool, station, limit).await?;
            render(&rows, format, &mut out)?;
        }
        Cmd::Seeding {
            limit,
            format,
            station,
            min_profit_per_day,
        } => {
            let rows = seeding::run(
                &pool,
                config.jita_region_id,
                station,
                min_profit_per_day,
                limit,
            )
            .await?;
            render(&rows, format, &mut out)?;
        }
        Cmd::StockHealthHistory {
            type_id,
            days,
            format,
            station,
        } => {
            let rows = stock_health_history::run(&pool, type_id, station, days).await?;
            render(&rows, format, &mut out)?;
        }
    }
    Ok(())
}
