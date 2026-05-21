//! Shared primitives for the EVE tools workspace.
//!
//! Currently holds the unified `AppError` enum and a tracing-subscriber
//! initializer. Kept deliberately thin — anything Postgres-, ESI-, or
//! SSO-specific lives in the layer-1 crates.

pub mod error;
pub mod telemetry;

pub use error::{AppError, AppResult};
