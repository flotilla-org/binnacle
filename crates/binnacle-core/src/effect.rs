use crate::types::{Axis, Generation, Key, Position};

/// A request the Grid emits for its host or Table Source to act on. Effects
/// carry no continuations: answers come back later as ordinary inputs.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// The user typed text while the Grid had focus.
    TextTyped { text: String },
    /// Focus landed on a new cell, named by its row and column Keys. Sent once
    /// both Keys are known, and at most once per drain: a later change replaces
    /// a queued one.
    FocusChanged { row: Key, col: Key },
    /// The range or the picks changed; read them from the next Snapshot. At
    /// most one is queued at a time.
    SelectionChanged,
    /// A resize of the item with `key` on `axis` ended at `size` (its new Size
    /// Override). Hosts that persist layouts save it and restore it with
    /// [`crate::Grid::set_size`].
    SizeChanged { axis: Axis, key: Key, size: f32 },
    /// The user double-clicked the border after the item with `key` on
    /// `axis`. The Grid never measures: the host measures the content and
    /// answers with [`crate::Grid::set_size`].
    AutoSizeRequested { axis: Axis, key: Key },
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
    TypeText(String),
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
    /// focus and selection, or the size of an item being resized.
    Cancel,
    /// Scrolls by a distance in the host's unit along each axis (a wheel or
    /// trackpad): `dx` across the columns, `dy` down the rows.
    Scroll {
        dx: f32,
        dy: f32,
    },
    /// Scrolls `axis` so its thumb sits at `fraction` of its travel (a
    /// scrollbar drag): 0 is the start, 1 the end.
    ScrollTo {
        axis: Axis,
        fraction: f64,
    },
    /// Scrolls `axis` by whole items, landing on an item start.
    ScrollBy {
        axis: Axis,
        items: i64,
    },
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
