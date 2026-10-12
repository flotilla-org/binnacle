//! Seam 2 properties: the In-Memory Table Source against a naive model of the
//! spec's sort (tuple comparison in Sort Spec order, ties in base order, nulls
//! and NaNs last in either direction), over random tables and sort sequences.

use std::borrow::Cow;
use std::ops::Range;

use binnacle_core::{Axis, Key, Located, SortColumn, SortDirection, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// Most base rows a drawn table has. Order bugs show at a handful of rows and
/// the model and `logical_order` are quadratic, so this bounds test time.
const MAX_ROWS: usize = 60;

/// One column of host data, as the host would extract it for sorting.
#[derive(Clone, Debug)]
enum Values {
    I64(Vec<Option<i64>>),
    F64(Vec<Option<f64>>),
    Bytes(Vec<Option<Vec<u8>>>),
}

impl Values {
    fn sort_keys(&self) -> SortKeys<'_> {
        match self {
            Values::I64(v) => SortKeys::I64(v.clone()),
            Values::F64(v) => SortKeys::F64(v.clone()),
            Values::Bytes(v) => {
                SortKeys::Bytes(v.iter().map(|b| b.as_deref().map(Cow::Borrowed)).collect())
            }
        }
    }

    /// Whether row `i` is null, or NaN, which sorts as if it were.
    fn missing(&self, i: usize) -> bool {
        match self {
            Values::I64(v) => v[i].is_none(),
            Values::F64(v) => v[i].is_none_or(f64::is_nan),
            Values::Bytes(v) => v[i].is_none(),
        }
    }
}

#[derive(Clone, Debug)]
struct Table {
    rows: usize,
    columns: Vec<Values>,
    /// Host-supplied row Keys, or `None` for Logical Indices.
    keys: Option<Vec<u64>>,
}

impl Table {
    fn source(&self) -> InMemoryTableSource {
        let source = InMemoryTableSource::new(self.rows, self.columns.len());
        match &self.keys {
            None => source,
            Some(keys) => source.with_keys(Axis::Rows, keys.iter().copied().map(Key).collect()),
        }
    }

    fn key(&self, logical: usize) -> Key {
        match &self.keys {
            None => Key(logical as u64),
            Some(keys) => Key(keys[logical]),
        }
    }

    fn apply(&self, source: &mut InMemoryTableSource, spec: &SortSpec) {
        let keys = spec
            .columns()
            .iter()
            .map(|c| self.columns[c.key.0 as usize].sort_keys())
            .collect();
        source.apply_sort(spec, keys);
    }

    /// The model: rows in sorted order as Logical Indices. Each value becomes
    /// a rank (class, then order within the class) and rows sort stably by
    /// their tuple of ranks, so ties keep base order.
    fn sorted(&self, spec: &SortSpec) -> Vec<usize> {
        let ranks: Vec<Vec<(u8, i128)>> = (0..self.rows)
            .map(|i| {
                spec.columns()
                    .iter()
                    .map(|c| {
                        let (class, rank) = self.rank(c.key.0 as usize, i);
                        match c.direction {
                            SortDirection::Ascending => (class, rank),
                            SortDirection::Descending => (class, -rank),
                        }
                    })
                    .collect()
            })
            .collect();
        let mut order: Vec<usize> = (0..self.rows).collect();
        order.sort_by_key(|&i| ranks[i].clone());
        order
    }

    /// Class 0 for a value, 1 for NaN, 2 for null; values rank by magnitude.
    fn rank(&self, column: usize, i: usize) -> (u8, i128) {
        match &self.columns[column] {
            Values::I64(v) => v[i].map_or((2, 0), |x| (0, x as i128)),
            Values::F64(v) => match v[i] {
                None => (2, 0),
                Some(x) if x.is_nan() => (1, 0),
                // IEEE bits reordered so integer order is total float order.
                Some(x) => {
                    let bits = x.to_bits() as i64;
                    (0, (bits ^ (((bits >> 63) as u64) >> 1) as i64) as i128)
                }
            },
            Values::Bytes(v) => match &v[i] {
                None => (2, 0),
                Some(b) => {
                    let mut distinct: Vec<&Vec<u8>> = v.iter().flatten().collect();
                    distinct.sort();
                    distinct.dedup();
                    (0, distinct.binary_search(&b).unwrap() as i128)
                }
            },
        }
    }
}

/// One entry per row, each drawn from `entry`.
fn column<T, G: Generator<T>>(rows: usize, entry: G) -> gs::VecGenerator<G, T> {
    gs::vecs(entry).min_size(rows).max_size(rows)
}

#[hegel::composite]
fn values(tc: &TestCase, rows: usize) -> Values {
    match tc.draw(gs::integers::<u8>().max_value(2)) {
        0 => Values::I64(tc.draw(column(
            rows,
            gs::optional(hegel::one_of!(
                gs::integers::<i64>().min_value(-2).max_value(2),
                gs::sampled_from(vec![i64::MIN, i64::MAX]),
                gs::integers::<i64>(),
            )),
        ))),
        1 => Values::F64(tc.draw(column(
            rows,
            gs::optional(hegel::one_of!(
                gs::sampled_from(vec![
                    0.0,
                    -0.0,
                    1.0,
                    -1.0,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NAN,
                    -f64::NAN,
                ]),
                gs::floats::<f64>(),
            )),
        ))),
        _ => Values::Bytes(tc.draw(column(
            rows,
            gs::optional(hegel::one_of!(
                gs::vecs(gs::integers::<u8>().max_value(2)).max_size(3),
                gs::binary().max_size(8),
            )),
        ))),
    }
}

#[hegel::composite]
fn table(tc: &TestCase) -> Table {
    let rows = tc.draw(gs::integers::<usize>().max_value(MAX_ROWS));
    let width = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let columns = (0..width).map(|_| tc.draw(values(rows))).collect();
    let keys = tc.draw(gs::optional(
        gs::vecs(gs::integers::<u64>())
            .min_size(rows)
            .max_size(rows)
            .unique(true),
    ));
    Table {
        rows,
        columns,
        keys,
    }
}

/// A Sort Spec over the table's columns, which may name a column twice.
#[hegel::composite]
fn spec(tc: &TestCase, width: usize) -> SortSpec {
    let column = hegel::tuples!(
        gs::integers::<u64>().max_value(width as u64 - 1),
        gs::sampled_from(vec![SortDirection::Ascending, SortDirection::Descending]),
    )
    .map(|(key, direction)| SortColumn {
        key: Key(key),
        direction,
    });
    SortSpec::new(tc.draw(gs::vecs(column).max_size(5)))
}

/// A Position range, often past the count or empty or reversed.
#[hegel::composite]
fn range(tc: &TestCase) -> Range<u64> {
    let bound = || {
        hegel::one_of!(
            gs::integers::<u64>().max_value(MAX_ROWS as u64 + 2),
            gs::integers::<u64>()
        )
    };
    tc.draw(bound())..tc.draw(bound())
}

/// Draws a table and a sequence of sorts, applies each and hands `check` the
/// table, the source and the spec just applied.
fn sorts(
    tc: &TestCase,
    mut check: impl FnMut(&TestCase, &Table, &mut InMemoryTableSource, &SortSpec),
) {
    let table = tc.draw(table());
    let mut source = table.source();
    check(tc, &table, &mut source, &SortSpec::unsorted());
    for spec in tc.draw(gs::vecs(spec(table.columns.len())).min_size(1).max_size(4)) {
        table.apply(&mut source, &spec);
        check(tc, &table, &mut source, &spec);
    }
}

#[hegel::test(test_cases = 2000)]
fn window_matches_the_model(tc: TestCase) {
    sorts(&tc, |tc, table, source, spec| {
        let expected: Vec<Key> = table
            .sorted(spec)
            .into_iter()
            .map(|i| table.key(i))
            .collect();
        assert_eq!(source.count(Axis::Rows), table.rows as u64);
        assert_eq!(source.window(Axis::Rows, 0..u64::MAX), expected);
        let range = tc.draw(range());
        let end = range.end.min(expected.len() as u64);
        let start = range.start.min(end);
        assert_eq!(
            source.window(Axis::Rows, range),
            expected[start as usize..end as usize]
        );
    });
}

#[hegel::test(test_cases = 2000)]
fn locate_agrees_with_window(tc: TestCase) {
    sorts(&tc, |tc, _, source, _| {
        let window = source.window(Axis::Rows, 0..u64::MAX);
        for (position, &key) in window.iter().enumerate() {
            assert_eq!(
                source.locate(Axis::Rows, key),
                Located::Found(position as u64)
            );
        }
        let key = Key(tc.draw(hegel::one_of!(
            gs::integers::<u64>().max_value(MAX_ROWS as u64 + 2),
            gs::integers::<u64>(),
        )));
        if !window.contains(&key) {
            assert_eq!(
                source.locate(Axis::Rows, key),
                Located::Gone { suggest: None }
            );
        }
    });
}

#[hegel::test(test_cases = 2000)]
fn nulls_and_nans_go_last_in_either_direction(tc: TestCase) {
    sorts(&tc, |_, table, source, spec| {
        let Some(first) = spec.columns().first() else {
            return;
        };
        let column = &table.columns[first.key.0 as usize];
        let missing: Vec<bool> = logical_order(table, source)
            .into_iter()
            .map(|i| column.missing(i))
            .collect();
        assert!(missing.is_sorted(), "{missing:?} for {spec:?}");
    });
}

#[hegel::test(test_cases = 2000)]
fn rows_with_equal_keys_keep_base_order(tc: TestCase) {
    sorts(&tc, |_, table, source, spec| {
        let order = logical_order(table, source);
        let equal = |a: usize, b: usize| {
            spec.columns()
                .iter()
                .all(|c| table.rank(c.key.0 as usize, a) == table.rank(c.key.0 as usize, b))
        };
        for pair in order.windows(2) {
            if equal(pair[0], pair[1]) {
                assert!(pair[0] < pair[1], "{order:?} for {spec:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 500)]
fn columns_keep_base_order_through_row_sorts(tc: TestCase) {
    sorts(&tc, |_, table, source, _| {
        let expected: Vec<Key> = (0..table.columns.len() as u64).map(Key).collect();
        assert_eq!(source.window(Axis::Cols, 0..u64::MAX), expected);
    });
}

/// The source's rows as Logical Indices, read back through the Keys.
fn logical_order(table: &Table, source: &InMemoryTableSource) -> Vec<usize> {
    source
        .window(Axis::Rows, 0..u64::MAX)
        .into_iter()
        .map(|key| (0..table.rows).find(|&i| table.key(i) == key).unwrap())
        .collect()
}
