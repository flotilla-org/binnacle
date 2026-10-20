//! Seam 1 scenarios for pinning: Keys pinned to an axis's leading edge are
//! drawn in their own Strip between the Header Band and the scrolling
//! Strip, which skips them.

use binnacle_core::{
    Action, Axis, AxisConfig, Edge, Grid, Key, Modifiers, PointerEvent, SortColumn, SortSpec,
    ViewSpec, Viewport,
};
use binnacle_testkit::{ColumnMajorTable, IdentityTable, Scenario};

/// 4 rows of 1 under a 1-high header band; 10 columns of 6 with a Gap of 1
/// after a 4-wide row header band. Column p starts at x `4 + 7p`.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(10, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 32.0,
    height: 5.0,
};

#[test]
fn a_pinned_column_stays_at_the_left_while_the_body_scrolls() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(2)));
    out += &s.step("pin column 2: it leads, the body skips it", V);
    s.input(|g| g.action(Action::Scroll { dx: 10.0, dy: 0.0 }));
    out += &s.step("scroll right 10: only the body moves", V);
    s.input(|g| g.unpin(Axis::Cols, Key(2)));
    out += &s.step("unpin: back in its place, the anchor kept", V);
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(2));
        g.action(Action::Scroll { dx: 100.0, dy: 0.0 });
    });
    out += &s.step(
        "pin again and go far right: the scroll extent leaves it out",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn clicks_and_borders_land_on_the_pinned_and_the_scrolling_strips() {
    let mut s = table();
    s.input(|g| {
        g.set_border_grab(0.5);
        g.pin(Axis::Cols, Edge::Leading, Key(5));
    });
    s.frame(V);
    s.input(|g| g.action(Action::Scroll { dx: 10.0, dy: 0.0 }));
    let mut out = s.step("column 5 pinned, the body scrolled right 10", V);
    s.click(6.0, 2.5, Modifiers::NONE);
    out += &s.step("click the pinned column's cell: row 1, column 5", V);
    s.click(15.0, 3.5, Modifiers::NONE);
    out += &s.step("click the scrolling Strip: row 2, column 2", V);
    s.click(10.5, 2.5, Modifiers::NONE);
    out += &s.step("click the edge Gap: the pinned column's Gap, nothing", V);
    s.input(|g| g.pointer(PointerEvent::moved(10.5, 0.5)));
    out += &s.step("hover the pinned header's border: a resize cursor", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
    });
    out += &s.step("drag it 2 right: the pinned column widens, the body follows", V);
    s.input(|g| g.pointer(PointerEvent::up(12.5, 0.5)));
    out += &s.step("release: SizeChanged", V);
    insta::assert_snapshot!(out);
}

/// Six records as columns over three fields: field 0 holds
/// `[5, 13, 8, 31, 9, 23]` and sorts, field 1 the record's index times 10.
/// Fields of 1 under a 1-high header band, records of 4 with a Gap of 1
/// after a 4-wide row header band.
fn records() -> Scenario<ColumnMajorTable> {
    let field0 = [5, 13, 8, 31, 9, 23];
    let records = (0..6)
        .map(|i| vec![Some(field0[i]), Some(i as i64 * 10)])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(2, 1.0).header(1.0),
        AxisConfig::new(6, 4.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, ColumnMajorTable::new(records, 2, [0]))
}

const NARROW: Viewport = Viewport {
    width: 24.0,
    height: 3.0,
};

#[test]
fn a_pin_follows_its_key_through_a_sort_and_sleeps_while_filtered_out() {
    let mut s = records();
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(4)));
    let mut out = s.step("record 4 pinned", NARROW);
    s.input(|g| {
        g.request_view(ViewSpec::default().with_sort(
            Axis::Cols,
            SortSpec::new(vec![SortColumn::descending(Key(0))]),
        ))
    });
    out += &s.step(
        "the records sorted by field 0, descending: record 4 (9) is Position 3 now, still pinned",
        NARROW,
    );
    let notice = s.source.filter(|r| r[0].is_some_and(|v| v > 10));
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step(
        "filter to field 0 over 10: record 4 has gone, its pin sleeps",
        NARROW,
    );
    let notice = s.source.filter(|_| true);
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step("the filter cleared: record 4 is back in its pinned Strip", NARROW);
    insta::assert_snapshot!(out);
}
