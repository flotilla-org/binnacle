//! binnacle-core: the Grid, a toolkit-agnostic table view controller, and every
//! type on its boundary with hosts and Table Sources.
//!
//! The Grid never reads a cell value and holds nothing proportional to the
//! number of rows or columns. Hosts drive it through a two-phase frame:
//! [`Grid::begin_frame`] says which Window of Positions each axis needs, the
//! host answers with [`Grid::set_window`], then pulls a [`Snapshot`] to draw.
//! Input calls return nothing; anything the host must act on is queued as an
//! [`Effect`] and drained with [`Grid::poll_effect`].

mod axis;
mod effect;
mod grid;
mod measure;
mod snapshot;
mod types;

pub use effect::{Action, Effect, Stamped};
pub use grid::{AxisConfig, Grid, WindowRequests};
pub use measure::{AxisMeasure, Located, WindowChange};
pub use snapshot::{Damage, ScrollAnchor, Snapshot, StripItem, Thumb};
pub use types::{Axis, Generation, Key, Position, Viewport};
