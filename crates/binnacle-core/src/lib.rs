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
mod distinct;
mod drawn;
mod edit;
mod effect;
mod expand;
mod find;
mod focus;
mod gesture;
mod grid;
mod grouping;
mod measure;
mod moves;
mod notice;
mod pin;
mod pointer;
mod refilter;
mod selection;
mod snapshot;
mod sort;
mod summary;
mod types;
mod view;

pub use distinct::{Blanks, Distinct, DistinctQuery, DistinctValue};
pub use edit::EditEnd;
pub use effect::{Action, Effect, FocusStep, Stamped};
pub use find::{FindDirection, FindMatch, FindQuery};
pub use focus::FocusEnd;
pub use gesture::Mode;
pub use grid::{AxisConfig, DEFAULT_DRAG_THRESHOLD, Grid, WindowRequests};
pub use grouping::{
    Aggregate, AggregateValue, Depth, DrillDown, EXCEPT_CAP, ExceptCapped, Expand, Form,
    GRAND_TOTAL, GroupOf, GroupedAxis, Grouping, ItemKind, Level, OpenState, Outline, Placement,
};
pub use measure::{AxisMeasure, ItemAt, WindowChange};
pub use notice::ChangeNotice;
pub use pointer::{Button, Modifiers, PointerEvent, PointerKind};
pub use selection::{CellRange, HeaderClick, SelectionUnit};
pub use snapshot::{
    CellState, Cursor, Damage, DropIndicator, DropTarget, HeaderPlacement, PinZone, Rect,
    ResizeGuide, ScrollAnchor, Snapshot, StripItem, Thumb,
};
pub use sort::{HeaderSort, Located, SortColumn, SortDirection, SortSpec};
pub use types::{Axis, Edge, Generation, Key, Position, Viewport};
pub use view::{
    AxisView, Compared, Comparison, Condition, ConditionKind, ConditionState, ConditionTree,
    FilterSpec, HIDE_CAP, HeaderFilter, HideCapped, Placements, Scalar, Searched, Support,
    TextQuery, TextSupport, Totals, Value, ViewEdit, ViewSpec,
};
