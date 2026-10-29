//! Seam 2: the In-Memory Table Source sorts from host-extracted keys.

use binnacle_core::{Axis, Key, Located, SortColumn, SortDirection, SortSpec};
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

    source.apply_sort(
        Axis::Rows,
        &spec,
        vec![SortKeys::Bytes(city), SortKeys::I64(qty)],
    );

    assert_eq!(source.window(Axis::Rows, 0..5), keys(&[1, 4, 2, 0, 3]));
}

#[test]
fn host_supplied_keys_follow_their_rows_through_a_sort() {
    let mut source = InMemoryTableSource::new(3, 1).with_keys(Axis::Rows, keys(&[70, 10, 40]));
    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[70, 10, 40]));

    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    source.apply_sort(
        Axis::Rows,
        &spec,
        vec![SortKeys::I64(vec![Some(9), Some(8), Some(7)])],
    );

    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[40, 10, 70]));
    assert_eq!(source.locate(Axis::Rows, Key(70)), Located::Found(2));
    assert_eq!(
        source.locate(Axis::Rows, Key(0)),
        Located::Gone { suggest: None }
    );
}

#[test]
#[should_panic(expected = "one SortKeys per Sort Spec item")]
fn sorting_needs_keys_for_every_spec_column() {
    let mut source = InMemoryTableSource::new(2, 2);
    let spec = SortSpec::new(vec![
        SortColumn::ascending(Key(0)),
        SortColumn::ascending(Key(1)),
    ]);
    source.apply_sort(
        Axis::Rows,
        &spec,
        vec![SortKeys::I64(vec![Some(1), Some(2)])],
    );
}

#[test]
#[should_panic(expected = "one sort key per base item")]
fn sorting_needs_a_key_for_every_base_row() {
    let mut source = InMemoryTableSource::new(3, 1);
    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    source.apply_sort(
        Axis::Rows,
        &spec,
        vec![SortKeys::I64(vec![Some(1), Some(2)])],
    );
}

fn sort_rows(keys: SortKeys<'_>, direction: SortDirection) -> Vec<Key> {
    let mut source = InMemoryTableSource::new(keys.len(), 1);
    let spec = SortSpec::new(vec![SortColumn {
        key: Key(0),
        direction,
    }]);
    let n = keys.len() as u64;
    source.apply_sort(Axis::Rows, &spec, vec![keys]);
    source.window(Axis::Rows, 0..n)
}

#[test]
fn nulls_sort_first_ascending_and_last_descending() {
    let values = || SortKeys::I64(vec![Some(2), None, Some(-1), None, Some(0)]);
    assert_eq!(
        sort_rows(values(), SortDirection::Ascending),
        keys(&[1, 3, 2, 4, 0])
    );
    assert_eq!(
        sort_rows(values(), SortDirection::Descending),
        keys(&[0, 4, 2, 1, 3])
    );
}

#[test]
fn nan_sorts_after_every_number_ascending_and_before_them_descending() {
    let values = || {
        SortKeys::F64(vec![
            Some(f64::NAN),
            Some(f64::INFINITY),
            None,
            Some(1.0),
            Some(-f64::NAN),
        ])
    };
    // NaNs tie with each other, so Key order decides between them.
    assert_eq!(
        sort_rows(values(), SortDirection::Ascending),
        keys(&[2, 3, 1, 0, 4])
    );
    assert_eq!(
        sort_rows(values(), SortDirection::Descending),
        keys(&[0, 4, 1, 3, 2])
    );
}

#[test]
fn unsigned_keys_compare_exactly_above_i64_max_and_2_pow_53() {
    let values = SortKeys::U64(vec![
        Some(u64::MAX),
        Some(i64::MAX as u64 + 1),
        Some((1 << 53) + 1),
        Some(1 << 53),
        Some(i64::MAX as u64),
    ]);
    assert_eq!(
        sort_rows(values, SortDirection::Ascending),
        keys(&[3, 2, 4, 1, 0])
    );
}

#[test]
fn text_compares_by_code_point() {
    let text = |s: &'static str| Some(std::borrow::Cow::Borrowed(s.as_bytes()));
    // U+FFFF is below U+10000 in code points, as in UTF-8 bytes.
    let values = SortKeys::Bytes(vec![
        text("\u{10000}"),
        text("\u{ffff}"),
        text("Z"),
        text("a"),
    ]);
    assert_eq!(
        sort_rows(values, SortDirection::Ascending),
        keys(&[2, 3, 1, 0])
    );
}

#[test]
fn equal_values_keep_key_order_in_both_directions() {
    let mut source = InMemoryTableSource::new(4, 1).with_keys(Axis::Rows, keys(&[30, 10, 40, 20]));
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let spec = SortSpec::new(vec![SortColumn {
            key: Key(0),
            direction,
        }]);
        source.apply_sort(Axis::Rows, &spec, vec![SortKeys::I64(vec![Some(1); 4])]);
        assert_eq!(source.window(Axis::Rows, 0..4), keys(&[10, 20, 30, 40]));
    }
}
