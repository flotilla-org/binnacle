//! Seam 2: the In-Memory Table Source sorts from host-extracted keys.

use binnacle_core::{Axis, Key, Located, SortColumn, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};

fn keys(ids: &[u64]) -> Vec<Key> {
    ids.iter().copied().map(Key).collect()
}

#[test]
fn multi_column_sorts_compare_as_tuples_in_spec_order() {
    let mut source = InMemoryTableSource::new(5, 3);
    let city: Vec<Option<std::borrow::Cow<[u8]>>> = ["Oslo", "Baku", "Oslo", "baku", "Baku"]
        .into_iter()
        .map(|c| Some(c.as_bytes().into()))
        .collect();
    let qty = vec![Some(1), Some(2), Some(5), Some(0), Some(2)];
    let spec = SortSpec::new(vec![
        SortColumn::ascending(Key(2)),
        SortColumn::descending(Key(0)),
    ]);

    source.apply_sort(&spec, vec![SortKeys::Bytes(city), SortKeys::I64(qty)]);

    assert_eq!(source.window(Axis::Rows, 0..5), keys(&[1, 4, 2, 0, 3]));
}

#[test]
fn host_supplied_keys_follow_their_rows_through_a_sort() {
    let mut source = InMemoryTableSource::new(3, 1).with_keys(Axis::Rows, keys(&[70, 10, 40]));
    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[70, 10, 40]));

    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    source.apply_sort(&spec, vec![SortKeys::I64(vec![Some(9), Some(8), Some(7)])]);

    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[40, 10, 70]));
    assert_eq!(source.locate(Axis::Rows, Key(70)), Located::Found(2));
    assert_eq!(
        source.locate(Axis::Rows, Key(0)),
        Located::Gone { suggest: None }
    );
}

#[test]
#[should_panic(expected = "one SortKeys per Sort Spec column")]
fn sorting_needs_keys_for_every_spec_column() {
    let mut source = InMemoryTableSource::new(2, 2);
    let spec = SortSpec::new(vec![
        SortColumn::ascending(Key(0)),
        SortColumn::ascending(Key(1)),
    ]);
    source.apply_sort(&spec, vec![SortKeys::I64(vec![Some(1), Some(2)])]);
}

#[test]
#[should_panic(expected = "one sort key per base row")]
fn sorting_needs_a_key_for_every_base_row() {
    let mut source = InMemoryTableSource::new(3, 1);
    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    source.apply_sort(&spec, vec![SortKeys::I64(vec![Some(1), Some(2)])]);
}
