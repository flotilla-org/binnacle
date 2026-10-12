//! Seam 1 scenarios for filtering: the Table Source sends `Reset` with the new
//! count, and focus, picks, Size Overrides and the Scroll Anchor settle.

use binnacle_core::{
    Action, Axis, AxisConfig, FocusStep, Grid, Key, Modifiers, PointerEvent, Viewport,
};
use binnacle_testkit::{RecordTable, Scenario, ScenarioSource};

const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};

/// Eight records whose column 0 holds `[5, 3, 8, 1, 9, 2, 7, 4]`, over three
/// columns, with a 1-high header band and 4-wide row header band; rows of 1,
/// columns of 6 with a Gap of 1.
fn records() -> Scenario<RecordTable> {
    let col0 = [5, 3, 8, 1, 9, 2, 7, 4];
    let rows = (0..8)
        .map(|i| vec![Some(col0[i]), None, Some(i as i64)])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(8, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, RecordTable::new(rows, 3, [0]))
}

fn click(s: &mut Scenario<RecordTable>, x: f32, y: f32, modifiers: Modifiers) {
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).modifiers(modifiers));
        g.pointer(PointerEvent::up(x, y).modifiers(modifiers));
    });
}

fn filter(s: &mut Scenario<RecordTable>, keep: impl Fn(&Vec<Option<i64>>) -> bool) {
    let notice = s.source.filter(keep);
    s.input(|g| g.notify(Axis::Rows, notice));
}

#[test]
fn a_filtered_out_focus_moves_to_the_nearest_survivor_and_leaves_the_picks() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_size(Axis::Rows, Key(0), 2.0);
        g.set_size(Axis::Cols, Key(0), 5.0);
    });
    click(&mut s, 5.0, 4.0, Modifiers::CTRL);
    click(&mut s, 12.0, 3.0, Modifiers::CTRL);
    let mut out = s.step(
        "row 0 sized 2, column 0 sized 5; pick records 2 and 1, focus record 1",
        V,
    );
    filter(&mut s, |r| r[0] != Some(3));
    out += &s.step(
        "filter out record 1 (value 3): focus goes to record 2, the next one, which is now row 1; the pick of 1 goes; row sizes drop, column sizes stay",
        V,
    );
    assert_eq!(s.grid.size_override(Axis::Rows, Key(0)), None);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(0)), Some(5.0));
    filter(&mut s, |r| r[0] >= Some(5));
    out += &s.step(
        "keep values of 5 and up: record 2 stays and focus follows it (FocusChanged for record 2 was queued when the last frame's Window named it)",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_sorted_and_filtered_focus_lands_on_the_nearest_survivor_in_sort_order() {
    let mut s = records();
    s.frame(V);
    let notice = s
        .source
        .sort(&binnacle_core::SortSpec::new(vec![
            binnacle_core::SortColumn::ascending(Key(0)),
        ]))
        .unwrap();
    s.input(|g| g.notify(Axis::Rows, notice));
    // Sorted values 1 2 3 4 5 7 8 9 are records 3 5 1 7 0 6 2 4.
    click(&mut s, 5.0, 3.0, Modifiers::NONE);
    let mut out = s.step("sort by column 0; focus value 3 (record 1) at row 2", V);
    filter(&mut s, |r| matches!(r[0], Some(1 | 2 | 7 | 8 | 9)));
    out += &s.step(
        "keep 1, 2, 7, 8, 9: value 3 is one from 2 and three from 7, so focus goes to 2 at row 1",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn filtering_everything_out_unplaces_focus_and_clamps_the_anchor() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 4,
        });
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Edge,
            forward: true,
            extend: false,
        });
        g.action(Action::SelectAll);
    });
    let mut out = s.step("scroll to the end, focus the last row, select all", V);
    filter(&mut s, |r| r[0] > Some(6));
    out += &s.step(
        "keep 7, 8, 9: the anchor clamps to the count, the whole range stays, focus moves to record 6, the nearest survivor in base order",
        V,
    );
    filter(&mut s, |_| false);
    out += &s.step("filter out everything: focus is unplaced", V);
    let notice = s.source.source.clear_filter();
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step("clear the filter: focus stays unplaced until a move", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_row_header_press_whose_row_a_filter_removes_selects_nothing_on_release() {
    let mut s = records();
    s.frame(V);
    // Row header of row 3 (y = 1 + 3).
    s.input(|g| g.pointer(PointerEvent::down(1.0, 4.5)));
    filter(&mut s, |r| r[0] > Some(6));
    s.input(|g| g.pointer(PointerEvent::up(1.0, 4.5)));
    let out = s.step(
        "press row 3's header, filter down to 3 rows, release: no row 3 to select",
        V,
    );
    insta::assert_snapshot!(out);
    let snap = s.grid.snapshot();
    assert_eq!(snap.focus(Axis::Rows), None);
    assert_eq!(snap.selection_range(), None);
}

#[test]
fn cancel_after_a_departed_key_is_located_keeps_focus_on_the_suggestion() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::CTRL);
    let mut out = s.step("pick and focus record 1 (value 3)", V);
    // Filter it out behind the Grid, then press a row header before the
    // pump answers `Locate`: the press saves record 1 as the focus to restore.
    filter(&mut s, |r| r[0] != Some(3));
    s.input(|g| g.pointer(PointerEvent::down(1.0, 3.5)));
    out += &s.step(
        "filter out record 1 and press row 2's header: focus moves to the suggestion, record 2",
        V,
    );
    s.input(|g| g.action(Action::Cancel));
    out += &s.step(
        "cancel the press: focus stays on record 2 and the departed pick does not come back",
        V,
    );
    insta::assert_snapshot!(out);
    let snap = s.grid.snapshot();
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(Key(2)));
    assert_eq!(snap.picked(), &[] as &[Key]);
}
