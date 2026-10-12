//! Seam 2: the In-Memory Table Source moves columns.

use binnacle_core::{Axis, Key, Located, SortColumn, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};

fn keys(ids: &[u64]) -> Vec<Key> {
    ids.iter().copied().map(Key).collect()
}

fn columns(source: &InMemoryTableSource) -> Vec<Key> {
    source.window(Axis::Cols, 0..u64::MAX)
}

#[test]
fn moved_columns_land_before_the_drop_position() {
    let mut source = InMemoryTableSource::new(3, 5);
    source.move_columns(&keys(&[0]), 3);
    assert_eq!(columns(&source), keys(&[1, 2, 0, 3, 4]));
    // Positions are those before the move: 0 goes to the very end.
    source.move_columns(&keys(&[0]), 5);
    assert_eq!(columns(&source), keys(&[1, 2, 3, 4, 0]));
    source.move_columns(&keys(&[4, 2]), 0);
    assert_eq!(columns(&source), keys(&[4, 2, 1, 3, 0]));
    assert_eq!(source.locate(Axis::Cols, Key(3)), Located::Found(3));
    assert_eq!(source.window(Axis::Cols, 1..3), keys(&[2, 1]));
}

#[test]
fn a_drop_inside_the_moved_block_keeps_the_block_together() {
    let mut source = InMemoryTableSource::new(0, 6);
    source.move_columns(&keys(&[1, 3]), 2);
    assert_eq!(columns(&source), keys(&[0, 1, 3, 2, 4, 5]));
}

#[test]
fn unknown_and_repeated_keys_are_skipped_and_the_drop_clamps_to_the_count() {
    let mut source = InMemoryTableSource::new(0, 4);
    source.move_columns(&keys(&[9, 1, 1]), 99);
    assert_eq!(columns(&source), keys(&[0, 2, 3, 1]));
    source.move_columns(&[], 0);
    assert_eq!(columns(&source), keys(&[0, 2, 3, 1]));
}

#[test]
fn host_column_keys_move_and_row_sorts_leave_the_column_order_alone() {
    let mut source = InMemoryTableSource::new(3, 3).with_keys(Axis::Cols, keys(&[10, 20, 30]));
    source.move_columns(&keys(&[30]), 0);
    assert_eq!(columns(&source), keys(&[30, 10, 20]));
    source.apply_sort(
        &SortSpec::new(vec![SortColumn::descending(Key(30))]),
        vec![SortKeys::I64(vec![Some(1), Some(3), Some(2)])],
    );
    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[1, 2, 0]));
    assert_eq!(columns(&source), keys(&[30, 10, 20]));
    assert_eq!(source.locate(Axis::Cols, Key(20)), Located::Found(2));
}

/// The legacy column remap computed `columns - 1` and underflowed on an empty
/// table.
#[test]
fn an_empty_table_moves_and_sorts_nothing() {
    let mut source = InMemoryTableSource::new(0, 0);
    source.move_columns(&keys(&[0]), 0);
    source.move_columns(&keys(&[0]), 1);
    source.apply_sort(&SortSpec::unsorted(), vec![]);
    assert_eq!(columns(&source), keys(&[]));
    assert_eq!(source.window(Axis::Rows, 0..u64::MAX), keys(&[]));
    assert_eq!(
        source.locate(Axis::Cols, Key(0)),
        Located::Gone { suggest: None }
    );
}
