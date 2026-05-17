//! Seeding-opportunity report (Phase 7b).
//!
//! Built in the next phase. Module skeleton lives here so `analysis/mod.rs`
//! has both submodules.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SeedingRow {
    pub type_id: i64,
    pub type_name: String,
    pub station_id: i64,
    pub jita_p5_sell: Option<f64>,
    pub hub_p5_sell: Option<f64>,
    pub gross_margin_per_unit: Option<f64>,
    pub consumption_30d_units: i64,
    pub expected_isk_per_day: Option<f64>,
}
