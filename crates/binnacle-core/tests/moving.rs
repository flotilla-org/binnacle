//! Seam 1 scenarios for column moves: dragging a column header past the
//! threshold, the drop indicator, the move cursor, `MoveRequested` and
//! `Cancel`.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Cursor, Effect, Grid, Key, Mode, Modifiers,
    PointerEvent, SortColumn, SortSpec, Support, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{IdentityTable, RecordTable, Scenario};

/// 20 rows of 1 with a Gap of 1 under a 1-high header band; 5 columns of 6
/// with a Gap of 1 after a 4-wide row header band. Column p spans x
/// `4 + 7p .. 10 + 7p`; the Gap after it is the next unit.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 40.0,
    height: 6.0,
};

#[test]
fn dragging_a_column_header_moves_it_with_a_drop_indicator() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| g.pointer(PointerEvent::down(7.0, 0.5)));
    out += &s.step("press on column 0's header: nothing yet", V);
    s.input(|g| g.pointer(PointerEvent::moved(7.5, 0.5)));
    out += &s.step("move under the threshold: still pressed", V);
    s.input(|g| g.pointer(PointerEvent::moved(13.0, 0.5)));
    out += &s.step(
        "past the threshold, over column 1's first half: drop at 1",
        V,
    );
    s.input(|g| g.pointer(PointerEvent::moved(22.0, 3.0)));
    out += &s.step("over column 2's second half, in the body: drop at 3", V);
    s.input(|g| g.pointer(PointerEvent::up(22.0, 3.0)));
    out += &s.step("release: MoveRequested, idle", V);
    insta::assert_snapshot!(out);
}

#[test]
fn the_drop_position_follows_the_pointer_and_clamps_past_either_end() {
    let mut s = table();
    s.frame(V);
    let drop_at = |s: &mut Scenario<IdentityTable>, x: f32| {
        s.input(|g| g.pointer(PointerEvent::moved(x, 0.5)));
        let snap = s.frame(V);
        let Mode::MovingHeaders { axis, key, to } = snap.mode() else {
            panic!("{:?}", snap.mode());
        };
        assert_eq!((axis, key), (Axis::Cols, Key(2)));
        assert_eq!(snap.cursor(), Cursor::Move);
        let indicator = snap.drop_indicator().expect("an indicator while moving");
        assert_eq!((indicator.axis, indicator.to), (Axis::Cols, to));
        (to, indicator.offset)
    };
    s.input(|g| g.pointer(PointerEvent::down(20.0, 0.5)));
    // The indicator sits in the middle of the Gap before the drop Position.
    assert_eq!(drop_at(&mut s, 12.0), (1, 10.5));
    assert_eq!(drop_at(&mut s, 6.9), (0, 3.5));
    assert_eq!(drop_at(&mut s, 7.0), (1, 10.5));
    assert_eq!(drop_at(&mut s, 10.5), (1, 10.5));
    assert_eq!(drop_at(&mut s, 1.0), (0, 3.5));
    assert_eq!(drop_at(&mut s, -30.0), (0, 3.5));
    assert_eq!(drop_at(&mut s, 37.5), (5, 38.5));
    assert_eq!(drop_at(&mut s, 39.5), (5, 38.5));
    assert_eq!(drop_at(&mut s, 90.0), (5, 38.5));
}

#[test]
fn cancel_ends_the_move_and_the_release_requests_nothing() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(30.0, 0.5));
        g.action(Action::Cancel);
    });
    let snap = s.frame(V);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
    assert_eq!(snap.drop_indicator(), None);
    assert_eq!(snap.cursor(), Cursor::Default);
    s.input(|g| g.pointer(PointerEvent::up(30.0, 0.5)));
    assert_eq!(s.effects(), vec![]);
}

#[test]
fn a_drop_that_leaves_the_order_unchanged_requests_nothing() {
    let mut s = table();
    s.frame(V);
    for x in [8.0, 16.0] {
        // Column 1, dropped just before or just after itself.
        s.input(|g| {
            g.pointer(PointerEvent::down(14.0, 0.5));
            g.pointer(PointerEvent::moved(x, 0.5));
            g.pointer(PointerEvent::up(x, 0.5));
        });
        assert_eq!(s.effects(), vec![]);
    }
}

#[test]
fn a_header_drag_is_no_click_and_a_border_drag_resizes_instead() {
    let mut s = table();
    s.input(|g| g.set_header_click(Axis::Cols, binnacle_core::HeaderClick::Select));
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(22.0, 0.5));
        g.pointer(PointerEvent::up(22.0, 0.5));
    });
    assert_eq!(
        s.effects(),
        vec![Effect::MoveRequested {
            axis: Axis::Cols,
            keys: vec![Key(0)],
            to: 3
        }]
    );
    assert_eq!(s.frame(V).selection_range(), None);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(22.0, 0.5));
    });
    assert!(matches!(s.frame(V).mode(), Mode::Resizing { .. }));
    s.input(|g| g.pointer(PointerEvent::up(22.0, 0.5)));
    assert!(matches!(s.effects()[..], [Effect::SizeChanged { .. }]));
}

#[test]
fn row_header_drags_move_nothing() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 1.5));
        g.pointer(PointerEvent::moved(1.0, 5.5));
    });
    assert_eq!(s.frame(V).mode(), Mode::Idle);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 5.5)));
    assert_eq!(s.effects(), vec![]);
}

#[test]
fn dragging_a_selected_column_moves_every_selected_column() {
    let mut s = table();
    s.input(|g| g.set_header_click(Axis::Cols, binnacle_core::HeaderClick::Select));
    s.frame(V);
    // Select columns 1 to 2 by their headers.
    s.input(|g| {
        g.pointer(PointerEvent::down(14.0, 0.5));
        g.pointer(PointerEvent::up(14.0, 0.5));
        g.pointer(PointerEvent::down(21.0, 0.5).modifiers(binnacle_core::Modifiers::SHIFT));
        g.pointer(PointerEvent::up(21.0, 0.5));
    });
    s.pump();
    s.input(|g| {
        g.pointer(PointerEvent::down(21.0, 0.5));
        g.pointer(PointerEvent::moved(37.0, 0.5));
    });
    let snap = s.frame(V);
    assert_eq!(snap.moving_keys(), &[Key(1), Key(2)]);
    s.input(|g| g.pointer(PointerEvent::up(37.0, 0.5)));
    assert_eq!(
        s.effects(),
        vec![Effect::MoveRequested {
            axis: Axis::Cols,
            keys: vec![Key(1), Key(2)],
            to: 5
        }]
    );
    // An unselected column moves alone.
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(37.0, 0.5));
        g.pointer(PointerEvent::up(37.0, 0.5));
    });
    assert_eq!(
        s.effects(),
        vec![Effect::MoveRequested {
            axis: Axis::Cols,
            keys: vec![Key(0)],
            to: 5
        }]
    );
}

/// Three records of four integer columns behind an In-Memory Table Source,
/// laid out like [`table`].
fn records() -> Scenario<RecordTable> {
    let rows = vec![
        vec![Some(3), Some(30), Some(300), Some(3000)],
        vec![Some(1), Some(10), Some(100), Some(1000)],
        vec![Some(2), Some(20), Some(200), Some(2000)],
    ];
    Scenario::new(
        Grid::new(
            AxisConfig::new(3, 1.0).header(1.0).gap(1.0),
            AxisConfig::new(4, 6.0).header(4.0).gap(1.0),
        ),
        RecordTable::new(rows, 4, [0, 1, 2, 3]),
    )
}

#[test]
fn a_dropped_column_moves_in_the_source_and_its_width_and_focus_follow_it() {
    let mut s = records();
    s.input(|g| g.set_size(Axis::Cols, Key(0), 8.0));
    s.frame(V);
    // Column 0 now spans x 4..12, column 1 13..19, column 2 20..26.
    s.input(|g| {
        g.pointer(PointerEvent::down(6.0, 3.0));
        g.pointer(PointerEvent::up(6.0, 3.0));
    });
    let mut out = s.step("focus row 1 of column 0, which is 8 wide", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(24.0, 0.5));
    });
    out += &s.step("drag its header past column 2's middle", V);
    s.input(|g| g.pointer(PointerEvent::up(24.0, 0.5)));
    out += &s.step(
        "drop: the source moves it, the Grid refetches the columns and locates the focus",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn an_unsortable_column_stays_unsortable_at_the_position_it_moves_to() {
    let mut s = records();
    s.input(|g| g.set_support(Axis::Rows, Key(0), Support::default().with_sort(false)));
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(24.0, 0.5));
        g.pointer(PointerEvent::up(24.0, 0.5));
    });
    let snap = s.frame(V);
    let keys: Vec<_> = snap.cols().iter().map(|c| c.key).collect();
    assert_eq!(keys, [1, 2, 0, 3].map(|k| Some(Key(k))));
    // Key 0's header is now at Position 2, x 18..24.
    s.click(21.0, 0.5, Modifiers::NONE);
    assert_eq!(s.effects(), vec![]);
    // Position 0 now holds Key 1, which sorts.
    s.click(7.0, 0.5, Modifiers::NONE);
    assert_eq!(
        s.effects(),
        vec![Effect::ViewRequested {
            spec: ViewSpec::default().with_sort(
                Axis::Rows,
                SortSpec::new(vec![SortColumn::ascending(Key(1))])
            ),
            edit: ViewEdit::CycleSort {
                axis: Axis::Rows,
                key: Key(1),
                multi: false
            }
        }]
    );
}

#[test]
fn a_move_is_cancelled_when_its_axis_is_reordered_but_not_the_other() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(24.0, 0.5));
        g.notify(Axis::Rows, ChangeNotice::Reordered);
    });
    let snap = s.frame(V);
    assert!(matches!(snap.mode(), Mode::MovingHeaders { .. }));
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    let snap = s.frame(V);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
    assert_eq!(snap.drop_indicator(), None);
    s.input(|g| g.pointer(PointerEvent::up(24.0, 0.5)));
    assert!(
        !s.pump()
            .iter()
            .any(|e| matches!(e.effect, Effect::MoveRequested { .. }))
    );
}

#[test]
fn a_move_request_drains_before_focus_and_selection_news() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(6.0, 3.0));
        g.pointer(PointerEvent::up(6.0, 3.0));
        g.pointer(PointerEvent::down(7.0, 0.5));
        g.pointer(PointerEvent::moved(30.0, 0.5));
        g.pointer(PointerEvent::up(30.0, 0.5));
    });
    let drained = s.effects();
    assert!(
        matches!(
            drained[..],
            [Effect::MoveRequested { .. }, Effect::FocusChanged { .. }]
        ),
        "{drained:?}"
    );
}

/// Regression (found by the buffered-moves property): focus placed while a
/// dropped column's move is still unanswered is placed by Position, gets that
/// column's Key, and follows the Key when the source moves the column.
#[test]
fn focus_placed_before_a_move_is_answered_follows_its_column() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(14.0, 0.5));
        g.pointer(PointerEvent::moved(5.0, 0.5));
        g.pointer(PointerEvent::up(5.0, 0.5));
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: binnacle_core::FocusStep::Item,
            forward: true,
            extend: false,
        });
    });
    let mut out = s.step("drop column 1 first, then place focus before the pump", V);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: binnacle_core::FocusStep::Item,
            forward: true,
            extend: false,
        })
    });
    out += &s.step(
        "focus stayed on column 0's Key, now at Position 1; move right",
        V,
    );
    insta::assert_snapshot!(out);
}
