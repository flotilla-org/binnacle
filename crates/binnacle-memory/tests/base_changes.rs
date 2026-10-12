//! Seam 2: a host reports base data changes in base terms (append, value
//! edit, anything else) and `MemorySource<R>` answers with the Change Notice
//! the Grid needs.

use binnacle_core::{Axis, ChangeNotice, Key, Located, SortColumn, SortSpec};
use binnacle_memory::{InMemoryTableSource, MemorySource, SortKeys};

#[derive(Clone, Debug)]
struct Rec {
    qty: i64,
    price: i64,
}

fn recs(qty: &[i64]) -> Vec<Rec> {
    qty.iter().map(|&qty| Rec { qty, price: 0 }).collect()
}

/// Column 0 sorts by qty, column 1 by price.
fn source(rows: &[Rec]) -> MemorySource<Rec> {
    MemorySource::<Rec>::new(rows.len(), 2)
        .i64_column(Key(0), |r| Some(r.qty))
        .i64_column(Key(1), |r| Some(r.price))
}

fn keys(source: &MemorySource<Rec>) -> Vec<u64> {
    let count = source.count(Axis::Rows);
    source
        .window(Axis::Rows, 0..count)
        .into_iter()
        .map(|k| k.0)
        .collect()
}

fn by_qty() -> SortSpec {
    SortSpec::new(vec![SortColumn::ascending(Key(0))])
}

#[test]
fn an_append_with_no_sort_or_filter_is_inserted_at_the_end() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    rows.extend(recs(&[5, 4]));
    assert_eq!(
        source.appended(&rows, &[]),
        ChangeNotice::Inserted { at: 3, n: 2 }
    );
    assert_eq!(keys(&source), [0, 1, 2, 3, 4]);
}

#[test]
fn an_append_while_sorted_or_filtered_places_the_rows_in_order_and_filters_them() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.sort(&rows, &by_qty()).unwrap();
    rows.extend(recs(&[0, 5]));
    assert_eq!(source.appended(&rows, &[]), ChangeNotice::Reset(5));
    assert_eq!(keys(&source), [3, 1, 2, 0, 4]);

    source.filter(&rows, |r| r.qty % 2 == 1);
    rows.extend(recs(&[7, 6]));
    // The 7 lands at the end and the filter drops the 6.
    assert_eq!(
        source.appended(&rows, &[]),
        ChangeNotice::Inserted { at: 3, n: 1 }
    );
    assert_eq!(keys(&source), [1, 0, 4, 5]);
}

#[test]
fn appended_rows_take_the_host_keys_given_for_them() {
    let mut rows = recs(&[3, 1]);
    let mut source = source(&rows).with_keys(Axis::Rows, vec![Key(30), Key(10)]);
    assert_eq!(source.locate(Axis::Rows, Key(10)), Located::Found(1));
    rows.extend(recs(&[2]));
    assert_eq!(
        source.appended(&rows, &[Key(20)]),
        ChangeNotice::Inserted { at: 2, n: 1 }
    );
    source.sort(&rows, &by_qty()).unwrap();
    assert_eq!(keys(&source), [10, 20, 30]);
    assert_eq!(source.locate(Axis::Rows, Key(20)), Located::Found(1));
}

#[test]
fn a_value_edit_is_content_changed_at_its_position_or_reordered_if_its_column_is_sorted() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.sort(&rows, &by_qty()).unwrap();
    // Sorted: 1, 2, 3 are rows 1, 2, 0.
    rows[0].price = 9;
    assert_eq!(
        source.edited(&rows, 0, Key(1)),
        Some(ChangeNotice::ContentChanged(2..3))
    );
    rows[0].qty = 0;
    assert_eq!(
        source.edited(&rows, 0, Key(0)),
        Some(ChangeNotice::Reordered)
    );
    assert_eq!(keys(&source), [0, 1, 2]);
}

#[test]
fn an_edit_that_moves_a_row_across_the_filter_resets_and_one_out_of_sight_says_nothing() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.filter(&rows, |r| r.qty >= 2);
    assert_eq!(keys(&source), [0, 2]);
    rows[1].price = 5;
    assert_eq!(source.edited(&rows, 1, Key(1)), None);
    rows[1].qty = 4;
    assert_eq!(
        source.edited(&rows, 1, Key(0)),
        Some(ChangeNotice::Reset(3))
    );
    assert_eq!(keys(&source), [0, 1, 2]);
    rows[2].qty = 0;
    assert_eq!(
        source.edited(&rows, 2, Key(0)),
        Some(ChangeNotice::Reset(2))
    );
    assert_eq!(keys(&source), [0, 1]);
}

#[test]
fn any_other_change_recomputes_the_order_and_resets() {
    let mut rows = recs(&[3, 1, 2, 5]);
    let mut source = source(&rows);
    source.sort(&rows, &by_qty()).unwrap();
    source.filter(&rows, |r| r.qty != 2);
    rows.remove(0);
    rows.push(Rec { qty: 0, price: 0 });
    // Base now 1, 2, 5, 0: sorted 0, 1, 5 without the 2.
    assert_eq!(source.replaced(&rows, None), ChangeNotice::Reset(3));
    assert_eq!(keys(&source), [3, 0, 2]);
    assert_eq!(source.sort_spec(), &by_qty());
}

#[test]
fn any_other_change_with_no_sort_or_filter_resets_to_the_new_count() {
    let rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    let rows = recs(&[7]);
    assert_eq!(source.replaced(&rows, None), ChangeNotice::Reset(1));
    assert_eq!(keys(&source), [0]);
}

fn table_keys(table: &InMemoryTableSource) -> Vec<u64> {
    let count = table.count(Axis::Rows);
    table
        .window(Axis::Rows, 0..count)
        .iter()
        .map(|k| k.0)
        .collect()
}

fn qty(values: &[i64]) -> Vec<SortKeys<'static>> {
    vec![SortKeys::I64(values.iter().map(|&v| Some(v)).collect())]
}

#[test]
fn the_table_source_sorts_appended_rows_into_place_and_inserts_a_contiguous_run() {
    let mut table = InMemoryTableSource::new(3, 1);
    table.apply_sort(&by_qty(), qty(&[3, 1, 5]));
    // Sorted 1, 3, 5. Two new rows of 4 land together before the 5.
    assert_eq!(
        table.append_rows(2, &[], qty(&[3, 1, 5, 4, 4]), &[]),
        ChangeNotice::Inserted { at: 2, n: 2 }
    );
    assert_eq!(table_keys(&table), [1, 0, 3, 4, 2]);
    assert_eq!(table.row_position(4), Some(3));
}

#[test]
fn the_table_source_resets_when_appended_rows_scatter_and_ties_keep_base_order() {
    let mut table = InMemoryTableSource::new(3, 1);
    table.apply_sort(&by_qty(), qty(&[3, 1, 5]));
    assert_eq!(
        table.append_rows(2, &[], qty(&[3, 1, 5, 9, 1]), &[]),
        ChangeNotice::Reset(5)
    );
    assert_eq!(table_keys(&table), [1, 4, 0, 2, 3]);
}

#[test]
fn the_table_source_keeps_appended_rows_out_when_the_filter_drops_them() {
    let mut table = InMemoryTableSource::new(3, 1);
    table.apply_filter(vec![true, false, true]);
    assert_eq!(
        table.append_rows(3, &[], vec![], &[false, true, true]),
        ChangeNotice::Inserted { at: 2, n: 2 }
    );
    assert_eq!(table_keys(&table), [0, 2, 4, 5]);
    // Nothing kept: nothing to tell the Grid.
    assert_eq!(
        table.append_rows(1, &[], vec![], &[false]),
        ChangeNotice::Inserted { at: 4, n: 0 }
    );
    assert_eq!(table.count(Axis::Rows), 4);
    assert_eq!(table.row_position(6), None);
}

#[test]
fn the_table_source_keeps_its_sort_and_filter_when_the_rows_are_replaced() {
    let mut table = InMemoryTableSource::new(3, 1);
    table.apply_sort(&by_qty(), qty(&[3, 1, 5]));
    table.apply_filter(vec![true; 3]);
    assert_eq!(
        table.replace_rows(4, None, qty(&[7, 2, 8, 1]), vec![true, true, false, true]),
        ChangeNotice::Reset(3)
    );
    assert_eq!(table_keys(&table), [3, 1, 0]);
    assert_eq!(table.sort_spec(), &by_qty());
}

#[test]
fn the_table_source_replaces_unsorted_unfiltered_rows_in_base_order() {
    let mut table = InMemoryTableSource::new(3, 1);
    assert_eq!(
        table.replace_rows(2, Some(vec![Key(9), Key(8)]), vec![], vec![]),
        ChangeNotice::Reset(2)
    );
    assert_eq!(table_keys(&table), [9, 8]);
}
