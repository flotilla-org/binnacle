use std::borrow::Cow;
use std::cmp::Ordering;

use binnacle_core::SortDirection;

/// One column's sort keys over every base row, extracted by the host. `None`
/// is a null.
///
/// `I64` and byte strings compare naturally (bytes lexicographically); `F64`
/// uses the IEEE total order with every NaN after every number. Nulls and NaNs
/// go last in either direction.
#[derive(Clone, Debug, PartialEq)]
pub enum SortKeys<'a> {
    I64(Vec<Option<i64>>),
    F64(Vec<Option<f64>>),
    Bytes(Vec<Option<Cow<'a, [u8]>>>),
}

impl SortKeys<'_> {
    pub fn len(&self) -> usize {
        match self {
            SortKeys::I64(v) => v.len(),
            SortKeys::F64(v) => v.len(),
            SortKeys::Bytes(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Compares the keys of base rows `a` and `b`.
    pub(crate) fn compare(&self, a: usize, b: usize, direction: SortDirection) -> Ordering {
        match self {
            SortKeys::I64(v) => last_if_none(&v[a], &v[b], |x, y| directed(x.cmp(y), direction)),
            SortKeys::F64(v) => last_if_none(&v[a], &v[b], |x, y| {
                let x = (!x.is_nan()).then_some(x);
                let y = (!y.is_nan()).then_some(y);
                last_if_none(&x, &y, |x, y| directed(x.total_cmp(y), direction))
            }),
            SortKeys::Bytes(v) => last_if_none(&v[a], &v[b], |x, y| directed(x.cmp(y), direction)),
        }
    }
}

fn directed(ordering: Ordering, direction: SortDirection) -> Ordering {
    match direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

/// Orders `None` after every `Some`, whichever way `compare` runs.
fn last_if_none<T>(a: &Option<T>, b: &Option<T>, compare: impl Fn(&T, &T) -> Ordering) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => compare(a, b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}
