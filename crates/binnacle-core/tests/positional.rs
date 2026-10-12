//! Seam 1 scenarios for positional Change Notices: the Table Source inserts,
//! removes or edits items, and focus, the range, picks, Size Overrides, the
//! Scroll Anchor and the active Mode follow the rules.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Effect, FocusStep, Grid, Key, Mode, Modifiers,
    PointerEvent, Viewport,
};
use binnacle_testkit::{KeyedTable, Scenario};

/// 20 rows of 1 under a 1-high header band; 3 columns of 6 with a Gap of 1
/// after a 4-wide row header band.
fn table() -> Scenario<KeyedTable> {
    let grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, KeyedTable::new(20, 3))
}

const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};

fn click(s: &mut Scenario<KeyedTable>, x: f32, y: f32, modifiers: Modifiers) {
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).modifiers(modifiers));
        g.pointer(PointerEvent::up(x, y).modifiers(modifiers));
    });
}

fn insert(s: &mut Scenario<KeyedTable>, axis: Axis, at: u64, n: u64) {
    let notice = s.source.insert(axis, at, n);
    s.input(|g| g.notify(axis, notice));
}

fn remove(s: &mut Scenario<KeyedTable>, axis: Axis, at: u64, n: u64) {
    let notice = s.source.remove(axis, at, n);
    s.input(|g| g.notify(axis, notice));
}

fn scroll_rows(s: &mut Scenario<KeyedTable>, items: i64) {
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items,
        })
    });
}

#[test]
fn rows_inserted_above_the_view_move_the_anchor_and_focus_with_their_items() {
    let mut s = table();
    s.frame(V);
    scroll_rows(&mut s, 5);
    s.frame(V);
    click(&mut s, 12.0, 2.5, Modifiers::NONE);
    let mut out = s.step("scroll to row 5, click row 6 column 1", V);
    insert(&mut s, Axis::Rows, 3, 2);
    out += &s.step(
        "insert 2 rows at 3: the anchor moves to row 7 and focus to row 8, the same items",
        V,
    );
    insert(&mut s, Axis::Rows, 9, 2);
    out += &s.step(
        "insert 2 rows at 9, below the focus: rows 9 and 10 are new, nothing else moves",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_removed_focus_falls_to_the_item_that_took_its_place_and_removed_keys_leave_picks_and_sizes() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.set_size(Axis::Rows, Key(2), 2.0);
        g.set_size(Axis::Cols, Key(1), 4.0);
    });
    s.frame(V);
    click(&mut s, 5.0, 1.5, Modifiers::CTRL);
    click(&mut s, 12.0, 4.5, Modifiers::CTRL);
    let mut out = s.step(
        "row 2 sized 2, column 1 sized 4; pick rows 0 and 2 (record 2), focus record 2 in column 1",
        V,
    );
    remove(&mut s, Axis::Rows, 1, 2);
    out += &s.step(
        "remove rows 1 and 2: focus falls to record 3, now row 1; record 2 leaves the picks and its row size goes",
        V,
    );
    assert_eq!(s.grid.size_override(Axis::Rows, Key(2)), None);
    remove(&mut s, Axis::Cols, 1, 1);
    out += &s.step(
        "remove column 1: focus falls to column 2, now column 1; column sizes are kept",
        V,
    );
    assert_eq!(s.grid.size_override(Axis::Cols, Key(1)), Some(4.0));
    remove(&mut s, Axis::Cols, 1, 1);
    out += &s.step("remove the last column: focus falls back to column 0", V);
    remove(&mut s, Axis::Rows, 0, 18);
    out += &s.step("remove every row: focus is unplaced", V);
    insta::assert_snapshot!(out);
}

#[test]
fn rows_removed_above_the_view_keep_it_steady_and_a_removed_anchor_row_gives_way_to_the_next() {
    let mut s = table();
    s.frame(V);
    scroll_rows(&mut s, 8);
    let mut out = s.step("scroll to row 8", V);
    remove(&mut s, Axis::Rows, 2, 3);
    out += &s.step("remove rows 2..5: the anchor follows record 8 to row 5", V);
    remove(&mut s, Axis::Rows, 4, 2);
    out += &s.step(
        "remove rows 4..6, the anchor's row among them: the view starts at record 9, now row 4",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn the_range_shifts_and_shrinks_with_its_rows_and_goes_when_they_all_go() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 1.5, Modifiers::NONE);
    click(&mut s, 12.0, 4.5, Modifiers::SHIFT);
    let mut out = s.step("select rows 0..4, columns 0..2", V);
    insert(&mut s, Axis::Rows, 0, 1);
    out += &s.step("insert a row at 0: the range moves to rows 1..5", V);
    insert(&mut s, Axis::Rows, 2, 1);
    out += &s.step("insert a row at 2, inside it: the range grows to 1..6", V);
    remove(&mut s, Axis::Rows, 4, 4);
    out += &s.step(
        "remove rows 4..8, its tail: it shrinks to 1..4 and focus falls to row 4",
        V,
    );
    remove(&mut s, Axis::Rows, 0, 2);
    out += &s.step("remove rows 0..2, its head: it shrinks to 0..2", V);
    remove(&mut s, Axis::Rows, 0, 2);
    out += &s.step("remove rows 0..2, all of it: no range", V);
    insta::assert_snapshot!(out);
}

#[test]
fn removed_rows_shrink_a_whole_selection_and_select_all_stays() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.action(Action::SelectAll));
    let mut out = s.step("select all", V);
    remove(&mut s, Axis::Rows, 0, 5);
    out += &s.step("remove 5 rows: still every row, SelectionChanged", V);
    remove(&mut s, Axis::Rows, 0, 15);
    out += &s.step("remove every row: no range", V);
    insta::assert_snapshot!(out);
}

/// A host may answer `Locate` late. An answer worked out before rows were
/// inserted names a stale Position; the Grid asks again and waits for the
/// later answer.
#[test]
fn an_insert_while_locating_asks_again_and_ignores_the_stale_answer() {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 2.5, Modifiers::NONE);
    s.frame(V);
    let notice = s.source.reverse(Axis::Rows);
    s.input(|g| g.notify(Axis::Rows, notice));
    // Drain the Locate for record 1 and work out its answer, but hold it.
    let locate = std::iter::from_fn(|| s.grid.poll_effect())
        .find(|e| matches!(e.effect, Effect::Locate { .. }))
        .expect("a Locate");
    let Effect::Locate { axis, key } = locate.effect else {
        unreachable!()
    };
    let stale = binnacle_testkit::ScenarioSource::locate(&mut s.source, axis, key);
    assert_eq!(stale, binnacle_core::Located::Found(18));
    insert(&mut s, Axis::Rows, 0, 3);
    s.input(|g| g.located(axis, key, stale));
    let snap = s.frame(V);
    assert_eq!(
        snap.focus(Axis::Rows),
        Some(binnacle_core::FocusEnd {
            position: 21,
            key: Some(Key(1))
        })
    );
}

#[test]
fn an_insert_cancels_a_selection_drag_and_restores_the_focus_from_before_the_press_moved_with_its_row()
 {
    let mut s = table();
    s.frame(V);
    click(&mut s, 5.0, 1.5, Modifiers::NONE);
    s.input(|g| {
        g.pointer(PointerEvent::down(12.0, 2.5));
        g.pointer(PointerEvent::moved(19.0, 4.5));
    });
    let mut out = s.step(
        "focus row 0; press row 1 column 1, drag to row 3 column 2",
        V,
    );
    insert(&mut s, Axis::Rows, 0, 1);
    s.input(|g| g.pointer(PointerEvent::moved(19.0, 3.5)));
    out += &s.step(
        "insert a row at 0: the drag is cancelled, focus is back on record 0, now row 1; moves do nothing",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn content_changes_keep_a_selection_drag_and_refetch_the_window() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(5.0, 1.5));
        g.pointer(PointerEvent::moved(12.0, 3.5));
    });
    let mut out = s.step("drag from row 0 column 0 to row 2 column 1", V);
    s.input(|g| g.notify(Axis::Rows, ChangeNotice::ContentChanged(1..2)));
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::ContentChanged(5..9)));
    s.input(|g| g.pointer(PointerEvent::moved(12.0, 4.5)));
    out += &s.step(
        "row 1's content changes (its Window is fetched again), columns past the count change (nothing to fetch); the drag goes on to row 3",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_header_move_is_cancelled_by_columns_inserted_or_removed_but_not_by_rows() {
    for notice in [
        ChangeNotice::Inserted { at: 3, n: 1 },
        ChangeNotice::Removed { at: 2, n: 1 },
    ] {
        let mut s = table();
        s.frame(V);
        // Column 0's header is x 4..10.
        s.input(|g| {
            g.pointer(PointerEvent::down(6.0, 0.5));
            g.pointer(PointerEvent::moved(20.0, 0.5));
        });
        assert!(matches!(s.frame(V).mode(), Mode::MovingHeaders { .. }));
        s.input(|g| g.notify(Axis::Rows, notice.clone()));
        assert!(matches!(s.frame(V).mode(), Mode::MovingHeaders { .. }));
        match notice {
            ChangeNotice::Inserted { at, n } => s.source.insert(Axis::Cols, at, n),
            ChangeNotice::Removed { at, n } => s.source.remove(Axis::Cols, at, n),
            _ => unreachable!(),
        };
        s.input(|g| {
            g.notify(Axis::Cols, notice.clone());
            g.pointer(PointerEvent::up(20.0, 0.5));
        });
        assert_eq!(s.frame(V).mode(), Mode::Idle, "{notice:?}");
        let moves = s
            .pump()
            .into_iter()
            .filter(|e| matches!(e.effect, Effect::MoveRequested { .. }))
            .count();
        assert_eq!(moves, 0, "{notice:?}");
    }
}

#[test]
fn a_resize_carries_on_through_inserts_and_removals_and_its_guide_follows_the_column() {
    let mut s = table();
    s.frame(V);
    // Column 1 spans x 11..17; its border is the Gap at x 17..18.
    s.input(|g| {
        g.pointer(PointerEvent::down(17.5, 0.5));
        g.pointer(PointerEvent::moved(18.5, 0.5));
    });
    let mut out = s.step("drag column 1's border 1 right: 7 wide", V);
    remove(&mut s, Axis::Cols, 0, 1);
    insert(&mut s, Axis::Rows, 0, 3);
    remove(&mut s, Axis::Rows, 5, 2);
    s.input(|g| g.pointer(PointerEvent::moved(19.5, 0.5)));
    out += &s.step(
        "remove column 0, insert and remove rows: still resizing column 1 (now at Position 0), 8 wide, the guide at its new end",
        V,
    );
    s.input(|g| g.pointer(PointerEvent::up(19.5, 0.5)));
    out += &s.step("release: SizeChanged for column 1", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_header_click_held_across_an_insert_or_removal_clicks_the_same_row_or_nothing() {
    let mut s = table();
    s.frame(V);
    // Row 2's header is y 3..4.
    s.input(|g| g.pointer(PointerEvent::down(1.0, 3.5)));
    insert(&mut s, Axis::Rows, 0, 2);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 3.5)));
    let snap = s.frame(V);
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.key), Some(Some(Key(2))));
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.position), Some(4));
    // The anchor moved with its row to 2, so y 2..3 is row 3, record 1.
    s.input(|g| g.pointer(PointerEvent::down(1.0, 2.5)));
    remove(&mut s, Axis::Rows, 3, 1);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 2.5)));
    let snap = s.frame(V);
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.key), Some(Some(Key(2))));
    assert_eq!(snap.mode(), Mode::Idle);
}

#[test]
fn an_axis_scrolled_to_its_end_keeps_following_the_end_as_rows_are_appended() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    let mut out = s.step("scroll to the end", V);
    insert(&mut s, Axis::Rows, 20, 2);
    out += &s.step("append 2 rows: the view follows the end", V);
    insta::assert_snapshot!(out);
}

/// Orchestrator decision from #23: a click is absolute focus input, so it
/// drops a reveal a focus move left pending as well as buffered moves.
#[test]
fn a_click_after_a_focus_move_in_the_same_frame_scrolls_nothing() {
    // A cell in row 3, then row 3's header.
    for x in [5.0, 1.0] {
        let mut s = table();
        s.frame(V);
        s.input(|g| {
            g.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: false,
            });
            g.pointer(PointerEvent::down(x, 4.5));
            g.pointer(PointerEvent::up(x, 4.5));
        });
        // A shorter viewport leaves the clicked row 3 below the body.
        let snap = s.frame(Viewport::new(26.0, 3.0));
        assert_eq!(snap.focus(Axis::Rows).map(|f| f.position), Some(3), "x {x}");
        assert_eq!(snap.scroll_anchor(Axis::Rows).position, 0, "x {x}");
    }
}
