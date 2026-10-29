use std::borrow::Cow;
use std::cmp::Ordering;

use binnacle_core::SortDirection;

/// One column's sort keys over every base row, extracted by the host. `None`
/// is a null.
///
/// Each column sorts by one total order: nulls are lowest (first ascending,
/// last descending); `I64` and `U64` compare exactly; `F64` uses the IEEE
/// total order with every NaN equal to the others and greater than every
/// number; byte strings compare lexicographically by bytes, which for UTF-8
/// text is code-point order. Descending reverses the whole order, nulls
/// included. Ties between rows are broken by the table on the item's Key.
///
/// `Typed` is a column of Typed Values, which may hold invalid values: see
/// [`TypedKey`] for where they sort.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum SortKeys<'a> {
    I64(Vec<Option<i64>>),
    U64(Vec<Option<u64>>),
    F64(Vec<Option<f64>>),
    Bytes(Vec<Option<Cow<'a, [u8]>>>),
    Typed(Vec<Option<TypedKey<'a>>>),
}

/// One row's sort key in a column of Typed Values. An `Invalid` value (what
/// the source could not read as the column's kind) sorts above nulls and
/// below every typed value, and invalid values compare by their text, by
/// code point. Typed keys compare as the other [`SortKeys`] do: integers
/// exactly, signed or not, floats with NaN the greatest, instants in time
/// order and bytes lexicographically. Integers and floats compare with each
/// other exactly, a float before an integer of the same value (so `-0.0`,
/// `0.0` then `0`). Keys of other kinds order numbers first, then instants,
/// then bytes.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum TypedKey<'a> {
    Invalid(Cow<'a, str>),
    I64(i64),
    U64(u64),
    F64(f64),
    /// Nanoseconds since the Unix epoch, UTC.
    Instant(i128),
    Bytes(Cow<'a, [u8]>),
}

impl TypedKey<'_> {
    /// The key's tier and, among typed keys, its kind: invalid lowest.
    fn rank(&self) -> u8 {
        match self {
            TypedKey::Invalid(_) => 0,
            TypedKey::I64(_) | TypedKey::U64(_) | TypedKey::F64(_) => 1,
            TypedKey::Instant(_) => 2,
            TypedKey::Bytes(_) => 3,
        }
    }

    /// An integer key as an `i128`, exactly.
    fn integer(&self) -> Option<i128> {
        match self {
            TypedKey::I64(v) => Some(i128::from(*v)),
            TypedKey::U64(v) => Some(i128::from(*v)),
            _ => None,
        }
    }
}

/// An integer against a float, exactly: NaN is the greatest, and a float
/// sorts before an integer of the same value.
fn integer_against_float(i: i128, x: f64) -> Ordering {
    if x.is_nan() {
        return Ordering::Less;
    }
    let floor = x.floor();
    // Every i128 lies in [-2^127, 2^127).
    if floor >= 2f64.powi(127) {
        return Ordering::Less;
    }
    if floor < -(2f64.powi(127)) {
        return Ordering::Greater;
    }
    match i.cmp(&(floor as i128)) {
        Ordering::Equal if x > floor => Ordering::Less,
        Ordering::Equal => Ordering::Greater,
        unequal => unequal,
    }
}

impl Ord for TypedKey<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        let by_rank = self.rank().cmp(&other.rank());
        if by_rank.is_ne() {
            return by_rank;
        }
        match (self, other) {
            (TypedKey::Invalid(a), TypedKey::Invalid(b)) => a.cmp(b),
            (TypedKey::Instant(a), TypedKey::Instant(b)) => a.cmp(b),
            (TypedKey::Bytes(a), TypedKey::Bytes(b)) => a.cmp(b),
            (TypedKey::F64(a), TypedKey::F64(b)) => compare_f64(*a, *b),
            (TypedKey::F64(x), b) => integer_against_float(b.integer().unwrap_or(0), *x).reverse(),
            (a, TypedKey::F64(x)) => integer_against_float(a.integer().unwrap_or(0), *x),
            (a, b) => a.integer().cmp(&b.integer()),
        }
    }
}

impl PartialOrd for TypedKey<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TypedKey<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for TypedKey<'_> {}

impl SortKeys<'_> {
    pub fn len(&self) -> usize {
        match self {
            SortKeys::I64(v) => v.len(),
            SortKeys::U64(v) => v.len(),
            SortKeys::F64(v) => v.len(),
            SortKeys::Bytes(v) => v.len(),
            SortKeys::Typed(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Compares the keys of base rows `a` and `b`.
    pub(crate) fn compare(&self, a: usize, b: usize, direction: SortDirection) -> Ordering {
        let ascending = match self {
            SortKeys::I64(v) => v[a].cmp(&v[b]),
            SortKeys::U64(v) => v[a].cmp(&v[b]),
            SortKeys::F64(v) => v[a].map(F64Key).cmp(&v[b].map(F64Key)),
            SortKeys::Bytes(v) => v[a].cmp(&v[b]),
            SortKeys::Typed(v) => v[a].cmp(&v[b]),
        };
        match direction {
            SortDirection::Ascending => ascending,
            SortDirection::Descending => ascending.reverse(),
        }
    }
}

/// A float in the sort order: IEEE total order, but every NaN equal.
#[derive(Clone, Copy)]
struct F64Key(f64);

impl PartialEq for F64Key {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for F64Key {}

impl PartialOrd for F64Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for F64Key {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_f64(self.0, other.0)
    }
}

/// Floats in the sort order: IEEE total order, but every NaN equal and
/// greater than every number.
pub fn compare_f64(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.total_cmp(&b),
    }
}
