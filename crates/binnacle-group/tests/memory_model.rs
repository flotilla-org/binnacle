//! The In-Memory Table Source under the grouping layer (`GroupedMemory`)
//! against a naive model: rows grouped by one or two columns, with a count,
//! a sum and a distinct count, Subtotals and the Grand Total placed before,
//! after or nowhere. One sort rule orders siblings at every level: groups by
//! the aggregate of the sorted column, or by their value when sorting by
//! their own column, else in ascending order of their value (blanks last);
//! Leaves in source order or the user's sort within their group. Hidden
//! rows are left out, and the grouped columns leave the columns. A slicer
//! on any column, hidden or not, leaves rows out before aggregating; a
//! comparison in the rows' filter on a column a value aggregates leaves out
//! the groups whose figure fails it (`HAVING`), and on any other column the
//! Leaves that fail it, before aggregating.

use std::cmp::Ordering;

use binnacle_core::{
    Aggregate, AggregateValue, Axis, ChangeNotice, Comparison, Condition, Expand, FilterSpec,
    FindDirection, FindQuery, Grouping, ItemKind, Key, Level, OpenState, Placement, Placements,
    Scalar, SortColumn, SortDirection, SortSpec, TextQuery, VALUES_FIELD, Value, ViewSpec,
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
            gs::integers::<i64>()
                .min_value(-(1 << 40))
                .max_value(1 << 40)
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

/// A comparison as the model reads it: a column's value (or a figure) at
/// least, or less than, a threshold; with `not` the other way. A blank
/// never passes, so `not` keeps it.
#[derive(Clone, Copy, Debug)]
struct Compare {
    column: usize,
    at_least: bool,
    threshold: i64,
    not: bool,
}

impl Compare {
    fn passes(&self, n: Option<f64>) -> bool {
        let t = self.threshold as f64;
        let passed = n.is_some_and(|n| match self.at_least {
            true => n >= t,
            false => n < t,
        });
        passed != self.not
    }

    fn condition(&self) -> Condition {
        let t = Scalar::Number(self.threshold as f64);
        let comparison = match self.at_least {
            true => Comparison::AtLeast(t),
            false => Comparison::Less(t),
        };
        Condition::compare(Key(self.column as u64), comparison).not(self.not)
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
    slicer: Option<Compare>,
    /// A comparison in the rows' filter.
    filter: Option<Compare>,
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
        grouping.slicers = self.slicer.iter().map(Compare::condition).collect();
        grouping
    }

    /// The value whose figure the filter's comparison reads, if it is on a
    /// column a value aggregates.
    fn having(&self) -> Option<usize> {
        let f = self.filter?;
        (self.values().iter()).position(|v| v.field == Some(Key(f.column as u64)))
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
    let mut distinct: Vec<i64> = kept
        .iter()
        .filter_map(|&r| rows[r][asked.distinct])
        .collect();
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
    let number = |r: usize, c: Compare| rows[r][c.column].map(|n| n as f64);
    let kept: Vec<usize> = (0..rows.len())
        .filter(|&i| !hidden.contains(&Key(i as u64)))
        .filter(|&i| asked.slicer.is_none_or(|c| c.passes(number(i, c))))
        .filter(|&i| match (asked.filter, asked.having()) {
            (Some(c), None) => c.passes(number(i, c)),
            _ => true,
        })
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
        let label = sort
            .find(Key(field as u64))
            .map_or(SortDirection::Ascending, |(_, d)| d);
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
            if let (Some(c), Some(i)) = (asked.filter, asked.having()) {
                let n = match figures[i] {
                    Figure::Count(n) => Some(n as f64),
                    Figure::Number(n) => Some(n),
                    Figure::Blank => None,
                };
                if !c.passes(n) {
                    continue;
                }
            }
            out.push(Item::Summary(
                ItemKind::Group,
                path.clone(),
                figures.clone(),
            ));
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
    let total = Item::Summary(
        ItemKind::GrandTotal,
        Vec::new(),
        figures(rows, &kept, asked),
    );
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
fn compare(tc: &TestCase) -> Compare {
    Compare {
        column: tc.draw(column()),
        at_least: tc.draw(gs::booleans()),
        // Near the small values, and the small sums and counts of them.
        threshold: tc.draw(gs::integers::<i64>().min_value(-4).max_value(30)),
        not: tc.draw(gs::booleans()),
    }
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
        slicer: tc.draw(gs::optional(compare())),
        filter: tc.draw(gs::optional(compare())),
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
    let filter: Vec<Condition> = asked.filter.iter().map(Compare::condition).collect();
    // The slicer's column is hidden now and then: it slices all the same.
    let hidden_cols: Vec<Key> = match (asked.slicer, tc.draw(gs::booleans())) {
        (Some(c), true) => vec![Key(c.column as u64)],
        _ => Vec::new(),
    };
    let spec = ViewSpec::default()
        .with_sort(Axis::Rows, sort)
        .with_hidden(Axis::Rows, hidden)
        .with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(filter.into()),
        )
        .with_hidden(Axis::Cols, hidden_cols)
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
    assert_eq!(viewed.applied.rows.filter, spec.rows.filter);
    assert_eq!(
        viewed.applied.rows.sort, spec.rows.sort,
        "the levels' sort is not added"
    );
    assert!(
        (viewed.applied.cols.hidden.iter()).all(|k| spec.cols.hidden.contains(k)),
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

    // The grouped columns leave the columns, with the hidden ones.
    let cols = source.window(&rows, Axis::Cols, 0..COLS as u64).keys;
    let want_cols: Vec<Key> = (0..COLS)
        .filter(|c| !asked.levels.iter().any(|l| l.0 == *c))
        .filter(|&c| !spec.cols.hidden.contains(&Key(c as u64)))
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
    let shown = (0..COLS as u64).filter(|c| !spec.cols.hidden.contains(&Key(*c)));
    assert_eq!(
        source.window(&rows, Axis::Cols, 0..COLS as u64).keys.len(),
        shown.count()
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
fn the_columns_pivot_by_what_the_rows_group_by_and_values_take_the_standard_aggregates() {
    let source = GroupedMemory::new(
        MemorySource::<Record>::new(1, COLS)
            .i64_column(Key(0), |r: &Record| r[0])
            .text_column(Key(2), |r: &Record| r[2].map(|n| n.to_string().into())),
    );
    assert!(source.support(Axis::Rows, Key(0)).pivot);
    assert!(!source.support(Axis::Rows, Key(1)).pivot, "no closure");
    let names = |key| -> Vec<String> {
        let aggregates = source.aggregates(Axis::Rows, Key(key));
        aggregates.into_iter().map(|a| a.name).collect()
    };
    let numbers = [
        "count",
        "distinct count",
        "sum",
        "average",
        "minimum",
        "maximum",
    ];
    assert_eq!(names(0), numbers);
    assert_eq!(names(2), ["count", "distinct count"]);
    assert_eq!(names(1), Vec::<String>::new(), "no closure");
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

#[test]
fn closing_a_group_hides_its_leaves_with_collapsed_and_regrouping_prunes_it() {
    let rows: Vec<Record> = vec![
        [Some(1), Some(5), None],
        [Some(2), Some(6), None],
        [Some(1), Some(5), None],
    ];
    let mut source = source(rows.len());
    let mut grouping = Grouping::rows_by(Key(0));
    source.view(
        &rows,
        &ViewSpec::default().with_grouping(Some(grouping.clone())),
    );
    let first = source.window(&rows, Axis::Rows, 0..1).keys[0];
    grouping.rows.expand = Expand::default().with(0, first, false);
    let viewed = source.view(
        &rows,
        &ViewSpec::default().with_grouping(Some(grouping.clone())),
    );
    assert_eq!(
        viewed.notices,
        vec![(Axis::Rows, ChangeNotice::Collapsed { at: 1, n: 2 })]
    );
    let window = source.window(&rows, Axis::Rows, 0..3);
    let states: Vec<OpenState> = window.outlines.unwrap().iter().map(|o| o.open).collect();
    assert_eq!(
        states,
        [OpenState::Closed, OpenState::Open, OpenState::CantOpen]
    );
    let summary = source.summary(first).unwrap();
    assert_eq!(
        summary.aggregates,
        [Figure::Count(2)],
        "a closed group's count"
    );

    // Grouping by another column drops the Key, whose path no longer fits.
    grouping.row_levels = vec![Level::field(Key(1))];
    let viewed = source.view(&rows, &ViewSpec::default().with_grouping(Some(grouping)));
    assert_eq!(
        viewed.applied.grouping.unwrap().rows.expand,
        Expand::default()
    );
}

/// Columns 0 and 2 hold numbers; column 1 has no closure, so the rows
/// neither group nor slice by it.
fn partial(rows: usize) -> GroupedMemory<Record> {
    GroupedMemory::new(
        MemorySource::<Record>::new(rows, COLS)
            .i64_column(Key(0), |r: &Record| r[0])
            .i64_column(Key(2), |r: &Record| r[2]),
    )
}

#[test]
fn the_source_declares_what_it_groups_and_aggregates_per_column() {
    let source = partial(1);
    let support = source.support(Axis::Rows, Key(0));
    assert!(support.group && support.pivot && !support.native);
    assert_eq!(support.subtotals, Placements::BOTH);
    assert!(!source.support(Axis::Rows, Key(1)).group);
    assert_eq!(source.aggregates(Axis::Rows, Key(0)), Aggregate::numbers());
    assert_eq!(source.aggregates(Axis::Rows, Key(1)), vec![]);
    assert_eq!(source.aggregates(Axis::Cols, Key(0)), vec![]);
    let texts = GroupedMemory::new(
        MemorySource::<Record>::new(1, 1)
            .text_column(Key(0), |r: &Record| r[0].map(|n| n.to_string().into())),
    );
    assert_eq!(texts.aggregates(Axis::Rows, Key(0)), Aggregate::counts());
}

#[test]
fn a_grouping_beyond_support_is_applied_as_far_as_it_can_be() {
    let rows: Vec<Record> = vec![
        [Some(1), Some(5), Some(1)],
        [Some(2), Some(6), Some(0)],
        [Some(1), Some(5), Some(3)],
    ];
    let mut source = partial(rows.len());
    let mut asked = Grouping::rows_by(Key(1));
    asked.row_levels = vec![
        Level::field(Key(1)),
        Level::Field {
            field: Key(0),
            subtotal: Placement::Before,
        },
    ];
    asked.col_levels = vec![Level::field(Key(1))];
    let sum = |c| AggregateValue::of(Key(c), "sum");
    asked.values = vec![
        AggregateValue::count(),
        sum(1),
        sum(2),
        AggregateValue::of(Key(2), "median"),
    ];
    let sliced = Condition::compare(Key(2), Comparison::AtLeast(Scalar::Number(1.0)));
    asked.slicers = vec![
        Condition::native(Key(0), "x > 1"),
        Condition::compare(Key(1), Comparison::AtLeast(Scalar::Number(1.0))),
        sliced.clone(),
    ];
    let viewed = source.view(&rows, &ViewSpec::default().with_grouping(Some(asked)));

    let mut want = Grouping::rows_by(Key(0));
    want.row_levels = vec![Level::Field {
        field: Key(0),
        subtotal: Placement::Before,
    }];
    want.values = vec![AggregateValue::count(), sum(2)];
    want.slicers = vec![sliced];
    assert_eq!(viewed.applied.grouping, Some(want));
    // Grouped by column 0 over the two rows the slicer keeps, each group
    // with a Subtotal before it.
    let window = source.window(&rows, Axis::Rows, 0..10);
    let kinds: Vec<ItemKind> = window.outlines.unwrap().iter().map(|o| o.kind).collect();
    assert_eq!(
        kinds,
        [
            ItemKind::Group,
            ItemKind::Subtotal,
            ItemKind::Leaf,
            ItemKind::Leaf
        ]
    );
    assert_eq!(&window.keys[2..], &[Key(0), Key(2)]);
}

#[test]
fn slicers_with_no_levels_slice_the_plain_rows() {
    let rows: Vec<Record> = vec![
        [Some(1), None, Some(1)],
        [Some(2), None, Some(0)],
        [Some(1), None, Some(3)],
    ];
    let mut source = partial(rows.len());
    let mut asked = Grouping::default();
    asked.values = vec![AggregateValue::count()];
    asked.slicers = vec![Condition::compare(
        Key(2),
        Comparison::AtLeast(Scalar::Number(1.0)),
    )];
    let spec = ViewSpec::default().with_grouping(Some(asked));
    let viewed = source.view(&rows, &spec);
    assert_eq!(viewed.applied, spec, "the slicer stays in the grouping");
    assert_eq!(
        viewed.notices,
        vec![(Axis::Rows, ChangeNotice::Refiltered(2))]
    );
    let window = source.window(&rows, Axis::Rows, 0..3);
    assert_eq!(window.keys, vec![Key(0), Key(2)]);
    assert_eq!(window.outlines, None, "not grouped");
}

/// The rows (not `hidden`) in the group at `path` by the values they hold
/// now, of those the slicer and the rows' filter admitted when the View
/// Spec applied, by the values they held then (`viewed`).
fn members(
    rows: &[Record],
    viewed: &[Record],
    asked: &Asked,
    path: &[Value],
    hidden: &[Key],
) -> Vec<usize> {
    let number = |r: usize, c: Compare| viewed[r][c.column].map(|n| n as f64);
    (0..rows.len())
        .filter(|&r| !hidden.contains(&Key(r as u64)))
        .filter(|&r| asked.slicer.is_none_or(|c| c.passes(number(r, c))))
        .filter(|&r| match (asked.filter, asked.having()) {
            (Some(c), None) => c.passes(number(r, c)),
            _ => true,
        })
        .filter(|&r| {
            path.iter()
                .zip(&asked.levels)
                .all(|(v, &(c, _))| text(rows[r][c]) == *v)
        })
        .collect()
}

/// Every item shown, as the model describes it.
fn shown(source: &mut GroupedMemory<Record>, rows: &[Record]) -> Vec<(Key, Item)> {
    let count = source.count(Axis::Rows);
    let window = source.window(rows, Axis::Rows, 0..count);
    let outlines = window.outlines.expect("grouped rows have outlines");
    (window.keys.iter().zip(&outlines))
        .map(|(&key, outline)| match outline.kind {
            ItemKind::Leaf => (key, Item::Leaf(key)),
            kind => {
                let summary = source.summary(key).expect("a known Summary Item");
                let path = summary.path.iter().map(|(_, v)| v.clone()).collect();
                (key, Item::Summary(kind, path, summary.aggregates.to_vec()))
            }
        })
        .collect()
}

#[hegel::test(test_cases = 200)]
fn an_edit_moves_nothing_and_every_figure_follows_it_at_once(tc: TestCase) {
    let mut rows = tc.draw(gs::vecs(record()).min_size(1).max_size(60));
    let mut source = source(rows.len());
    let (asked, spec) = tc.draw(spec(rows.len()));
    source.view(&rows, &spec);
    let hidden: Vec<Key> = spec.rows.hidden.clone();
    let before = shown(&mut source, &rows);

    let row = tc.draw(gs::integers::<usize>().max_value(rows.len() - 1));
    let column = tc.draw(column());
    let viewed = rows.clone();
    let was = rows[row];
    rows[row][column] = tc.draw(record())[column];
    let notices = source.edited(&rows, row, Key(column as u64));
    let count = source.count(Axis::Rows);
    for (axis, notice) in &notices {
        assert_eq!(*axis, Axis::Rows);
        let ChangeNotice::ContentChanged(range) = notice else {
            panic!("only figures change: {notice:?}");
        };
        assert!(range.end <= count);
    }

    let after = shown(&mut source, &rows);
    let keys = |items: &[(Key, Item)]| items.iter().map(|(k, _)| *k).collect::<Vec<_>>();
    assert_eq!(keys(&after), keys(&before), "nothing moves");
    for (_, item) in &after {
        if let Item::Summary(_, path, figures) = item {
            let kept = members(&rows, &viewed, &asked, path, &hidden);
            assert_eq!(*figures, self::figures(&rows, &kept, &asked), "{path:?}");
        }
    }
    let leaves: Vec<Key> = (after.iter())
        .filter(|(_, i)| matches!(i, Item::Leaf(_)))
        .map(|(k, _)| *k)
        .collect();
    // The edited Leaf is unmatched if it left its group or stopped passing
    // the slicer or the rows' filter; it stays in view all the same.
    let unmatched = source.unmatched(&rows, Axis::Rows, &leaves);
    let number = |c: Compare| rows[row][c.column].map(|n| n as f64);
    let filtered = match (asked.filter, asked.having()) {
        (Some(c), None) => Some(c),
        _ => None,
    };
    let failed = (asked.slicer.iter().chain(&filtered)).any(|&c| !c.passes(number(c)));
    for (key, unmatched) in leaves.iter().zip(unmatched) {
        let regrouped = (asked.levels.iter()).any(|&(c, _)| rows[row][c] != was[c]);
        let want = key.0 as usize == row && (regrouped || failed);
        assert_eq!(unmatched, want, "Leaf {key:?}");
    }

    // The next View Spec, even the same one, groups the rows again.
    source.view(&rows, &spec);
    let regrouped: Vec<Item> = shown(&mut source, &rows)
        .into_iter()
        .map(|(_, i)| i)
        .collect();
    assert_eq!(regrouped, model(&rows, &asked, &spec.rows.sort, &hidden));
}

/// A match as the model describes it: a Group Item's label by its path, or
/// a Leaf's cell.
#[derive(Clone, Debug, PartialEq)]
enum Found {
    Label(Vec<Value>),
    Cell(Key, Key),
}

#[hegel::test(test_cases = 200)]
fn find_steps_through_every_match_as_if_every_group_were_open(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(50));
    let mut source = source(rows.len());
    let (asked, mut spec) = tc.draw(spec(rows.len()));
    let depth = tc.draw(gs::integers::<u32>().max_value(2));
    let mut grouping = spec.grouping.clone().unwrap();
    grouping.rows.expand = Expand::to(binnacle_core::Depth::Levels(depth));
    spec.grouping = Some(grouping);
    source.view(&rows, &spec);
    let hidden: Vec<Key> = spec.rows.hidden.clone();
    // One or two digits, so a query may match a cell, a label, or nothing.
    let digits = tc.draw(gs::integers::<u8>().max_value(99)).to_string();
    let query = FindQuery::new(TextQuery::new(digits.clone()));

    // The model: every item open, a label then the shown columns in order.
    let searched: Vec<usize> = (0..COLS)
        .filter(|c| !asked.levels.iter().any(|l| l.0 == *c))
        .filter(|&c| !spec.cols.hidden.contains(&Key(c as u64)))
        .collect();
    let mut want = Vec::new();
    for item in model(&rows, &asked, &spec.rows.sort, &hidden) {
        let has = |v: &Value| matches!(v, Value::Text(t) if t.contains(&digits));
        match item {
            Item::Summary(ItemKind::Group, path, _) => {
                if has(path.last().unwrap()) {
                    // A group at level L hides from depth L on.
                    want.push((Found::Label(path.clone()), path.len() as u32 > depth + 1));
                }
            }
            Item::Leaf(key) => {
                for &c in &searched {
                    if has(&text(rows[key.0 as usize][c])) {
                        let hid = asked.levels.len() as u32 > depth;
                        want.push((Found::Cell(key, Key(c as u64)), hid));
                    }
                }
            }
            Item::Summary(..) => {}
        }
    }
    let count = source.find_count(&rows, &query);
    assert_eq!(count.total, want.len() as u64);
    assert_eq!(count.hidden, want.iter().filter(|(_, h)| *h).count() as u64);

    let mut from = None;
    for (i, (found, _)) in want.iter().enumerate() {
        let step = source
            .find_next(&rows, &query, from, FindDirection::Next)
            .expect("a match");
        assert_eq!(step.index, Some(i as u64));
        assert!(!step.wrapped);
        let got = match step.at.row.0 & binnacle_group::SUMMARY_KEY_BIT {
            0 => Found::Cell(step.at.row, step.at.col),
            _ => {
                let path = source.path(step.at.row).expect("an interned group");
                assert_eq!(
                    step.at.col,
                    path.last().unwrap().0,
                    "a label's column is its field"
                );
                Found::Label(path.iter().map(|(_, v)| v.clone()).collect())
            }
        };
        assert_eq!(got, *found);
        // Its ancestors are the groups on its path.
        let ancestors = source.ancestors(Axis::Rows, step.at.row);
        let depth_of = match &got {
            Found::Label(path) => path.len() - 1,
            Found::Cell(..) => asked.levels.len(),
        };
        assert_eq!(ancestors.len(), depth_of);
        from = Some(step.at);
    }
    if !want.is_empty() {
        let step = source
            .find_next(&rows, &query, from, FindDirection::Next)
            .unwrap();
        assert!(step.wrapped, "past the last it wraps");
        assert_eq!(step.index, Some(0));
    } else {
        assert_eq!(
            source.find_next(&rows, &query, None, FindDirection::Next),
            None
        );
    }
}

/// The Leaves of each group shown, in order, by the group's path.
fn runs(source: &mut GroupedMemory<Record>, items: &[(Key, Item)]) -> Vec<(String, Vec<Key>)> {
    let mut runs: Vec<(String, Vec<Key>)> = Vec::new();
    for (key, item) in items {
        if let Item::Leaf(_) = item {
            let path = format!("{:?}", source.ancestors(Axis::Rows, *key));
            match runs.iter_mut().find(|(p, _)| *p == path) {
                Some((_, leaves)) => leaves.push(*key),
                None => runs.push((path, vec![*key])),
            }
        }
    }
    runs
}

#[hegel::test(test_cases = 200)]
fn a_leaf_moves_within_its_group_and_nowhere_else(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).min_size(1).max_size(50));
    let mut source = source(rows.len());
    let (asked, mut spec) = tc.draw(spec(rows.len()));
    let mut grouping = spec.grouping.clone().unwrap();
    grouping.rows.expand = Expand::default();
    spec.grouping = Some(grouping);
    source.view(&rows, &spec);
    let before = shown(&mut source, &rows);
    let leaves: Vec<usize> = (0..before.len())
        .filter(|&i| matches!(before[i].1, Item::Leaf(_)))
        .collect();
    if leaves.is_empty() {
        return;
    }
    let from = leaves[tc.draw(gs::integers::<usize>().max_value(leaves.len() - 1))];
    let key = before[from].0;
    // The run of Leaves around it: its group's.
    let (mut start, mut end) = (from, from + 1);
    while start > 0 && matches!(before[start - 1].1, Item::Leaf(_)) {
        start -= 1;
    }
    while end < before.len() && matches!(before[end].1, Item::Leaf(_)) {
        end += 1;
    }
    let to = tc.draw(gs::integers::<usize>().max_value(before.len()));
    let moved = source.move_items(&rows, Axis::Rows, &[key], to as u64);
    if !(start..=end).contains(&to) {
        assert_eq!(
            moved.map(|_| ()),
            Err("Can't move between groups".to_string())
        );
        let keys: Vec<Key> = shown(&mut source, &rows).iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, before.iter().map(|(k, _)| *k).collect::<Vec<_>>());
        return;
    }
    let moved = moved.expect("a move within the group");
    assert_eq!(moved.notices, vec![(Axis::Rows, ChangeNotice::Reordered)]);
    // Every group keeps its Leaves in order but the one moved; with a sort
    // on the Leaves dropped, the groups may take their unsorted order.
    let mut want = runs(&mut source, &before);
    let run = (want.iter_mut())
        .find(|(_, leaves)| leaves.contains(&key))
        .unwrap();
    let item = run.1.remove(from - start);
    run.1
        .insert(if to > from { to - 1 } else { to } - start, item);
    want.sort();
    let after = shown(&mut source, &rows);
    let mut got = runs(&mut source, &after);
    got.sort();
    assert_eq!(got, want);
    let sorts_leaves = (spec.rows.sort.columns().iter())
        .any(|c| !asked.levels.iter().any(|l| l.0 as u64 == c.key.0));
    assert_eq!(
        moved.view.is_some(),
        sorts_leaves,
        "a sort on the Leaves is dropped"
    );

    // A Summary Item never moves.
    let summary = before
        .iter()
        .find(|(_, i)| matches!(i, Item::Summary(..)))
        .unwrap()
        .0;
    assert_eq!(
        source
            .move_items(&rows, Axis::Rows, &[summary], 0)
            .map(|_| ()),
        Err("Group rows and totals can't be moved".to_string())
    );
}

/// A value's place among ascending values as the pivot orders column
/// groups: numbers ascending, blanks last.
fn ascending(a: &Option<i64>, b: &Option<i64>) -> Ordering {
    order(*a, *b, SortDirection::Ascending)
}

#[hegel::test(test_cases = 200)]
fn a_pivot_matches_a_whole_table_cross_tabulation(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(60));
    let cols = tc.draw(gs::permutations(vec![0usize, 1, 2]));
    let (r, c, summed) = (cols[0], cols[1], cols[2]);
    // Columns unsorted, or by the Grand Total row's count or by their own
    // value, either way.
    let direction = gs::sampled_from(vec![SortDirection::Ascending, SortDirection::Descending]);
    let col_sort = tc.draw(gs::optional(hegel::tuples!(
        gs::sampled_from(vec![binnacle_core::GRAND_TOTAL, Key(c as u64)]),
        direction
    )));
    let placement = || gs::sampled_from(vec![Placement::Before, Placement::After, Placement::Off]);
    let (row_total, col_total) = (tc.draw(placement()), tc.draw(placement()));
    let mut source = source(rows.len());
    source.view(&rows, &ViewSpec::default());
    let mut grouping = Grouping::rows_by(Key(r as u64));
    grouping.col_levels = vec![Level::field(Key(c as u64))];
    grouping.values = vec![
        AggregateValue::count(),
        AggregateValue::of(Key(summed as u64), "sum"),
    ];
    grouping.rows.grand_total = row_total;
    grouping.cols.grand_total = col_total;
    let mut spec = ViewSpec::default().with_grouping(Some(grouping));
    if let Some((key, direction)) = col_sort {
        spec.cols.sort = SortSpec::new(vec![SortColumn { key, direction }]);
    }
    let viewed = source.view(&rows, &spec);
    assert!(source.pivot());
    assert_eq!(viewed.applied.cols.sort, spec.cols.sort);

    let distinct = |i: usize| {
        let mut v: Vec<Option<i64>> = rows.iter().map(|x| x[i]).collect();
        v.sort_by(ascending);
        v.dedup();
        v
    };
    let count_of = |cv: &Option<i64>| rows.iter().filter(|x| x[c] == *cv).count();
    let mut col_values = distinct(c);
    match col_sort {
        Some((binnacle_core::GRAND_TOTAL, d)) => {
            col_values.sort_by(|a, b| order(Some(count_of(a)), Some(count_of(b)), d));
        }
        Some((_, d)) => col_values.sort_by(|a, b| order(*a, *b, d)),
        None => {}
    }
    /// Where a Grand Total placed `at` goes among `items`.
    fn placed<T>(items: Vec<T>, totals: Vec<T>, at: Placement) -> Vec<T> {
        match at {
            Placement::Before => totals.into_iter().chain(items).collect(),
            Placement::After => items.into_iter().chain(totals).collect(),
            Placement::Off => items,
        }
    }
    // Each row's group value, `None` for the Grand Total.
    let row_values = placed(
        distinct(r).into_iter().map(Some).collect(),
        vec![None],
        row_total,
    );
    let ids = ["count()".to_string(), format!("sum({summed})")];

    let ncols = source.count(Axis::Cols);
    assert!(
        viewed
            .notices
            .contains(&(Axis::Cols, ChangeNotice::Refiltered(ncols)))
    );
    let window = source.window(&rows, Axis::Cols, 0..ncols);
    let outlines = window.outlines.expect("pivot columns have outlines");
    // Each column as the value of its column group (`None` for an All
    // column) and its value's identity.
    let got: Vec<(Option<Value>, String)> = (window.keys.iter().zip(&outlines))
        .map(|(_, o)| {
            let group = o.group.as_ref().unwrap();
            let Value::Text(id) = &group.value else {
                panic!()
            };
            let up = o
                .ancestors
                .first()
                .map(|&a| source.summary(a).unwrap().path[0].1.clone());
            (up, id.clone())
        })
        .collect();
    let want: Vec<(Option<Value>, String)> = placed(
        (col_values.iter())
            .flat_map(|v| ids.iter().map(move |id| (Some(text(*v)), id.clone())))
            .collect(),
        ids.iter().map(|id| (None, id.clone())).collect(),
        col_total,
    );
    assert_eq!(got, want, "columns sorted {col_sort:?}");

    let nrows = source.count(Axis::Rows);
    assert_eq!(
        nrows as usize,
        row_values.len(),
        "one row per group and the Grand Total where placed, no Leaves"
    );
    let row_window = source.window(&rows, Axis::Rows, 0..nrows);
    let row_kinds: Vec<ItemKind> = row_window
        .outlines
        .unwrap()
        .iter()
        .map(|o| o.kind)
        .collect();
    let want_kinds: Vec<ItemKind> = (row_values.iter())
        .map(|v| match v {
            Some(_) => ItemKind::Group,
            None => ItemKind::GrandTotal,
        })
        .collect();
    assert_eq!(row_kinds, want_kinds);
    for (rk, rv) in row_window.keys.iter().zip(&row_values) {
        for ((ck, (cv, id)), _) in window.keys.iter().zip(&want).zip(&outlines) {
            let under: Vec<&Record> = (rows.iter())
                .filter(|x| {
                    rv.is_none_or(|rv| x[r] == rv) && cv.as_ref().is_none_or(|cv| text(x[c]) == *cv)
                })
                .collect();
            let figure = match id.as_str() {
                "count()" => Figure::Count(under.len() as u64),
                _ => {
                    let sums: Vec<i64> = under.iter().filter_map(|x| x[summed]).collect();
                    match sums.is_empty() {
                        true => Figure::Blank,
                        false => Figure::Number(sums.iter().sum::<i64>() as f64),
                    }
                }
            };
            // Groups that share no rows have no figure; a Grand Total always does.
            let want = (!under.is_empty() || rv.is_none() || cv.is_none()).then_some(figure);
            assert_eq!(
                source.cell(*rk, *ck),
                want,
                "row {rv:?}, column {cv:?} {id}"
            );
        }
    }
}

#[test]
fn closing_a_pivot_column_group_shows_its_total_columns_with_collapsed() {
    let rows: Vec<Record> = vec![
        [Some(1), Some(5), Some(10)],
        [Some(2), Some(6), Some(20)],
        [Some(1), Some(5), Some(30)],
    ];
    let mut source = source(rows.len());
    let mut grouping = Grouping::rows_by(Key(0));
    grouping.col_levels = vec![
        Level::Field {
            field: Key(1),
            subtotal: Placement::After,
        },
        Level::Values,
    ];
    grouping.values = vec![AggregateValue::count(), AggregateValue::of(Key(2), "sum")];
    let spec = |g: &Grouping| ViewSpec::default().with_grouping(Some(g.clone()));
    source.view(&rows, &spec(&grouping));
    let cols = |source: &mut GroupedMemory<Record>| {
        let n = source.count(Axis::Cols);
        source.window(&rows, Axis::Cols, 0..n)
    };
    let before = cols(&mut source);
    assert_eq!(
        before.keys.len(),
        8,
        "each port's count and sum, then a total of each"
    );
    let five = before.outlines.as_ref().unwrap()[0].ancestors[0];

    grouping.cols.expand = Expand::default().with(0, five, false);
    let viewed = source.view(&rows, &spec(&grouping));
    assert_eq!(
        viewed.notices,
        vec![(Axis::Cols, ChangeNotice::Collapsed { at: 0, n: 2 })]
    );
    let after = cols(&mut source);
    assert_eq!(after.keys[..], before.keys[2..], "5's total columns stay");
    let outlines = after.outlines.unwrap();
    for (outline, id) in outlines[..2].iter().zip(["count()", "sum(2)"]) {
        assert_eq!(
            (outline.kind, outline.ancestors.as_slice()),
            (ItemKind::Subtotal, [five].as_slice())
        );
        let group = outline.group.as_ref().unwrap();
        assert_eq!(
            (group.field, &group.value),
            (VALUES_FIELD, &Value::Text(id.into()))
        );
    }
    let one = source.window(&rows, Axis::Rows, 0..1).keys[0];
    assert_eq!(
        (
            source.cell(one, after.keys[0]),
            source.cell(one, after.keys[1])
        ),
        (Some(Figure::Count(2)), Some(Figure::Number(40.0))),
        "each of the closed group's values over its rows"
    );
}

/// A pivot by `r` on the rows and `c` on the columns, counting and summing
/// `summed`, the column Grand Total after and the rows' off.
fn crossed(r: usize, c: usize, summed: usize) -> Grouping {
    let mut grouping = Grouping::rows_by(Key(r as u64));
    grouping.col_levels = vec![Level::field(Key(c as u64))];
    grouping.values = vec![
        AggregateValue::count(),
        AggregateValue::of(Key(summed as u64), "sum"),
    ];
    grouping.cols.grand_total = Placement::After;
    grouping
}

/// Each pivot column of `source` by its Key: its column group's value
/// (`None` for an All column) and its value's index.
fn pivot_columns(
    source: &mut GroupedMemory<Record>,
    rows: &[Record],
) -> Vec<(Key, Option<Value>, usize)> {
    let n = source.count(Axis::Cols);
    let window = source.window(rows, Axis::Cols, 0..n);
    let outlines = window.outlines.expect("pivot columns have outlines");
    (window.keys.iter().zip(&outlines))
        .map(|(&k, o)| {
            let up = (o.ancestors.first()).map(|&a| source.summary(a).unwrap().path[0].1.clone());
            let Some(Value::Text(id)) = o.group.as_ref().map(|g| &g.value) else {
                panic!("a pivot column names its value")
            };
            let value = usize::from(id != "count()");
            (k, up, value)
        })
        .collect()
}

/// The value of each row group shown, in order.
fn row_groups(source: &mut GroupedMemory<Record>, rows: &[Record]) -> Vec<Value> {
    let n = source.count(Axis::Rows);
    let window = source.window(rows, Axis::Rows, 0..n);
    (window.keys.iter())
        .map(|&k| source.summary(k).unwrap().path[0].1.clone())
        .collect()
}

#[hegel::test(test_cases = 200)]
fn a_pivots_rows_sort_by_a_pivot_columns_figures(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).min_size(1).max_size(40));
    let cols = tc.draw(gs::permutations(vec![0usize, 1, 2]));
    let (r, c, summed) = (cols[0], cols[1], cols[2]);
    let mut source = source(rows.len());
    let spec = ViewSpec::default().with_grouping(Some(crossed(r, c, summed)));
    source.view(&rows, &spec);
    let columns = pivot_columns(&mut source, &rows);
    let (key, cv, value) = columns[tc.draw(gs::integers::<usize>().max_value(columns.len() - 1))]
        .clone();
    let direction = tc.draw(gs::sampled_from(vec![
        SortDirection::Ascending,
        SortDirection::Descending,
    ]));
    let mut sorted = spec.clone();
    sorted.rows.sort = SortSpec::new(vec![SortColumn { key, direction }]);
    let viewed = source.view(&rows, &sorted);
    assert_eq!(viewed.applied.rows.sort, sorted.rows.sort, "the column is there");

    // The groups ascending, then stably by their figure in the column: a
    // cell where no rows meet is blank, and blanks go last either way.
    let mut want: Vec<Option<i64>> = rows.iter().map(|x| x[r]).collect();
    want.sort_by(ascending);
    want.dedup();
    let figure = |rv: &Option<i64>| -> Option<i64> {
        let under: Vec<&Record> = (rows.iter())
            .filter(|x| x[r] == *rv && cv.as_ref().is_none_or(|cv| text(x[c]) == *cv))
            .collect();
        match value {
            _ if under.is_empty() => None,
            0 => Some(under.len() as i64),
            _ => {
                let sums: Vec<i64> = under.iter().filter_map(|x| x[summed]).collect();
                (!sums.is_empty()).then(|| sums.iter().sum())
            }
        }
    };
    want.sort_by(|a, b| order(figure(a), figure(b), direction));
    let want: Vec<Value> = want.into_iter().map(text).collect();
    assert_eq!(row_groups(&mut source, &rows), want, "by {cv:?} value {value}");
}

#[hegel::test(test_cases = 200)]
fn a_pivots_columns_filtered_by_the_grand_total_row_keep_those_whose_figure_passes(
    tc: TestCase,
) {
    let rows = tc.draw(gs::vecs(record()).max_size(40));
    let cols = tc.draw(gs::permutations(vec![0usize, 1, 2]));
    let (r, c, summed) = (cols[0], cols[1], cols[2]);
    // A comparison on the count, the first value.
    let compare = Compare {
        column: 0,
        at_least: tc.draw(gs::booleans()),
        threshold: tc.draw(gs::integers::<i64>().min_value(0).max_value(6)),
        not: tc.draw(gs::booleans()),
    };
    let mut condition = compare.condition();
    condition.key = binnacle_core::GRAND_TOTAL;
    let mut source = source(rows.len());
    let mut spec = ViewSpec::default().with_grouping(Some(crossed(r, c, summed)));
    spec.cols.filter = FilterSpec::default().with_conditions(vec![condition].into());
    let viewed = source.view(&rows, &spec);
    assert_eq!(viewed.applied.cols.filter, spec.cols.filter);

    let mut want: Vec<Option<i64>> = rows.iter().map(|x| x[c]).collect();
    want.sort_by(ascending);
    want.dedup();
    want.retain(|cv| {
        let count = rows.iter().filter(|x| x[c] == *cv).count();
        compare.passes(Some(count as f64))
    });
    let got: Vec<Value> = (pivot_columns(&mut source, &rows).into_iter())
        .filter_map(|(_, cv, value)| cv.filter(|_| value == 0))
        .collect();
    assert_eq!(got, want.into_iter().map(text).collect::<Vec<_>>());
}

/// Rows of regions 1 and 2 over quarters 1 and 2, the second quarter only
/// in region 2, each row weighing its region.
fn quarters() -> Vec<Record> {
    vec![
        [Some(1), Some(1), Some(1)],
        [Some(2), Some(1), Some(2)],
        [Some(2), Some(2), Some(2)],
    ]
}

#[test]
fn a_sort_term_on_a_pivot_column_the_data_lacks_is_left_out_until_it_returns() {
    let rows = quarters();
    let mut source = source(rows.len());
    let spec = ViewSpec::default().with_grouping(Some(crossed(0, 1, 2)));
    source.view(&rows, &spec);
    let q2 = (pivot_columns(&mut source, &rows).into_iter())
        .find(|(_, cv, value)| *cv == Some(text(Some(2))) && *value == 1)
        .map(|(k, ..)| k)
        .unwrap();
    let mut sorted = spec.clone();
    sorted.rows.sort = SortSpec::new(vec![SortColumn::descending(q2)]);
    assert_eq!(source.view(&rows, &sorted).applied.rows.sort, sorted.rows.sort);
    assert_eq!(row_groups(&mut source, &rows), [text(Some(2)), text(Some(1))]);

    // A slicer leaves out the second quarter, and its sort with it.
    let mut sliced = sorted.clone();
    let slicer = Condition::compare(Key(1), Comparison::Less(Scalar::Number(2.0)));
    sliced.grouping.as_mut().unwrap().slicers = vec![slicer];
    assert_eq!(source.view(&rows, &sliced).applied.rows.sort, SortSpec::default());
    assert_eq!(row_groups(&mut source, &rows), [text(Some(1)), text(Some(2))]);

    assert_eq!(source.view(&rows, &sorted).applied.rows.sort, sorted.rows.sort);
    assert_eq!(row_groups(&mut source, &rows), [text(Some(2)), text(Some(1))]);
}

#[test]
fn a_pivot_columns_key_fits_until_the_column_levels_change() {
    let rows = quarters();
    let mut source = source(rows.len());
    let spec = ViewSpec::default().with_grouping(Some(crossed(0, 1, 2)));
    source.view(&rows, &spec);
    let columns = pivot_columns(&mut source, &rows);
    let fits = |source: &GroupedMemory<Record>| -> Vec<bool> {
        (columns.iter()).map(|(k, ..)| source.fits(Axis::Cols, *k)).collect()
    };
    assert!(fits(&source).iter().all(|&f| f));

    // Slicing the data away keeps them.
    let mut sliced = spec.clone();
    let slicer = Condition::compare(Key(1), Comparison::Less(Scalar::Number(0.0)));
    sliced.grouping.as_mut().unwrap().slicers = vec![slicer];
    source.view(&rows, &sliced);
    assert!(fits(&source).iter().all(|&f| f));

    // Grouping the columns by another field leaves only the All columns.
    let mut other = spec.clone();
    other.grouping.as_mut().unwrap().col_levels = vec![Level::field(Key(2))];
    source.view(&rows, &other);
    let all: Vec<bool> = columns.iter().map(|(_, cv, _)| cv.is_none()).collect();
    assert_eq!(fits(&source), all);
    assert!(source.fits(Axis::Rows, Key(0)), "a Leaf Key keeps today's rule");
}
