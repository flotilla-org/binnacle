//! Seam 2: a host reports base data changes in base terms (append, value
//! edit, anything else) and `MemorySource<R>` answers with the Change Notice
//! the Grid needs.

use binnacle_core::{
    Axis, ChangeNotice, Condition, FilterSpec, Key, Located, SortColumn, SortSpec, TextQuery,
    ViewSpec,
};
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
        vec![(Axis::Rows, ChangeNotice::Inserted { at: 3, n: 2 })]
    );
    assert_eq!(keys(&source), [0, 1, 2, 3, 4]);
}

#[test]
fn an_append_while_sorted_or_filtered_places_the_rows_in_order_and_filters_them() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.view(&rows, &rows_sorted(by_qty())).unwrap();
    rows.extend(recs(&[0, 5]));
    assert_eq!(
        source.appended(&rows, &[]),
        vec![(Axis::Rows, ChangeNotice::Reset(5))]
    );
    assert_eq!(keys(&source), [3, 1, 2, 0, 4]);

    source.filter(&rows, |r| r.qty % 2 == 1);
    rows.extend(recs(&[7, 6]));
    // The 7 lands at the end and the filter drops the 6.
    assert_eq!(
        source.appended(&rows, &[]),
        vec![(Axis::Rows, ChangeNotice::Inserted { at: 3, n: 1 })]
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
        vec![(Axis::Rows, ChangeNotice::Inserted { at: 2, n: 1 })]
    );
    source.view(&rows, &rows_sorted(by_qty())).unwrap();
    assert_eq!(keys(&source), [10, 20, 30]);
    assert_eq!(source.locate(Axis::Rows, Key(20)), Located::Found(1));
}

#[test]
fn a_value_change_is_content_changed_at_its_position_or_reordered_if_its_column_is_sorted() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.view(&rows, &rows_sorted(by_qty())).unwrap();
    // Sorted: 1, 2, 3 are rows 1, 2, 0.
    rows[0].price = 9;
    assert_eq!(
        source.changed(&rows, 0, Key(1)),
        vec![(Axis::Rows, ChangeNotice::ContentChanged(2..3))]
    );
    rows[0].qty = 0;
    assert_eq!(
        source.changed(&rows, 0, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Reordered)]
    );
    assert_eq!(keys(&source), [0, 1, 2]);
}

#[test]
fn a_change_that_moves_a_row_across_the_filter_refilters_and_one_out_of_sight_says_nothing() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.filter(&rows, |r| r.qty >= 2);
    assert_eq!(keys(&source), [0, 2]);
    rows[1].price = 5;
    assert_eq!(source.changed(&rows, 1, Key(1)), vec![]);
    rows[1].qty = 4;
    assert_eq!(
        source.changed(&rows, 1, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Refiltered(3))]
    );
    assert_eq!(keys(&source), [0, 1, 2]);
    rows[2].qty = 0;
    assert_eq!(
        source.changed(&rows, 2, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Refiltered(2))]
    );
    assert_eq!(keys(&source), [0, 1]);
}

#[test]
fn a_users_edit_that_stops_a_row_matching_keeps_it_unmatched_until_the_next_view_spec() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.filter(&rows, |r| r.qty >= 2);
    rows[2].qty = 0;
    assert_eq!(
        source.edited(&rows, 2, Key(0)),
        vec![(Axis::Rows, ChangeNotice::ContentChanged(1..2))]
    );
    assert_eq!(keys(&source), [0, 2]);
    assert_eq!(
        source.unmatched(Axis::Rows, &[Key(0), Key(2), Key(1)]),
        [false, true, false]
    );
    // Matching again, it is no longer flagged.
    rows[2].qty = 5;
    source.edited(&rows, 2, Key(0));
    assert_eq!(source.unmatched(Axis::Rows, &[Key(2)]), [false]);
    rows[2].qty = 0;
    source.edited(&rows, 2, Key(0));
    // The same View Spec again applies the filter to it.
    assert_eq!(
        source.view(&rows, &ViewSpec::default()),
        Ok(vec![(Axis::Rows, ChangeNotice::Refiltered(1))])
    );
    assert_eq!(keys(&source), [0]);
    assert_eq!(source.unmatched(Axis::Rows, &[Key(2)]), [false]);
}

#[test]
fn an_unmatched_row_sorts_by_its_edit_and_a_change_from_the_data_takes_it_out() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.view(&rows, &rows_sorted(by_qty())).unwrap();
    source.filter(&rows, |r| r.qty >= 2);
    assert_eq!(keys(&source), [2, 0]);
    rows[0].qty = 0;
    assert_eq!(
        source.edited(&rows, 0, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Reordered)]
    );
    assert_eq!(keys(&source), [0, 2]);
    // The live data moves it again: now the filter takes it.
    rows[0].qty = -1;
    assert_eq!(
        source.changed(&rows, 0, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Refiltered(1))]
    );
    assert_eq!(keys(&source), [2]);
}

#[test]
fn a_users_edit_out_of_view_is_filtered_at_once() {
    let mut rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    source.filter(&rows, |r| r.qty >= 2);
    rows[1].qty = 4;
    assert_eq!(
        source.edited(&rows, 1, Key(0)),
        vec![(Axis::Rows, ChangeNotice::Refiltered(3))]
    );
    assert_eq!(keys(&source), [0, 1, 2]);
}

#[test]
fn any_other_change_recomputes_the_order_and_resets() {
    let mut rows = recs(&[3, 1, 2, 5]);
    let mut source = source(&rows);
    source.view(&rows, &rows_sorted(by_qty())).unwrap();
    source.filter(&rows, |r| r.qty != 2);
    rows.remove(0);
    rows.push(Rec { qty: 0, price: 0 });
    // Base now 1, 2, 5, 0: sorted 0, 1, 5 without the 2.
    assert_eq!(
        source.replaced(&rows, None),
        vec![(Axis::Rows, ChangeNotice::Reset(3))]
    );
    assert_eq!(keys(&source), [3, 0, 2]);
    assert_eq!(source.view_spec(), rows_sorted(by_qty()));
}

#[test]
fn any_other_change_with_no_sort_or_filter_resets_to_the_new_count() {
    let rows = recs(&[3, 1, 2]);
    let mut source = source(&rows);
    let rows = recs(&[7]);
    assert_eq!(
        source.replaced(&rows, None),
        vec![(Axis::Rows, ChangeNotice::Reset(1))]
    );
    assert_eq!(keys(&source), [0]);
}

fn columns(source: &MemorySource<Rec>) -> Vec<u64> {
    let count = source.count(Axis::Cols);
    (source.window(Axis::Cols, 0..count).into_iter())
        .map(|k| k.0)
        .collect()
}

/// The columns whose text in row 0 contains `5`.
fn row_0_has_a_5() -> ViewSpec {
    let five = Condition::text(Key(0), TextQuery::new("5"));
    let filter = FilterSpec::default().with_conditions(vec![five].into());
    ViewSpec::default().with_filter(Axis::Cols, filter)
}

#[test]
fn a_users_edit_that_stops_a_column_matching_keeps_it_unmatched_until_the_next_view_spec() {
    let mut rows = vec![Rec { qty: 5, price: 15 }, Rec { qty: 1, price: 2 }];
    let mut source = source(&rows);
    let spec = row_0_has_a_5();
    source.view(&rows, &spec).unwrap();
    assert_eq!(columns(&source), [0, 1]);
    rows[0].price = 2;
    assert_eq!(
        source.edited(&rows, 0, Key(1)),
        vec![(Axis::Rows, ChangeNotice::ContentChanged(0..1))]
    );
    assert_eq!(columns(&source), [0, 1]);
    assert_eq!(
        source.unmatched(Axis::Cols, &[Key(0), Key(1)]),
        [false, true]
    );
    // The same View Spec again applies the filter to it.
    assert_eq!(
        source.view(&rows, &spec),
        Ok(vec![
            (Axis::Rows, ChangeNotice::Reordered),
            (Axis::Cols, ChangeNotice::Refiltered(1))
        ])
    );
    assert_eq!(columns(&source), [0]);
    assert_eq!(source.unmatched(Axis::Cols, &[Key(1)]), [false]);
}

#[test]
fn a_change_from_the_data_or_new_rows_filter_the_columns_at_once() {
    let mut rows = vec![Rec { qty: 5, price: 15 }];
    let mut source = source(&rows);
    source.view(&rows, &row_0_has_a_5()).unwrap();
    rows[0].price = 2;
    assert_eq!(
        source.changed(&rows, 0, Key(1)),
        vec![
            (Axis::Rows, ChangeNotice::ContentChanged(0..1)),
            (Axis::Cols, ChangeNotice::Refiltered(1))
        ]
    );
    assert_eq!(columns(&source), [0]);

    // A quick filter on the columns searches every row, new ones too.
    let quick = FilterSpec::default().with_quick(Some(TextQuery::new("7")));
    source
        .view(&rows, &ViewSpec::default().with_filter(Axis::Cols, quick))
        .unwrap();
    assert_eq!(columns(&source), []);
    rows.push(Rec { qty: 1, price: 7 });
    assert_eq!(
        source.appended(&rows, &[]),
        vec![
            (Axis::Rows, ChangeNotice::Inserted { at: 1, n: 1 }),
            (Axis::Cols, ChangeNotice::Refiltered(1))
        ]
    );
    assert_eq!(columns(&source), [1]);
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
    table.apply_sort(Axis::Rows, &by_qty(), qty(&[3, 1, 5]));
    // Sorted 1, 3, 5. Two new rows of 4 land together before the 5.
    assert_eq!(
        table.append_rows(2, &[], qty(&[3, 1, 5, 4, 4]), &[]),
        ChangeNotice::Inserted { at: 2, n: 2 }
    );
    assert_eq!(table_keys(&table), [1, 0, 3, 4, 2]);
    assert_eq!(table.position(Axis::Rows, 4), Some(3));
}

/// A View Spec sorting the rows by `sort`.
fn rows_sorted(sort: SortSpec) -> ViewSpec {
    ViewSpec::default().with_sort(Axis::Rows, sort)
}
