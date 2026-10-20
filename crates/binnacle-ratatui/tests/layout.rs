//! Saving and restoring a layout through the Grid's public API: a layout
//! saved from one Grid and restored onto another over the same records
//! saves back the same; restoring Keys the source lacks falls back as
//! decided; and a restore before the source answers shows the view
//! pending while pins, sizes and bands apply at once.

use binnacle_core::{
    Axis, AxisConfig, Condition, Edge, FilterSpec, Grid, Key, SortColumn, SortSpec, TextQuery,
    Value, ViewSpec, Viewport,
};
use binnacle_ratatui::{LAYOUT_VERSION, SavedLayout, restore_layout, save_layout};
use binnacle_testkit::{ColumnMajorTable, Record, RecordTable, Scenario, ScenarioSource};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const ROWS: u64 = 12;
const COLS: u64 = 3;
const V: Viewport = Viewport {
    width: 60.0,
    height: 20.0,
};

fn records() -> Vec<Record> {
    (0..ROWS as i64)
        .map(|i| vec![Some(i * 7 % 5), Some(i % 3), (i % 4 != 0).then_some(i)])
        .collect()
}

fn table() -> Scenario<RecordTable> {
    let grid = Grid::new(
        AxisConfig::new(ROWS, 1.0).header(1.0),
        AxisConfig::new(COLS, 8.0).header(4.0).gap(1.0),
    );
    let mut s = Scenario::new(grid, RecordTable::new(records(), COLS as usize, [0, 1, 2]));
    s.input(|g| {
        g.set_hide_support(Axis::Rows, true);
        g.set_hide_support(Axis::Cols, true);
    });
    s.frame(V);
    s
}

/// Puts the Grid of `s` into some state by the inputs a user has: pins,
/// sizes, band thicknesses and a View Spec, with Keys the table lacks
/// among them.
fn scramble(tc: &TestCase, s: &mut Scenario<RecordTable>) {
    let row = || gs::integers::<u64>().max_value(ROWS + 2).map(Key);
    let col = || gs::integers::<u64>().max_value(COLS + 1).map(Key);
    let edge = || gs::sampled_from(Edge::ALL.to_vec());
    for (key, e) in tc.draw(gs::vecs(hegel::tuples!(col(), edge())).max_size(3)) {
        s.input(|g| g.pin(Axis::Cols, e, key));
    }
    for (key, e) in tc.draw(gs::vecs(hegel::tuples!(row(), edge())).max_size(3)) {
        s.input(|g| g.pin(Axis::Rows, e, key));
    }
    // Any f32: the Grid ignores or caps what it does not take.
    let size = gs::floats::<f32>;
    for (key, sz) in tc.draw(gs::vecs(hegel::tuples!(col(), size())).max_size(3)) {
        s.input(|g| g.set_size(Axis::Cols, key, sz));
    }
    for (key, sz) in tc.draw(gs::vecs(hegel::tuples!(row(), size())).max_size(2)) {
        s.input(|g| g.set_size(Axis::Rows, key, sz));
    }
    for axis in Axis::ALL {
        if tc.draw(gs::booleans()) {
            let band = tc.draw(gs::floats::<f32>());
            s.input(|g| g.set_header(axis, band));
        }
    }
    let mut spec = ViewSpec::default();
    for axis in Axis::ALL {
        // Keys of the other axis, which sort and filter this one.
        let across = || match axis {
            Axis::Rows => col(),
            Axis::Cols => row(),
        };
        let mut sort: Vec<SortColumn> = Vec::new();
        for (key, descending) in
            tc.draw(gs::vecs(hegel::tuples!(across(), gs::booleans())).max_size(2))
        {
            if !sort.iter().any(|c| c.key == key) {
                sort.push(match descending {
                    true => SortColumn::descending(key),
                    false => SortColumn::ascending(key),
                });
            }
        }
        let text = || gs::sampled_from(vec!["1", "2", "", "("]).map(TextQuery::new);
        let quick = tc.draw(gs::optional(hegel::tuples!(text(), gs::booleans())));
        let quick = quick.map(|(q, regex)| q.regex(regex));
        let mut conditions = Vec::new();
        for (key, one_of, not) in
            tc.draw(gs::vecs(hegel::tuples!(across(), gs::booleans(), gs::booleans())).max_size(2))
        {
            let condition = match one_of {
                true => Condition::one_of(key, vec![Value::of(Some("1")), Value::Blank]),
                false => Condition::text(key, tc.draw(text())),
            };
            conditions.push(condition.not(not));
        }
        let hidden = match axis {
            Axis::Rows => tc.draw(gs::vecs(row()).max_size(3)),
            Axis::Cols => tc.draw(gs::vecs(col()).max_size(2)),
        };
        spec = spec
            .with_sort(axis, SortSpec::new(sort))
            .with_filter(
                axis,
                FilterSpec::default()
                    .with_quick(quick)
                    .with_conditions(conditions.into()),
            )
            .with_hidden(axis, hidden);
    }
    s.input(|g| g.request_view(spec));
    s.frame(V);
}

/// Restores `layout` onto the Grid of `s` and runs a frame, answering what
/// it asks.
fn restore<S: ScenarioSource>(s: &mut Scenario<S>, layout: &SavedLayout) -> bool {
    let mut done = false;
    s.input(|g| done = restore_layout(g, layout));
    s.frame(V);
    done
}

/// Whatever the Grid it lands on held, a restored layout saves back as it
/// was saved: View Spec, pins, sizes and band thicknesses.
#[hegel::test(test_cases = 200)]
fn a_restored_layout_saves_back_the_same(tc: TestCase) {
    let mut a = table();
    let mut b = table();
    scramble(&tc, &mut a);
    scramble(&tc, &mut b);
    let saved = save_layout(&mut a.grid);
    assert_eq!(saved.version, LAYOUT_VERSION);
    assert!(restore(&mut b, &saved));
    assert_eq!(save_layout(&mut b.grid), saved);
}

#[test]
fn a_layout_holds_what_was_set_by_key() {
    let mut s = table();
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Trailing, Key(2));
        g.pin(Axis::Rows, Edge::Leading, Key(5));
        g.set_size(Axis::Cols, Key(0), 12.0);
        g.set_header(Axis::Cols, 6.0);
        g.request_view(ViewSpec::default().with_sort(
            Axis::Rows,
            SortSpec::new(vec![SortColumn::descending(Key(1))]),
        ));
    });
    s.frame(V);
    let layout = save_layout(&mut s.grid);
    let sorted = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::descending(Key(1))]),
    );
    assert_eq!(
        layout,
        SavedLayout {
            version: 1,
            view: sorted,
            pins: [[vec![Key(5)], vec![]], [vec![], vec![Key(2)]]],
            sizes: [vec![], vec![(Key(0), 12.0)]],
            bands: [1.0, 6.0],
        }
    );
}

#[test]
fn restoring_keys_the_source_lacks_falls_back_and_never_fails() {
    let mut s = table();
    let gone = Key(40);
    let layout = SavedLayout {
        version: LAYOUT_VERSION,
        view: ViewSpec::default()
            .with_filter(
                Axis::Rows,
                FilterSpec::default()
                    .with_conditions(vec![Condition::text(gone, TextQuery::new("q"))].into()),
            )
            .with_sort(
                Axis::Rows,
                SortSpec::new(vec![
                    SortColumn::ascending(gone),
                    SortColumn::descending(Key(1)),
                ]),
            )
            .with_hidden(Axis::Rows, vec![Key(2), Key(99)])
            .with_hidden(Axis::Cols, vec![gone]),
        pins: [[vec![], vec![]], [vec![gone, Key(1)], vec![]]],
        sizes: [vec![(Key(99), 3.0)], vec![(gone, 9.0)]],
        bands: [2.0, 5.0],
    };
    assert!(restore(&mut s, &layout));
    let snap = s.grid.snapshot();
    // The pin on the missing column sleeps; column 1 is drawn pinned.
    assert_eq!(
        snap.pins(Axis::Cols, Edge::Leading).collect::<Vec<_>>(),
        [gone, Key(1)]
    );
    assert_eq!(snap.pinned(Axis::Cols, Edge::Leading).len(), 1);
    // The source answers with the sort terms and hidden Keys it has.
    assert_eq!(
        snap.applied_view().rows.sort,
        SortSpec::new(vec![SortColumn::descending(Key(1))])
    );
    assert_eq!(snap.applied_view().rows.hidden, [Key(2)]);
    assert!(snap.applied_view().cols.hidden.is_empty());
    // The condition on the missing column is restored; how it reads (the
    // In-Memory Table Source reads a missing item as blanks) is the source's.
    let conditions: Vec<Condition> = (snap.conditions(Axis::Rows).into_iter())
        .map(|(c, _)| c)
        .collect();
    assert_eq!(conditions, layout.view.rows.filter.conditions.conditions());
    // Sizes wait by Key; the bands apply.
    assert_eq!(s.grid.size_override(Axis::Cols, gone), Some(9.0));
    assert_eq!(s.grid.size_override(Axis::Rows, Key(99)), Some(3.0));
    assert_eq!(
        [s.grid.header(Axis::Rows), s.grid.header(Axis::Cols)],
        [2.0, 5.0]
    );
}

#[test]
fn a_layout_of_another_version_restores_nothing() {
    let mut s = table();
    let before = save_layout(&mut s.grid);
    let mut other = before.clone();
    other.version = LAYOUT_VERSION + 1;
    other.bands = [3.0, 3.0];
    assert!(!restore(&mut s, &other));
    assert_eq!(save_layout(&mut s.grid), before);
}

#[test]
fn restored_before_the_source_answers_the_view_is_pending_and_the_rest_applies_at_once() {
    let mut s = table();
    let sorted = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::ascending(Key(2))]),
    );
    let layout = SavedLayout {
        version: LAYOUT_VERSION,
        view: sorted.clone(),
        pins: [[vec![], vec![]], [vec![Key(2)], vec![]]],
        sizes: [vec![], vec![(Key(2), 11.0)]],
        bands: [2.0, 6.0],
    };
    s.input(|g| {
        restore_layout(g, &layout);
    });
    s.step_waiting("restored, the source not yet answering", V);
    let snap = s.grid.snapshot();
    assert!(snap.view_pending());
    assert_eq!(snap.applied_view(), &ViewSpec::default());
    assert_eq!(snap.pinned(Axis::Cols, Edge::Leading)[0].size, 11.0);
    assert_eq!(snap.header(Axis::Cols), 6.0);
    // Saved while pending, the layout carries the View Spec asked for.
    assert_eq!(save_layout(&mut s.grid).view, sorted);
    s.frame(V);
    let snap = s.grid.snapshot();
    assert!(!snap.view_pending());
    assert_eq!(snap.applied_view(), &sorted);
    assert_eq!(save_layout(&mut s.grid), layout);
}

/// Records as columns, as a transposed host has them: the layout pins a
/// record and sorts the records by a field, and a fresh Grid over the same
/// records takes it back.
#[test]
fn a_column_major_table_saves_and_restores_its_layout() {
    let table = || {
        let grid = Grid::new(
            AxisConfig::new(COLS, 1.0).header(1.0),
            AxisConfig::new(ROWS, 8.0).header(4.0).gap(1.0),
        );
        let mut s = Scenario::new(
            grid,
            ColumnMajorTable::new(records(), COLS as usize, [0, 1, 2]),
        );
        s.frame(V);
        s
    };
    let mut a = table();
    a.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(7));
        g.set_size(Axis::Cols, Key(7), 12.0);
        g.request_view(ViewSpec::default().with_sort(
            Axis::Cols,
            SortSpec::new(vec![SortColumn::descending(Key(0))]),
        ));
    });
    a.frame(V);
    let saved = save_layout(&mut a.grid);
    let mut b = table();
    assert!(restore(&mut b, &saved));
    assert_eq!(save_layout(&mut b.grid), saved);
    let snap = b.grid.snapshot();
    assert_eq!(snap.applied_view().cols.sort, saved.view.cols.sort);
    let pinned: Vec<_> = snap
        .pinned(Axis::Cols, Edge::Leading)
        .iter()
        .map(|i| (i.key, i.size))
        .collect();
    assert_eq!(pinned, [(Some(Key(7)), 12.0)]);
}
