use clap::Parser;
use eve_core::telemetry;
use eve_trade_hub_analyzer::{Config, db};

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
    let args = Args::parse();
    let config = Config::from_env()?;
    let pool = db::build_pool(&config).await?;
    let http = reqwest::Client::builder()
        .user_agent(&config.eve_user_agent)
        .gzip(true)
        .build()?;

    if args.force {
        // Clear the version row so sync proceeds even on an unchanged upstream.
        sqlx::query("DELETE FROM eve_sde_meta WHERE id = 1")
            .execute(&pool)
            .await?;
    }

    match eve_sde::sync(&pool, &http).await? {
        eve_sde::SdeReport::UpToDate { version } => {
            tracing::info!(%version, "SDE up to date");
        }
        eve_sde::SdeReport::Loaded {
            version,
            categories,
            groups,
            market_groups,
            types,
        } => {
            tracing::info!(
                %version,
                categories,
                groups,
                market_groups,
                types,
                "SDE loaded"
            );
        }
    }

    Ok(())
}
