use std::ops::Range;

use crate::find::{FindDirection, FindMatch, FindQuery};
use crate::grouping::DrillDown;
use crate::types::{Axis, Edge, Generation, Key, Position};
use crate::view::{ViewEdit, ViewSpec};

/// A request the Grid emits for its host or Table Source to act on. Effects
/// carry no continuations: answers come back later as ordinary inputs.
// Effects are polled one at a time, so a large View Spec costs nothing to keep unboxed.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// The user typed text while the Grid had focus. The host decides what it
    /// means: start an edit, jump to a matching row, or nothing.
    TextTyped { text: String },
    /// The user activated the cell with `row` and `col` (Enter, or a
    /// double-click): the host decides whether that opens, edits or does
    /// nothing. On a cell of a Summary Item, `drill_down` says which base
    /// rows lie under it, for the host to open, show beside or ignore.
    Activated {
        row: Key,
        col: Key,
        drill_down: Option<DrillDown>,
    },
    /// The user asked to edit the cell with `row` and `col` (F2). The host
    /// starts its own editor and calls [`crate::Grid::edit_started`], or
    /// ignores it (a read-only grid). Never sent for a Summary Item's cell.
    EditRequested { row: Key, col: Key },
    /// The Grid ended the edit of the cell with `row` and `col` itself: its
    /// Key left the Table Source, the user cancelled, a press elsewhere
    /// ended it, or the cell is a Summary Item's, which never edits. The
    /// host drops its editor without committing.
    EditCancelled { row: Key, col: Key },
    /// Focus landed on a new cell, named by its row and column Keys. Sent once
    /// both Keys are known, and at most once per drain: a later change replaces
    /// a queued one.
    FocusChanged { row: Key, col: Key },
    /// The range or the picks changed; read them from the next Snapshot. At
    /// most one is queued at a time.
    SelectionChanged,
    /// The user changed the view by `edit`, asking for the whole table to be
    /// viewed by `spec`. Answer with [`crate::Grid::view_applied`]: the spec
    /// if applied, the part of it applied, or the previous one to reject it.
    ViewRequested { spec: ViewSpec, edit: ViewEdit },
    /// A Change Notice left the Grid holding `key` without its Position.
    /// Answer with [`crate::Grid::located`].
    Locate { axis: Axis, key: Key },
    /// A resize of the item with `key` on `axis` ended at `size` (its new Size
    /// Override). Hosts that persist layouts save it and restore it with
    /// [`crate::Grid::set_size`].
    SizeChanged { axis: Axis, key: Key, size: f32 },
    /// The Keys pinned to `edge` of `axis` changed to `keys`, in pin order,
    /// dormant pins included (see [`crate::Snapshot::pins`]): by a pin, an
    /// unpin, a freeze or [`crate::Grid::set_pins`]. Hosts that persist
    /// layouts save it and restore it with `set_pins`. At most one per edge
    /// is queued: a later change replaces it.
    PinsChanged {
        axis: Axis,
        edge: Edge,
        keys: Vec<Key>,
    },
    /// The user double-clicked the border after the item with `key` on
    /// `axis`. The Grid never measures: the host measures the content and
    /// answers with [`crate::Grid::set_size`].
    AutoSizeRequested { axis: Axis, key: Key },
    /// A resize of the Header Band leading `axis` ended at `thickness`.
    /// Hosts that persist layouts save it and restore it with
    /// [`crate::Grid::set_header`].
    BandSizeChanged { axis: Axis, thickness: f32 },
    /// The user double-clicked the edge of the Header Band leading `axis`.
    /// The host measures the band's content and answers with
    /// [`crate::Grid::set_header`].
    BandAutoSizeRequested { axis: Axis },
    /// The user dropped the headers of the items with `keys` on `axis` before
    /// the item at Position `to` (the count for the end), Positions as they
    /// were before the move. The Grid reorders nothing itself: the Table
    /// Source decides, and answers with a `Reordered` Change Notice if it
    /// moves them.
    MoveRequested {
        axis: Axis,
        keys: Vec<Key>,
        to: Position,
    },
    /// The user asked to copy the selection (Ctrl/Cmd+C): the cells where
    /// the items at `rows` cross those at `cols`, each a list of runs of
    /// Positions in drawn order, as shown (a collapsed group's items are
    /// not at any Position, so they are left out). With `row_labels`, the
    /// rows are grouped and each copied row leads with its label from the
    /// row Header Band (a Group Item's value, say, without indent);
    /// `col_labels` likewise for grouped columns. The host fills the
    /// clipboard, and may copy something else instead.
    CopyRequested {
        rows: Vec<Range<Position>>,
        cols: Vec<Range<Position>>,
        row_labels: bool,
        col_labels: bool,
    },
    /// The Find Query changed to `query`, or find closed (`None`): the host
    /// fetches the marks for what it shows again, and the count.
    FindRequested { query: Option<FindQuery> },
    /// The user asked for the next or previous match of `query` after
    /// `from` (the current match, else the focused cell, else `None` for
    /// the start). Answer with [`crate::Grid::find_stepped`].
    FindStepRequested {
        query: FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    },
}

/// An Effect stamped with the generation of the Snapshot it was based on.
#[derive(Clone, Debug, PartialEq)]
pub struct Stamped {
    pub generation: Generation,
    pub effect: Effect,
}

/// A semantic input the host sends on the user's behalf, mapped from native keys
/// by the host's keymap.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Text the user typed; the Grid passes it on as `TextTyped`. In the
    /// outline column `*` opens everything under the focused group and a
    /// digit `n` opens the rows to `n` levels instead.
    TypeText(String),
    /// Activates the focused cell (Enter): emits `Activated`. In the outline
    /// column it toggles the focused group instead.
    Activate,
    /// Asks to edit the focused cell (F2): emits `EditRequested`, unless it
    /// is a Summary Item's.
    BeginEdit,
    /// Asks to copy the selection, or the focused cell without one
    /// (Ctrl/Cmd+C): emits `CopyRequested`.
    Copy,
    /// Moves focus along `axis` by one item, a page or to the edge, towards the
    /// end if `forward`, and scrolls to show the focused cell. `extend` asks
    /// for the move to stretch the selection range instead of replacing it.
    /// On rows grouped in outline form, one item back along the columns from
    /// the first column enters the outline column, where one item along the
    /// columns walks the tree: forward opens a closed group or steps in to
    /// its first child (else into the body), back closes an open group or
    /// steps out to its parent.
    MoveFocus {
        axis: Axis,
        by: FocusStep,
        forward: bool,
        extend: bool,
    },
    /// Selects every cell, keeping focus where it is.
    SelectAll,
    /// Selects every row of the range's columns, or of the focused column.
    SelectColumn,
    /// Selects every column of the range's rows, or of the focused row.
    SelectRow,
    /// Ends the gesture in progress and restores the state from before it:
    /// focus and selection, or the size of an item or band being resized.
    Cancel,
    /// Scrolls by a distance in the host's unit along each axis (a wheel or
    /// trackpad): `dx` across the columns, `dy` down the rows.
    Scroll { dx: f32, dy: f32 },
    /// Scrolls `axis` so its thumb sits at `fraction` of its travel (a
    /// scrollbar drag): 0 is the start, 1 the end.
    ScrollTo { axis: Axis, fraction: f64 },
    /// Scrolls `axis` by whole items, landing on an item start.
    ScrollBy { axis: Axis, items: i64 },
    /// Focus leaves the outline column for the body (Tab).
    LeaveOutline,
    /// Steps to the next match of the Find Query: emits `FindStepRequested`.
    FindNext,
    /// Steps to the previous match of the Find Query.
    FindPrevious,
}

/// How far a [`Action::MoveFocus`] goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusStep {
    /// To the neighbouring item.
    Item,
    /// To the item a viewport's length away (at least one item).
    Page,
    /// To the first or last item.
    Edge,
}
