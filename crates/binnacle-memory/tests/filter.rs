//! Seam 2: filtering an In-Memory Table Source by a keep mask, independent of
//! its sort.

use binnacle_core::{Axis, ChangeNotice, Key, Located, SortColumn, SortSpec};
use binnacle_memory::{InMemoryTableSource, SortKeys};

fn keys(ks: &[u64]) -> Vec<Key> {
    ks.iter().copied().map(Key).collect()
}

#[test]
fn a_filter_keeps_the_masked_rows_in_their_order_and_resets_the_count() {
    let mut source = InMemoryTableSource::new(5, 2);
    let notice = source.apply_filter(vec![true, false, true, false, true]);
    assert_eq!(notice, ChangeNotice::Reset(3));
    assert_eq!(source.count(Axis::Rows), 3);
    assert_eq!(source.window(Axis::Rows, 0..10), keys(&[0, 2, 4]));
    assert_eq!(source.locate(Axis::Rows, Key(4)), Located::Found(2));

    assert_eq!(source.clear_filter(), ChangeNotice::Reset(5));
    assert_eq!(source.window(Axis::Rows, 0..10), keys(&[0, 1, 2, 3, 4]));
}

#[test]
fn sorting_keeps_the_filter_and_filtering_keeps_the_sort() {
    let mut source = InMemoryTableSource::new(5, 1);
    let descending = SortSpec::new(vec![SortColumn::descending(Key(0))]);
    let by_value = || {
        vec![SortKeys::I64(vec![
            Some(3),
            Some(1),
            Some(4),
            Some(1),
            Some(5),
        ])]
    };
    source.apply_sort(&descending, by_value());
    assert_eq!(source.window(Axis::Rows, 0..5), keys(&[4, 2, 0, 1, 3]));

    source.apply_filter(vec![true, true, false, true, true]);
    assert_eq!(source.window(Axis::Rows, 0..5), keys(&[4, 0, 1, 3]));

    let notice = source.apply_sort(&SortSpec::unsorted(), vec![]);
    assert_eq!(notice, ChangeNotice::Reordered);
    assert_eq!(source.window(Axis::Rows, 0..5), keys(&[0, 1, 3, 4]));
}

#[test]
fn a_filtered_out_row_is_gone_with_the_nearest_survivor_in_sort_order_suggested() {
    let mut source =
        InMemoryTableSource::new(7, 1).with_keys(Axis::Rows, keys(&[10, 11, 12, 13, 14, 15, 16]));
    // Sorted order (descending by value): 16 15 14 13 12 11 10.
    let values = vec![SortKeys::I64((0..7).map(Some).collect())];
    source.apply_sort(&SortSpec::new(vec![SortColumn::descending(Key(0))]), values);
    source.apply_filter(vec![true, false, false, false, true, true, false]);
    assert_eq!(source.window(Axis::Rows, 0..7), keys(&[15, 14, 10]));

    // 16 sorts first; the nearest survivor is 15, now at Position 0.
    assert_eq!(
        source.locate(Axis::Rows, Key(16)),
        Located::Gone { suggest: Some(0) }
    );
    // 13 sits one from 14 and three from 10.
    assert_eq!(
        source.locate(Axis::Rows, Key(13)),
        Located::Gone { suggest: Some(1) }
    );
    // 12 sits two from 14 and two from 10: the tie goes to the row after it.
    assert_eq!(
        source.locate(Axis::Rows, Key(12)),
        Located::Gone { suggest: Some(2) }
    );
    // 11 is next to 10.
    assert_eq!(
        source.locate(Axis::Rows, Key(11)),
        Located::Gone { suggest: Some(2) }
    );
    // A Key the source never had.
    assert_eq!(
        source.locate(Axis::Rows, Key(99)),
        Located::Gone { suggest: None }
    );
}

#[test]
fn filtering_out_every_row_leaves_nothing_to_suggest() {
    let mut source = InMemoryTableSource::new(3, 1);
    assert_eq!(source.apply_filter(vec![false; 3]), ChangeNotice::Reset(0));
    assert_eq!(source.window(Axis::Rows, 0..3), keys(&[]));
    assert_eq!(
        source.locate(Axis::Rows, Key(1)),
        Located::Gone { suggest: None }
    );
}
