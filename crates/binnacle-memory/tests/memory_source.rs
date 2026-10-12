//! Seam 2: `MemorySource<R>` sorts a host's rows through per-column closures.

use binnacle_core::{Axis, Key, Located, SortColumn, SortSpec};
use binnacle_memory::{MemorySource, Unsortable};

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
    assert_eq!(source.sort(&rows, &by_qty_then_amount), Ok(()));
    assert_eq!(
        source.window(Axis::Rows, 0..3),
        vec![Key(2), Key(0), Key(1)]
    );
    assert_eq!(source.locate(Axis::Rows, Key(1)), Located::Found(2));

    assert_eq!(
        source.sort(&rows, &SortSpec::new(vec![SortColumn::ascending(Key(0))])),
        Ok(())
    );
    assert_eq!(
        source.window(Axis::Rows, 0..3),
        vec![Key(2), Key(0), Key(1)]
    );

    assert_eq!(
        source.sort(&rows, &SortSpec::new(vec![SortColumn::ascending(Key(1))])),
        Ok(())
    );
    assert_eq!(
        source.window(Axis::Rows, 0..3),
        vec![Key(0), Key(2), Key(1)]
    );
    assert_eq!(source.count(Axis::Cols), 4);
}

#[test]
fn a_column_without_a_closure_is_unsortable_and_leaves_the_order_alone() {
    let rows = [rec("eve", None, 2), rec("ada", None, 1)];
    let mut source = source(&rows);
    source
        .sort(&rows, &SortSpec::new(vec![SortColumn::ascending(Key(2))]))
        .unwrap();

    let spec = SortSpec::new(vec![
        SortColumn::ascending(Key(0)),
        SortColumn::ascending(Key(3)),
    ]);
    assert_eq!(
        source.sort(&rows, &spec),
        Err(Unsortable { column: Key(3) })
    );
    assert_eq!(source.window(Axis::Rows, 0..2), vec![Key(1), Key(0)]);
}
