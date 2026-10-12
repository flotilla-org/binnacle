//! Seam 1 scenarios for column moves: dragging a column header past the
//! threshold, the drop indicator, the move cursor, `MoveRequested` and
//! `Cancel`.

use binnacle_core::{Action, Axis, AxisConfig, Cursor, Effect, Key, Mode, PointerEvent, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

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

fn effects(s: &mut Scenario<IdentityTable>) -> Vec<Effect> {
    s.pump().into_iter().map(|e| e.effect).collect()
}

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
    assert_eq!(effects(&mut s), vec![]);
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
        assert_eq!(effects(&mut s), vec![]);
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
        effects(&mut s),
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
    assert!(matches!(effects(&mut s)[..], [Effect::SizeChanged { .. }]));
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
    assert_eq!(effects(&mut s), vec![]);
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
        effects(&mut s),
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
        effects(&mut s),
        vec![Effect::MoveRequested {
            axis: Axis::Cols,
            keys: vec![Key(0)],
            to: 5
        }]
    );
}
