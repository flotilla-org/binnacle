//! Seam 1 scenarios for pointer input and v1 selection: clicks, Shift-click,
//! drags, Ctrl-click picks, header clicks, `SelectAll`, whole-column selection
//! and `Cancel`, seen through Snapshots and drained Effects.

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FocusStep, HeaderClick, Key, Mode, Modifiers, PointerEvent,
    SelectionUnit, Viewport,
};
use binnacle_testkit::{IdentityTable, Scenario};

/// 20 rows of 1 with no Gap under a 1-high header band; 5 columns of 6 with a
/// Gap of 1 after a 4-wide row header band.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};

fn click(s: &mut Scenario<IdentityTable>, x: f32, y: f32, modifiers: Modifiers) {
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).modifiers(modifiers));
        g.pointer(PointerEvent::up(x, y).modifiers(modifiers));
    });
}

#[test]
fn a_click_focuses_the_cell_under_the_pointer_even_with_no_row_gap() {
    let mut s = table();
    let mut out = s.step("start", V);
    click(&mut s, 5.0, 3.0, Modifiers::NONE);
    out += &s.step("click row 2 (y 3), column 0", V);
    click(&mut s, 11.5, 3.99, Modifiers::NONE);
    out += &s.step("click the last sliver of row 2, column 1", V);
    click(&mut s, 18.0, 1.0, Modifiers::NONE);
    out += &s.step("click the first unit of row 0, column 2", V);
    click(&mut s, 17.5, 2.0, Modifiers::NONE);
    out += &s.step("click the column Gap: nothing changes", V);
    click(&mut s, 30.0, 2.0, Modifiers::NONE);
    out += &s.step("click outside the viewport: nothing changes", V);
    insta::assert_snapshot!(out);
}

#[test]
fn shift_click_selects_the_range_from_the_focused_cell_and_a_plain_click_clears_it() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    let mut out = s.step("click row 1, column 0", V);
    click(&mut s, 12.0, 4.0, Modifiers::SHIFT);
    out += &s.step("shift-click row 3, column 1: the range spans both", V);
    click(&mut s, 5.0, 1.0, Modifiers::SHIFT);
    out += &s.step("shift-click row 0, column 0: same anchor, new end", V);
    click(&mut s, 19.0, 3.0, Modifiers::NONE);
    out += &s.step("plain click: the range is gone", V);
    insta::assert_snapshot!(out);
}

#[test]
fn dragging_past_the_threshold_selects_a_range_and_cancel_restores_the_state_before_the_press() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 1.0, Modifiers::NONE);
    s.input(|g| g.set_drag_threshold(2.0));
    let mut out = s.step("focus row 0, column 0; threshold 2", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(12.0, 2.0));
        g.pointer(PointerEvent::moved(12.0, 3.5));
    });
    out += &s.step("press row 1, column 1 and move 1.5: still a press", V);
    s.input(|g| g.pointer(PointerEvent::moved(19.0, 3.5)));
    out += &s.step("move on to row 2, column 2: dragging", V);
    s.input(|g| g.pointer(PointerEvent::moved(40.0, 9.0)));
    out += &s.step("past the viewport: the nearest visible cell", V);
    s.input(|g| g.action(Action::Cancel));
    out += &s.step("cancel: focus and selection as before the press", V);
    s.input(|g| {
        g.pointer(PointerEvent::moved(5.0, 1.0));
        g.pointer(PointerEvent::up(5.0, 1.0));
    });
    out += &s.step("the rest of the cancelled press is ignored", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(12.0, 2.0));
        g.pointer(PointerEvent::moved(19.0, 3.0));
        g.pointer(PointerEvent::up(19.0, 3.0));
    });
    out += &s.step("drag again and release: the range stays", V);
    insta::assert_snapshot!(out);
}

#[test]
fn ctrl_click_toggles_row_picks_which_survive_shift_click_and_go_on_a_plain_click() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 1.0, Modifiers::CTRL);
    click(&mut s, 12.0, 3.0, Modifiers::META);
    let mut out = s.step("ctrl-click row 0, cmd-click row 2", V);
    click(&mut s, 12.0, 4.0, Modifiers::SHIFT);
    out += &s.step("shift-click row 3: range from row 2, picks kept", V);
    click(&mut s, 12.0, 1.0, Modifiers::CTRL);
    out += &s.step("ctrl-click row 0 again: unpicked", V);
    click(&mut s, 12.0, 2.0, Modifiers::NONE);
    out += &s.step("plain click: range and picks gone", V);
    insta::assert_snapshot!(out);
}

#[test]
fn row_headers_select_rows_and_column_headers_follow_their_header_click_setting() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 12.0, 2.0, Modifiers::NONE);
    s.input(|g| g.pointer(PointerEvent::moved(1.0, 3.0)));
    let mut out = s.step("focus row 1, column 1; hover row header 2", V);
    click(&mut s, 1.0, 3.0, Modifiers::NONE);
    out += &s.step(
        "click row header 2: the whole row, focus keeps its column",
        V,
    );
    click(&mut s, 1.0, 4.0, Modifiers::SHIFT);
    out += &s.step("shift-click row header 3: rows 2 to 3", V);
    click(&mut s, 1.0, 1.0, Modifiers::CTRL);
    out += &s.step("ctrl-click row header 0: picks the row", V);
    click(&mut s, 13.0, 0.0, Modifiers::NONE);
    out += &s.step(
        "click column header 1: Sort, which the identity source rejects",
        V,
    );
    s.input(|g| g.set_header_click(Axis::Cols, HeaderClick::Select));
    click(&mut s, 13.0, 0.0, Modifiers::NONE);
    out += &s.step("with Select: the whole column", V);
    s.input(|g| g.set_header_click(Axis::Rows, HeaderClick::None));
    click(&mut s, 1.0, 2.0, Modifiers::NONE);
    s.input(|g| g.pointer(PointerEvent::moved(17.5, 0.0)));
    out += &s.step("row headers set to None; hover the Gap after a header", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_header_press_dragged_past_the_threshold_is_not_a_click() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 2.0));
        g.pointer(PointerEvent::moved(1.0, 4.0));
    });
    let snap = s.frame(V);
    assert!(snap.capturing());
    assert_eq!(snap.mode(), Mode::Idle);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 4.0)));
    let snap = s.frame(V);
    assert!(!snap.capturing());
    assert_eq!(snap.selection_range(), None);
    assert_eq!(snap.focus(Axis::Rows), None);
}

#[test]
fn with_rows_selection_a_cell_click_selects_its_row() {
    let mut s = table();
    s.input(|g| g.set_selection_unit(SelectionUnit::Rows));
    s.frame(V);
    click(&mut s, 12.0, 2.0, Modifiers::NONE);
    let mut out = s.step("click row 1", V);
    click(&mut s, 5.0, 4.0, Modifiers::SHIFT);
    out += &s.step("shift-click row 3: rows 1 to 3, every column", V);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Item,
            forward: false,
            extend: false,
        })
    });
    out += &s.step("up: row 2 alone", V);
    insta::assert_snapshot!(out);
}

#[test]
fn shift_arrows_select_all_and_whole_columns() {
    let mut s = table();
    s.frame(V);
    let extend = |axis, forward| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward,
        extend: true,
    };
    s.input(|g| g.action(Action::SelectColumn));
    s.input(|g| g.action(extend(Axis::Rows, true)));
    let mut out = s.step(
        "nothing focused: select column does nothing; shift-down places focus",
        V,
    );
    s.input(|g| {
        g.action(extend(Axis::Rows, true));
        g.action(extend(Axis::Cols, true));
    });
    out += &s.step("shift-down, shift-right: rows 0 to 1, columns 0 to 1", V);
    s.input(|g| g.action(Action::SelectColumn));
    out += &s.step("select column: every row of columns 0 to 1", V);
    s.input(|g| g.action(extend(Axis::Rows, true)));
    out += &s.step("shift-down: focus moves, the columns stay whole", V);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        })
    });
    out += &s.step("plain right: the range is gone", V);
    s.input(|g| g.action(Action::SelectAll));
    out += &s.step("select all", V);
    insta::assert_snapshot!(out);
}

#[test]
fn selection_changed_comes_at_most_once_per_drain() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(5.0, 1.0));
        for y in [2.0, 3.0, 4.0, 2.0] {
            g.pointer(PointerEvent::moved(12.0, y));
        }
        g.pointer(PointerEvent::up(12.0, 2.0));
        g.action(Action::SelectAll);
    });
    let drained: Vec<Effect> = s.pump().into_iter().map(|e| e.effect).collect();
    let count = |e: &Effect| drained.iter().filter(|d| *d == e).count();
    assert_eq!(drained.len(), 2, "{drained:?}");
    assert_eq!(count(&Effect::SelectionChanged), 1);
    assert_eq!(
        count(&Effect::FocusChanged {
            row: Key(1),
            col: Key(1)
        }),
        1
    );
}
