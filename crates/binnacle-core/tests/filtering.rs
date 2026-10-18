//! Seam 1 scenarios for filtering: the Table Source sends `Refiltered` with the
//! new count, and focus, picks, Size Overrides and the Scroll Anchor settle by
//! Key.

use binnacle_core::{
    Action, Axis, AxisConfig, FocusStep, Grid, Key, Modifiers, PointerEvent, ViewSpec, Viewport,
};
use binnacle_testkit::{RecordTable, Scenario};

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

fn filter(s: &mut Scenario<RecordTable>, keep: impl Fn(&Vec<Option<i64>>) -> bool + 'static) {
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
    s.click(5.0, 4.0, Modifiers::CTRL);
    s.click(12.0, 3.0, Modifiers::CTRL);
    let mut out = s.step(
        "row 0 sized 2, column 0 sized 5; pick records 2 and 1, focus record 1",
        V,
    );
    filter(&mut s, |r| r[0] != Some(3));
    out += &s.step(
        "filter out record 1 (value 3): focus goes to record 2, the next one, which is now row 1; the pick of 1 goes; Size Overrides stay",
        V,
    );
    assert_eq!(s.grid.size_override(Axis::Rows, Key(0)), Some(2.0));
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
    let spec = binnacle_core::SortSpec::new(vec![binnacle_core::SortColumn::ascending(Key(0))]);
    let t = &mut s.source;
    let notice = t
        .source
        .view(&t.rows, &ViewSpec::default().with_sort(Axis::Rows, spec))
        .unwrap();
    s.input(|g| g.notify(Axis::Rows, notice));
    // Sorted values 1 2 3 4 5 7 8 9 are records 3 5 1 7 0 6 2 4.
    s.click(5.0, 3.0, Modifiers::NONE);
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
fn filtering_everything_out_returns_the_anchor_to_the_start() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_size(Axis::Rows, Key(0), 4.0);
        g.action(Action::Scroll { dx: 0.0, dy: 2.5 });
    });
    let mut out = s.step("row 0 sized 4; scroll 2.5 into it, past a default row", V);
    filter(&mut s, |_| false);
    out += &s.step(
        "filter out everything: with no item to hold it, the anchor goes back to 0",
        V,
    );
    assert_eq!(s.grid.snapshot().scroll_anchor(Axis::Rows).offset, 0.0);
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
    s.click(5.0, 2.0, Modifiers::CTRL);
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

#[test]
fn picks_of_rows_a_filter_hides_are_dropped_and_the_others_stay() {
    let mut s = records();
    s.frame(V);
    for y in [1.5, 2.5, 4.5] {
        s.click(5.0, y, Modifiers::CTRL);
    }
    let mut out = s.step("pick records 0, 1 and 3, focusing 3", V);
    filter(&mut s, |r| r[0] != Some(3) && r[0] != Some(1));
    out += &s.step(
        "filter out records 1 and 3 (values 3 and 1): their picks go, record 0's stays; focus goes to record 4, the next one",
        V,
    );
    assert_eq!(s.grid.snapshot().picked(), &[Key(0)]);
    let notice = s.source.source.clear_filter();
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step("clear the filter: the dropped picks do not come back", V);
    assert_eq!(s.grid.snapshot().picked(), &[Key(0)]);
    insta::assert_snapshot!(out);
}

#[test]
fn size_overrides_survive_a_filter_round_trip() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_size(Axis::Rows, Key(1), 2.0);
        g.set_size(Axis::Rows, Key(3), 3.0);
    });
    let mut out = s.step("records 1 and 3 sized 2 and 3", V);
    filter(&mut s, |r| r[0] != Some(3));
    out += &s.step(
        "filter out record 1: record 3, now row 2, keeps its size",
        V,
    );
    let notice = s.source.source.clear_filter();
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step("clear the filter: record 1 is back at its size", V);
    assert_eq!(s.grid.size_override(Axis::Rows, Key(1)), Some(2.0));
    assert_eq!(s.grid.size_override(Axis::Rows, Key(3)), Some(3.0));
    insta::assert_snapshot!(out);
}

#[test]
fn the_topmost_visible_row_that_survives_stays_at_the_top() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 3.5 }));
    let mut out = s.step("scroll half into record 3: records 3 to 7 show", V);
    filter(&mut s, |r| {
        r[0] != Some(1) && r[0] != Some(3) && r[0] != Some(5)
    });
    out += &s.step(
        "filter out records 0, 1 and 3: record 4, the topmost survivor, now row 1, starts the body",
        V,
    );
    assert_eq!(
        s.grid.snapshot().scroll_anchor(Axis::Rows),
        binnacle_core::ScrollAnchor {
            position: 1,
            offset: 0.0
        }
    );
    let notice = s.source.source.clear_filter();
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step("clear the filter: record 4 stays on top, now row 4", V);
    assert_eq!(s.grid.snapshot().scroll_anchor(Axis::Rows).position, 4);
    insta::assert_snapshot!(out);
}

#[test]
fn the_top_row_keeps_its_offset_when_it_survives() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 2.5 }));
    let mut out = s.step("scroll half into record 2", V);
    filter(&mut s, |r| r[0] != Some(5));
    out += &s.step(
        "filter out record 0: record 2, now row 1, stays half scrolled at the top",
        V,
    );
    assert_eq!(
        s.grid.snapshot().scroll_anchor(Axis::Rows),
        binnacle_core::ScrollAnchor {
            position: 1,
            offset: 0.5
        }
    );
    insta::assert_snapshot!(out);
}

#[test]
fn with_no_visible_row_surviving_the_focused_row_is_shown() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_focus(Key(7), Key(0)));
    let mut out = s.step(
        "focus record 7, scrolled to the bottom: records 4 to 7 show",
        V,
    );
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: -8,
        })
    });
    out += &s.step("scroll back to the top, focus out of view", V);
    filter(&mut s, |r| r[0] == Some(4) || r[0] == Some(7));
    out += &s.step(
        "keep records 6 and 7 (values 7 and 4): none shown survives, so focus, now row 1, is brought into view",
        V,
    );
    assert_eq!(
        s.grid.snapshot().focus(Axis::Rows).and_then(|f| f.key),
        Some(Key(7))
    );
    insta::assert_snapshot!(out);
}

#[test]
fn with_no_visible_row_surviving_and_no_focus_the_view_goes_to_the_top() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 4,
        })
    });
    let mut out = s.step("scroll to records 4 to 7, nothing focused", V);
    filter(&mut s, |r| {
        r[0] == Some(5) || r[0] == Some(3) || r[0] == Some(8) || r[0] == Some(1) || r[0] == Some(9)
    });
    out += &s.step(
        "keep records 0 to 4: of those shown only record 4 survives; at row 4 it goes as high as the end allows",
        V,
    );
    filter(&mut s, |r| {
        r[0] == Some(5) || r[0] == Some(3) || r[0] == Some(8) || r[0] == Some(1)
    });
    out += &s.step("keep records 0 to 3: none shown survives and nothing is focused, so the view goes to the top", V);
    assert_eq!(
        s.grid.snapshot().scroll_anchor(Axis::Rows),
        binnacle_core::ScrollAnchor::default()
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_scroll_while_a_filter_is_located_keeps_the_scroll() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 2,
        })
    });
    s.step("scroll to record 2", V);
    filter(&mut s, |r| r[0] != Some(5));
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 1,
        })
    });
    let out = s.step(
        "filter out record 0, then scroll one row before the answers: the scroll wins",
        V,
    );
    assert_eq!(s.grid.snapshot().scroll_anchor(Axis::Rows).position, 3);
    insta::assert_snapshot!(out);
}

#[test]
fn an_edit_on_a_row_the_filter_hides_ends() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.edit_started(Key(1), Key(0)));
    let mut out = s.step("edit record 1, column 0", V);
    filter(&mut s, |r| r[0] != Some(5));
    out += &s.step("filter out record 0: the edit on record 1 goes on", V);
    assert_eq!(
        s.grid.snapshot().mode(),
        binnacle_core::Mode::Editing {
            row: Key(1),
            col: Key(0)
        }
    );
    filter(&mut s, |r| r[0] != Some(3));
    out += &s.step("filter out record 1: its edit ends", V);
    assert_eq!(s.grid.snapshot().mode(), binnacle_core::Mode::Idle);
    insta::assert_snapshot!(out);
}

#[test]
fn a_range_is_dropped_unless_it_spans_the_axis() {
    let mut s = records();
    s.frame(V);
    s.click(5.0, 1.5, Modifiers::NONE);
    s.click(12.0, 3.5, Modifiers::SHIFT);
    let mut out = s.step("select records 0 to 2 in columns 0 and 1", V);
    filter(&mut s, |r| r[0] != Some(9));
    out += &s.step("filter out record 4: the range goes, focus stays", V);
    assert_eq!(s.grid.snapshot().selection_range(), None);
    s.input(|g| g.action(Action::SelectColumn));
    out += &s.step("select column 1 whole", V);
    filter(&mut s, |r| r[0] != Some(8));
    out += &s.step("filter out record 2: the whole column stays selected", V);
    assert!(s.grid.snapshot().selection_range().is_some());
    insta::assert_snapshot!(out);
}

/// Regression (found by the state machine): a pick made by a press still
/// held came back on `Cancel` after a filter emptied the rows.
#[test]
fn cancel_after_a_filter_empties_the_rows_brings_no_pick_back() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.pointer(PointerEvent::down(5.0, 1.5).modifiers(Modifiers::CTRL)));
    filter(&mut s, |_| false);
    s.input(|g| g.action(Action::Cancel));
    let out = s.step(
        "Ctrl-press record 0, filter out everything, then cancel the press: no pick comes back",
        V,
    );
    assert_eq!(s.grid.snapshot().picked(), &[] as &[Key]);
    insta::assert_snapshot!(out);
}
