//! Test support for binnacle (not published).
//!
//! [`Scenario`] drives a real Grid through the frame loop against a Table Source
//! and renders each Snapshot as text with [`render`], for `insta` snapshots and
//! Hegel properties alike. See `decisions.md` in the build notes for usage.

mod render;
mod scenario;

pub use binnacle_memory::{PumpSource, Transposed};
pub use render::render;
pub use scenario::{
    ColumnMajorTable, IdentityTable, KeyedTable, Record, RecordTable, Scenario, ScenarioSource,
};
