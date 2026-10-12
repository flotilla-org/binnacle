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
