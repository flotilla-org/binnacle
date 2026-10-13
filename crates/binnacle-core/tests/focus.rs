//! Seam 1 scenarios for keyboard focus: `MoveFocus`, the view following the
//! focused cell, and `FocusChanged`, seen through Snapshots and drained Effects.

use std::ops::Range;

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FocusStep, Grid, Key, Position, ScrollAnchor, Viewport,
};
use binnacle_testkit::{IdentityTable, PumpSource, Scenario, ScenarioSource};

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

#[test]
fn focus_changed_is_sent_at_most_once_per_drain_naming_the_latest_cell() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    s.frame(v);
    s.input(|g| {
        g.action(step(Axis::Rows, FocusStep::Item, true));
        g.action(step(Axis::Rows, FocusStep::Item, true));
        g.action(Action::TypeText("a".into()));
        g.action(step(Axis::Cols, FocusStep::Item, true));
    });
    let drained: Vec<Effect> = s.effects();
    assert_eq!(
        drained,
        vec![
            Effect::TextTyped { text: "a".into() },
            Effect::FocusChanged {
                row: Key(1),
                col: Key(1)
            },
        ]
    );
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, false)));
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Item, true)));
    assert_eq!(
        s.effects(),
        vec![],
        "back where the host last heard it was: nothing to report"
    );
}

#[test]
fn a_scroll_after_a_focus_move_wins_over_revealing_the_focus() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    s.frame(v);
    s.input(|g| {
        g.action(step(Axis::Rows, FocusStep::Edge, true));
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.0,
        });
    });
    let snap = s.frame(v);
    assert_eq!(snap.scroll_anchor(Axis::Rows), ScrollAnchor::default());
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.position), Some(19));
    assert_eq!(snap.focus_rect(), None, "the focused row is scrolled away");
}

#[test]
fn focus_moves_reveal_against_the_viewport_of_the_next_frame() {
    let mut s = table();
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, true)));
    let snap = s.frame(Viewport::new(22.0, 4.0));
    assert_eq!(snap.scroll_anchor(Axis::Rows).position, 17);
    assert!(snap.focus_rect().is_some());
}

#[test]
fn focus_stays_unplaced_while_either_axis_is_empty() {
    for (rows, cols) in [(0, 5), (20, 0)] {
        let mut s = Scenario::identity(AxisConfig::new(rows, 1.0), AxisConfig::new(cols, 6.0));
        for by in [FocusStep::Item, FocusStep::Page, FocusStep::Edge] {
            s.input(|g| g.action(step(Axis::Rows, by, true)));
        }
        let snap = s.frame(Viewport::new(22.0, 4.0));
        assert_eq!(snap.focus(Axis::Rows), None);
        assert_eq!(snap.focus(Axis::Cols), None);
        assert!(s.pump().is_empty());
    }
}

/// Identity Keys; row 1 is the largest size the default size range allows,
/// 2^20, where an f32 anchor offset steps by 1/16, and every other row 8.
struct TallMiddleRow;

impl PumpSource for TallMiddleRow {}

impl ScenarioSource for TallMiddleRow {
    fn window(&mut self, _axis: Axis, range: Range<Position>) -> Vec<Key> {
        range.map(Key).collect()
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        let size = |k: &Key| if k.0 == 1 { 1_048_576.0 } else { 8.0 };
        (axis == Axis::Rows).then(|| (keys.iter().map(size).collect(), 8.0))
    }
}

/// Revealing the last row anchors 13 1/32 before the end of the tall row above
/// it, which an f32 offset there cannot hold: the anchor rounds toward the
/// start (never showing space past the last item), so the focused row ends
/// past the body by less than one f32 step of the tall row's stride, here
/// 1/32 against a step of 1/16.
#[test]
fn a_revealed_row_after_a_far_larger_one_ends_within_one_anchor_step_of_the_body() {
    let grid = Grid::new(AxisConfig::new(3, 8.0), AxisConfig::new(1, 6.0));
    let mut s = Scenario::new(grid, TallMiddleRow);
    let v = Viewport::new(6.0, 21.03125);
    s.frame(v);
    s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, true)));
    let snap = s.frame(v);
    assert_eq!(
        snap.scroll_anchor(Axis::Rows),
        ScrollAnchor {
            position: 1,
            offset: 1_048_576.0 - 13.062_5
        }
    );
    let last = *snap.rows().last().unwrap();
    assert_eq!((last.position, last.offset, last.size), (2, 13.0625, 8.0));
    assert!(snap.focus_rect().is_some());
}

/// Rows whose content is one unit tall (also the minimum-size hint) under a
/// default of 513.
struct OneUnitRows;

impl PumpSource for OneUnitRows {}

impl ScenarioSource for OneUnitRows {
    fn window(&mut self, _axis: Axis, range: Range<Position>) -> Vec<Key> {
        range.map(Key).collect()
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        (axis == Axis::Rows).then(|| (vec![1.0; keys.len()], 1.0))
    }
}

/// Found by Hegel: End placed the end clamp with the default size, the Window
/// then brought one-unit rows and the clamp pulled the anchor onto a row
/// outside the Window, still 513 tall. Its f32 offset there steps by 2^-14,
/// so the focused last row ended past the body. With the Window reaching
/// back from the end, the last row ends the body exactly, first frame or not.
#[test]
fn end_with_rows_smaller_than_the_default_ends_the_body_exactly() {
    for first_frame in [false, true] {
        let grid = Grid::new(
            AxisConfig::new(999_999_999_999_000, 513.0),
            AxisConfig::new(1, 1.0),
        );
        let mut s = Scenario::new(grid, OneUnitRows);
        let v = Viewport::new(1.0, 3.000_030_5);
        if first_frame {
            s.frame(v);
        }
        s.input(|g| g.action(step(Axis::Rows, FocusStep::Edge, true)));
        let snap = s.frame(v);
        for item in snap.rows() {
            assert_eq!((item.key, item.size), (Some(Key(item.position)), 1.0));
        }
        let last = *snap.rows().last().unwrap();
        assert_eq!(last.position, 999_999_999_998_999);
        assert_eq!(
            last.end(),
            snap.extent(Axis::Rows),
            "first frame {first_frame}"
        );
    }
}
