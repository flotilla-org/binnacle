//! Seam 1 scenarios and properties for the column-major testkit table: the
//! records on their side, each record a column and each field a row. It is
//! the row-major records seen through `Transposed`, so whatever the Grid
//! does for rows it must do for columns here.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Condition, FilterSpec, FindDirection, FindMatch,
    FindQuery, Grid, HeaderClick, HeaderSort, Key, Modifiers, Searched, Snapshot, SortColumn,
    SortSpec, TextQuery, Value, ViewSpec, Viewport,
};
use binnacle_testkit::{
    ColumnMajorTable, PumpSource, Record, RecordTable, Scenario, ScenarioSource,
};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const V: Viewport = Viewport {
    width: 40.0,
    height: 5.0,
};

/// Six records over three fields: field 0 holds `[5, 13, 8, 31, 9, 23]`,
/// field 1 the record's index times 10, field 2 nothing. Fields 0 and 1
/// sort. A 1-high header band and 4-wide row header band; fields of 1,
/// records of 4 with a Gap of 1.
fn records() -> Scenario<ColumnMajorTable> {
    let field0 = [5, 13, 8, 31, 9, 23];
    let records = (0..6)
        .map(|i| vec![Some(field0[i]), Some(i as i64 * 10), None])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(6, 4.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, ColumnMajorTable::new(records, 3, [0, 1]))
}

#[test]
fn each_record_is_a_column_and_the_records_sort_filter_and_change_as_columns() {
    let mut s = records();
    let mut out = s.step("six records as columns, three fields as rows", V);
    s.input(|g| {
        g.request_view(ViewSpec::default().with_sort(
            Axis::Cols,
            SortSpec::new(vec![SortColumn::descending(Key(0))]),
        ))
    });
    out += &s.step(
        "the columns sorted by field 0, descending: 31, 23, 13, 9, 8, 5",
        V,
    );
    s.input(|g| g.set_quick_filter(Axis::Cols, Some(TextQuery::new("3"))));
    out += &s.step(
        "quick filter \"3\" on the columns: the records with a 3 in a field (31 and its 30, 23, 13), 3 of 6",
        V,
    );
    let notice = s.source.filter(|r| r[1].is_some_and(|v| v >= 20));
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step(
        "the host's own filter on the records as well, field 1 at least 20: 31 and 23 stay, 13 goes",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn edits_appends_and_replacements_reach_the_columns() {
    let mut s = records();
    s.frame(V);
    let notices = s.source.append(vec![vec![Some(1), Some(60), Some(7)]]);
    assert_eq!(notices.len(), 1);
    s.input(|g| notices.into_iter().for_each(|(a, n)| g.notify(a, n)));
    let snap = s.frame(V);
    assert_eq!(snap.count(Axis::Cols), 7);
    assert_eq!(s.source.records()[6], vec![Some(1), Some(60), Some(7)]);
    s.source.edit(6, 2, Some(8));
    assert_eq!(s.source.records()[6][2], Some(8));
    let notices = s.source.replace(vec![vec![None, None, None]; 2]);
    s.input(|g| notices.into_iter().for_each(|(axis, n)| g.notify(axis, n)));
    assert_eq!(s.frame(V).count(Axis::Cols), 2);
}

#[test]
fn fields_and_records_move_as_the_records_columns_and_rows_do() {
    let mut s = records();
    s.frame(V);
    let moved = s.source.move_items(Axis::Rows, &[Key(0)], 3).unwrap();
    assert_eq!(moved.notices, vec![(Axis::Rows, ChangeNotice::Reordered)]);
    s.input(|g| g.notify(Axis::Rows, ChangeNotice::Reordered));
    let snap = s.frame(V);
    let fields: Vec<_> = snap.rows().iter().map(|r| r.key).collect();
    assert_eq!(fields, [Some(Key(1)), Some(Key(2)), Some(Key(0))]);
    let moved = s.source.move_items(Axis::Cols, &[Key(0)], 2).unwrap();
    assert_eq!(moved.notices, vec![(Axis::Cols, ChangeNotice::Reordered)]);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    let snap = s.frame(V);
    let records: Vec<_> = snap.cols().iter().take(3).map(|c| c.key).collect();
    assert_eq!(records, [Some(Key(1)), Some(Key(0)), Some(Key(2))]);
}

#[test]
fn a_find_naming_the_columns_it_searches_finds_nothing_on_its_side() {
    let mut s = records();
    let named = TextQuery::new("3").searching(Searched::Keys(vec![Key(1)]));
    let next = |s: &mut Scenario<ColumnMajorTable>, query| {
        s.source
            .find_next(&FindQuery::new(query), None, FindDirection::Next)
            .map(|step| step.at)
    };
    assert_eq!(next(&mut s, named), None);
    // Searching every shown column, 13 in record 1 is the first match.
    let shown = next(&mut s, TextQuery::new("3"));
    assert_eq!(shown, Some(FindMatch::new(Key(0), Key(1))));
}

/// Records of `fields` small integers or nulls, so sorts tie and nulls
/// matter.
#[hegel::composite]
fn table(tc: &TestCase) -> (Vec<Record>, usize) {
    let fields = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let value = gs::optional(gs::integers::<i64>().min_value(-3).max_value(3));
    let record = gs::vecs(value).min_size(fields).max_size(fields);
    (tc.draw(gs::vecs(record).max_size(20)), fields)
}

/// A sort by some fields and, maybe, a quick filter, for the axis that
/// holds the records.
#[hegel::composite]
fn record_view(tc: &TestCase, fields: usize) -> (SortSpec, Option<TextQuery>) {
    let field = gs::integers::<u64>().max_value(fields as u64 - 1);
    let by = tc.draw(gs::vecs(field).max_size(fields).unique(true));
    let sort = (by.into_iter())
        .map(|f| match tc.draw(gs::booleans()) {
            true => SortColumn::ascending(Key(f)),
            false => SortColumn::descending(Key(f)),
        })
        .collect();
    let quick = tc.draw(gs::optional(gs::sampled_from(vec!["1", "-", "3", "2"])));
    let quick = quick.map(|text| {
        // The shown fields, or some named ones: either way, items of the
        // other axis, which turning leaves where they are.
        let named = tc.draw(gs::optional(
            gs::vecs(gs::integers::<u64>().max_value(fields as u64 - 1)).max_size(fields),
        ));
        let query = TextQuery::new(text).regex(tc.draw(gs::booleans()));
        match named {
            Some(keys) => query.searching(Searched::Keys(keys.into_iter().map(Key).collect())),
            None => query,
        }
    });
    (SortSpec::new(sort), quick)
}

/// The Keys `axis` shows, in order, after `spec` is requested and
/// answered, and the axis's total: the viewport holds every item.
fn shown<S: ScenarioSource>(
    s: &mut Scenario<S>,
    axis: Axis,
    spec: ViewSpec,
) -> (Vec<Option<Key>>, Option<u64>) {
    s.input(|g| g.request_view(spec));
    let all = Viewport::new(1000.0, 1000.0);
    let snap = s.frame(all);
    let keys = snap.strip(axis).iter().map(|i| i.key).collect();
    (keys, snap.total(axis))
}

#[hegel::test(test_cases = 200)]
fn the_column_major_table_is_the_row_major_one_on_its_side(tc: TestCase) {
    let (records, fields) = tc.draw(table());
    let (sort, quick) = tc.draw(record_view(fields));
    let all: Vec<u64> = (0..fields as u64).collect();
    let n = records.len() as u64;
    let mut rows = Scenario::new(
        Grid::new(AxisConfig::new(n, 1.0), AxisConfig::new(fields as u64, 1.0)),
        RecordTable::new(records.clone(), fields, all.clone()),
    );
    let mut cols = Scenario::new(
        Grid::new(AxisConfig::new(fields as u64, 1.0), AxisConfig::new(n, 1.0)),
        ColumnMajorTable::new(records, fields, all),
    );
    let filter = FilterSpec::default().with_quick(quick);
    let spec = |axis| {
        ViewSpec::default()
            .with_sort(axis, sort.clone())
            .with_filter(axis, filter.clone())
    };
    let row_major = shown(&mut rows, Axis::Rows, spec(Axis::Rows));
    let column_major = shown(&mut cols, Axis::Cols, spec(Axis::Cols));
    assert_eq!(row_major, column_major);
    assert_eq!(rows.grid.snapshot().applied_view(), &spec(Axis::Rows));
    assert_eq!(cols.grid.snapshot().applied_view(), &spec(Axis::Cols));
}

/// The first `steps` matches find steps to, from the top-left cell.
fn found<S: ScenarioSource>(s: &mut Scenario<S>, query: FindQuery, steps: usize) -> Vec<FindMatch> {
    let all = Viewport::new(1000.0, 1000.0);
    s.frame(all);
    s.input(|g| g.set_find(Some(query)));
    let mut matches = Vec::new();
    for _ in 0..steps {
        s.input(|g| g.action(Action::FindNext));
        if let Some(m) = s.frame(all).find_match() {
            matches.push(m);
        }
    }
    matches
}

#[hegel::test(test_cases = 100)]
fn find_steps_through_the_column_major_table_as_through_the_row_major_one_on_its_side(
    tc: TestCase,
) {
    let (records, fields) = tc.draw(table());
    // Each field's values are its own text, so these match some cells.
    let text = tc.draw(gs::sampled_from(vec!["1", "-", "3", "2"]));
    let by = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let steps = tc.draw(gs::integers::<usize>().max_value(12));
    let n = records.len() as u64;
    let mut rows = Scenario::new(
        Grid::new(AxisConfig::new(n, 1.0), AxisConfig::new(fields as u64, 1.0)),
        RecordTable::new(records.clone(), fields, 0..fields as u64),
    );
    let mut cols = Scenario::new(
        Grid::new(AxisConfig::new(fields as u64, 1.0), AxisConfig::new(n, 1.0)),
        ColumnMajorTable::new(records, fields, 0..fields as u64),
    );
    let query = |by| FindQuery::new(TextQuery::new(text)).by(by);
    let row_major = found(&mut rows, query(by), steps);
    let column_major = found(&mut cols, query(by.other()), steps);
    let turned: Vec<_> = (column_major.into_iter())
        .map(|m| FindMatch::new(m.col, m.row))
        .collect();
    assert_eq!(row_major, turned);
}

#[hegel::test(test_cases = 200)]
fn row_header_clicks_sort_the_column_major_table_as_column_header_clicks_sort_it_upright(
    tc: TestCase,
) {
    let (records, fields) = tc.draw(table());
    let field = gs::integers::<u64>().max_value(fields as u64 - 1);
    let clicks = tc.draw(gs::vecs(hegel::tuples!(field, gs::booleans())).max_size(6));
    let n = records.len() as u64;
    let all: Vec<u64> = (0..fields as u64).collect();
    // Items 2 wide and 1 high, and header bands to match, so a header's
    // middle is easy to find.
    let mut rows = Scenario::new(
        Grid::new(
            AxisConfig::new(n, 1.0).header(1.0),
            AxisConfig::new(fields as u64, 2.0).header(2.0),
        ),
        RecordTable::new(records.clone(), fields, all.clone()),
    );
    let mut cols = Scenario::new(
        Grid::new(
            AxisConfig::new(fields as u64, 1.0).header(1.0),
            AxisConfig::new(n, 2.0).header(2.0),
        ),
        ColumnMajorTable::new(records, fields, all),
    );
    cols.input(|g| {
        g.set_header_click(Axis::Rows, HeaderClick::Sort);
        g.set_header_click(Axis::Cols, HeaderClick::Select);
    });
    let screen = Viewport::new(1000.0, 1000.0);
    rows.frame(screen);
    cols.frame(screen);
    for (f, shift) in clicks {
        let modifiers = if shift {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        rows.click(2.0 + 2.0 * f as f32 + 1.0, 0.5, modifiers);
        cols.click(1.0, 1.0 + f as f32 + 0.5, modifiers);
        rows.frame(screen);
        cols.frame(screen);
    }
    let order = |s: Snapshot<'_>, axis| -> Vec<Option<Key>> {
        s.strip(axis).iter().map(|i| i.key).collect()
    };
    let row_major = order(rows.frame(screen), Axis::Rows);
    let column_major = order(cols.frame(screen), Axis::Cols);
    assert_eq!(row_major, column_major);
    assert_eq!(
        rows.grid.snapshot().applied_view().rows.sort,
        cols.grid.snapshot().applied_view().cols.sort
    );
    // The fields' headers show the sort the records' headers do upright.
    let marks =
        |s: Snapshot<'_>, axis| -> Vec<HeaderSort> { s.headers(axis).map(|h| h.sort).collect() };
    assert_eq!(
        marks(rows.frame(screen), Axis::Cols),
        marks(cols.frame(screen), Axis::Rows)
    );
}

/// A filter on `axis` of a table `rows` by `cols`: maybe a quick filter,
/// and conditions on items of the other axis, some past its end.
#[hegel::composite]
fn axis_filter(tc: &TestCase, axis: Axis, rows: usize, cols: usize) -> FilterSpec {
    let across = match axis {
        Axis::Rows => cols,
        Axis::Cols => rows,
    } as u64;
    // Pieces of the records' values, a pattern, and one RegExp rejects.
    let text = || gs::sampled_from(vec!["1", "-", "3", "2", "^-?[12]$", "^("]);
    let quick = tc.draw(gs::optional(text())).map(|t| {
        let query = TextQuery::new(t)
            .regex(t.starts_with('^'))
            .case_sensitive(tc.draw(gs::booleans()));
        match tc.draw(gs::booleans()) {
            true => query,
            false => {
                let keys = gs::vecs(gs::integers::<u64>().max_value(across)).max_size(2);
                query.searching(Searched::Keys(tc.draw(keys).into_iter().map(Key).collect()))
            }
        }
    });
    let condition = || {
        hegel::tuples!(
            gs::integers::<u64>().max_value(across),
            gs::booleans(),
            text(),
            gs::booleans(),
            gs::booleans()
        )
        .map(|(key, one_of, t, not, case)| {
            let condition = match one_of {
                true => Condition::one_of(Key(key), vec![Value::Text(t.into()), Value::Blank]),
                false => {
                    let query = TextQuery::new(t).regex(t.starts_with('^'));
                    Condition::text(Key(key), query.case_sensitive(case))
                }
            };
            condition.not(not)
        })
    };
    let conditions = tc.draw(gs::vecs(condition()).max_size(3));
    FilterSpec::default()
        .with_quick(quick)
        .with_conditions(conditions.into())
}

/// Story 55: filtering the columns by their rows' values, natively in the
/// In-Memory Table Source, shows what filtering the rows of the same table
/// turned on its side shows, and the other way about: the Grid over the
/// records and the Grid over their transpose turned back agree on both
/// axes, their totals and the View Spec applied.
#[hegel::test(test_cases = 200)]
fn the_columns_filter_natively_as_the_turned_tables_rows_do(tc: TestCase) {
    let (records, fields) = tc.draw(table());
    let n = records.len();
    let transpose: Vec<Record> = (0..fields)
        .map(|f| records.iter().map(|r| r[f]).collect())
        .collect();
    let grid = || {
        Grid::new(
            AxisConfig::new(n as u64, 1.0),
            AxisConfig::new(fields as u64, 1.0),
        )
    };
    let mut native = Scenario::new(grid(), RecordTable::new(records, fields, 0..fields as u64));
    let mut turned = Scenario::new(grid(), ColumnMajorTable::new(transpose, n, 0..n as u64));
    let spec = ViewSpec::default()
        .with_filter(Axis::Rows, tc.draw(axis_filter(Axis::Rows, n, fields)))
        .with_filter(Axis::Cols, tc.draw(axis_filter(Axis::Cols, n, fields)));
    tc.note(&format!("{spec:?}"));
    let all = Viewport::new(1000.0, 1000.0);
    let mut seen = Vec::new();
    for s in [&mut native as &mut dyn Shows, &mut turned] {
        seen.push(s.shows(spec.clone(), all));
    }
    assert_eq!(seen[0], seen[1]);
}

/// What a scenario shows on both axes once a View Spec is answered.
type Shown = ([Vec<Option<Key>>; 2], [Option<u64>; 2], ViewSpec);

trait Shows {
    fn shows(&mut self, spec: ViewSpec, viewport: Viewport) -> Shown;
}

impl<S: ScenarioSource> Shows for Scenario<S> {
    fn shows(&mut self, spec: ViewSpec, viewport: Viewport) -> Shown {
        self.input(|g| g.request_view(spec));
        let snap = self.frame(viewport);
        let keys = |axis| snap.strip(axis).iter().map(|i| i.key).collect();
        (
            [keys(Axis::Rows), keys(Axis::Cols)],
            [snap.total(Axis::Rows), snap.total(Axis::Cols)],
            snap.applied_view().clone(),
        )
    }
}
