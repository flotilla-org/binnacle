//! Seam 2 properties: the In-Memory Table Source against a naive model of the
//! spec's sort (tuple comparison in Sort Spec order, ties in base order, nulls
//! and NaNs last in either direction) and filter (skip the rows the keep mask
//! drops), over random tables and sequences of sorts and filters of either
//! axis and column moves (the drop Position read in the filtered order).

mod support;

use std::borrow::Cow;
use std::ops::Range;

use binnacle_core::{Axis, ChangeNotice, Key, Located, SortColumn, SortDirection, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// Most base rows a drawn table has. Order bugs show at a handful of rows and
/// the model is quadratic, so this bounds test time.
const MAX_ROWS: usize = 60;

/// Most columns a drawn table has, of which at most four have sort keys.
const MAX_COLS: usize = 40;

/// Most rows with sort keys for the columns.
const MAX_SORTING_ROWS: usize = 4;

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
}

/// A drawn table. Sort Specs name the columns (or rows) with sort keys by
/// Logical Index, whatever Keys they have: the source never reads a spec's
/// Keys, only the sort keys handed over with it.
#[derive(Clone, Debug)]
struct Table {
    rows: usize,
    /// The sort keys of the first columns, one per row.
    columns: Vec<Values>,
    /// The sort keys of the first rows, one per column.
    by_rows: Vec<Values>,
    /// How many columns there are, `columns` and the rest.
    width: usize,
    /// Host-supplied row Keys, or `None` for Logical Indices.
    keys: Option<Vec<u64>>,
    /// Host-supplied column Keys, or `None` for Logical Indices.
    col_keys: Option<Vec<u64>>,
}

impl Table {
    fn source(&self) -> InMemoryTableSource {
        let mut source = InMemoryTableSource::new(self.rows, self.width);
        for (axis, keys) in [(Axis::Rows, &self.keys), (Axis::Cols, &self.col_keys)] {
            if let Some(keys) = keys {
                source = source.with_keys(axis, keys.iter().copied().map(Key).collect());
            }
        }
        source
    }

    /// The columns' Keys in base order.
    fn col_keys(&self) -> Vec<Key> {
        match &self.col_keys {
            None => (0..self.width as u64).map(Key).collect(),
            Some(keys) => keys.iter().copied().map(Key).collect(),
        }
    }

    fn key(&self, logical: usize) -> Key {
        match &self.keys {
            None => Key(logical as u64),
            Some(keys) => Key(keys[logical]),
        }
    }

    /// Sorts `axis` by `spec`, handing over the sort keys of the items it
    /// names.
    fn apply(&self, source: &mut InMemoryTableSource, axis: Axis, spec: &SortSpec) -> ChangeNotice {
        let by = match axis {
            Axis::Rows => &self.columns,
            Axis::Cols => &self.by_rows,
        };
        let keys = spec
            .columns()
            .iter()
            .map(|c| by[c.key.0 as usize].sort_keys())
            .collect();
        source.apply_sort(axis, spec, keys)
    }

    /// The model's rows as Logical Indices: sorted by `state.spec`, then the
    /// rows `state.keep` drops skipped.
    fn kept(&self, state: &State) -> Vec<usize> {
        let keep = |i: usize| state.keep.as_ref().is_none_or(|k| k[i]);
        support::model(
            self.rows,
            &state.spec,
            |c, i| rank(&self.columns[c], i),
            keep,
        )
    }

    /// The model's rows in sorted order as Logical Indices.
    fn sorted(&self, spec: &SortSpec) -> Vec<usize> {
        support::model(self.rows, spec, |c, i| rank(&self.columns[c], i), |_| true)
    }

    /// The model's columns as Keys, filtered out ones included: the order
    /// the moves left, sorted by `state.col_spec`.
    fn sorted_columns(&self, state: &State) -> Vec<Key> {
        let keys = self.col_keys();
        let base = (state.columns.iter())
            .map(|k| keys.iter().position(|c| c == k).unwrap())
            .collect();
        let rank = |r: usize, i: usize| rank(&self.by_rows[r], i);
        (support::model_from(base, &state.col_spec, rank, |_| true).into_iter())
            .map(|i| keys[i])
            .collect()
    }
}

/// Class 0 for a value, 1 for NaN, 2 for null; values rank by magnitude.
fn rank(values: &Values, i: usize) -> (u8, i128) {
    match values {
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
    let sortable = tc.draw(gs::integers::<usize>().max_value(4));
    let columns: Vec<Values> = (0..sortable).map(|_| tc.draw(values(rows))).collect();
    let width = sortable + tc.draw(gs::integers::<usize>().max_value(MAX_COLS - sortable));
    let sorting = tc.draw(gs::integers::<usize>().max_value(rows.min(MAX_SORTING_ROWS)));
    let by_rows = (0..sorting).map(|_| tc.draw(values(width))).collect();
    let keys = |n| {
        gs::optional(
            gs::vecs(gs::integers::<u64>())
                .min_size(n)
                .max_size(n)
                .unique(true),
        )
    };
    Table {
        rows,
        columns,
        by_rows,
        width,
        keys: tc.draw(keys(rows)),
        col_keys: tc.draw(keys(width)),
    }
}

/// A Sort Spec over the table's columns, which may name a column twice.
#[hegel::composite]
fn spec(tc: &TestCase, width: usize) -> SortSpec {
    if width == 0 {
        return SortSpec::unsorted();
    }
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

/// A column move: Keys (the table's, any others, and repeats) and a drop
/// Position, which may be past the count.
#[hegel::composite]
fn column_move(tc: &TestCase, table: Table) -> (Vec<Key>, u64) {
    let known = table.col_keys();
    let key = hegel::one_of!(
        gs::sampled_from(if known.is_empty() {
            vec![Key(0)]
        } else {
            known
        }),
        gs::integers::<u64>().map(Key),
    );
    let keys = tc.draw(gs::vecs(key).max_size(6));
    let to = tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(table.width as u64 + 1),
        gs::integers::<u64>()
    ));
    (keys, to)
}

/// The model of a column move: the moved Keys, first sight of each that is a
/// column, then the rest in order with those before `to` ahead of them.
/// `to` is a Position among the columns `shown` keeps: it stands for the
/// place in `order` of the column shown there, or the end past the last.
fn moved(order: &[Key], keys: &[Key], to: u64, shown: impl Fn(&Key) -> bool) -> Vec<Key> {
    let visible: Vec<&Key> = order.iter().filter(|k| shown(k)).collect();
    let to = visible
        .get(to as usize)
        .map_or(order.len(), |k| order.iter().position(|o| o == *k).unwrap());
    let mut moving: Vec<Key> = Vec::new();
    for k in keys {
        if order.contains(k) && !moving.contains(k) {
            moving.push(*k);
        }
    }
    let stays = |k: &&Key| !moving.contains(k);
    let mut out: Vec<Key> = order[..to].iter().filter(stays).copied().collect();
    out.extend(&moving);
    out.extend(order[to..].iter().filter(stays));
    out
}

/// What the source was last told: the rows' Sort Spec and the keep masks in
/// force (the columns' by Logical Index), the columns' Sort Spec, and the
/// column order the moves so far make before that sort, filtered out columns
/// included.
#[derive(Clone, Debug, Default)]
struct State {
    spec: SortSpec,
    keep: Option<Vec<bool>>,
    col_spec: SortSpec,
    col_keep: Option<Vec<bool>>,
    columns: Vec<Key>,
}

impl State {
    /// Whether the column filter keeps column `key` of `table`.
    fn shows(&self, table: &Table, key: &Key) -> bool {
        let logical = table.col_keys().iter().position(|k| k == key);
        self.col_keep
            .as_ref()
            .is_none_or(|keep| logical.is_some_and(|i| keep[i]))
    }

    /// The model's columns shown, in order.
    fn shown_columns(&self, table: &Table) -> Vec<Key> {
        let mut order = table.sorted_columns(self);
        order.retain(|k| self.shows(table, k));
        order
    }
}

#[derive(Clone, Debug)]
enum Op {
    Sort(SortSpec),
    /// A sort of the columns by the rows with sort keys.
    SortColumns(SortSpec),
    /// A keep mask, or `None` to clear the filter.
    Filter(Option<Vec<bool>>),
    /// A keep mask for the columns, or `None` to clear their filter.
    FilterColumns(Option<Vec<bool>>),
    /// A column move: Keys and a drop Position.
    Move(Vec<Key>, u64),
}

#[hegel::composite]
fn op(tc: &TestCase, table: Table) -> Op {
    let mask = |n| {
        gs::optional(hegel::one_of!(
            column(n, gs::booleans()),
            column(n, gs::weighted_booleans(0.9)),
            column(n, gs::weighted_booleans(0.1)),
        ))
    };
    match tc.draw(gs::integers::<u8>().max_value(4)) {
        0 => Op::Sort(tc.draw(spec(table.columns.len()))),
        3 => Op::SortColumns(tc.draw(spec(table.by_rows.len()))),
        1 => {
            let (keys, to) = tc.draw(column_move(table.clone()));
            Op::Move(keys, to)
        }
        2 => Op::FilterColumns(tc.draw(mask(table.width))),
        _ => Op::Filter(tc.draw(mask(table.rows))),
    }
}

/// Draws a table and a sequence of sorts, filters and column moves, applies
/// each and hands `check` the table, the source and the state it left (the
/// first call sees the fresh source). Asserts the Change Notice each op
/// returns: `Reordered` for a sort or move, `Refiltered` with the new count for a
/// filter of either axis. A move bakes the columns' sort into their order
/// and clears it.
fn ops(tc: &TestCase, mut check: impl FnMut(&TestCase, &Table, &mut InMemoryTableSource, &State)) {
    let table = tc.draw(table());
    let mut source = table.source();
    let mut state = State {
        columns: table.col_keys(),
        ..State::default()
    };
    check(tc, &table, &mut source, &state);
    for op in tc.draw(gs::vecs(op(table.clone())).min_size(1).max_size(5)) {
        match op {
            Op::Sort(spec) => {
                state.spec = spec;
                let notice = table.apply(&mut source, Axis::Rows, &state.spec);
                assert_eq!(notice, ChangeNotice::Reordered);
            }
            Op::SortColumns(spec) => {
                state.col_spec = spec;
                let notice = table.apply(&mut source, Axis::Cols, &state.col_spec);
                assert_eq!(notice, ChangeNotice::Reordered);
            }
            Op::Filter(keep) => {
                let notice = match &keep {
                    Some(keep) => source.apply_filter(Axis::Rows, keep.clone()),
                    None => source.clear_filter(Axis::Rows),
                };
                state.keep = keep;
                let kept = table.kept(&state).len() as u64;
                assert_eq!(notice, ChangeNotice::Refiltered(kept));
            }
            Op::FilterColumns(keep) => {
                let notice = match &keep {
                    Some(keep) => source.apply_filter(Axis::Cols, keep.clone()),
                    None => source.clear_filter(Axis::Cols),
                };
                state.col_keep = keep;
                let kept = state.shown_columns(&table).len() as u64;
                assert_eq!(notice, ChangeNotice::Refiltered(kept));
            }
            Op::Move(keys, to) => {
                let notice = source.move_columns(&keys, to);
                assert_eq!(notice, ChangeNotice::Reordered);
                let order = table.sorted_columns(&state);
                state.columns = moved(&order, &keys, to, |k| state.shows(&table, k));
                state.col_spec = SortSpec::unsorted();
            }
        }
        assert_eq!(source.sort_spec(Axis::Rows), &state.spec);
        assert_eq!(source.sort_spec(Axis::Cols), &state.col_spec);
        check(tc, &table, &mut source, &state);
    }
}

#[hegel::test(test_cases = 2000)]
fn window_matches_the_model(tc: TestCase) {
    ops(&tc, |tc, table, source, state| {
        let expected: Vec<Key> = table
            .kept(state)
            .into_iter()
            .map(|i| table.key(i))
            .collect();
        assert_eq!(source.count(Axis::Rows), expected.len() as u64);
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
    ops(&tc, |tc, table, source, _| {
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
        if !(0..table.rows).any(|i| table.key(i) == key) {
            assert_eq!(
                source.locate(Axis::Rows, key),
                Located::Gone { suggest: None }
            );
        }
    });
}

#[hegel::test(test_cases = 2000)]
fn a_filtered_out_row_is_gone_suggesting_the_nearest_survivor_in_sort_order(tc: TestCase) {
    ops(&tc, |_, table, source, state| {
        let sorted = table.sorted(&state.spec);
        let kept = table.kept(state);
        for (rank, &i) in sorted.iter().enumerate() {
            if kept.contains(&i) {
                continue;
            }
            // Brute force over every survivor: the least distance in sorted
            // order, the one after on a tie.
            let suggest = kept
                .iter()
                .enumerate()
                .min_by_key(|&(_, j)| {
                    let r = sorted.iter().position(|k| k == j).unwrap();
                    (r.abs_diff(rank), r < rank)
                })
                .map(|(position, _)| position as u64);
            assert_eq!(
                source.locate(Axis::Rows, table.key(i)),
                Located::Gone { suggest },
                "row {i} at rank {rank} of {sorted:?}, kept {kept:?}"
            );
        }
    });
}

/// The columns shown through moves of any Keys to any Position, sorts of
/// the columns and column filters, mixed with the rows' sorts and filters,
/// which leave them alone. A filtered out column is gone, suggesting the
/// nearest shown one in column order (the following one on a tie).
#[hegel::test(test_cases = 1000)]
fn columns_match_the_model(tc: TestCase) {
    ops(&tc, |tc, table, source, state| {
        let order = table.sorted_columns(state);
        let shown = state.shown_columns(table);
        let columns = shown.as_slice();
        for (rank, key) in order.iter().enumerate() {
            if state.shows(table, key) {
                continue;
            }
            let suggest = (order.iter().enumerate())
                .filter(|(_, k)| state.shows(table, k))
                .min_by_key(|&(r, _)| (r.abs_diff(rank), r < rank))
                .map(|(_, k)| columns.iter().position(|c| c == k).unwrap() as u64);
            assert_eq!(source.locate(Axis::Cols, *key), Located::Gone { suggest });
        }
        assert_eq!(source.count(Axis::Cols), columns.len() as u64);
        assert_eq!(source.window(Axis::Cols, 0..u64::MAX), columns);
        let range = tc.draw(range());
        let end = range.end.min(columns.len() as u64);
        let start = range.start.min(end);
        assert_eq!(
            source.window(Axis::Cols, range),
            columns[start as usize..end as usize]
        );
        for (position, &key) in columns.iter().enumerate() {
            assert_eq!(
                source.locate(Axis::Cols, key),
                Located::Found(position as u64)
            );
        }
        let key = Key(tc.draw(hegel::one_of!(
            gs::integers::<u64>().max_value(MAX_COLS as u64 + 2),
            gs::integers::<u64>(),
        )));
        if !state.columns.contains(&key) {
            assert_eq!(
                source.locate(Axis::Cols, key),
                Located::Gone { suggest: None }
            );
        }
    });
}
