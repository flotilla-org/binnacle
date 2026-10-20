//! Seam 2 properties for hiding: `MemorySource<R>` over random integer
//! records, put through View Specs that hide rows and columns (Keys the data
//! lacks among them), sort and quick-filter the rows and quick-filter the
//! columns, with host filters, appends, data changes and replacements
//! between, against a naive model: sort, filter, then leave the hidden items
//! out. A quick filter searching the shown items of the other axis searches
//! the ones not hidden.

mod support;

use std::collections::BTreeSet;

use binnacle_core::{
    Axis, ChangeNotice, FilterSpec, Key, SortColumn, SortSpec, TextQuery, ViewSpec,
};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const COLS: usize = 3;
const MAX_ROWS: usize = 8;

type Record = [Option<i64>; COLS];

#[derive(Clone, Debug)]
enum Op {
    View(Box<ViewSpec>),
    /// The host's own filter on the rows: keeps values of at least `min` in
    /// column 0, or everything with `None`.
    Filter(Option<i64>),
    Append(Vec<Record>),
    /// Sets base row `row % len` in column `col`, as from a live feed.
    Change {
        row: usize,
        col: usize,
        value: Option<i64>,
    },
    Replace(Vec<Record>),
}

#[hegel::composite]
fn value(tc: &TestCase) -> Option<i64> {
    // Some values hold a 1, which the quick filters look for.
    tc.draw(gs::optional(
        gs::integers::<i64>().min_value(-2).max_value(12),
    ))
}

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    [tc.draw(value()), tc.draw(value()), tc.draw(value())]
}

/// Hidden Keys of an axis with `n` items: some past the last, some repeated.
#[hegel::composite]
fn hidden(tc: &TestCase, n: usize) -> Vec<Key> {
    let key = gs::integers::<u64>().max_value(n as u64 + 1).map(Key);
    tc.draw(gs::vecs(key).max_size(4))
}

#[hegel::composite]
fn quick(tc: &TestCase) -> FilterSpec {
    let text = tc.draw(gs::sampled_from(vec!["1", "2", "-"]));
    let quick = tc.draw(gs::booleans()).then(|| TextQuery::new(text));
    FilterSpec::default().with_quick(quick)
}

#[hegel::composite]
fn spec(tc: &TestCase) -> ViewSpec {
    let sort = match tc.draw(gs::optional(
        gs::integers::<u64>().max_value(COLS as u64 - 1),
    )) {
        Some(c) => SortSpec::new(vec![SortColumn::descending(Key(c))]),
        None => SortSpec::unsorted(),
    };
    ViewSpec::default()
        .with_sort(Axis::Rows, sort)
        .with_filter(Axis::Rows, tc.draw(quick()))
        .with_hidden(Axis::Rows, tc.draw(hidden(MAX_ROWS)))
        .with_filter(Axis::Cols, tc.draw(quick()))
        .with_hidden(Axis::Cols, tc.draw(hidden(COLS)))
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let change = hegel::tuples!(
        gs::integers::<usize>(),
        gs::integers::<usize>().max_value(COLS - 1),
        value()
    )
    .map(|(row, col, value)| Op::Change { row, col, value });
    tc.draw(hegel::one_of!(
        spec().map(|s| Op::View(Box::new(s))),
        spec().map(|s| Op::View(Box::new(s))),
        gs::optional(gs::integers::<i64>().min_value(-2).max_value(12)).map(Op::Filter),
        gs::vecs(record()).min_size(1).max_size(3).map(Op::Append),
        change,
        gs::vecs(record()).max_size(MAX_ROWS).map(Op::Replace),
    ))
}

/// What the model holds: the data, the View Spec asked for, the hidden Keys
/// in force and the host's filter.
struct Model {
    rows: Vec<Record>,
    spec: ViewSpec,
    hidden: [Vec<Key>; 2],
    host: Option<i64>,
}

impl Model {
    fn len(&self, axis: Axis) -> usize {
        match axis {
            Axis::Rows => self.rows.len(),
            Axis::Cols => COLS,
        }
    }

    /// Takes the hidden Keys of `keys` the axis has, first of each, in
    /// order: those the source answers with. New items arrive shown, so
    /// Keys are pruned only when hidden Keys are given and when the data is
    /// replaced.
    fn prune(&mut self, axis: Axis, keys: &[Key]) {
        let mut kept: Vec<Key> = Vec::new();
        for &k in keys {
            if (k.0 as usize) < self.len(axis) && !kept.contains(&k) {
                kept.push(k);
            }
        }
        self.hidden[axis.index()] = kept;
    }

    fn hidden(&self, axis: Axis) -> Vec<Key> {
        self.hidden[axis.index()].clone()
    }

    fn shown(&self, axis: Axis, i: usize) -> bool {
        !self.hidden(axis).contains(&Key(i as u64))
    }

    /// Whether the quick filter of `axis` keeps item `i`, searching the
    /// items of the other axis not hidden.
    fn quick_keeps(&self, axis: Axis, i: usize) -> bool {
        let Some(q) = &self.spec.axis(axis).filter.quick else {
            return true;
        };
        let across = axis.other();
        (0..self.len(across))
            .filter(|&j| self.shown(across, j))
            .any(|j| {
                let (r, c) = match axis {
                    Axis::Rows => (i, j),
                    Axis::Cols => (j, i),
                };
                self.rows[r][c].is_some_and(|v| v.to_string().contains(&q.text))
            })
    }

    /// The Keys shown on `axis`, in order.
    fn visible(&self, axis: Axis) -> Vec<Key> {
        let order = match axis {
            Axis::Rows => {
                let rank =
                    |c: usize, i: usize| self.rows[i][c].map_or((2, 0), |v| (0, i128::from(v)));
                let host = |i: usize| self.host.is_none_or(|m| self.rows[i][0] >= Some(m));
                let keep = |i: usize| host(i) && self.quick_keeps(axis, i);
                support::model(self.rows.len(), &self.spec.rows.sort, rank, keep)
            }
            Axis::Cols => (0..COLS).filter(|&c| self.quick_keeps(axis, c)).collect(),
        };
        (order.into_iter())
            .filter(|&i| self.shown(axis, i))
            .map(|i| Key(i as u64))
            .collect()
    }
}

fn visible(source: &MemorySource<Record>, axis: Axis) -> Vec<Key> {
    source.window(axis, 0..source.count(axis))
}

#[hegel::test(test_cases = 400)]
fn hidden_items_leave_the_view_until_shown_and_the_answer_prunes_keys_the_data_lacks(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(MAX_ROWS));
    let mut source = (0..COLS).fold(MemorySource::<Record>::new(rows.len(), COLS), |s, c| {
        s.i64_column(Key(c as u64), move |r: &Record| r[c])
    });
    let mut model = Model {
        rows,
        spec: ViewSpec::default(),
        hidden: Default::default(),
        host: None,
    };
    for op in tc.draw(gs::vecs(op()).max_size(8)) {
        let before = Axis::ALL.map(|a| visible(&source, a));
        let notices = match op.clone() {
            Op::View(spec) => {
                for axis in Axis::ALL {
                    model.prune(axis, &spec.axis(axis).hidden);
                }
                model.spec = *spec;
                source.view(&model.rows, &model.spec).unwrap()
            }
            Op::Filter(min) => {
                model.host = min;
                let notice = match min {
                    Some(m) => source.filter(&model.rows, move |r: &Record| r[0] >= Some(m)),
                    None => source.clear_filter(&model.rows),
                };
                vec![(Axis::Rows, notice)]
            }
            Op::Append(records) => {
                model.rows.extend(records);
                source.appended(&model.rows, &[])
            }
            Op::Change { row, col, value } => {
                if model.rows.is_empty() {
                    continue;
                }
                let row = row % model.rows.len();
                model.rows[row][col] = value;
                source.changed(&model.rows, row, Key(col as u64))
            }
            Op::Replace(records) => {
                model.rows = records;
                model.prune(Axis::Rows, &model.hidden(Axis::Rows));
                source.replaced(&model.rows, None)
            }
        };
        let applied = source.view_spec();
        for axis in Axis::ALL {
            let shown = visible(&source, axis);
            assert_eq!(shown, model.visible(axis), "{axis:?} after {op:?}");
            assert_eq!(applied.axis(axis).hidden, model.hidden(axis), "{axis:?}");
            let hidden: BTreeSet<Key> = model.hidden(axis).into_iter().collect();
            assert!(
                shown.iter().all(|k| !hidden.contains(k)),
                "{axis:?} shows a hidden item"
            );
            let told: Vec<&ChangeNotice> = (notices.iter())
                .filter(|(a, _)| *a == axis)
                .map(|(_, n)| n)
                .collect();
            for notice in &told {
                if let ChangeNotice::Refiltered(n) | ChangeNotice::Reset(n) = notice {
                    assert_eq!(*n, shown.len() as u64, "{axis:?} {notice:?}");
                }
            }
            let set = |keys: &[Key]| keys.iter().copied().collect::<BTreeSet<Key>>();
            let came_or_went = set(&shown) != set(&before[axis.index()]);
            if let Op::Change { .. } | Op::Append(_) = op {
                // A hidden item the change keeps or drops stays out of view.
                let refiltered = told
                    .iter()
                    .any(|n| matches!(n, ChangeNotice::Refiltered(_)));
                assert!(
                    !refiltered || came_or_went,
                    "{axis:?} refiltered with nothing coming or going: {notices:?} after {op:?}"
                );
            }
            if came_or_went {
                assert!(
                    told.iter().any(|n| matches!(
                        n,
                        ChangeNotice::Refiltered(_)
                            | ChangeNotice::Reset(_)
                            | ChangeNotice::Inserted { .. }
                    )),
                    "{axis:?} items came or went with no notice saying so: {notices:?} after {op:?}"
                );
            }
        }
    }
}

#[hegel::test(test_cases = 200)]
fn clearing_the_filters_leaves_hidden_items_hidden(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).min_size(1).max_size(MAX_ROWS));
    let mut source = (0..COLS).fold(MemorySource::<Record>::new(rows.len(), COLS), |s, c| {
        s.i64_column(Key(c as u64), move |r: &Record| r[c])
    });
    let spec = tc.draw(spec());
    source.view(&rows, &spec).unwrap();
    let cleared = spec
        .clone()
        .with_filter(Axis::Rows, FilterSpec::default())
        .with_filter(Axis::Cols, FilterSpec::default());
    source.view(&rows, &cleared).unwrap();
    let mut model = Model {
        rows,
        spec: cleared,
        hidden: Default::default(),
        host: None,
    };
    for axis in Axis::ALL {
        model.prune(axis, &spec.axis(axis).hidden);
        assert_eq!(visible(&source, axis), model.visible(axis), "{axis:?}");
    }
}
