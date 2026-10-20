//! Seam 2 properties for sorting the columns: `MemorySource<R>` over random
//! integer records, its columns sorted by rows' values and put through row
//! sorts, value edits, wholesale replacements and column moves, against a
//! naive model (a stable sort of the columns, as moves left them, with
//! nulls last, comparing as integers, numbers or text by the columns' kinds
//! of sort key). Each op's Change Notices must say whether the columns moved.

#[allow(dead_code)]
mod support;

use std::borrow::Cow;

use binnacle_core::{Axis, ChangeNotice, Key, SortColumn, SortDirection, SortSpec, ViewSpec};
use binnacle_memory::MemorySource;
use hegel::TestCase;
use hegel::generators as gs;

/// Most columns a drawn table has.
const MAX_COLS: usize = 6;

type Record = Vec<Option<i64>>;

#[derive(Clone, Debug)]
enum Op {
    /// Sort the columns by these rows (Logical Indices, which may be past
    /// the last row) and directions.
    SortColumns(Vec<(usize, bool)>),
    /// Sort the rows by a column, which leaves the columns alone.
    SortRows(usize),
    /// Set a value as from the data (`changed`) or as the user's edit.
    Edit {
        row: usize,
        col: usize,
        value: Option<i64>,
        user: bool,
    },
    Replace(Vec<Record>),
    /// Move columns (Logical Indices) to before a Position.
    Move(Vec<usize>, u64),
}

#[hegel::composite]
fn value(tc: &TestCase) -> Option<i64> {
    tc.draw(gs::optional(
        gs::integers::<i64>().min_value(-2).max_value(2),
    ))
}

#[hegel::composite]
fn records(tc: &TestCase, width: usize) -> Vec<Record> {
    let record = gs::vecs(value()).min_size(width).max_size(width);
    tc.draw(gs::vecs(record).max_size(5))
}

#[hegel::composite]
fn op(tc: &TestCase, width: usize) -> Op {
    let index = |n: usize| gs::integers::<usize>().max_value(n);
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 | 1 => {
            Op::SortColumns(tc.draw(gs::vecs(hegel::tuples!(index(5), gs::booleans())).max_size(3)))
        }
        2 => Op::SortRows(tc.draw(index(width - 1))),
        3 => Op::Edit {
            row: tc.draw(index(5)),
            col: tc.draw(index(width - 1)),
            value: tc.draw(value()),
            user: tc.draw(gs::booleans()),
        },
        4 => Op::Replace(tc.draw(records(width))),
        _ => Op::Move(
            tc.draw(gs::vecs(index(width - 1)).max_size(3)),
            tc.draw(gs::integers::<u64>().max_value(width as u64 + 1)),
        ),
    }
}

/// The kind of sort key a column has: none, an integer (the value), a
/// float (half the value) or text (`t` and the value).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    None,
    I64,
    F64,
    Text,
}

/// What the model holds: the records, each column's kind of sort key, the
/// column order moves left (Logical Indices) and the columns' Sort Spec.
struct Model {
    rows: Vec<Record>,
    kinds: Vec<Kind>,
    base: Vec<usize>,
    spec: SortSpec,
}

impl Model {
    /// A row's sort key in column `c` as the source compares it: integers
    /// while every column with keys is an integer one, numbers while none is
    /// text (doubled, so halves stay whole), else text written with
    /// `Display`.
    fn key(&self, r: usize, c: usize) -> Option<Result<i128, String>> {
        let v = self.rows[r][c]?;
        let present = || self.kinds.iter().filter(|k| **k != Kind::None);
        let text = present().any(|k| *k == Kind::Text);
        let floats = present().any(|k| *k == Kind::F64);
        match (self.kinds[c], text, floats) {
            (Kind::None, ..) => None,
            (Kind::I64, false, false) => Some(Ok(i128::from(v))),
            (Kind::I64, false, true) => Some(Ok(2 * i128::from(v))),
            (Kind::F64, false, _) => Some(Ok(i128::from(v))),
            (Kind::I64, true, _) => Some(Err(v.to_string())),
            (Kind::F64, true, _) => Some(Err((v as f64 / 2.0).to_string())),
            (Kind::Text, ..) => Some(Err(format!("t{v}"))),
        }
    }

    /// The columns as Keys (their Logical Indices) in the model's order.
    fn columns(&self) -> Vec<Key> {
        let rank = |r: usize, c: usize| match self.key(r, c) {
            None => (2, 0),
            Some(Ok(n)) => (0, n),
            Some(Err(text)) => {
                let mut texts: Vec<String> = (0..self.kinds.len())
                    .filter_map(|c| self.key(r, c)?.err())
                    .collect();
                texts.sort();
                (0, texts.binary_search(&text).unwrap() as i128)
            }
        };
        (support::model_from(self.base.clone(), &self.spec, rank, |_| true).into_iter())
            .map(|c| Key(c as u64))
            .collect()
    }
}

/// A View Spec sorting the columns by `sort`.
fn cols_sorted(sort: SortSpec) -> ViewSpec {
    ViewSpec::default().with_sort(Axis::Cols, sort)
}

fn moved_columns(notices: &[(Axis, ChangeNotice)]) -> bool {
    notices.contains(&(Axis::Cols, ChangeNotice::Reordered))
}

#[hegel::test(test_cases = 1000)]
fn the_columns_follow_the_model_through_sorts_edits_replacements_and_moves(tc: TestCase) {
    let width = tc.draw(gs::integers::<usize>().min_value(1).max_value(MAX_COLS));
    // Mostly integers, so the integer case is common.
    let kind = gs::sampled_from(vec![
        Kind::None,
        Kind::I64,
        Kind::I64,
        Kind::F64,
        Kind::Text,
    ]);
    let kinds: Vec<Kind> = tc.draw(gs::vecs(kind).min_size(width).max_size(width));
    let rows = tc.draw(records(width));
    let mut source = MemorySource::<Record>::new(rows.len(), width);
    for (c, kind) in kinds.iter().enumerate() {
        let key = Key(c as u64);
        source = match kind {
            Kind::None => source,
            Kind::I64 => source.i64_column(key, move |r: &Record| r[c]),
            Kind::F64 => source.f64_column(key, move |r: &Record| r[c].map(|v| v as f64 / 2.0)),
            Kind::Text => source.bytes_column(key, move |r: &Record| {
                r[c].map(|v| Cow::Owned(format!("t{v}").into_bytes()))
            }),
        };
    }
    let sortable: Vec<bool> = kinds.iter().map(|k| *k != Kind::None).collect();
    let mut model = Model {
        rows,
        kinds,
        base: (0..width).collect(),
        spec: SortSpec::unsorted(),
    };
    for op in tc.draw(gs::vecs(op(width)).max_size(8)) {
        match op {
            Op::SortColumns(by) => {
                let spec = SortSpec::new(
                    (by.iter())
                        .map(|&(row, ascending)| SortColumn {
                            key: Key(row as u64),
                            direction: match ascending {
                                true => SortDirection::Ascending,
                                false => SortDirection::Descending,
                            },
                        })
                        .collect(),
                );
                let there = by.iter().all(|&(row, _)| row < model.rows.len());
                let answer = source.view(&model.rows, &cols_sorted(spec.clone()));
                assert_eq!(answer.is_ok(), there);
                if there {
                    let moved = spec != model.spec || !spec.is_empty();
                    model.spec = spec;
                    assert_eq!(moved_columns(&answer.unwrap()), moved);
                }
            }
            Op::SortRows(col) => {
                let spec = SortSpec::new(vec![SortColumn::ascending(Key(col as u64))]);
                let asked = ViewSpec::default()
                    .with_sort(Axis::Rows, spec)
                    .with_sort(Axis::Cols, model.spec.clone());
                let answer = source.view(&model.rows, &asked);
                assert_eq!(answer.is_ok(), sortable[col]);
            }
            Op::Edit {
                row,
                col,
                value,
                user,
            } => {
                if row >= model.rows.len() {
                    continue;
                }
                model.rows[row][col] = value;
                let notices = match user {
                    true => source.edited(&model.rows, row, Key(col as u64)),
                    false => source.changed(&model.rows, row, Key(col as u64)),
                };
                let by_row = model.spec.find(Key(row as u64)).is_some();
                assert_eq!(moved_columns(&notices), by_row);
            }
            Op::Replace(rows) => {
                model.rows = rows;
                let notices = source.replaced(&model.rows, None);
                assert_eq!(moved_columns(&notices), !model.spec.is_empty());
                let n = model.rows.len() as u64;
                let left = (model.spec.columns().iter())
                    .filter(|c| c.key.0 < n)
                    .copied()
                    .collect();
                model.spec = SortSpec::new(left);
            }
            Op::Move(cols, to) => {
                let keys: Vec<Key> = cols.iter().map(|&c| Key(c as u64)).collect();
                source.move_columns(&keys, to);
                let order: Vec<usize> = model.columns().iter().map(|k| k.0 as usize).collect();
                let mut moving: Vec<usize> = Vec::new();
                for c in cols {
                    if !moving.contains(&c) {
                        moving.push(c);
                    }
                }
                let to = (to as usize).min(width);
                let stays = |c: &&usize| !moving.contains(c);
                let mut base: Vec<usize> = order[..to].iter().filter(stays).copied().collect();
                base.extend(&moving);
                base.extend(order[to..].iter().filter(stays));
                model.base = base;
                model.spec = SortSpec::unsorted();
            }
        }
        let shown = source.window(Axis::Cols, 0..width as u64);
        assert_eq!(
            shown,
            model.columns(),
            "{:?} by {:?}",
            model.rows,
            model.spec
        );
        assert_eq!(source.view_spec().cols.sort, model.spec);
    }
}
