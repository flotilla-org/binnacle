use std::ops::Range;

use crate::axis::AxisState;
use crate::focus::FocusEnd;
use crate::types::{Axis, Key, Position};

/// What a plain click on a cell selects (the `selection` setting).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SelectionUnit {
    /// Just the cell: it is focused and nothing else is selected.
    #[default]
    Cells,
    /// The cell's whole row.
    Rows,
    /// The cell's whole column.
    Cols,
}

/// What clicking the header of an item on an axis does (the per-axis
/// `header_click` setting). Row headers default to [`HeaderClick::Select`],
/// column headers to [`HeaderClick::Sort`]. Under either, Ctrl/Cmd toggles
/// the item's pick instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HeaderClick {
    /// Cycles the other axis's sort by the item: a column header's the
    /// rows', a row header's the columns'. Shift adds the item to the sort.
    Sort,
    /// Selects the whole row or column; Shift extends.
    Select,
    /// Nothing, picks included.
    None,
}

/// The one contiguous range, as the Positions it covers on each axis. It is
/// contiguous in drawn order (see [`crate::Snapshot::drawn`]), so with items
/// pinned it may cover Positions apart from each other: each axis lists
/// half-open runs of Positions, sorted, apart and not empty. With nothing
/// pinned in it, an axis has one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellRange {
    pub rows: Vec<Range<Position>>,
    pub cols: Vec<Range<Position>>,
}

impl CellRange {
    pub fn contains(&self, row: Position, col: Position) -> bool {
        self.covers(Axis::Rows, row) && self.covers(Axis::Cols, col)
    }

    /// Whether the range covers the item at `p` on `axis`.
    pub fn covers(&self, axis: Axis, p: Position) -> bool {
        let runs = self.span(axis);
        let i = runs.partition_point(|r| r.end <= p);
        runs.get(i).is_some_and(|r| r.contains(&p))
    }

    /// The runs of Positions the range covers on `axis`.
    pub fn span(&self, axis: Axis) -> &[Range<Position>] {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }

    /// How many items the range covers on `axis`.
    pub fn len(&self, axis: Axis) -> u64 {
        self.span(axis).iter().map(|r| r.end - r.start).sum()
    }
}

/// A cell the Grid tracks: one end per axis, rows first.
pub(crate) type Cell = [FocusEnd; 2];

/// The one contiguous range: from its anchor corner (where an extending
/// gesture started) to its head corner, by Position with Keys at each end.
/// An axis marked `whole` spans every item on it whatever the corners say,
/// which keeps a whole-row or whole-column selection whole as counts change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub anchor: Cell,
    pub head: Cell,
    pub whole: [bool; 2],
}

impl Span {
    /// The Positions the range covers: on each axis every item drawn from
    /// one corner to the other, in drawn order, or every item where the
    /// axis is whole.
    pub(crate) fn range(&self, axes: &[AxisState; 2]) -> CellRange {
        let along = |axis: Axis| {
            let (i, st) = (axis.index(), &axes[axis.index()]);
            let count = st.measure.count();
            if self.whole[i] {
                return std::iter::once(0..count)
                    .filter(|r| !r.is_empty())
                    .collect();
            }
            let (a, b) = (self.anchor[i], self.head[i]);
            if count == 0 || a.position.min(b.position) >= count {
                return Vec::new();
            }
            let (a, b) = (st.drawn_index(st.slot_of(a)), st.drawn_index(st.slot_of(b)));
            st.drawn_runs(a.min(b)..a.max(b) + 1)
        };
        CellRange {
            rows: along(Axis::Rows),
            cols: along(Axis::Cols),
        }
    }
}

/// v1 selection: the one contiguous range and the picked Keys of each axis
/// (sorted, so lookups are a binary search). Focus is kept beside it in the
/// Grid.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Selection {
    pub span: Option<Span>,
    pub picks: [Vec<Key>; 2],
}

impl Selection {
    pub fn picked(&self, axis: Axis, key: Key) -> bool {
        self.picks[axis.index()].binary_search(&key).is_ok()
    }

    pub fn toggle_pick(&mut self, axis: Axis, key: Key) {
        let picks = &mut self.picks[axis.index()];
        match picks.binary_search(&key) {
            Ok(i) => {
                picks.remove(i);
            }
            Err(i) => picks.insert(i, key),
        }
    }

    /// Drops `key` from the picks of `axis`, if it is there.
    pub fn unpick(&mut self, axis: Axis, key: Key) {
        self.picks[axis.index()].retain(|&k| k != key);
    }
}
