//! Seam 2 properties for base data changes: `MemorySource<R>` over random
//! integer records, put through sorts, filters, appends, value edits and
//! wholesale replacements, against a naive model (stable sort with nulls
//! last, then skip what the filter drops). Each Change Notice must describe
//! what happened to the visible order.

use binnacle_core::{Axis, ChangeNotice, Key, SortColumn, SortSpec};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const COLS: usize = 2;

type Record = [Option<i64>; COLS];

/// Keeps records whose value in `col` is at least `min` (nulls never).
#[derive(Clone, Copy, Debug)]
struct Keep {
    col: usize,
    min: i64,
}

impl Keep {
    fn keeps(self, r: &Record) -> bool {
        r[self.col].is_some_and(|v| v >= self.min)
    }
}

#[derive(Clone, Debug)]
enum Op {
    Sort(SortSpec),
    Filter(Option<Keep>),
    Append(Vec<Record>),
    /// Sets base row `row % len` in column `col`.
    Edit {
        row: usize,
        col: usize,
        value: Option<i64>,
    },
    Replace(Vec<Record>),
}

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    // Few distinct values and some nulls, so sorts tie and filters bite.
    let value = || gs::optional(gs::integers::<i64>().min_value(-3).max_value(3));
    [tc.draw(value()), tc.draw(value())]
}

#[hegel::composite]
fn column(tc: &TestCase) -> usize {
    tc.draw(gs::integers::<usize>().max_value(COLS - 1))
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let spec = gs::vecs(hegel::tuples!(column(), gs::booleans()))
        .max_size(2)
        .map(|cols| {
            let mut spec: Vec<SortColumn> = Vec::new();
            for (c, descending) in cols {
                let key = Key(c as u64);
                if spec.iter().all(|s| s.key != key) {
                    spec.push(if descending {
                        SortColumn::descending(key)
                    } else {
                        SortColumn::ascending(key)
                    });
                }
            }
            SortSpec::new(spec)
        });
    let keep = hegel::tuples!(column(), gs::integers::<i64>().min_value(-3).max_value(3))
        .map(|(col, min)| Keep { col, min });
    let edit = || {
        hegel::tuples!(
            gs::integers::<usize>(),
            column(),
            gs::optional(gs::integers::<i64>().min_value(-3).max_value(3))
        )
        .map(|(row, col, value)| Op::Edit { row, col, value })
    };
    tc.draw(hegel::one_of!(
        spec.map(Op::Sort),
        gs::optional(keep).map(Op::Filter),
        gs::vecs(record()).min_size(1).max_size(4).map(Op::Append),
        edit(),
        edit(),
        gs::vecs(record()).max_size(12).map(Op::Replace),
    ))
}

/// The model's visible order as Logical Indices.
fn model(rows: &[Record], spec: &SortSpec, keep: Option<Keep>) -> Vec<u64> {
    let rank = |r: &Record| -> Vec<(bool, i64)> {
        spec.columns()
            .iter()
            .map(|c| {
                let v = r[c.key.0 as usize];
                let v = match c.direction {
                    binnacle_core::SortDirection::Ascending => v,
                    binnacle_core::SortDirection::Descending => v.map(|x| -x),
                };
                (v.is_none(), v.unwrap_or(0))
            })
            .collect()
    };
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by_key(|&i| rank(&rows[i]));
    order
        .into_iter()
        .filter(|&i| keep.is_none_or(|k| k.keeps(&rows[i])))
        .map(|i| i as u64)
        .collect()
}

/// The visible order as Logical Indices, mapping host Keys back if given.
fn visible(source: &MemorySource<Record>, keys: Option<&[Key]>) -> Vec<u64> {
    let count = source.count(Axis::Rows);
    source
        .window(Axis::Rows, 0..count)
        .into_iter()
        .map(|k| match keys {
            None => k.0,
            Some(keys) => keys.iter().position(|&h| h == k).unwrap() as u64,
        })
        .collect()
}

/// What one op did.
struct Step<'a> {
    op: &'a Op,
    rows: &'a [Record],
    ordered: bool,
    notice: Option<ChangeNotice>,
    /// The visible order before and after, as Logical Indices.
    before: Vec<u64>,
    after: Vec<u64>,
    model: Vec<u64>,
}

/// Draws records (keyed by Logical Index or by host Keys) and a sequence of
/// ops; `check` sees every step.
fn ops(tc: &TestCase, mut check: impl FnMut(&Step)) {
    let mut rows = tc.draw(gs::vecs(record()).max_size(12));
    let keyed = tc.draw(gs::booleans());
    let mut next = 1000;
    let mut fresh = |n: usize| -> Vec<Key> {
        let keys = (next..next + n as u64).map(Key).collect();
        next += n as u64;
        keys
    };
    let mut keys = keyed.then(|| fresh(rows.len()));
    let mut source = MemorySource::<Record>::new(rows.len(), COLS)
        .i64_column(Key(0), |r: &Record| r[0])
        .i64_column(Key(1), |r: &Record| r[1]);
    if let Some(keys) = &keys {
        source = source.with_keys(Axis::Rows, keys.clone());
    }
    let (mut spec, mut keep) = (SortSpec::unsorted(), None);
    for op in tc.draw(gs::vecs(op()).max_size(10)) {
        let before = visible(&source, keys.as_deref());
        let notice = match op.clone() {
            Op::Sort(s) => {
                spec = s;
                Some(source.sort(&rows, &spec).unwrap())
            }
            Op::Filter(k) => {
                keep = k;
                Some(match k {
                    Some(k) => source.filter(&rows, move |r| k.keeps(r)),
                    None => source.clear_filter(),
                })
            }
            Op::Append(records) => {
                let new = keys.as_ref().map(|_| fresh(records.len()));
                rows.extend(records);
                let notice = source.appended(&rows, new.as_deref().unwrap_or(&[]));
                if let (Some(keys), Some(new)) = (&mut keys, new) {
                    keys.extend(new);
                }
                Some(notice)
            }
            Op::Edit { row, col, value } => {
                if rows.is_empty() {
                    continue;
                }
                let row = row % rows.len();
                rows[row][col] = value;
                source.edited(&rows, row, Key(col as u64))
            }
            Op::Replace(records) => {
                rows = records;
                keys = keys.map(|_| fresh(rows.len()));
                Some(source.replaced(&rows, keys.clone()))
            }
        };
        let op = match op {
            Op::Edit { row, col, value } => Op::Edit {
                row: row % rows.len(),
                col,
                value,
            },
            op => op,
        };
        check(&Step {
            op: &op,
            rows: &rows,
            ordered: !spec.is_empty() || keep.is_some(),
            notice,
            before,
            after: visible(&source, keys.as_deref()),
            model: model(&rows, &spec, keep),
        });
    }
}

#[hegel::test(test_cases = 1000)]
fn the_visible_order_matches_the_model_after_any_base_change(tc: TestCase) {
    ops(&tc, |step| {
        assert_eq!(step.after, step.model, "{:?}", step.op)
    });
}

#[hegel::test(test_cases = 1000)]
fn each_change_notice_describes_what_happened_to_the_visible_order(tc: TestCase) {
    ops(&tc, |step| {
        let Step {
            op, before, after, ..
        } = step;
        match &step.notice {
            Some(ChangeNotice::Inserted { at, n }) => {
                assert!(matches!(op, Op::Append(_)) && !step.ordered);
                assert_eq!(*at, before.len() as u64);
                assert_eq!(after[..*at as usize], before[..]);
                assert_eq!(after.len() as u64, at + n);
                let added =
                    (step.rows.len() as u64 - n..step.rows.len() as u64).collect::<Vec<_>>();
                assert_eq!(after[*at as usize..], added[..]);
            }
            Some(ChangeNotice::ContentChanged(r)) => {
                let Op::Edit { row, .. } = op else {
                    panic!("{op:?}")
                };
                assert_eq!(after, before);
                let p = after.iter().position(|&i| i == *row as u64).unwrap() as u64;
                assert_eq!(*r, p..p + 1);
            }
            Some(ChangeNotice::Reordered) => {
                let (mut a, mut b) = (after.clone(), before.clone());
                a.sort();
                b.sort();
                assert_eq!(a, b, "{op:?}");
            }
            Some(ChangeNotice::Reset(count)) => {
                assert_eq!(*count, after.len() as u64);
                // An append with nothing sorted or filtered is never a Reset.
                assert!(!matches!(op, Op::Append(_)) || step.ordered, "{op:?}");
            }
            Some(ChangeNotice::Removed { .. }) => panic!("{op:?}"),
            None => {
                let Op::Edit { row, .. } = op else {
                    panic!("{op:?}")
                };
                assert_eq!(after, before);
                assert!(!after.contains(&(*row as u64)));
            }
        }
    });
}
