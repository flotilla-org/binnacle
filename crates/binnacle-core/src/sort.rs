use crate::types::{Key, Position};

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

/// The sort part of one axis of a View Spec: the ordered list of items on the
/// other axis (columns, for the rows) and directions to sort by, compared as
/// a tuple in this order. Empty means unsorted: the Table Source's base order.
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

/// How one header shows the sort of the other axis, from
/// [`crate::Snapshot::header_sort`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct HeaderSort {
    /// The item's direction and place (0 first) in the applied Sort Spec.
    pub applied: Option<(SortDirection, usize)>,
    /// A requested View Spec that would change this item's part of the sort
    /// is still unanswered.
    pub pending: bool,
}
