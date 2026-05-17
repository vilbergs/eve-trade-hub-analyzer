use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

/// Initialize a global tracing subscriber.
///
/// Uses pretty formatting when stdout is a TTY, JSON otherwise.
/// Reads filter from `RUST_LOG`, defaulting to `info`.
pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(filter);

    if std::io::stdout().is_terminal() {
        registry.with(fmt::layer().pretty()).init();
    } else {
        registry.with(fmt::layer().json()).init();
    }
}
