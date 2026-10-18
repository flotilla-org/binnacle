//! Seam 2: `MemorySource<R>` sorts a host's rows through per-column closures.

use binnacle_core::{Axis, ChangeNotice, Key, Located, SortColumn, SortSpec, ViewSpec};
use binnacle_memory::{MemorySource, PumpSource, Unsortable, Viewed};

struct Rec {
    name: String,
    amount: Option<f64>,
    qty: i64,
}

fn rec(name: &str, amount: Option<f64>, qty: i64) -> Rec {
    Rec {
        name: name.into(),
        amount,
        qty,
    }
}

fn source(rows: &[Rec]) -> MemorySource<Rec> {
    MemorySource::<Rec>::new(rows.len(), 4)
        .bytes_column(Key(0), |r| Some(r.name.as_bytes().into()))
        .f64_column(Key(1), |r| r.amount)
        .i64_column(Key(2), |r| Some(r.qty))
}

#[test]
fn sorts_rows_by_the_extracted_keys() {
    let rows = [
        rec("eve", Some(2.5), 3),
        rec("gus", None, 1),
        rec("ada", Some(9.0), 3),
    ];
    let mut source = source(&rows);

    let by_qty_then_amount = SortSpec::new(vec![
        SortColumn::descending(Key(2)),
        SortColumn::descending(Key(1)),
    ]);
    assert_eq!(
        source.view(&rows, &rows_sorted(by_qty_then_amount)),
        Ok(ChangeNotice::Reordered)
    );
    assert_eq!(
        source.window(Axis::Rows, 0..3),
        vec![Key(2), Key(0), Key(1)]
    );
    assert_eq!(source.locate(Axis::Rows, Key(1)), Located::Found(2));

    assert_eq!(
        source.view(
            &rows,
            &rows_sorted(SortSpec::new(vec![SortColumn::ascending(Key(0))]))
        ),
        Ok(ChangeNotice::Reordered)
    );
    assert_eq!(
        source.window(Axis::Rows, 0..3),
        vec![Key(2), Key(0), Key(1)]
    );
    assert_eq!(source.count(Axis::Cols), 4);
}

#[test]
fn a_column_without_a_closure_is_unsortable_and_leaves_the_order_alone() {
    let rows = [rec("eve", None, 2), rec("ada", None, 1)];
    let mut source = source(&rows);
    source
        .view(
            &rows,
            &rows_sorted(SortSpec::new(vec![SortColumn::ascending(Key(2))])),
        )
        .unwrap();

    let spec = SortSpec::new(vec![
        SortColumn::ascending(Key(0)),
        SortColumn::ascending(Key(3)),
    ]);
    assert_eq!(
        source.view(&rows, &rows_sorted(spec)),
        Err(Unsortable { column: Key(3) })
    );
    assert_eq!(source.window(Axis::Rows, 0..2), vec![Key(1), Key(0)]);
}

#[test]
fn filters_rows_by_a_closure_and_keeps_the_sort() {
    let rows = [
        rec("eve", Some(2.5), 3),
        rec("gus", None, 1),
        rec("ada", Some(9.0), 3),
        rec("bob", Some(1.0), 0),
    ];
    let mut source = source(&rows);
    source
        .view(
            &rows,
            &rows_sorted(SortSpec::new(vec![SortColumn::ascending(Key(0))])),
        )
        .unwrap();
    assert_eq!(
        source.window(Axis::Rows, 0..4),
        vec![Key(2), Key(3), Key(0), Key(1)]
    );

    assert_eq!(
        source.filter(&rows, |r| r.qty > 0),
        ChangeNotice::Refiltered(3)
    );
    assert_eq!(
        source.window(Axis::Rows, 0..4),
        vec![Key(2), Key(0), Key(1)]
    );
    assert_eq!(
        source.locate(Axis::Rows, Key(3)),
        Located::Gone { suggest: Some(1) }
    );

    assert_eq!(source.clear_filter(), ChangeNotice::Refiltered(4));
    assert_eq!(source.count(Axis::Rows), 4);
}

/// Story 37: sort and filter are independent state. Changing the filter
/// never asks a column closure for sort keys, and sorting never calls the
/// filter's predicate; each asks only what it needs, once per row.
#[hegel::test(test_cases = 500)]
fn filtering_never_asks_for_sort_keys_and_sorting_never_runs_the_filter(tc: hegel::TestCase) {
    use hegel::generators as gs;
    use std::cell::Cell;
    use std::rc::Rc;

    let values = tc.draw(gs::vecs(gs::integers::<i64>().min_value(-5).max_value(5)).max_size(30));
    let rows: Vec<Rec> = values.iter().map(|&v| rec("", Some(v as f64), v)).collect();
    // Calls made to each column's closure (Key(0), Key(1)) and the filters'.
    let extracted = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
    let kept = Rc::new(Cell::new(0));
    let (a, b) = (Rc::clone(&extracted[0]), Rc::clone(&extracted[1]));
    let mut source = MemorySource::<Rec>::new(rows.len(), 2)
        .i64_column(Key(0), move |r| {
            a.set(a.get() + 1);
            Some(r.qty)
        })
        .f64_column(Key(1), move |r| {
            b.set(b.get() + 1);
            r.amount
        });
    let n = rows.len();
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(8)) {
        let before = [extracted[0].get(), extracted[1].get(), kept.get()];
        let mut expected = before;
        match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 => {
                let columns = tc.draw(
                    gs::vecs(hegel::tuples!(
                        gs::integers::<u64>().max_value(1),
                        gs::booleans()
                    ))
                    .max_size(3),
                );
                let spec = SortSpec::new(
                    columns
                        .iter()
                        .map(|&(key, up)| match up {
                            true => SortColumn::ascending(Key(key)),
                            false => SortColumn::descending(Key(key)),
                        })
                        .collect(),
                );
                tc.note(&format!("sort {spec:?}"));
                for &(key, _) in &columns {
                    expected[key as usize] += n;
                }
                assert_eq!(
                    source.view(&rows, &rows_sorted(spec)),
                    Ok(ChangeNotice::Reordered)
                );
            }
            1 => {
                let at_least = tc.draw(gs::integers::<i64>().min_value(-6).max_value(6));
                tc.note(&format!("filter qty >= {at_least}"));
                let k = Rc::clone(&kept);
                source.filter(&rows, move |r| {
                    k.set(k.get() + 1);
                    r.qty >= at_least
                });
                expected[2] += n;
            }
            _ => {
                tc.note("clear filter");
                source.clear_filter();
            }
        }
        let after = [extracted[0].get(), extracted[1].get(), kept.get()];
        assert_eq!(
            after, expected,
            "calls to the Key(0), Key(1) and filter closures"
        );
    }
}

/// A View Spec sorting the rows by `sort`.
fn rows_sorted(sort: SortSpec) -> ViewSpec {
    ViewSpec::default().with_sort(Axis::Rows, sort)
}

#[test]
fn a_view_is_answered_with_the_part_applied_and_the_rows_notice() {
    let rows = [rec("eve", None, 2), rec("ada", None, 1)];
    let mut source = source(&rows);
    let by_qty = SortSpec::new(vec![SortColumn::ascending(Key(2))]);
    let by_row = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    let asked = rows_sorted(by_qty.clone()).with_sort(Axis::Cols, by_row);
    let viewed = source.over(&rows).view(&asked);
    assert_eq!(
        viewed,
        Viewed {
            applied: rows_sorted(by_qty.clone()),
            notices: vec![(Axis::Rows, ChangeNotice::Reordered)],
        }
    );
    let unsortable = rows_sorted(SortSpec::new(vec![SortColumn::ascending(Key(3))]));
    let viewed = source.over(&rows).view(&unsortable);
    assert_eq!(
        viewed,
        Viewed {
            applied: rows_sorted(by_qty),
            notices: vec![],
        }
    );
}

#[test]
fn the_rows_support_sorting_by_the_columns_with_a_closure_and_the_columns_nothing() {
    let rows = [rec("eve", None, 2)];
    let mut source = source(&rows);
    let sorts = |s: &mut MemorySource<Rec>, axis, key| s.over(&rows).support(axis, Key(key)).sort;
    assert!(sorts(&mut source, Axis::Rows, 0));
    assert!(!sorts(&mut source, Axis::Rows, 3));
    assert!(!sorts(&mut source, Axis::Cols, 0));
}
