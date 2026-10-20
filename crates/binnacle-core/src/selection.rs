use std::ops::Range;

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

/// The one contiguous range as Positions, half-open on each axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellRange {
    pub rows: Range<Position>,
    pub cols: Range<Position>,
}

impl CellRange {
    pub fn contains(&self, row: Position, col: Position) -> bool {
        self.rows.contains(&row) && self.cols.contains(&col)
    }

    pub fn span(&self, axis: Axis) -> &Range<Position> {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
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
    pub fn range(&self, counts: [u64; 2]) -> CellRange {
        let along = |axis: Axis| {
            let i = axis.index();
            let count = counts[i];
            if self.whole[i] {
                return 0..count;
            }
            let (a, b) = (self.anchor[i].position, self.head[i].position);
            let start = a.min(b).min(count);
            let end = a.max(b).saturating_add(1).min(count);
            start..end
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
