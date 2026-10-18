use crate::view::FilterSpec;

/// The checklist's question to a Table Source: the distinct values of one
/// item of the other axis (a column, for the rows), as the source writes
/// them (its own text form).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct DistinctQuery {
    /// Only values containing this text, ignoring case; all when empty. A
    /// search never lists blanks.
    pub search: String,
    /// At most this many values.
    pub limit: usize,
    /// Only values in the items this filter keeps: the applied Filter Spec
    /// less the item's own conditions. A source that can't narrow ignores
    /// it and says so.
    pub narrow: FilterSpec,
}

impl DistinctQuery {
    /// Up to `limit` values, unsearched and unnarrowed.
    pub fn new(limit: usize) -> Self {
        DistinctQuery {
            limit,
            ..DistinctQuery::default()
        }
    }

    pub fn search(mut self, search: impl Into<String>) -> Self {
        self.search = search.into();
        self
    }

    pub fn narrow(mut self, narrow: FilterSpec) -> Self {
        self.narrow = narrow;
        self
    }
}

/// One distinct value and, if the source counts them, how many items hold it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistinctValue {
    pub text: String,
    pub count: Option<u64>,
}

/// Whether some items are blank in the item listed, and how many if known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Blanks {
    Counted(u64),
    Uncounted,
}

/// A Table Source's answer to a [`DistinctQuery`]: the values in the item's
/// natural order (numbers numerically, dates chronologically), the blanks if
/// there are any, whether there were more values than the limit, and
/// whether the source narrowed them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Distinct {
    pub values: Vec<DistinctValue>,
    pub blanks: Option<Blanks>,
    pub partial: bool,
    pub narrowed: bool,
}
