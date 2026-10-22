//! The In-Memory Table Source under the grouping layer (`GroupedMemory`)
//! against a naive model: rows grouped by one or two columns, with a count,
//! a sum and a distinct count, Subtotals and the Grand Total placed before,
//! after or nowhere. One sort rule orders siblings at every level: groups by
//! the aggregate of the sorted column, or by their value when sorting by
//! their own column, else in ascending order of their value (blanks last);
//! Leaves in source order or the user's sort within their group. Hidden
//! rows are left out, and the grouped columns leave the columns.

use std::cmp::Ordering;

use binnacle_core::{
    AggregateValue, Axis, ChangeNotice, Grouping, ItemKind, Key, Level, Placement, SortColumn,
    SortDirection, SortSpec, Value, ViewSpec,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const COLS: usize = 3;

type Record = [Option<i64>; COLS];

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    // Mostly a few values, so groups fill up; larger ones now and then,
    // small enough that any sum of them is exact.
    let value = || {
        gs::optional(hegel::one_of!(
            gs::integers::<i64>().min_value(-3).max_value(12),
            gs::integers::<i64>().min_value(-(1 << 40)).max_value(1 << 40)
        ))
    };
    [tc.draw(value()), tc.draw(value()), tc.draw(value())]
}

fn source(rows: usize) -> GroupedMemory<Record> {
    let source = (0..COLS).fold(MemorySource::<Record>::new(rows, COLS), |s, c| {
        s.i64_column(Key(c as u64), move |r: &Record| r[c])
    });
    GroupedMemory::new(source)
}

fn text(v: Option<i64>) -> Value {
    Value::of(v.map(|n| n.to_string()).as_deref())
}

/// Orders by `a` then `b` as a sort column does: nulls last either way.
fn order<T: PartialOrd>(a: Option<T>, b: Option<T>, direction: SortDirection) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => {
            let o = a.partial_cmp(&b).unwrap();
            match direction {
                SortDirection::Ascending => o,
                SortDirection::Descending => o.reverse(),
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

/// What a grouping asks for, as the model reads it.
#[derive(Clone, Debug)]
struct Asked {
    /// The level columns, outermost first, each with its Subtotal.
    levels: Vec<(usize, Placement)>,
    grand_total: Placement,
    /// The column summed, and the one counted distinctly.
    sum: usize,
    distinct: usize,
}

impl Asked {
    fn values(&self) -> Vec<AggregateValue> {
        vec![
            AggregateValue::count(),
            AggregateValue::of(Key(self.sum as u64), "sum"),
            AggregateValue::of(Key(self.distinct as u64), "distinct count"),
        ]
    }

    fn grouping(&self) -> Grouping {
        let mut grouping = Grouping::rows_by(Key(self.levels[0].0 as u64));
        grouping.row_levels = (self.levels.iter())
            .map(|&(c, subtotal)| Level::Field {
                field: Key(c as u64),
                subtotal,
            })
            .collect();
        grouping.values = self.values();
        grouping.rows.grand_total = self.grand_total;
        grouping
    }
}

/// An item as the model describes it: a Summary Item by kind, its group's
/// values outermost first and its aggregates; a Leaf by Key.
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Summary(ItemKind, Vec<Value>, Vec<Figure>),
    Leaf(Key),
}

/// The count, the sum and the distinct count of `rows`.
fn figures(rows: &[Record], kept: &[usize], asked: &Asked) -> Vec<Figure> {
    let sums: Vec<i64> = kept.iter().filter_map(|&r| rows[r][asked.sum]).collect();
    let mut distinct: Vec<i64> = kept.iter().filter_map(|&r| rows[r][asked.distinct]).collect();
    distinct.sort();
    distinct.dedup();
    vec![
        Figure::Count(kept.len() as u64),
        match sums.is_empty() {
            true => Figure::Blank,
            false => Figure::Number(sums.iter().sum::<i64>() as f64),
        },
        Figure::Count(distinct.len() as u64),
    ]
}

/// The model: the grouped items as the one sort rule orders them.
fn model(rows: &[Record], asked: &Asked, sort: &SortSpec, hidden: &[Key]) -> Vec<Item> {
    let kept: Vec<usize> = (0..rows.len())
        .filter(|&i| !hidden.contains(&Key(i as u64)))
        .collect();
    let levels: Vec<usize> = asked.levels.iter().map(|l| l.0).collect();
    fn walk(
        rows: &[Record],
        kept: Vec<usize>,
        asked: &Asked,
        sort: &SortSpec,
        path: Vec<Value>,
        out: &mut Vec<Item>,
    ) {
        let depth = path.len();
        let levels: Vec<usize> = asked.levels.iter().map(|l| l.0).collect();
        let Some(&(field, subtotal)) = asked.levels.get(depth) else {
            // Leaves: the user's sort on columns that are not levels, else
            // source order.
            let mut leaves = kept;
            leaves.sort_by(|&a, &b| {
                (sort.columns().iter())
                    .filter(|c| !levels.contains(&(c.key.0 as usize)))
                    .map(|c| {
                        let c0 = c.key.0 as usize;
                        order(rows[a][c0], rows[b][c0], c.direction)
                    })
                    .find(|o| o.is_ne())
                    .unwrap_or(Ordering::Equal)
            });
            out.extend(leaves.into_iter().map(|r| Item::Leaf(Key(r as u64))));
            return;
        };
        let mut groups: Vec<(Option<i64>, Vec<usize>)> = Vec::new();
        for &r in &kept {
            match groups.iter_mut().find(|(v, _)| *v == rows[r][field]) {
                Some((_, members)) => members.push(r),
                None => groups.push((rows[r][field], vec![r])),
            }
        }
        let label = sort.find(Key(field as u64)).map_or(SortDirection::Ascending, |(_, d)| d);
        groups.sort_by(|a, b| {
            let mut o = Ordering::Equal;
            for c in sort.columns() {
                if c.key.0 as usize == field {
                    break;
                }
                let value = |v: &AggregateValue| v.field == Some(c.key);
                if let Some(i) = asked.values().iter().position(value) {
                    let (fa, fb) = (figures(rows, &a.1, asked)[i], figures(rows, &b.1, asked)[i]);
                    let n = |f: Figure| match f {
                        Figure::Count(n) => Some(n as f64),
                        Figure::Number(n) => Some(n),
                        Figure::Blank => None,
                    };
                    o = o.then(order(n(fa), n(fb), c.direction));
                }
            }
            o.then(order(a.0, b.0, label))
        });
        for (value, members) in groups {
            let mut path = path.clone();
            path.push(text(value));
            let figures = figures(rows, &members, asked);
            out.push(Item::Summary(ItemKind::Group, path.clone(), figures.clone()));
            let sub = Item::Summary(ItemKind::Subtotal, path.clone(), figures);
            if subtotal == Placement::Before {
                out.push(sub.clone());
            }
            walk(rows, members, asked, sort, path, out);
            if subtotal == Placement::After {
                out.push(sub);
            }
        }
    }
    let _ = levels;
    let total = Item::Summary(ItemKind::GrandTotal, Vec::new(), figures(rows, &kept, asked));
    let mut out = Vec::new();
    if asked.grand_total == Placement::Before {
        out.push(total.clone());
    }
    walk(rows, kept, asked, sort, Vec::new(), &mut out);
    if asked.grand_total == Placement::After {
        out.push(total);
    }
    out
}

#[hegel::composite]
fn placement(tc: &TestCase) -> Placement {
    tc.draw(gs::sampled_from(vec![
        Placement::Before,
        Placement::After,
        Placement::Off,
    ]))
}

#[hegel::composite]
fn column(tc: &TestCase) -> usize {
    tc.draw(gs::integers::<usize>().max_value(COLS - 1))
}

#[hegel::composite]
fn asked(tc: &TestCase) -> Asked {
    let outer = tc.draw(column());
    let mut levels = vec![(outer, tc.draw(placement()))];
    if tc.draw(gs::booleans()) {
        let inner = tc.draw(column().filter(move |&c| c != outer));
        levels.push((inner, tc.draw(placement())));
    }
    Asked {
        levels,
        grand_total: tc.draw(placement()),
        sum: tc.draw(column()),
        distinct: tc.draw(column()),
    }
}

#[hegel::composite]
fn sort(tc: &TestCase) -> SortSpec {
    let columns = tc.draw(gs::vecs(column()).max_size(2).unique(true));
    SortSpec::new(
        columns
            .into_iter()
            .map(|c| match tc.draw(gs::booleans()) {
                true => SortColumn::ascending(Key(c as u64)),
                false => SortColumn::descending(Key(c as u64)),
            })
            .collect(),
    )
}

#[hegel::composite]
fn spec(tc: &TestCase, rows: usize) -> (Asked, ViewSpec) {
    let asked = tc.draw(asked());
    let sort = tc.draw(sort());
    let hidden =
        tc.draw(gs::vecs(gs::integers::<u64>().max_value(rows as u64).map(Key)).max_size(4));
    let spec = ViewSpec::default()
        .with_sort(Axis::Rows, sort)
        .with_hidden(Axis::Rows, hidden)
        .with_grouping(Some(asked.grouping()));
    (asked, spec)
}

#[hegel::test(test_cases = 300)]
fn grouped_rows_match_a_whole_table_grouping(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(90));
    let mut source = source(rows.len());
    source.view(&rows, &ViewSpec::default());
    let (asked, spec) = tc.draw(spec(rows.len()));
    let viewed = source.view(&rows, &spec);

    let hidden: Vec<Key> = spec
        .rows
        .hidden
        .iter()
        .filter(|k| (k.0 as usize) < rows.len())
        .copied()
        .collect();
    let want = model(&rows, &asked, &spec.rows.sort, &hidden);
    let count = source.count(Axis::Rows);
    assert_eq!(count, want.len() as u64);
    assert!(
        viewed
            .notices
            .contains(&(Axis::Rows, ChangeNotice::Refiltered(count)))
    );
    assert_eq!(viewed.applied.grouping, spec.grouping);
    assert_eq!(
        viewed.applied.rows.sort, spec.rows.sort,
        "the levels' sort is not added"
    );
    assert!(
        viewed.applied.cols.hidden.is_empty(),
        "the grouped columns are not written in"
    );

    let window = source.window(&rows, Axis::Rows, 0..count);
    let outlines = window.outlines.expect("grouped rows have outlines");
    let got: Vec<Item> = window
        .keys
        .iter()
        .zip(&outlines)
        .map(|(&key, outline)| match outline.kind {
            ItemKind::Leaf => Item::Leaf(key),
            kind => {
                let summary = source.summary(key).expect("a known Summary Item");
                let path = summary.path.iter().map(|(_, v)| v.clone()).collect();
                Item::Summary(kind, path, summary.aggregates.to_vec())
            }
        })
        .collect();
    assert_eq!(got, want);

    // The grouped columns leave the columns.
    let cols = source.window(&rows, Axis::Cols, 0..COLS as u64).keys;
    let want_cols: Vec<Key> = (0..COLS)
        .filter(|c| !asked.levels.iter().any(|l| l.0 == *c))
        .map(|c| Key(c as u64))
        .collect();
    assert_eq!(cols, want_cols);
}

#[hegel::test(test_cases = 200)]
fn ungrouping_brings_back_the_plain_rows_and_the_column(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(40));
    let mut source = source(rows.len());
    let (_, spec) = tc.draw(spec(rows.len()));
    source.view(&rows, &spec);
    let plain = ViewSpec {
        grouping: None,
        ..spec.clone()
    };
    let viewed = source.view(&rows, &plain);
    assert_eq!(viewed.applied.grouping, None);
    let count = source.count(Axis::Rows);
    assert!(
        viewed
            .notices
            .contains(&(Axis::Rows, ChangeNotice::Refiltered(count)))
    );
    let window = source.window(&rows, Axis::Rows, 0..count);
    assert_eq!(window.outlines, None);
    let mut fresh = self::source(rows.len());
    fresh.view(&rows, &plain);
    assert_eq!(window.keys, fresh.window(&rows, Axis::Rows, 0..count).keys);
    assert_eq!(
        source.window(&rows, Axis::Cols, 0..COLS as u64).keys.len(),
        COLS
    );
}

#[test]
fn the_rows_group_by_columns_that_sort_and_have_text() {
    let source = GroupedMemory::new(
        MemorySource::<Record>::new(1, COLS).i64_column(Key(0), |r: &Record| r[0]),
    );
    assert!(source.support(Axis::Rows, Key(0)).group);
    assert!(!source.support(Axis::Rows, Key(1)).group, "no closure");
    assert!(
        !source.support(Axis::Cols, Key(0)).group,
        "the columns don't group"
    );
}

#[test]
fn a_leaf_keeps_its_key_and_summary_items_carry_the_summary_bit() {
    let rows: Vec<Record> = vec![
        [Some(1), None, None],
        [Some(2), None, None],
        [Some(1), None, None],
    ];
    let mut source = source(rows.len());
    source.view(
        &rows,
        &ViewSpec::default().with_grouping(Some(Grouping::rows_by(Key(0)))),
    );
    let window = source.window(&rows, Axis::Rows, 0..5);
    let kinds: Vec<ItemKind> = window.outlines.unwrap().iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        [
            ItemKind::Group,
            ItemKind::Leaf,
            ItemKind::Leaf,
            ItemKind::Group,
            ItemKind::Leaf
        ]
    );
    assert_eq!(&window.keys[1..3], &[Key(0), Key(2)]);
    assert_eq!(window.keys[4], Key(1));
    assert!(window.keys[0].0 >= binnacle_group::SUMMARY_KEY_BIT);
}
