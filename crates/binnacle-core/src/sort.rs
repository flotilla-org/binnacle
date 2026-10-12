use std::collections::BTreeSet;

use crate::effect::Effect;
use crate::grid::Grid;
use crate::pointer::Modifiers;
use crate::types::{Axis, Key, Position};

/// Which way one column of a Sort Spec orders its values. Nulls go last either
/// way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One column of a Sort Spec: the column's Key and its direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SortColumn {
    pub key: Key,
    pub direction: SortDirection,
}

impl SortColumn {
    pub fn ascending(key: Key) -> Self {
        SortColumn {
            key,
            direction: SortDirection::Ascending,
        }
    }

    pub fn descending(key: Key) -> Self {
        SortColumn {
            key,
            direction: SortDirection::Descending,
        }
    }
}

/// The ordered list of columns and directions to sort by, compared as a tuple
/// in this order. Empty means unsorted: the Table Source's base order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SortSpec {
    columns: Vec<SortColumn>,
}

impl SortSpec {
    pub fn new(columns: Vec<SortColumn>) -> Self {
        SortSpec { columns }
    }

    pub fn unsorted() -> Self {
        SortSpec::default()
    }

    pub fn columns(&self) -> &[SortColumn] {
        &self.columns
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
}

impl From<Vec<SortColumn>> for SortSpec {
    fn from(columns: Vec<SortColumn>) -> Self {
        SortSpec::new(columns)
    }
}

/// A Table Source's answer to `Locate`: where a Key sits now, or that it has
/// left the axis, optionally suggesting a Position for focus to move to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Located {
    Found(Position),
    Gone { suggest: Option<Position> },
}

impl SortSpec {
    /// Where `key` sits in the spec and its direction.
    pub fn find(&self, key: Key) -> Option<(usize, SortDirection)> {
        self.columns
            .iter()
            .position(|c| c.key == key)
            .map(|i| (i, self.columns[i].direction))
    }

    /// The spec a click on column `key`'s header asks for: ascending, then
    /// descending, then unsorted. A plain click sorts by that column alone; a
    /// `multi` (Shift) click cycles it in place and adds it last if absent.
    pub(crate) fn cycled(&self, key: Key, multi: bool) -> SortSpec {
        let next = match self.find(key).map(|(_, d)| d) {
            None => Some(SortColumn::ascending(key)),
            Some(SortDirection::Ascending) => Some(SortColumn::descending(key)),
            Some(SortDirection::Descending) => None,
        };
        if !multi {
            return SortSpec::new(next.into_iter().collect());
        }
        let mut columns = self.columns.clone();
        match (self.find(key), next) {
            (Some((i, _)), Some(c)) => columns[i] = c,
            (Some((i, _)), None) => {
                columns.remove(i);
            }
            (None, Some(c)) => columns.push(c),
            (None, None) => {}
        }
        SortSpec::new(columns)
    }
}

/// How one column header shows the sort, from [`crate::Snapshot::header_sort`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HeaderSort {
    /// The column's direction and place (0 first) in the applied Sort Spec.
    pub applied: Option<(SortDirection, usize)>,
    /// A requested Sort Spec that would change this column's part is still
    /// unanswered.
    pub pending: bool,
}

/// The Grid's sort header state: the Sort Spec the Table Source last applied
/// and the one the user last asked for while answers are still due.
#[derive(Clone, Debug, Default)]
pub(crate) struct SortState {
    pub applied: SortSpec,
    /// The latest request, kept while any `sort_applied` is still due.
    pub requested: Option<SortSpec>,
    due: u32,
    /// Columns the Table Source cannot sort by (sparse, set by the host).
    unsortable: BTreeSet<Key>,
}

impl SortState {
    pub fn header(&self, key: Key) -> HeaderSort {
        let applied = self.applied.find(key).map(|(i, d)| (d, i));
        let pending = self
            .requested
            .as_ref()
            .is_some_and(|r| r.find(key).map(|(i, d)| (d, i)) != applied);
        HeaderSort { applied, pending }
    }
}

impl Grid {
    /// Whether clicking column `key`'s header may request a sort by it.
    /// Columns are sortable until set otherwise.
    pub fn set_sortable(&mut self, key: Key, sortable: bool) {
        if sortable {
            self.sort.unsortable.remove(&key);
        } else {
            self.sort.unsortable.insert(key);
        }
    }

    /// The Table Source's answer to a `SortRequested`: the Sort Spec it now
    /// applies, which is the previous one if it rejected the request. Headers
    /// show the applied spec; the pending flag clears once every request has
    /// had its answer.
    pub fn sort_applied(&mut self, spec: SortSpec) {
        self.sort.applied = spec;
        self.sort.due = self.sort.due.saturating_sub(1);
        if self.sort.due == 0 {
            self.sort.requested = None;
        }
        self.pending_damage.selection = true;
    }

    /// A Sort header click on column header `position`: requests the next
    /// spec in the cycle from the latest request (or the applied spec).
    pub(crate) fn sort_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        if axis != Axis::Cols {
            return;
        }
        let Some(key) = self.axis(axis).measure.key_at(position) else {
            return;
        };
        if self.sort.unsortable.contains(&key) {
            return;
        }
        let from = self.sort.requested.as_ref().unwrap_or(&self.sort.applied);
        let spec = from.cycled(key, modifiers.shift);
        self.sort.requested = Some(spec.clone());
        self.sort.due += 1;
        self.pending_damage.selection = true;
        self.push(Effect::SortRequested { spec });
    }
}
