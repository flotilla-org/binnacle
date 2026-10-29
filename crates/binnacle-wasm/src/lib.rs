//! binnacle-wasm: the internal bridge between the binnacle Grid and the
//! TypeScript web adapter in `web/`. It has one consumer and changes in
//! lockstep with it, so it promises no stability.
//!
//! Keys cross as exact `u64` (`BigUint64Array` in bulk, `bigint` singly);
//! counts and Positions as `f64`, capped at 2^53 - 1. The Snapshot crosses as
//! two flat arrays ([`snapshot`]); Effects as tagged objects ([`effect`]),
//! and View Specs' grouping and Window items' outlines likewise ([`grouping`]).

pub mod codes;
pub mod effect;
mod format;
mod grid;
pub mod grouping;
mod layer;
mod memory;
pub mod snapshot;
pub mod testing;

pub use format::{WebColumnFormat, fewest_decimals};
pub use grid::{WebAxis, WebGrid, debug_every_effect, debug_notice, debug_scenarios};
pub use layer::WebGroupingLayer;
pub use memory::WebMemory;

use wasm_bindgen::prelude::*;

/// The Key of a grouped axis's Grand Total, which the widget names the
/// source's Grand Total by.
#[wasm_bindgen]
pub fn grand_total_key() -> u64 {
    binnacle_core::GRAND_TOTAL.0
}

/// The field of the values pseudo-level in a pivot's outlines, as a Key.
#[wasm_bindgen]
pub fn values_field() -> u64 {
    binnacle_core::VALUES_FIELD.0
}

/// Runs when the module loads: debug builds report panics to the console.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(debug_assertions)]
    console_error_panic_hook::set_once();
}
