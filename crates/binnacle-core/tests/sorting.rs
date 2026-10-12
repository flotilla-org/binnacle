//! Seam 1 scenarios for sorting: header clicks request Sort Specs, the Table
//! Source answers, and focus and picks follow their Keys through the
//! `Reordered` Change Notice.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Effect, FocusStep, HeaderSort, Key, Mode, Modifiers,
    PointerEvent, SortColumn, SortDirection, SortSpec, Viewport,
};
use binnacle_testkit::{IdentityTable, RecordTable, Scenario, ScenarioSource, render};

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

fn click<S: ScenarioSource>(s: &mut Scenario<S>, x: f32, y: f32, modifiers: Modifiers) {
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).modifiers(modifiers));
        g.pointer(PointerEvent::up(x, y).modifiers(modifiers));
    });
}

/// The x of the middle of column header `c` in [`table`].
fn header_x(c: u64) -> f32 {
    4.0 + 7.0 * c as f32 + 3.0
}

/// The Sort Specs requested since the last drain, without answering them.
fn requested<S: ScenarioSource>(s: &mut Scenario<S>) -> Vec<SortSpec> {
    std::iter::from_fn(|| s.grid.poll_effect())
        .filter_map(|e| match e.effect {
            Effect::SortRequested { spec } => Some(spec),
            _ => None,
        })
        .collect()
}

#[test]
fn a_column_header_click_cycles_ascending_descending_none_and_shift_adds_a_column() {
    let mut s = table();
    s.frame(V);
    let (asc, desc) = (SortColumn::ascending, SortColumn::descending);
    let spec = |c: Vec<SortColumn>| SortSpec::new(c);
    let mut seen = Vec::new();
    for _ in 0..3 {
        click(&mut s, header_x(1), 0.0, Modifiers::NONE);
        seen.extend(requested(&mut s));
    }
    assert_eq!(
        seen,
        vec![
            spec(vec![asc(Key(1))]),
            spec(vec![desc(Key(1))]),
            spec(vec![])
        ]
    );
    let mut seen = Vec::new();
    click(&mut s, header_x(1), 0.0, Modifiers::NONE);
    click(&mut s, header_x(2), 0.0, Modifiers::SHIFT);
    click(&mut s, header_x(1), 0.0, Modifiers::SHIFT);
    click(&mut s, header_x(1), 0.0, Modifiers::SHIFT);
    click(&mut s, header_x(2), 0.0, Modifiers::NONE);
    seen.extend(requested(&mut s));
    assert_eq!(
        seen,
        vec![
            spec(vec![asc(Key(1))]),
            spec(vec![asc(Key(1)), asc(Key(2))]),
            spec(vec![desc(Key(1)), asc(Key(2))]),
            spec(vec![asc(Key(2))]),
            spec(vec![desc(Key(2))]),
        ]
    );
}

/// Renders the Grid as it stands, without pumping, so unanswered requests
/// show.
fn unanswered<S: ScenarioSource>(s: &mut Scenario<S>, label: &str) -> String {
    format!("--- {label}\n{}", render(&s.grid.snapshot()))
}

#[test]
fn headers_show_the_applied_sort_and_a_pending_flag_until_answered() {
    let mut s = table();
    s.frame(V);
    click(&mut s, header_x(1), 0.0, Modifiers::NONE);
    let mut out = unanswered(&mut s, "click column 1: requested, nothing applied");
    let spec = requested(&mut s).pop().unwrap();
    s.input(|g| g.sort_applied(spec));
    out += &unanswered(&mut s, "applied");
    click(&mut s, header_x(2), 0.0, Modifiers::SHIFT);
    click(&mut s, header_x(2), 0.0, Modifiers::SHIFT);
    out += &unanswered(&mut s, "shift-click column 2 twice: two requests out");
    let specs = requested(&mut s);
    s.input(|g| g.sort_applied(specs[0].clone()));
    out += &unanswered(&mut s, "first applied: the second is still pending");
    let snap = s.grid.snapshot();
    let header = |_: Key, applied, pending| HeaderSort { applied, pending };
    assert_eq!(
        snap.header_sort(Key(1)),
        header(Key(1), Some((SortDirection::Ascending, 0)), false)
    );
    assert_eq!(
        snap.header_sort(Key(2)),
        header(Key(2), Some((SortDirection::Ascending, 1)), true)
    );
    assert_eq!(snap.header_sort(Key(3)), HeaderSort::default());
    s.input(|g| g.sort_applied(specs[0].clone()));
    out += &unanswered(&mut s, "second rejected: the first stays applied");
    s.input(|g| g.set_sortable(Key(0), false));
    click(&mut s, header_x(0), 0.0, Modifiers::NONE);
    out += &unanswered(&mut s, "column 0 is not sortable: no request");
    assert_eq!(requested(&mut s), vec![]);
    insta::assert_snapshot!(out);
}

/// Eight records over three columns; columns 0 and 1 sort, column 2 does not
/// (the source has no sort keys for it). Laid out like [`table`].
fn records() -> Scenario<RecordTable> {
    let col0 = [5, 3, 8, 1, 9, 2, 7, 4];
    let col1 = [
        Some(1),
        None,
        Some(1),
        Some(0),
        None,
        Some(2),
        Some(0),
        Some(1),
    ];
    let rows = (0..8)
        .map(|i| vec![Some(col0[i]), col1[i], Some(i as i64)])
        .collect();
    let grid = binnacle_core::Grid::new(
        AxisConfig::new(8, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, RecordTable::new(rows, 3, [0, 1]))
}

#[test]
fn focus_and_picks_follow_their_records_through_a_sort_and_the_range_is_dropped() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 1.0, Modifiers::CTRL);
    click(&mut s, 5.0, 3.0, Modifiers::CTRL);
    click(&mut s, 12.0, 4.0, Modifiers::SHIFT);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 1,
        })
    });
    let mut out = s.step(
        "pick records 0 and 2, shift-click record 3, column 1; scroll down one",
        V,
    );
    click(&mut s, header_x(0), 0.0, Modifiers::NONE);
    out += &s.step(
        "sort by column 0: focus follows record 3 off screen, picks follow, range gone, anchor stays",
        V,
    );
    click(&mut s, header_x(0), 0.0, Modifiers::NONE);
    out += &s.step("again: descending", V);
    click(&mut s, header_x(1), 0.0, Modifiers::SHIFT);
    out += &s.step(
        "shift-click column 1: added second; column 0 has no ties, so the order holds",
        V,
    );
    click(&mut s, header_x(2), 0.0, Modifiers::NONE);
    out += &s.step("column 2 has no sort keys: rejected, the sort stays", V);
    insta::assert_snapshot!(out);
}

fn down() -> Action {
    Action::MoveFocus {
        axis: Axis::Rows,
        by: FocusStep::Item,
        forward: true,
        extend: false,
    }
}

/// Sorts the records' source by column 0 and tells the Grid, leaving the
/// `Locate` it queues unanswered.
fn sort_behind_the_grid(s: &mut Scenario<RecordTable>) {
    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    let notice = s.source.sort(&spec).unwrap();
    s.input(|g| g.notify(Axis::Rows, notice));
}

#[test]
fn focus_moves_made_while_locating_are_applied_once_located_answers() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    let mut out = s.step("focus record 1 at row 1", V);
    sort_behind_the_grid(&mut s);
    s.input(|g| {
        g.action(down());
        g.action(down());
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        });
    });
    out += &unanswered(
        &mut s,
        "sorted by column 0 behind the Grid; down, down, right: buffered, focus drawn nowhere",
    );
    out += &s.step(
        "Locate answered: record 1 is at row 2, the moves land two rows on and one column right",
        V,
    );
    sort_behind_the_grid(&mut s);
    s.input(|g| g.action(down()));
    click(&mut s, 5.0, 1.0, Modifiers::NONE);
    out += &s.step(
        "again, with a click before the answer: the click wins and the buffered move is dropped",
        V,
    );
    sort_behind_the_grid(&mut s);
    out += &s.step(
        "a later Reordered and answer: the dropped move stays dropped",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn follow_focus_on_reorder_scrolls_to_the_focused_record() {
    let mut s = records();
    s.input(|g| g.set_follow_focus_on_reorder(true));
    s.frame(V);
    click(&mut s, 5.0, 4.0, Modifiers::NONE);
    let mut out = s.step("focus record 3 at row 3", V);
    click(&mut s, header_x(0), 0.0, Modifiers::NONE);
    click(&mut s, header_x(0), 0.0, Modifiers::NONE);
    out += &s.step(
        "sort by column 0 descending: record 3 is last, the view follows it",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn select_all_and_shift_click_while_locating_do_not_use_the_old_position() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    s.frame(V);
    sort_behind_the_grid(&mut s);
    s.input(|g| {
        g.action(down());
        g.action(Action::SelectAll);
    });
    let mut out = s.step(
        "record 1 sorted to row 2 behind the Grid; down, then select all: the move is dropped",
        V,
    );
    click(&mut s, 5.0, 1.0, Modifiers::NONE);
    out += &s.step("click record 3 at row 0", V);
    s.input(|g| g.notify(Axis::Rows, binnacle_core::ChangeNotice::Reordered));
    click(&mut s, 12.0, 4.0, Modifiers::SHIFT);
    out += &s.step(
        "Reordered, then a shift-click before the answer: no known focus to stretch from, so the cell alone",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn only_the_answer_to_the_latest_locate_places_the_focus() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    s.frame(V);
    sort_behind_the_grid(&mut s);
    // The host takes the Locate but has not answered when the order changes
    // again and the Grid asks once more.
    let first = s.grid.poll_effect().map(|e| e.effect);
    assert_eq!(
        first,
        Some(Effect::Locate {
            axis: Axis::Rows,
            key: Key(1)
        })
    );
    let descending = SortSpec::new(vec![SortColumn::descending(Key(0))]);
    let notice = s.source.source.sort(&s.source.rows, &descending).unwrap();
    s.input(|g| {
        g.notify(Axis::Rows, notice);
        g.located(Axis::Rows, Key(1), binnacle_core::Located::Found(2));
    });
    let snap = s.frame(V);
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.position), Some(5));
    assert!(!snap.locating(Axis::Rows));
}

#[test]
fn cancel_after_a_reorder_restores_the_focused_record_at_its_new_position() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    let mut out = s.step("focus record 1 at row 1", V);
    sort_behind_the_grid(&mut s);
    s.input(|g| g.pointer(PointerEvent::down(12.0, 4.0)));
    out += &s.step(
        "sorted behind the Grid, then a press on row 3 held while Locate is answered",
        V,
    );
    s.input(|g| g.action(Action::Cancel));
    out += &s.step("cancel: record 1 again, at row 2 where it now is", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_press_held_through_a_reorder_cancels_to_the_state_after_it() {
    let mut s = records();
    s.frame(V);
    click(&mut s, 5.0, 2.0, Modifiers::NONE);
    click(&mut s, 12.0, 4.0, Modifiers::SHIFT);
    s.input(|g| g.pointer(PointerEvent::down(1.0, 3.0)));
    let mut out = s.step("range rows 1 to 3; press row header 2 and hold", V);
    sort_behind_the_grid(&mut s);
    out += &s.step("sorted behind the Grid: the range is dropped", V);
    s.input(|g| g.action(Action::Cancel));
    out += &s.step("cancel: the dropped range does not come back", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_resize_carries_on_through_a_reorder_of_either_axis() {
    let mut s = records();
    s.frame(V);
    // Column 0 spans x 4..10; its border is the Gap at x 10..11.
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
    });
    let mut out = s.step("drag column 0's border 2 right: resizing", V);
    sort_behind_the_grid(&mut s);
    s.input(|g| g.notify(Axis::Cols, binnacle_core::ChangeNotice::Reordered));
    s.input(|g| g.pointer(PointerEvent::moved(13.5, 0.5)));
    out += &s.step(
        "rows sorted and columns reordered underneath: still resizing column 0, now 9 wide",
        V,
    );
    s.input(|g| g.pointer(PointerEvent::up(13.5, 0.5)));
    out += &s.step("release: SizeChanged for column 0", V);
    insta::assert_snapshot!(out);
}

/// Regression (found by the FocusChanged property): a focus whose Key a frame
/// completed is announced at the next drain, unless something moves focus
/// first. A Reordered that cancels a selection drag is such a move: it
/// restores the focus from before the press, and the queued announcement of
/// the abandoned cell goes with it.
#[test]
fn a_reorder_that_cancels_a_drag_withdraws_the_announcement_of_the_dragged_focus() {
    let mut s = Scenario::identity(
        AxisConfig::new(1, 1.0),
        AxisConfig::new(999_999_999_999_000, 1.0),
    );
    let v = Viewport::new(5.0, 3.0);
    s.frame(v);
    s.input(|g| {
        g.pointer(PointerEvent::down(0.5, 0.5));
        g.pointer(PointerEvent::moved(3.5, 0.5));
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Edge,
            forward: true,
            extend: false,
        });
    });
    s.pump();
    let snap = s.frame(v);
    let last = Key(999_999_999_998_999);
    assert_eq!(snap.focus(Axis::Cols).and_then(|f| f.key), Some(last));
    assert_eq!(snap.mode(), Mode::DraggingSelection);
    s.input(|g| g.notify(Axis::Rows, ChangeNotice::Reordered));
    let drained: Vec<Effect> = s.pump().into_iter().map(|e| e.effect).collect();
    assert!(
        !drained
            .iter()
            .any(|e| matches!(e, Effect::FocusChanged { .. })),
        "{drained:?}"
    );
    let snap = s.frame(v);
    assert_eq!(snap.mode(), Mode::Idle);
    assert_eq!(snap.focus(Axis::Rows), None);
}

/// Regression (found by the reveal property): a focus move made while
/// `located` is pending waits, and a click before the answer discards it, so
/// the click's cell is focused and, as clicks never scroll, nothing scrolls.
#[test]
fn a_click_discards_a_focus_move_waiting_for_located_and_scrolls_nothing() {
    let mut s = Scenario::identity(AxisConfig::new(1, 1.0), AxisConfig::new(50, 1.0));
    let wide = Viewport::new(4.0, 1.0);
    s.frame(wide);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction: 1.0,
        })
    });
    s.frame(wide);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::up(3.5, 0.5));
        g.notify(Axis::Cols, ChangeNotice::Reordered);
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Item,
            forward: false,
            extend: false,
        });
        g.pointer(PointerEvent::down(0.5, 0.5));
        g.pointer(PointerEvent::up(0.5, 0.5));
    });
    // A narrower viewport keeps the end in view, leaving column 46 off screen.
    let snap = s.frame(Viewport::new(2.0, 1.0));
    assert_eq!(snap.focus(Axis::Cols).map(|f| f.position), Some(46));
    assert_eq!(snap.scroll_anchor(Axis::Cols).position, 48);
    assert_eq!(snap.focus_rect(), None);
}
