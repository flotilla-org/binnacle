//! Seam 2: the In-Memory Table Source moves the items of either axis.

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Grid, Key, Located, SortColumn, SortSpec, Totals, ViewSpec,
};
use binnacle_memory::{InMemoryTableSource, MemorySource, MovePolicy, PumpSource, SortKeys};

fn keys(ids: &[u64]) -> Vec<Key> {
    ids.iter().copied().map(Key).collect()
}

fn columns(source: &InMemoryTableSource) -> Vec<Key> {
    source.window(Axis::Cols, 0..u64::MAX)
}

/// The legacy column remap computed `columns - 1` and underflowed on an empty
/// table.
#[test]
fn an_empty_table_moves_and_sorts_nothing() {
    let mut source = InMemoryTableSource::new(0, 0);
    for axis in Axis::ALL {
        source.move_items(axis, &keys(&[0]), 0);
        source.move_items(axis, &keys(&[0]), 1);
    }
    source.apply_sort(Axis::Rows, &SortSpec::unsorted(), vec![]);
    source.apply_sort(Axis::Cols, &SortSpec::unsorted(), vec![]);
    assert_eq!(columns(&source), keys(&[]));
    assert_eq!(source.window(Axis::Rows, 0..u64::MAX), keys(&[]));
    assert_eq!(
        source.locate(Axis::Cols, Key(0)),
        Located::Gone { suggest: None }
    );
}

fn rows(source: &InMemoryTableSource) -> Vec<Key> {
    source.window(Axis::Rows, 0..u64::MAX)
}

/// Six rows sorted descending by their single column's values `0..6`.
fn sorted_rows(policy: MovePolicy) -> InMemoryTableSource {
    let mut source = InMemoryTableSource::new(6, 1);
    source.set_move_policy(policy);
    let spec = SortSpec::new(vec![SortColumn::descending(Key(0))]);
    let values = SortKeys::I64((0..6).map(Some).collect());
    source.apply_sort(Axis::Rows, &spec, vec![values]);
    source
}

#[test]
fn a_move_on_a_filtered_axis_lands_before_the_item_shown_at_the_drop_position() {
    let mut source = InMemoryTableSource::new(6, 1);
    source.apply_filter(Axis::Rows, vec![true, false, true, false, true, true]);
    assert_eq!(rows(&source), keys(&[0, 2, 4, 5]));
    // Row 5 dropped at Position 1, before row 2; hidden rows keep their
    // places among the rest.
    let notice = source.move_items(Axis::Rows, &keys(&[5]), 1);
    assert_eq!(notice, Some(ChangeNotice::Reordered));
    assert_eq!(rows(&source), keys(&[0, 5, 2, 4]));
    source.clear_filter(Axis::Rows);
    assert_eq!(rows(&source), keys(&[0, 1, 5, 2, 3, 4]));
    // Dropped at the end of the filtered order: the end of the axis.
    source.apply_filter(Axis::Rows, vec![true, false, true, false, true, true]);
    source.move_items(Axis::Rows, &keys(&[0]), 4);
    source.clear_filter(Axis::Rows);
    assert_eq!(rows(&source), keys(&[1, 5, 2, 3, 4, 0]));
}

#[test]
fn a_move_on_a_sorted_axis_bakes_the_order_shown_and_clears_the_sort() {
    let mut source = sorted_rows(MovePolicy::Bake);
    assert!(source.movable(Axis::Rows));
    assert_eq!(rows(&source), keys(&[5, 4, 3, 2, 1, 0]));
    let notice = source.move_items(Axis::Rows, &keys(&[0]), 1);
    assert_eq!(notice, Some(ChangeNotice::Reordered));
    assert_eq!(rows(&source), keys(&[5, 0, 4, 3, 2, 1]));
    assert!(source.sort_spec(Axis::Rows).is_empty());
    // An empty spec now restores the baked order, not Logical Index order.
    source.apply_sort(Axis::Rows, &SortSpec::unsorted(), vec![]);
    assert_eq!(rows(&source), keys(&[5, 0, 4, 3, 2, 1]));
}

#[test]
fn refusing_moves_on_a_sorted_axis_moves_nothing_and_withdraws_movable() {
    let mut source = sorted_rows(MovePolicy::Refuse);
    assert!(!source.movable(Axis::Rows));
    assert!(source.movable(Axis::Cols));
    assert_eq!(source.move_items(Axis::Rows, &keys(&[0]), 1), None);
    assert_eq!(rows(&source), keys(&[5, 4, 3, 2, 1, 0]));
    assert!(!source.sort_spec(Axis::Rows).is_empty());
    source.apply_sort(Axis::Rows, &SortSpec::unsorted(), vec![]);
    assert!(source.movable(Axis::Rows));
    assert!(source.move_items(Axis::Rows, &keys(&[0]), 6).is_some());
    assert_eq!(rows(&source), keys(&[1, 2, 3, 4, 5, 0]));
}

/// The pump's view of a [`MemorySource`]: a baked move answers with the
/// View Spec it leaves in force, a refused one with its reason.
#[test]
fn a_memory_source_answers_a_baked_move_with_the_spec_it_leaves() {
    let records: Vec<i64> = vec![2, 0, 1];
    let spec = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::ascending(Key(0))]),
    );
    for policy in [MovePolicy::Bake, MovePolicy::Refuse] {
        let mut source = MemorySource::<i64>::new(3, 1)
            .i64_column(Key(0), |r: &i64| Some(*r))
            .with_move_policy(policy);
        source.view(&records, &spec).unwrap();
        let (movable, answer) = {
            let mut over = source.over(&records);
            (
                over.movable(Axis::Rows),
                over.move_items(Axis::Rows, &keys(&[0]), 0),
            )
        };
        assert_eq!(movable, Some(policy == MovePolicy::Bake));
        match policy {
            MovePolicy::Bake => {
                let moved = answer.unwrap();
                assert_eq!(moved.notices, vec![(Axis::Rows, ChangeNotice::Reordered)]);
                let totals = Totals {
                    rows: Some(3),
                    cols: Some(1),
                };
                assert_eq!(moved.view, Some((ViewSpec::default(), totals)));
                assert_eq!(rows_of(&source), keys(&[0, 1, 2]));
            }
            MovePolicy::Refuse => {
                assert_eq!(
                    answer.unwrap_err(),
                    "Can't move while sorted: clear the sort first"
                );
                assert_eq!(rows_of(&source), keys(&[1, 2, 0]));
            }
        }
    }
}

fn rows_of(source: &MemorySource<i64>) -> Vec<Key> {
    source.window(Axis::Rows, 0..u64::MAX)
}

/// The pump declares movability before draining, so a Grid over a source
/// that refuses moves while sorted stops offering them once sorted.
#[test]
fn the_pump_declares_movable_from_the_source() {
    let records: Vec<i64> = vec![2, 0, 1];
    let mut source = MemorySource::<i64>::new(3, 1)
        .i64_column(Key(0), |r: &i64| Some(*r))
        .with_move_policy(MovePolicy::Refuse);
    let mut grid = Grid::new(AxisConfig::new(3, 1.0), AxisConfig::new(1, 1.0));
    source.pump(&records, &mut grid, |_, _| {});
    assert!(grid.snapshot().movable(Axis::Rows));
    let spec = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::ascending(Key(0))]),
    );
    grid.request_view(spec);
    source.pump(&records, &mut grid, |_, _| {});
    assert!(!grid.snapshot().movable(Axis::Rows));
    assert!(grid.snapshot().movable(Axis::Cols));
}
