//! Seam 2: the In-Memory Table Source moves columns.

use binnacle_core::{Axis, Key, Located, SortSpec};
use binnacle_memory::InMemoryTableSource;

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
