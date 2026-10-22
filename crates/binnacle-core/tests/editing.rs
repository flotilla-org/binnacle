//! Seam 1 scenarios for activating, editing and typing: the Grid emits
//! `Activated`, `EditRequested` and `TextTyped` and takes no side on what they
//! mean; a host that calls `edit_started` puts it in `Editing`.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, EditEnd, Effect, FocusEnd, FocusStep, Grid, Key, Mode,
    Modifiers, PointerEvent, SelectionUnit, SortColumn, SortSpec, ViewSpec, Viewport,
};
use binnacle_testkit::{IdentityTable, PumpSource, RecordTable, Scenario, ScenarioSource};

const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};

/// Twenty rows of 1 under a 1-high header band, three columns of 6 with a Gap
/// of 1 after a 4-wide row header band. Keys are Positions.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    )
}

fn down(s: &mut Scenario<IdentityTable>) {
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        })
    });
}

#[test]
fn activate_and_begin_edit_name_the_focused_cell() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.action(Action::Activate));
    assert_eq!(s.effects(), vec![], "unplaced focus activates nothing");
    down(&mut s);
    down(&mut s);
    s.frame(V);
    s.input(|g| {
        g.action(Action::Activate);
        g.action(Action::BeginEdit);
        g.action(Action::TypeText("ab".into()));
    });
    let cell = (Key(1), Key(0));
    assert_eq!(
        s.effects(),
        vec![
            Effect::Activated {
                row: cell.0,
                col: cell.1,
                drill_down: None,
            },
            Effect::EditRequested {
                row: cell.0,
                col: cell.1
            },
            Effect::TextTyped { text: "ab".into() },
        ]
    );
}

fn press<S: ScenarioSource>(
    s: &mut Scenario<S>,
    x: f32,
    y: f32,
    modifiers: Modifiers,
    clicks: u32,
) {
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).modifiers(modifiers).clicks(clicks));
        g.pointer(PointerEvent::up(x, y).modifiers(modifiers).clicks(clicks));
    });
}

#[test]
fn a_plain_double_click_on_a_cell_activates_it() {
    let mut s = table();
    s.frame(V);
    // Row 2 (y 3), column 1 (x 11..17).
    press(&mut s, 12.0, 3.0, Modifiers::NONE, 1);
    let first = s.effects();
    assert!(!first.iter().any(|e| matches!(e, Effect::Activated { .. })));
    press(&mut s, 12.0, 3.0, Modifiers::NONE, 2);
    assert_eq!(
        s.effects(),
        vec![Effect::Activated {
            row: Key(2),
            col: Key(1),
            drill_down: None,
        }]
    );
    // A double press on a header, in a Gap, or under a modifier is no
    // activation.
    press(&mut s, 12.0, 0.0, Modifiers::NONE, 2);
    press(&mut s, 10.5, 3.0, Modifiers::NONE, 2);
    press(&mut s, 12.0, 3.0, Modifiers::SHIFT, 2);
    press(&mut s, 12.0, 3.0, Modifiers::CTRL, 2);
    let rest = s.effects();
    assert!(
        !rest.iter().any(|e| matches!(e, Effect::Activated { .. })),
        "{rest:?}"
    );
}

#[test]
fn edit_started_enters_editing_until_edit_ended() {
    let mut s = table();
    s.frame(V);
    down(&mut s);
    down(&mut s);
    s.input(|g| g.action(Action::BeginEdit));
    let mut out = s.step("focus row 1, F2: EditRequested, but no edit yet", V);
    s.input(|g| g.edit_started(Key(1), Key(0)));
    out += &s.step(
        "the host starts editing: Editing, editor rect, cell marked =",
        V,
    );
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 1.0 }));
    out += &s.step("scrolling a row carries the editor rect up", V);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 1.0 }));
    out += &s.step("scrolled out of view: still editing, no rect", V);
    s.input(|g| g.edit_ended(EditEnd::Commit));
    out += &s.step("the host commits: Idle, no Effect", V);
    s.input(|g| {
        g.edit_started(Key(4), Key(2));
        g.edit_ended(EditEnd::Cancel);
    });
    out += &s.step("an edit the host cancels itself sends nothing either", V);
    insta::assert_snapshot!(out);
}

#[test]
fn the_grid_cancels_an_edit_on_cancel_or_a_press_and_says_so() {
    let mut s = table();
    s.frame(V);
    // A double-click whose release comes after the host started editing.
    s.input(|g| g.pointer(PointerEvent::down(12.0, 3.0).clicks(2)));
    assert!(s.effects().contains(&Effect::Activated {
        row: Key(2),
        col: Key(1),
        drill_down: None,
    }));
    s.input(|g| {
        g.edit_started(Key(2), Key(1));
        g.pointer(PointerEvent::up(12.0, 3.0).clicks(2));
    });
    let snap = s.frame(V);
    assert_eq!(
        snap.mode(),
        Mode::Editing {
            row: Key(2),
            col: Key(1)
        },
        "the release keeps the edit"
    );
    assert!(!snap.capturing(), "an edit holds no button");
    s.input(|g| g.action(Action::Cancel));
    assert_eq!(
        s.effects(),
        vec![Effect::EditCancelled {
            row: Key(2),
            col: Key(1)
        }]
    );
    assert_eq!(s.frame(V).mode(), Mode::Idle);
    s.input(|g| g.action(Action::Cancel));
    assert_eq!(s.effects(), vec![], "Cancel with no edit sends nothing");

    s.input(|g| g.edit_started(Key(2), Key(1)));
    press(&mut s, 5.0, 4.0, Modifiers::NONE, 1);
    let drained = s.effects();
    assert_eq!(
        drained[0],
        Effect::EditCancelled {
            row: Key(2),
            col: Key(1)
        },
        "{drained:?}"
    );
    let snap = s.frame(V);
    assert_eq!(snap.mode(), Mode::Idle);
    assert_eq!(
        snap.focus(Axis::Rows).unwrap().key,
        Some(Key(3)),
        "and the press goes on"
    );
}

#[test]
fn edit_started_ends_a_gesture_in_progress() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(5.0, 1.0));
        g.pointer(PointerEvent::moved(12.0, 3.0));
    });
    assert_eq!(s.frame(V).mode(), Mode::DraggingSelection);
    s.input(|g| {
        g.edit_started(Key(0), Key(0));
        g.pointer(PointerEvent::moved(19.0, 4.0));
        g.pointer(PointerEvent::up(19.0, 4.0));
    });
    let snap = s.frame(V);
    assert_eq!(
        snap.mode(),
        Mode::Editing {
            row: Key(0),
            col: Key(0)
        }
    );
    let range = snap.selection_range().unwrap();
    assert_eq!(
        (range.rows.clone(), range.cols.clone()),
        (run(0..3), run(0..2)),
        "the drag stopped where it was"
    );
}

/// Eight records whose column 0 holds `[5, 3, 8, 1, 9, 2, 7, 4]` over three
/// columns, sortable by column 0, laid out like [`table`].
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

const TALL: Viewport = Viewport {
    width: 26.0,
    height: 10.0,
};

#[test]
fn an_edit_follows_its_record_through_a_sort_and_is_cancelled_when_filtered_out() {
    let mut s = records();
    s.frame(TALL);
    s.input(|g| g.edit_started(Key(1), Key(2)));
    let mut out = s.step("editing record 1, column 2; focus unplaced", TALL);
    let spec = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    let t = &mut s.source;
    let notices = t
        .source
        .view(&t.rows, &ViewSpec::default().with_sort(Axis::Rows, spec))
        .unwrap();
    s.input(|g| notices.into_iter().for_each(|(axis, n)| g.notify(axis, n)));
    out += &s.step(
        "sorted by column 0: record 1 (value 3) is now row 2 and the editor follows; no Locate for a reorder",
        TALL,
    );
    let notice = s.source.filter(|r| r[0] != Some(9));
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step(
        "filter out value 9: the edited record stays (Locate answers Found) and so does the edit",
        TALL,
    );
    let notice = s.source.filter(|r| r[0] != Some(3));
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step(
        "filter out value 3, the edited record: Locate answers Gone, EditCancelled",
        TALL,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn an_edit_on_the_focused_cell_shares_its_locate_and_ends_with_an_emptied_axis() {
    let mut s = records();
    s.frame(TALL);
    // Focus record 3 (row 3) and edit it.
    press(&mut s, 5.0, 4.0, Modifiers::NONE, 1);
    s.input(|g| g.edit_started(Key(3), Key(0)));
    let mut out = s.step("focus and edit record 3", TALL);
    let notice = s.source.filter(|r| r[0] != Some(1));
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step(
        "filter out record 3: one Locate answers for focus and edit; focus falls back to row 3, the edit is cancelled",
        TALL,
    );
    s.input(|g| g.edit_started(Key(4), Key(1)));
    s.frame(TALL);
    let notice = s.source.filter(|_| false);
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step(
        "editing record 4, then nothing left: cancelled at once",
        TALL,
    );
    insta::assert_snapshot!(out);
}

/// A thousand rows, otherwise like [`table`].
fn long_table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(1000, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    )
}

#[test]
fn set_focus_moves_focus_to_keys_locating_those_outside_the_window() {
    let mut s = long_table();
    s.frame(V);
    s.input(|g| g.set_focus(Key(2), Key(1)));
    let mut out = s.step("set_focus to a visible cell: no Locate", V);
    s.input(|g| g.set_focus(Key(500), Key(2)));
    out += &s.step(
        "set_focus to row 500, outside the Window: Locate, then focus lands there and is revealed",
        V,
    );
    s.input(|g| g.set_focus(Key(5000), Key(0)));
    out += &s.step(
        "set_focus to a row Key the source does not have: focus stays on row 500, column 0 taken",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn set_focus_selects_like_a_focus_move_once_placed() {
    let mut s = long_table();
    s.input(|g| g.set_selection_unit(SelectionUnit::Rows));
    s.frame(V);
    s.input(|g| {
        g.action(Action::SelectAll);
        g.set_focus(Key(1), Key(1));
    });
    let snap = s.frame(V);
    let range = snap.selection_range().unwrap();
    assert_eq!(
        (range.rows.clone(), range.cols.clone()),
        (run(1..2), run(0..3)),
        "the row, as a click would"
    );
    s.input(|g| g.set_focus(Key(700), Key(1)));
    let snap = s.frame(V);
    assert_eq!(snap.focus(Axis::Rows).unwrap().position, 700);
    let range = snap.selection_range().unwrap();
    assert_eq!(
        (range.rows.clone(), range.cols.clone()),
        (run(700..701), run(0..3)),
        "the located row once its Position is known"
    );
}

#[test]
fn focus_moves_after_set_focus_wait_for_its_position() {
    let mut s = long_table();
    s.frame(V);
    s.input(|g| {
        g.set_focus(Key(300), Key(0));
        g.set_focus(Key(600), Key(0));
        g.set_focus(Key(300), Key(0));
    });
    down(&mut s);
    down(&mut s);
    let snap = s.frame(V);
    assert!(!snap.locating(Axis::Rows), "placed, not left waiting");
    assert_eq!(
        snap.focus(Axis::Rows),
        Some(FocusEnd {
            position: 302,
            key: Some(Key(302))
        })
    );
    let rect = snap.focus_rect().expect("revealed");
    assert!(rect.y >= 1.0 && rect.y + rect.height <= 5.0, "{rect:?}");
}

/// An identity table whose `Locate` answers name a Position past the count,
/// as a stale Table Source might.
struct PastTheEnd(IdentityTable);

impl ScenarioSource for PastTheEnd {
    fn window(&mut self, axis: Axis, range: std::ops::Range<u64>) -> Vec<Key> {
        self.0.window(axis, range)
    }
}

impl PumpSource for PastTheEnd {
    fn locate(&mut self, _axis: Axis, _key: Key) -> binnacle_core::Located {
        binnacle_core::Located::Found(1000)
    }
}

#[test]
fn a_located_position_past_the_count_counts_as_the_edited_key_leaving() {
    let grid = Grid::new(AxisConfig::new(20, 1.0), AxisConfig::new(3, 6.0));
    let source = PastTheEnd(IdentityTable {
        rows: binnacle_memory::IdentitySource::new(20),
        cols: binnacle_memory::IdentitySource::new(3),
    });
    let mut s = Scenario::new(grid, source);
    s.frame(V);
    s.input(|g| {
        g.edit_started(Key(1), Key(1));
        g.notify(Axis::Rows, binnacle_core::ChangeNotice::Reset(20));
    });
    assert!(s.effects().contains(&Effect::EditCancelled {
        row: Key(1),
        col: Key(1)
    }));
    assert_eq!(s.frame(V).mode(), Mode::Idle);
}

#[test]
fn a_drag_cancelled_after_set_focus_named_a_departed_key_restores_a_real_cell() {
    let mut s = table();
    s.frame(V);
    // Row Key 50 does not exist; the press lands on row 2, column 1 before
    // `located` answers, and the drag holds the focus from before it.
    s.input(|g| {
        g.set_focus(Key(50), Key(0));
        g.pointer(PointerEvent::down(12.0, 3.0));
        g.pointer(PointerEvent::moved(19.0, 4.0));
    });
    let mut out = s.step("set_focus to a departed row Key, then a drag", V);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    out += &s.step(
        "a notice cancels the drag: focus goes back to where set_focus left it once Key 50 was found gone (row 0), not to Key 50",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_drag_cancelled_after_its_pick_was_filtered_out_does_not_restore_the_pick() {
    let mut s = records();
    s.frame(TALL);
    // Ctrl-click picks and focuses record 3 (row 3).
    press(&mut s, 5.0, 4.0, Modifiers::CTRL, 1);
    s.frame(TALL);
    let notice = s.source.filter(|r| r[0] != Some(1));
    s.input(|g| {
        g.notify(Axis::Rows, notice);
        g.pointer(PointerEvent::down(12.0, 2.0));
        g.pointer(PointerEvent::moved(19.0, 3.0));
    });
    s.frame(TALL);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    let snap = s.frame(TALL);
    assert_eq!(snap.mode(), Mode::Idle);
    assert_eq!(
        snap.picked(Axis::Rows),
        &[] as &[Key],
        "record 3 left while the drag was held"
    );
}

/// A range's span on one axis that is a single run.
fn run(r: std::ops::Range<u64>) -> Vec<std::ops::Range<u64>> {
    vec![r]
}
