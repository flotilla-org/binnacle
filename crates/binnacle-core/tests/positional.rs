//! Seam 1 scenarios for positional Change Notices: the Table Source inserts,
//! removes or edits items, and focus, the range, picks, Size Overrides, the
//! Scroll Anchor and the active Mode follow the rules.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Effect, FocusEnd, FocusStep, Grid, Key, Mode,
    Modifiers, PointerEvent, Viewport,
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
    s.click(12.0, 2.5, Modifiers::NONE);
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
    s.click(5.0, 1.5, Modifiers::CTRL);
    s.click(12.0, 4.5, Modifiers::CTRL);
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
    s.click(5.0, 1.5, Modifiers::NONE);
    s.click(12.0, 4.5, Modifiers::SHIFT);
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
    s.click(5.0, 2.5, Modifiers::NONE);
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
    let stale = binnacle_testkit::PumpSource::locate(&mut s.source, axis, key);
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
    s.click(5.0, 1.5, Modifiers::NONE);
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

#[test]
fn an_edit_follows_its_row_through_inserts_and_is_cancelled_when_its_row_or_column_is_removed() {
    // Removed in the Window, removed out of sight (found by Locate), and the
    // column removed.
    for (axis, at, scroll) in [(Axis::Rows, 3, 0), (Axis::Rows, 3, 10), (Axis::Cols, 1, 0)] {
        let mut s = table();
        s.frame(V);
        s.input(|g| g.edit_started(Key(3), Key(1)));
        insert(&mut s, Axis::Rows, 0, 2);
        remove(&mut s, Axis::Rows, 0, 2);
        scroll_rows(&mut s, scroll);
        assert_eq!(
            s.frame(V).mode(),
            Mode::Editing {
                row: Key(3),
                col: Key(1)
            }
        );
        assert_eq!(s.effects(), vec![], "{axis:?} {scroll}");
        remove(&mut s, axis, at, 1);
        let drained = s.effects();
        assert!(
            drained.contains(&Effect::EditCancelled {
                row: Key(3),
                col: Key(1)
            }),
            "{axis:?} {scroll}: {drained:?}"
        );
        assert_eq!(s.frame(V).mode(), Mode::Idle);
    }
}

#[test]
fn an_edit_survives_the_removal_of_other_rows_in_and_out_of_sight() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.edit_started(Key(3), Key(1)));
    remove(&mut s, Axis::Rows, 1, 1);
    remove(&mut s, Axis::Rows, 15, 2);
    scroll_rows(&mut s, 10);
    s.frame(V);
    remove(&mut s, Axis::Rows, 0, 1);
    let drained = s.effects();
    assert!(
        !drained
            .iter()
            .any(|e| matches!(e, Effect::EditCancelled { .. })),
        "{drained:?}"
    );
    assert_eq!(
        s.frame(V).mode(),
        Mode::Editing {
            row: Key(3),
            col: Key(1)
        }
    );
}

/// Regression (found by the FocusChanged property): a drag's press captured
/// a focus whose Key `set_focus` had named and `Locate` then found gone. A
/// notice cancelling the drag restored that departed Key and announced it.
/// A Key that leaves now leaves the state a press would restore too.
#[test]
fn cancelling_a_drag_never_restores_a_key_that_has_left() {
    let mut s = Scenario::identity(AxisConfig::new(1, 1.0), AxisConfig::new(1, 1.0));
    let v = Viewport::new(1.0, 1.0);
    s.frame(v);
    s.input(|g| {
        g.set_focus(Key(1), Key(0));
        g.pointer(PointerEvent::down(0.5, 0.5));
        g.pointer(PointerEvent::moved(0.5, 1.5));
    });
    s.frame(v);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    let drained: Vec<Effect> = s.effects();
    assert!(
        !drained.contains(&Effect::FocusChanged {
            row: Key(1),
            col: Key(0)
        }),
        "{drained:?}"
    );
    let snap = s.frame(v);
    assert_eq!(snap.mode(), Mode::Idle);
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(Key(0)));
}

/// 40 rows of 1 under a 1-high header band; 5 columns of 3 after a 2-wide
/// row header band.
fn tall() -> Scenario<KeyedTable> {
    let grid = Grid::new(
        AxisConfig::new(40, 1.0).header(1.0),
        AxisConfig::new(5, 3.0).header(2.0),
    );
    Scenario::new(grid, KeyedTable::new(40, 5))
}

const TALL: Viewport = Viewport {
    width: 20.0,
    height: 10.0,
};

/// Regression (found by the reviewer's chaos driver): a press saved the
/// focus while its row Key was being located, and `set_focus` during the
/// press then located another Key. Cancel restored the first Key at its
/// stale Position with nobody locating it. A `set_focus` during a press now
/// replaces what Cancel restores.
#[test]
fn cancel_after_set_focus_during_a_press_keeps_the_focus_set() {
    let mut s = tall();
    s.frame(TALL);
    s.click(3.0, 4.5, Modifiers::NONE);
    s.frame(TALL);
    s.source.axes[0].swap(3, 5);
    s.input(|g| {
        g.notify(Axis::Rows, ChangeNotice::Reordered);
        g.pointer(PointerEvent::down(9.0, 7.5));
    });
    s.frame(TALL);
    s.input(|g| {
        g.set_focus(Key(30), Key(2));
        g.action(Action::Cancel);
    });
    let snap = s.frame(TALL);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.locating(Axis::Rows));
    assert_eq!(
        snap.focus(Axis::Rows),
        Some(FocusEnd {
            position: 30,
            key: Some(Key(30))
        })
    );
}

/// The same with Keys the Table Source never had: Cancel must not leave
/// focus on a phantom Key nobody is locating, nor announce it.
#[test]
fn cancel_after_set_focus_to_missing_keys_during_a_press_never_announces_them() {
    let mut s = tall();
    s.frame(TALL);
    s.click(3.0, 4.5, Modifiers::NONE);
    s.frame(TALL);
    s.pump();
    s.input(|g| {
        g.pointer(PointerEvent::down(9.0, 7.5));
        g.set_focus(Key(683), Key(2));
    });
    insert(&mut s, Axis::Rows, 0, 1);
    s.input(|g| {
        g.set_focus(Key(139), Key(2));
        g.action(Action::Cancel);
    });
    let drained: Vec<Effect> = s.effects();
    assert!(
        !drained
            .iter()
            .any(|e| matches!(e, Effect::FocusChanged { row, .. } if row.0 >= 100)),
        "{drained:?}"
    );
    let snap = s.frame(TALL);
    let f = snap.focus(Axis::Rows).expect("focus is placed");
    assert!(!snap.locating(Axis::Rows));
    assert_eq!(f.key, s.source.axes[0].get(f.position as usize).copied());
}

/// Regression (found by the reviewer's chaos driver): a Reset while a press
/// held a focus end with no Key kept a Position learned from an earlier
/// `located`, and Cancel put focus there, past the new count.
#[test]
fn cancel_after_a_reset_keeps_focus_below_the_count() {
    let grid = Grid::new(
        AxisConfig::new(8, 1.0).header(1.0),
        AxisConfig::new(5, 3.0).header(2.0),
    );
    let mut s = Scenario::new(grid, KeyedTable::new(8, 5));
    s.frame(TALL);
    s.click(12.0, 2.5, Modifiers::NONE);
    s.frame(TALL);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    s.pump();
    s.input(|g| g.pointer(PointerEvent::down(6.0, 2.5)));
    s.source.axes[1] = vec![Key(0), Key(3)];
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reset(2)));
    s.frame(TALL);
    s.input(|g| g.action(Action::Cancel));
    let snap = s.frame(TALL);
    assert_eq!(
        snap.focus(Axis::Cols),
        Some(FocusEnd {
            position: 1,
            key: Some(Key(3))
        })
    );
}

/// Regression (found by review): an Inserted past `u64::MAX` items
/// overflowed the count (a panic in debug builds, focus at the count in
/// release). The Grid ignores the items that would not fit.
#[test]
fn an_insert_past_the_largest_count_is_clipped() {
    let mut s = Scenario::identity(
        AxisConfig::new(u64::MAX, 1.0).header(1.0),
        AxisConfig::new(5, 3.0).header(2.0),
    );
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Edge,
            forward: true,
            extend: false,
        })
    });
    s.frame(TALL);
    s.input(|g| g.notify(Axis::Rows, ChangeNotice::Inserted { at: 0, n: 5 }));
    let snap = s.frame(TALL);
    assert_eq!(snap.count(Axis::Rows), u64::MAX);
    let f = snap.focus(Axis::Rows).expect("focus is placed");
    assert_eq!(f.position, u64::MAX - 1);
}
