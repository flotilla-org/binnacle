//! Seam 1 scenarios for a pivot's column axis, which comes from the data:
//! pivot columns that can't move, the "All" columns pinned right by
//! default, and widths, pins and focus that survive a value leaving and
//! returning, since column changes come as `Refiltered`.

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Edge, GRAND_TOTAL, Grid, Grouping, Key, Level, Outline,
    Placement, PointerEvent, ViewSpec, Viewport,
};
use binnacle_testkit::{PivotTable, Scenario};

const V: Viewport = Viewport {
    width: 60.0,
    height: 8.0,
};

const REGION: Key = Key(1);
const QUARTER: Key = Key(3);
const WEIGHT: Key = Key(4);

fn values() -> Vec<AggregateValue> {
    vec![AggregateValue::of(WEIGHT, "sum"), AggregateValue::count()]
}

/// The pivot columns of `quarters` (by index: Q1 is 0), each over a sum
/// and a count, then the "All" columns, one per value.
fn columns(quarters: &[u64]) -> Vec<(Key, Outline)> {
    let mut cols = Vec::new();
    for &q in quarters {
        let name = format!("Q{}", q + 1);
        let up = [200 + q];
        for (i, v) in values().iter().enumerate() {
            let (key, mut outline) = PivotTable::column(10 + 2 * q + i as u64, &up, QUARTER, &name);
            outline.group.as_mut().unwrap().value = binnacle_core::Value::Text(v.identity());
            cols.push((key, outline));
        }
    }
    for (i, v) in values().iter().enumerate() {
        cols.push((Key(90 + i as u64), Outline::grand_total_of(v.identity())));
    }
    cols
}

/// Rows by region, columns by quarter then the values, the column Grand
/// Total placed after.
fn pivot() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.col_levels = vec![Level::field(QUARTER), Level::Values];
    grouping.values = values();
    grouping.cols.grand_total = Placement::After;
    grouping
}

/// Six plain rows; three plain columns until the pivot, columns 5 wide
/// after a 4-wide row band.
fn scenario(quarters: &[u64]) -> Scenario<PivotTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(2.0),
        AxisConfig::new(3, 5.0).header(4.0),
    );
    let mut s = Scenario::new(grid, PivotTable::new(6, 3, columns(quarters)));
    s.input(|g| g.set_movable(Axis::Cols, true));
    s.frame(V);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(pivot()))));
    s.frame(V);
    s
}

fn pins(s: &Scenario<PivotTable>, edge: Edge) -> Vec<Key> {
    s.grid.pins(Axis::Cols, edge).collect()
}

/// The columns drawn, by Key, scrolling and pinned alike, left to right.
fn drawn(s: &mut Scenario<PivotTable>) -> Vec<u64> {
    let snap = s.frame(V);
    let mut cols: Vec<_> = snap.drawn(Axis::Cols).collect();
    cols.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    cols.iter().filter_map(|c| c.key).map(|k| k.0).collect()
}

fn change_columns(s: &mut Scenario<PivotTable>, quarters: &[u64]) {
    let notice = s.source.set_columns(columns(quarters));
    s.input(|g| {
        if let Some(n) = notice {
            g.notify(Axis::Cols, n);
        }
    });
    s.frame(V);
}

#[test]
fn pivot_columns_cannot_move_as_their_axis_declares() {
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(6, 1.0).header(2.0),
            AxisConfig::new(3, 5.0).header(4.0),
        ),
        PivotTable::new(6, 3, columns(&[0, 1])),
    );
    assert!(s.frame(V).movable(Axis::Cols), "plain columns move");
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(pivot()))));
    assert!(!s.frame(V).movable(Axis::Cols));
    s.input(|g| {
        g.pointer(PointerEvent::down(6.0, 0.5));
        g.pointer(PointerEvent::moved(18.0, 3.0));
    });
    assert_eq!(s.frame(V).drop_indicator(), None, "a drag offers no drop");
    s.input(|g| g.pointer(PointerEvent::up(18.0, 3.0)));
    assert!(
        !(s.effects().iter()).any(|e| matches!(e, binnacle_core::Effect::MoveRequested { .. }))
    );
}

#[test]
fn the_all_columns_are_pinned_right_by_default_one_per_value() {
    let mut s = scenario(&[0, 1, 2]);
    assert_eq!(pins(&s, Edge::Trailing), vec![Key(90), Key(91)]);
    assert_eq!(&drawn(&mut s)[drawn(&mut s).len() - 2..], &[90, 91]);
}

#[test]
fn with_the_values_off_the_columns_the_all_column_is_the_grand_total_pinned_before_any_window_lists_it()
 {
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(6, 1.0).header(2.0),
            AxisConfig::new(3, 5.0).header(4.0),
        ),
        PivotTable::new(
            6,
            3,
            vec![
                PivotTable::column(10, &[], QUARTER, "Q1"),
                PivotTable::column(11, &[], QUARTER, "Q2"),
                (GRAND_TOTAL, Outline::grand_total()),
            ],
        ),
    );
    s.frame(V);
    let mut grouping = pivot();
    grouping.col_levels = vec![Level::field(QUARTER)];
    grouping.values = vec![AggregateValue::count()];
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.pump();
    assert_eq!(pins(&s, Edge::Trailing), vec![GRAND_TOTAL]);
}

#[test]
fn an_all_column_the_user_unpins_stays_unpinned_as_the_columns_change() {
    let mut s = scenario(&[0, 1, 2]);
    s.input(|g| g.unpin(Axis::Cols, Key(90)));
    s.frame(V);
    change_columns(&mut s, &[0, 2]);
    change_columns(&mut s, &[0, 1, 2]);
    assert_eq!(pins(&s, Edge::Trailing), vec![Key(91)]);
}

#[test]
fn ungrouping_the_columns_unpins_every_all_column() {
    let mut s = scenario(&[0, 1]);
    s.input(|g| g.request_view(ViewSpec::default()));
    s.frame(V);
    assert_eq!(pins(&s, Edge::Trailing), vec![]);
}

#[test]
fn a_value_leaving_and_returning_keeps_its_columns_width_and_pin() {
    let mut s = scenario(&[0, 1, 2]);
    s.input(|g| {
        g.set_size(Axis::Cols, Key(14), 9.0);
        g.pin(Axis::Cols, Edge::Leading, Key(15));
    });
    assert_eq!(drawn(&mut s), vec![15, 10, 11, 12, 13, 14, 90, 91]);
    change_columns(&mut s, &[0, 1]);
    assert_eq!(drawn(&mut s), vec![10, 11, 12, 13, 90, 91], "Q3 left");
    assert_eq!(pins(&s, Edge::Leading), vec![Key(15)], "its pin sleeps");
    assert_eq!(s.grid.size_override(Axis::Cols, Key(14)), Some(9.0));
    change_columns(&mut s, &[0, 1, 2]);
    assert_eq!(drawn(&mut s), vec![15, 10, 11, 12, 13, 14, 90, 91]);
    let snap = s.frame(V);
    let q3 = snap.drawn(Axis::Cols).find(|c| c.key == Some(Key(14)));
    assert_eq!(q3.map(|c| c.size), Some(9.0), "its width came back");
}

#[test]
fn a_new_pivot_column_takes_the_default_width() {
    let mut s = scenario(&[0, 1]);
    change_columns(&mut s, &[0, 1, 2]);
    let snap = s.frame(V);
    let sizes: Vec<f32> = (snap.drawn(Axis::Cols))
        .filter(|c| matches!(c.key, Some(Key(14 | 15))))
        .map(|c| c.size)
        .collect();
    assert_eq!(sizes, vec![5.0, 5.0]);
}

#[test]
fn focus_on_a_pivot_column_that_leaves_falls_back_to_the_next_column() {
    let mut s = scenario(&[0, 1, 2]);
    s.input(|g| g.set_focus(Key(2), Key(12)));
    s.frame(V);
    change_columns(&mut s, &[0, 2]);
    let snap = s.frame(V);
    assert_eq!(snap.focus(Axis::Cols).and_then(|f| f.key), Some(Key(14)));
}

#[test]
fn each_quarter_spans_its_value_columns_in_the_column_header_band_and_the_all_columns_none() {
    let mut s = scenario(&[0, 1]);
    s.input(|g| g.set_tiers(Axis::Cols, &[1.0]));
    let snap = s.frame(V);
    let spans: Vec<(u32, u64, u64, u64)> = (snap.spans(Axis::Cols).iter())
        .map(|sp| (sp.tier, sp.key.0, sp.first, sp.last))
        .collect();
    assert_eq!(spans, vec![(0, 200, 0, 1), (0, 201, 2, 3)]);
    let heads: Vec<(u64, f32)> = (snap.headers(Axis::Cols))
        .filter_map(|h| Some((h.key?.0, h.rect.y)))
        .collect();
    assert!(
        heads.contains(&(10, 1.0)),
        "a value column starts under its span"
    );
    assert!(heads.contains(&(90, 0.0)), "an All column takes the band");
}
