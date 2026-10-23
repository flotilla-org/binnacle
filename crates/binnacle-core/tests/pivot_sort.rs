//! Seam 1 scenarios for sorting and filtering across a pivot: a sort term
//! on a pivot column that has gone away is kept but not applied, and comes
//! back into force with the column; state on path Keys survives data
//! changes and goes when the source says their paths no longer fit.

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Comparison, Condition, ConditionState, Edge, GRAND_TOTAL,
    Grid, Grouping, Key, Level, Outline, Placement, Scalar, SortColumn, Totals, ViewSpec, Viewport,
};
use binnacle_testkit::{PivotTable, Scenario};

const V: Viewport = Viewport {
    width: 60.0,
    height: 8.0,
};

const REGION: Key = Key(1);
const QUARTER: Key = Key(3);
const WEIGHT: Key = Key(4);

/// The pivot column of quarter `q` (Q1 is 0), the sum of weight.
fn q(q: u64) -> Key {
    Key(10 + q)
}

/// The pivot columns of `quarters`, one per quarter, then the All column.
fn columns(quarters: &[u64]) -> Vec<(Key, Outline)> {
    let mut cols: Vec<_> = (quarters.iter())
        .map(|&n| PivotTable::column(q(n).0, &[], QUARTER, &format!("Q{}", n + 1)))
        .collect();
    cols.push((GRAND_TOTAL, Outline::grand_total()));
    cols
}

fn pivot() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.col_levels = vec![Level::field(QUARTER)];
    grouping.values = vec![AggregateValue::of(WEIGHT, "sum")];
    grouping.cols.grand_total = Placement::After;
    grouping
}

fn scenario(quarters: &[u64]) -> Scenario<PivotTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(2.0),
        AxisConfig::new(3, 5.0).header(4.0),
    );
    let mut s = Scenario::new(grid, PivotTable::new(6, 3, columns(quarters)));
    s.frame(V);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(pivot()))));
    s.frame(V);
    s
}

/// The data changes so the pivot has the columns of `quarters`: the
/// source tells the columns `Refiltered` and its View Spec now in force.
fn change_columns(s: &mut Scenario<PivotTable>, quarters: &[u64]) {
    let notice = s.source.set_columns(columns(quarters));
    let applied = s.source.applied();
    s.input(|g| {
        g.view_changed(applied, Totals::unknown());
        if let Some(n) = notice {
            g.notify(Axis::Cols, n);
        }
    });
    s.frame(V);
}

fn sorts(s: &mut Scenario<PivotTable>) -> Vec<(SortColumn, ConditionState)> {
    s.frame(V).sorts(Axis::Rows)
}

#[test]
fn the_rows_sort_by_a_pivot_column() {
    let mut s = scenario(&[0, 1]);
    s.input(|g| g.cycle_sort(Axis::Rows, q(1), false));
    let snap = s.frame(V);
    assert_eq!(
        snap.sorts(Axis::Rows),
        vec![(SortColumn::ascending(q(1)), ConditionState::Applied)]
    );
    assert!(snap.header_sort(q(1)).applied.is_some());
}

#[test]
fn a_sort_term_on_a_pivot_column_that_leaves_is_kept_not_applied_and_applies_on_its_return() {
    let mut s = scenario(&[0, 1, 2]);
    s.input(|g| g.cycle_sort(Axis::Rows, q(1), false));
    s.frame(V);
    change_columns(&mut s, &[0, 2]);
    let term = SortColumn::ascending(q(1));
    assert_eq!(sorts(&mut s), vec![(term, ConditionState::NotApplied)]);

    // Another edit asks for the term again, and it stays not applied.
    let slicer = Condition::compare(WEIGHT, Comparison::Greater(Scalar::Number(0.0)));
    s.input(|g| g.add_slicer(slicer));
    s.frame(V);
    assert_eq!(sorts(&mut s), vec![(term, ConditionState::NotApplied)]);

    change_columns(&mut s, &[0, 1, 2]);
    assert_eq!(sorts(&mut s), vec![(term, ConditionState::Applied)]);
}

#[test]
fn off_a_pivot_a_sort_term_the_source_leaves_out_is_dropped() {
    let mut s = scenario(&[0, 1]);
    s.input(|g| g.cycle_sort(Axis::Rows, q(1), false));
    s.frame(V);
    // The source sorts by plain columns alone, unpivoted.
    s.input(|g| g.view_changed(ViewSpec::default(), Totals::unknown()));
    assert_eq!(sorts(&mut s), vec![]);
}

#[test]
fn widths_pins_and_sorts_on_pivot_columns_survive_the_data_and_go_when_their_paths_no_longer_fit() {
    let mut s = scenario(&[0, 1, 2]);
    s.input(|g| {
        g.set_size(Axis::Cols, q(2), 9.0);
        g.pin(Axis::Cols, Edge::Leading, q(1));
        g.cycle_sort(Axis::Rows, q(0), false);
    });
    s.frame(V);
    change_columns(&mut s, &[2]);
    change_columns(&mut s, &[0, 1, 2]);
    assert_eq!(s.grid.size_override(Axis::Cols, q(2)), Some(9.0));
    assert_eq!(
        s.grid.pins(Axis::Cols, Edge::Leading).collect::<Vec<_>>(),
        [q(1)]
    );
    assert_eq!(sorts(&mut s).len(), 1);

    // The column levels go: no pivot column's path fits any more.
    let mut plain = pivot();
    plain.col_levels.clear();
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(plain))));
    s.frame(V);
    assert_eq!(s.grid.size_override(Axis::Cols, q(2)), None);
    assert_eq!(s.grid.pins(Axis::Cols, Edge::Leading).count(), 0);
    assert_eq!(
        sorts(&mut s),
        vec![],
        "the term is gone, not left not applied"
    );
}

#[test]
fn a_comparison_on_the_columns_naming_the_grand_total_row_is_a_condition_like_any() {
    let mut s = scenario(&[0, 1]);
    let having = Condition::compare(GRAND_TOTAL, Comparison::Greater(Scalar::Number(10.0)));
    s.input(|g| g.add_condition(Axis::Cols, having.clone()));
    let snap = s.frame(V);
    assert_eq!(
        snap.conditions(Axis::Cols),
        vec![(having, ConditionState::Applied)]
    );
}
