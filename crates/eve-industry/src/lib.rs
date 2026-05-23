//! EVE industry data primitives.
//!
//! Owns blueprint/recipe/PI schematic data from the SDE. Provides:
//!
//! - **Loader** (`sync`): downloads Fuzzwork industry CSVs and upserts
//!   into the `sde_blueprint_*` and `sde_planet_*` tables.
//! - **Recipe lookup** (`recipe_for`): given a product type_id, returns
//!   its direct inputs, outputs, and manufacturing time.
//! - **BOM expansion** (`bom_for`): recursive bill-of-materials with ME
//!   bonus and a caller-supplied "build vs buy" set.
//! - **Classification** (`classify`): maps a type_id to a node kind
//!   (raw mineral, moon material, PI, reaction, component, T1, T2, etc.)
//!   by inspecting its group/category/market-group in the SDE.

pub mod classify;
pub mod loader;
pub mod recipe;

pub use classify::{NodeKind, classify, classify_batch};
pub use loader::{IndustryReport, sync};
pub use recipe::{BomLine, BomResult, Recipe, RecipeInput, bom_for, recipe_for};
