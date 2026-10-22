//! The In-Memory Table Source under the grouping layer (`GroupedMemory`)
//! against a naive model: rows grouped by one column, groups in ascending
//! order of their value (blanks last) or in the user's direction when the
//! user sorts by that column, Leaves in source order or the user's sort
//! within their group, hidden rows left out, and the grouped column leaving
//! the columns.

use binnacle_core::{
    Axis, ChangeNotice, Grouping, ItemKind, Key, SortColumn, SortDirection, SortSpec, Value,
    ViewSpec,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::MemorySource;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const COLS: usize = 3;

type Record = [Option<i64>; COLS];

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    let value = || gs::optional(gs::integers::<i64>().min_value(-3).max_value(12));
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
fn order(a: Option<i64>, b: Option<i64>, direction: SortDirection) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => match direction {
            SortDirection::Ascending => a.cmp(&b),
            SortDirection::Descending => b.cmp(&a),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// What the model shows: each group's value with its count, then its Leaf
/// Keys.
fn model(
    rows: &[Record],
    field: usize,
    sort: &SortSpec,
    hidden: &[Key],
) -> Vec<Result<(Value, u64), Key>> {
    let mut kept: Vec<usize> = (0..rows.len())
        .filter(|&i| !hidden.contains(&Key(i as u64)))
        .collect();
    let group_direction = match sort.find(Key(field as u64)) {
        Some((_, d)) => d,
        None => SortDirection::Ascending,
    };
    kept.sort_by(|&a, &b| {
        let mut o = order(rows[a][field], rows[b][field], group_direction);
        for c in sort.columns() {
            o = o.then(order(
                rows[a][c.key.0 as usize],
                rows[b][c.key.0 as usize],
                c.direction,
            ));
        }
        o
    });
    let mut out = Vec::new();
    let mut i = 0;
    while i < kept.len() {
        let value = rows[kept[i]][field];
        let run: Vec<usize> = kept[i..]
            .iter()
            .take_while(|&&r| rows[r][field] == value)
            .copied()
            .collect();
        out.push(Ok((text(value), run.len() as u64)));
        out.extend(run.iter().map(|&r| Err(Key(r as u64))));
        i += run.len();
    }
    out
}

#[hegel::composite]
fn spec(tc: &TestCase, rows: usize) -> (usize, ViewSpec) {
    let field = tc.draw(gs::integers::<usize>().max_value(COLS - 1));
    let sort = match tc.draw(gs::optional(
        gs::integers::<u64>().max_value(COLS as u64 - 1),
    )) {
        Some(c) => match tc.draw(gs::booleans()) {
            true => SortSpec::new(vec![SortColumn::ascending(Key(c))]),
            false => SortSpec::new(vec![SortColumn::descending(Key(c))]),
        },
        None => SortSpec::unsorted(),
    };
    let hidden =
        tc.draw(gs::vecs(gs::integers::<u64>().max_value(rows as u64).map(Key)).max_size(4));
    let spec = ViewSpec::default()
        .with_sort(Axis::Rows, sort)
        .with_hidden(Axis::Rows, hidden)
        .with_grouping(Some(Grouping::rows_by(Key(field as u64))));
    (field, spec)
}

#[hegel::test(test_cases = 300)]
fn grouped_rows_match_a_whole_table_grouping(tc: TestCase) {
    let rows = tc.draw(gs::vecs(record()).max_size(90));
    let mut source = source(rows.len());
    source.view(&rows, &ViewSpec::default());
    let (field, spec) = tc.draw(spec(rows.len()));
    let viewed = source.view(&rows, &spec);

    let hidden: Vec<Key> = spec
        .rows
        .hidden
        .iter()
        .filter(|k| (k.0 as usize) < rows.len())
        .copied()
        .collect();
    let want = model(&rows, field, &spec.rows.sort, &hidden);
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
        "the level's sort is not added"
    );
    assert!(
        viewed.applied.cols.hidden.is_empty(),
        "the grouped column is not written in"
    );

    let window = source.window(&rows, Axis::Rows, 0..count);
    let outlines = window.outlines.expect("grouped rows have outlines");
    let got: Vec<Result<(Value, u64), Key>> = window
        .keys
        .iter()
        .zip(&outlines)
        .map(|(&key, outline)| match outline.kind {
            ItemKind::Group => {
                let summary = source.summary(key).expect("a known Group Item");
                let count = match summary.aggregates {
                    [Figure::Count(n)] => *n,
                    other => panic!("count expected, got {other:?}"),
                };
                assert_eq!(summary.children, count);
                Ok((outline.group.clone().unwrap().value, count))
            }
            _ => Err(key),
        })
        .collect();
    assert_eq!(got, want);

    // The grouped column leaves the columns.
    let cols = source.window(&rows, Axis::Cols, 0..COLS as u64).keys;
    let want_cols: Vec<Key> = (0..COLS as u64)
        .filter(|&c| c != field as u64)
        .map(Key)
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
