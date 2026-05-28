use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use clap::{Parser, Subcommand};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

use eve_core::AppResult;

mod api;
mod config;
mod db;
mod ingest;
mod parser;
mod report;
mod sde;
mod state;

#[derive(Parser)]
#[command(name = "intel", about = "EVE intel chatlog analyzer")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download Fuzzwork SDE CSVs (systems + ships) into the local SQLite DB.
    SdeSync,
    /// Parse historical chatlog files into the DB.
    Backfill {
        #[arg(long)]
        channel: Option<String>,
        /// ISO date (YYYY-MM-DD); only process files modified on/after this.
        #[arg(long)]
        since: Option<String>,
    },
    /// Tail the EVE chatlog directory and ingest new lines as they arrive.
    Watch {
        #[arg(long)]
        channel: Option<String>,
    },
    /// Show systems currently considered dirty (recent sighting, no clear).
    Current {
        #[arg(long)]
        channel: Option<String>,
    },
    /// Generate a TSV report. Pipe to pbcopy and paste into Sheets.
    Report {
        #[command(subcommand)]
        kind: ReportKind,
    },
    /// List configured intel channels.
    Channels,
    /// Start the HTTP API + static-file server.
    Serve {
        #[arg(long, default_value = "127.0.0.1:3002")]
        listen: String,
    },
}

#[derive(Subcommand)]
enum ReportKind {
    /// System × hour-of-day, values = fraction of observed minutes dirty.
    Safety {
        #[arg(long)]
        channel: String,
        #[arg(long, default_value_t = 4)]
        weeks: u32,
    },
    /// Hour-of-day × weekday for the channel overall.
    Heatmap {
        #[arg(long)]
        channel: String,
        #[arg(long, default_value_t = 4)]
        weeks: u32,
    },
    /// Per-system sighting frequency.
    Systems {
        #[arg(long)]
        channel: String,
        #[arg(long, default_value_t = 4)]
        weeks: u32,
    },
    /// Pilot rap sheet.
    Pilots {
        #[arg(long)]
        channel: String,
        #[arg(long, default_value_t = 50)]
        top: u32,
    },
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    let cfg = config::Config::from_env()?;
    let pool = db::open(&cfg).await?;

    match cli.cmd {
        Cmd::SdeSync => sde::sync(&pool).await?,
        Cmd::Backfill { channel, since } => {
            ingest::backfill::run(&pool, &cfg, channel.as_deref(), since.as_deref()).await?
        }
        Cmd::Watch { channel } => ingest::watch::run(&pool, &cfg, channel.as_deref()).await?,
        Cmd::Current { channel } => report::current::run(&pool, channel.as_deref()).await?,
        Cmd::Report { kind } => match kind {
            ReportKind::Safety { channel, weeks } => {
                report::safety::run(&pool, &channel, weeks).await?
            }
            ReportKind::Heatmap { channel, weeks } => {
                report::heatmap::run(&pool, &channel, weeks).await?
            }
            ReportKind::Systems { channel, weeks } => {
                report::systems::run(&pool, &channel, weeks).await?
            }
            ReportKind::Pilots { channel, top } => {
                report::pilots::run(&pool, &channel, top).await?
            }
        },
        Cmd::Channels => report::current::list_channels(&pool).await?,
        Cmd::Serve { listen } => {
            let state: api::AppState = Arc::new(pool);
            let api_routes = api::router(state);

            let frontend_dir =
                std::env::var("FRONTEND_DIR").unwrap_or_else(|_| "./frontend/dist".to_string());

            let app = Router::new()
                .nest("/api", api_routes)
                .fallback_service(
                    ServeDir::new(&frontend_dir).append_index_html_on_directories(true),
                )
                .layer(CorsLayer::permissive());

            let addr: SocketAddr = listen
                .parse()
                .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], 3002)));

            tracing::info!("Intel API listening on {addr}");
            let listener = tokio::net::TcpListener::bind(addr).await?;
            axum::serve(listener, app).await?;
        }
    }

    Ok(())
}
