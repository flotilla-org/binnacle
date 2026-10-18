use crate::types::{Axis, Generation, Key, Position};
use crate::view::{ViewEdit, ViewSpec};

/// A request the Grid emits for its host or Table Source to act on. Effects
/// carry no continuations: answers come back later as ordinary inputs.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// The user typed text while the Grid had focus. The host decides what it
    /// means: start an edit, jump to a matching row, or nothing.
    TextTyped { text: String },
    /// The user activated the cell with `row` and `col` (Enter, or a
    /// double-click): the host decides whether that opens, edits or does
    /// nothing.
    Activated { row: Key, col: Key },
    /// The user asked to edit the cell with `row` and `col` (F2). The host
    /// starts its own editor and calls [`crate::Grid::edit_started`], or
    /// ignores it (a read-only grid).
    EditRequested { row: Key, col: Key },
    /// The Grid ended the edit of the cell with `row` and `col` itself: its
    /// Key left the Table Source, the user cancelled, or a press elsewhere
    /// ended it. The host drops its editor without committing.
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
    /// Text the user typed; the Grid passes it on as `TextTyped`.
    TypeText(String),
    /// Activates the focused cell (Enter): emits `Activated`.
    Activate,
    /// Asks to edit the focused cell (F2): emits `EditRequested`.
    BeginEdit,
    /// Moves focus along `axis` by one item, a page or to the edge, towards the
    /// end if `forward`, and scrolls to show the focused cell. `extend` asks
    /// for the move to stretch the selection range instead of replacing it.
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
