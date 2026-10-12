//! Seam 1 scenarios for keyboard focus: `MoveFocus`, the view following the
//! focused cell, and `FocusChanged`, seen through Snapshots and drained Effects.

use binnacle_core::{Action, Axis, AxisConfig, FocusStep, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0),
    )
}

fn step(axis: Axis, by: FocusStep, forward: bool) -> Action {
    Action::MoveFocus {
        axis,
        by,
        forward,
        extend: false,
    }
}

#[test]
fn arrow_moves_focus_by_one_item_and_the_view_follows() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    let mut out = s.step("no focus yet", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, true)));
    out += &s.step("first move focuses the first visible cell", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, true)));
    out += &s.step("down", v);
    s.input(|g| {
        g.action(step(Axis::Rows, FocusStep::Item, true));
        g.action(step(Axis::Rows, FocusStep::Item, true));
    });
    out += &s.step("down twice: past the last visible row, so it scrolls", v);
    s.input(|g| g.action(step(Axis::Cols, FocusStep::Item, true)));
    s.input(|g| g.action(step(Axis::Cols, FocusStep::Item, true)));
    s.input(|g| g.action(step(Axis::Cols, FocusStep::Item, true)));
    out += &s.step("right three: the columns scroll too", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, false)));
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, false)));
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, false)));
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, false)));
    out += &s.step("up four: at the first row, the top shows it", v);
    insta::assert_snapshot!(out);
}

#[test]
fn page_moves_focus_a_viewport_length_and_edge_to_the_first_or_last_item() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, true)));
    let mut out = s.step("focus the first cell", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Page, true)));
    out += &s.step("page down: three rows show, so three rows on", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, true)));
    out += &s.step("end", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Page, true)));
    out += &s.step("page down at the last row: nowhere to go", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Page, false)));
    out += &s.step("page up", v);
    s.input(|g| g.action(step(Axis::Cols, FocusStep::Edge, true)));
    out += &s.step("last column", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, false)));
    s.input(|g| g.action(step(Axis::Cols, FocusStep::Edge, false)));
    out += &s.step("home on both axes", v);
    insta::assert_snapshot!(out);
}

#[test]
fn end_on_a_quadrillion_rows_focuses_a_position_whose_key_the_next_window_completes() {
    let mut s = Scenario::identity(
        AxisConfig::new(1_000_000_000_000_000, 1.0).header(1.0),
        AxisConfig::new(2, 20.0).header(17.0),
    );
    let v = Viewport::new(57.0, 5.0);
    let mut out = s.step("start", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, true)));
    let snap = s.grid.snapshot();
    // Before the next frame the Window still holds the first rows.
    let row = snap.focus(Axis::Rows).unwrap();
    assert_eq!((row.position, row.key), (999_999_999_999_999, None));
    assert_eq!(snap.focus_rect(), None);
    assert!(s.pump().is_empty(), "FocusChanged waits for the row's Key");
    out += &s.step("end: the Window there completes the focus", v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Page, false)));
    out += &s.step("page up", v);
    insta::assert_snapshot!(out);
}
