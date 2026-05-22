//! `sheets` binary — push reports to Google Sheets.
//!
//! Subcommands mirror the `report` binary. Auth is fully headless via a
//! Google service account (no browser dance), so this is safe to run
//! from cron / systemd timers. Each report writes to its own tab inside
//! the spreadsheet configured by `GOOGLE_SPREADSHEET_ID` (override per
//! invocation with `--spreadsheet-id`).
//!
//! Setup once:
//!   1. Create a service account in Google Cloud → IAM & Admin.
//!   2. Download its JSON key, point `GOOGLE_SERVICE_ACCOUNT_KEY_PATH`
//!      at the file.
//!   3. Share the target spreadsheet with the service account's
//!      `client_email` (Editor access).

use clap::{Parser, Subcommand};
use eve_core::AppResult;
use eve_core::telemetry;
use eve_sheets::auth::{AccessTokenCache, get_access_token};
use eve_sheets::push_report;
use eve_trade_hub_analyzer::analysis::output::Renderable;
use eve_trade_hub_analyzer::analysis::{seeding, stock_health, stock_health_history};
use eve_trade_hub_analyzer::{Config, db};

fn rows_of<T: Renderable>(items: &[T]) -> Vec<Vec<String>> {
    items.iter().map(|r| r.cells()).collect()
}

fn headers_for<T: Renderable>(_: &[T]) -> Vec<&'static str> {
    T::headers()
}

/// Push stock-health / seeding / stock-health-history reports to Google
/// Sheets, one tab per report.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Push the stock-health report.
    StockHealth {
        #[arg(long, default_value_t = 1000)]
        limit: i64,
        #[arg(long)]
        station: Option<i64>,
        #[arg(long)]
        spreadsheet_id: Option<String>,
        /// Tab name to write into. Created if missing.
        #[arg(long, default_value = "stock_health")]
        tab: String,
    },
    /// Push the seeding report.
    Seeding {
        #[arg(long, default_value_t = 500)]
        limit: i64,
        #[arg(long)]
        station: Option<i64>,
        #[arg(long, default_value_t = 0.0)]
        min_profit_per_day: f64,
        #[arg(long)]
        spreadsheet_id: Option<String>,
        #[arg(long, default_value = "seeding")]
        tab: String,
    },
    /// Push days-of-supply history for one type.
    StockHealthHistory {
        #[arg(long)]
        type_id: i64,
        #[arg(long, default_value_t = 30)]
        days: i64,
        #[arg(long)]
        station: Option<i64>,
        #[arg(long)]
        spreadsheet_id: Option<String>,
        #[arg(long, default_value = "stock_health_history")]
        tab: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    telemetry::init();
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let http = reqwest::Client::builder().gzip(true).build()?;

    match args.cmd {
        Cmd::StockHealth {
            limit,
            station,
            spreadsheet_id,
            tab,
        } => {
            let rows = stock_health::run(&pool, station, limit).await?;
            let id = pick_spreadsheet_id(&config, spreadsheet_id)?;
            let token = mint_token(&config, &http).await?;
            let headers = headers_for(&rows);
            push_report(&http, &token, &id, &tab, &headers, rows_of(&rows)).await?;
            println!(
                "Pushed {} stock_health row(s) to spreadsheet {id} tab '{tab}'.",
                rows.len()
            );
        }
        Cmd::Seeding {
            limit,
            station,
            min_profit_per_day,
            spreadsheet_id,
            tab,
        } => {
            let rows = seeding::run(
                &pool,
                config.jita_region_id,
                station,
                min_profit_per_day,
                limit,
            )
            .await?;
            let id = pick_spreadsheet_id(&config, spreadsheet_id)?;
            let token = mint_token(&config, &http).await?;
            let headers = headers_for(&rows);
            push_report(&http, &token, &id, &tab, &headers, rows_of(&rows)).await?;
            println!(
                "Pushed {} seeding row(s) to spreadsheet {id} tab '{tab}'.",
                rows.len()
            );
        }
        Cmd::StockHealthHistory {
            type_id,
            days,
            station,
            spreadsheet_id,
            tab,
        } => {
            let rows = stock_health_history::run(&pool, type_id, station, days).await?;
            let id = pick_spreadsheet_id(&config, spreadsheet_id)?;
            let token = mint_token(&config, &http).await?;
            let headers = headers_for(&rows);
            push_report(&http, &token, &id, &tab, &headers, rows_of(&rows)).await?;
            println!(
                "Pushed {} stock_health_history row(s) to spreadsheet {id} tab '{tab}'.",
                rows.len()
            );
        }
    }

    Ok(())
}

fn pick_spreadsheet_id(config: &Config, override_id: Option<String>) -> AppResult<String> {
    if let Some(id) = override_id {
        return Ok(id);
    }
    Ok(config.google()?.spreadsheet_id.clone())
}

async fn mint_token(config: &Config, http: &reqwest::Client) -> AppResult<String> {
    let google = config.google()?;
    let cache = AccessTokenCache::new();
    get_access_token(&cache, google, http).await
}
