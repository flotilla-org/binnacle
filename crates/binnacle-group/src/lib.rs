//! binnacle-group: the grouping layer, a sans-IO Table Source for grouped
//! axes over a narrower aggregate query interface, and the In-Memory Table
//! Source under it.
//!
//! The backend always does the arithmetic: the layer owns the structure
//! (groups into items, Keys from paths, outlines, Positions) and asks only
//! for what the items shown need, a batch per frame. Memory grows with the
//! groups open, never with the rows.

mod layer;
mod memory;
mod query;

pub use layer::{CACHE, GroupingLayer, SUMMARY_KEY_BIT, Summary, SummaryId, settle};
pub use memory::{Figure, GroupedMemory, Item, MemoryAggregates, Window};
pub use query::{
    AggregateBackend, Answer, Batch, Body, GroupRow, Path, Request, RequestId, Scope, Stamp, Want,
    having,
};
