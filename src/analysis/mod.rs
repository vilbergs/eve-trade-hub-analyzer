//! Reporting layer.
//!
//! All queries read from the snapshot / rollup tables created in earlier
//! phases. Pricing math follows ADDENDUM.md §3: alongside absolute lowest
//! sell / highest buy, we compute the 5th-percentile volume-weighted
//! price using a cumulative `SUM(volume_remain) OVER (...)` window.
//! Hauling cost is intentionally not modelled — gross margin only.

pub mod output;
pub mod seeding;
pub mod stock_health;
