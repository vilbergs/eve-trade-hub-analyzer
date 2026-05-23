use clap::Parser;
use eve_core::telemetry;
use eve_trade_hub_analyzer::{Config, db};

/// Download the latest Fuzzwork SDE CSVs and load type/group/market-group
/// tables + industry blueprint/PI data.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Force a reload even if the stored version matches upstream.
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
        sqlx::query("DELETE FROM eve_sde_meta WHERE id = 1")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM eve_industry_meta WHERE id = 1")
            .execute(&pool)
            .await
            .ok(); // table may not exist yet on first run
    }

    // ── Base SDE (types, groups, categories, market groups) ──────────────
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

    // ── Industry data (blueprints, materials, products, PI) ─────────────
    match eve_industry::sync(&pool, &http).await? {
        eve_industry::IndustryReport::UpToDate { version } => {
            tracing::info!(%version, "industry data up to date");
        }
        eve_industry::IndustryReport::Loaded {
            version,
            blueprints,
            activities,
            materials,
            products,
            pi_schematics,
            pi_types,
        } => {
            tracing::info!(
                %version,
                blueprints,
                activities,
                materials,
                products,
                pi_schematics,
                pi_types,
                "industry data loaded"
            );
        }
    }

    Ok(())
}
