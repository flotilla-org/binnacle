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
