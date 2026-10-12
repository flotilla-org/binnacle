//! Seam 1 scenarios for resizing: dragging a header border, the guide line,
//! the cursor hint, `Cancel`, `SizeChanged` and border double-click
//! auto-size.

use binnacle_core::{Action, Axis, AxisConfig, Cursor, Effect, Key, Mode, PointerEvent, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

/// 20 rows of 1 with a Gap of 1 under a 1-high header band; 5 columns of 6
/// with a Gap of 1 after a 4-wide row header band. Column 0 spans x 4..10,
/// its border (the Gap) is x 10..11; row 0 spans y 1..2, its border y 2..3.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 30.0,
    height: 6.0,
};

fn effects(s: &mut Scenario<IdentityTable>) -> Vec<Effect> {
    s.pump().into_iter().map(|e| e.effect).collect()
}

#[test]
fn dragging_a_column_border_resizes_the_column_and_reports_the_size_on_release() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| g.pointer(PointerEvent::moved(10.5, 0.5)));
    out += &s.step("hover the Gap after column 0's header: resize cursor", V);
    s.input(|g| g.pointer(PointerEvent::down(10.5, 0.5)));
    out += &s.step("press on it: resizing, capturing", V);
    s.input(|g| g.pointer(PointerEvent::moved(13.5, 0.5)));
    out += &s.step("move 3 right: column 0 is 9 wide, the guide follows", V);
    s.input(|g| g.pointer(PointerEvent::moved(12.5, 5.0)));
    out += &s.step("move back 1 and down into the body: 8 wide", V);
    s.input(|g| g.pointer(PointerEvent::up(12.5, 5.0)));
    out += &s.step("release: SizeChanged, idle", V);
    insta::assert_snapshot!(out);
}

#[test]
fn row_borders_resize_rows() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 2.5));
        g.pointer(PointerEvent::moved(1.0, 4.5));
    });
    let mut out = s.step("drag row 0's border down 2: row 0 is 3 high", V);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 4.5)));
    out += &s.step("release", V);
    insta::assert_snapshot!(out);
}

#[test]
fn cancel_restores_the_start_size_and_reports_nothing() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(15.5, 0.5));
    });
    let snap = s.frame(V);
    assert_eq!(
        snap.mode(),
        Mode::Resizing {
            axis: Axis::Cols,
            key: Key(0),
            start_size: 6.0
        }
    );
    assert_eq!(snap.cols()[0].size, 11.0);
    s.input(|g| {
        g.action(Action::Cancel);
        g.pointer(PointerEvent::moved(20.5, 0.5));
        g.pointer(PointerEvent::up(20.5, 0.5));
    });
    assert_eq!(effects(&mut s), vec![]);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(0)), None);
    let snap = s.frame(V);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
    assert_eq!(snap.cols()[0].size, 6.0);
}

#[test]
fn a_resize_that_ends_where_it_started_reports_nothing() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(14.5, 0.5));
        g.pointer(PointerEvent::moved(10.5, 0.5));
        g.pointer(PointerEvent::up(10.5, 0.5));
    });
    assert_eq!(effects(&mut s), vec![]);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(0)), None);
}

#[test]
fn a_drag_never_shrinks_an_item_below_one_unit() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(-40.0, 0.5));
        g.pointer(PointerEvent::up(-40.0, 0.5));
    });
    assert_eq!(
        effects(&mut s),
        vec![Effect::SizeChanged {
            axis: Axis::Cols,
            key: Key(0),
            size: 1.0
        }]
    );
    assert_eq!(s.frame(V).cols()[0].size, 1.0);
}

#[test]
fn set_size_restores_a_saved_size() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
        g.pointer(PointerEvent::up(12.5, 0.5));
    });
    let saved = effects(&mut s);
    let [Effect::SizeChanged { axis, key, size }] = saved[..] else {
        panic!("{saved:?}");
    };
    let mut restored = table();
    restored.input(|g| g.set_size(axis, key, size));
    let snap = restored.frame(V);
    assert_eq!(snap.cols()[0].size, 8.0);
}

#[test]
fn double_clicking_a_border_requests_auto_size() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(17.5, 0.5));
        g.pointer(PointerEvent::up(17.5, 0.5));
        g.pointer(PointerEvent::down(17.5, 0.5).clicks(2));
    });
    assert_eq!(
        effects(&mut s),
        vec![Effect::AutoSizeRequested {
            axis: Axis::Cols,
            key: Key(1)
        }]
    );
    let snap = s.frame(V);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
}

#[test]
fn the_cursor_hint_shows_on_borders_only() {
    let mut s = table();
    s.frame(V);
    let cursor_at = |s: &mut Scenario<IdentityTable>, x: f32, y: f32| {
        s.input(|g| g.pointer(PointerEvent::moved(x, y)));
        s.frame(V).cursor()
    };
    assert_eq!(cursor_at(&mut s, 10.5, 0.5), Cursor::ResizeCol);
    assert_eq!(cursor_at(&mut s, 9.5, 0.5), Cursor::Default);
    assert_eq!(cursor_at(&mut s, 1.0, 2.5), Cursor::ResizeRow);
    assert_eq!(cursor_at(&mut s, 1.0, 1.5), Cursor::Default);
    // Gaps in the body are gridlines between cells, not borders.
    assert_eq!(cursor_at(&mut s, 10.5, 1.5), Cursor::Default);
    assert_eq!(cursor_at(&mut s, 5.0, 2.5), Cursor::Default);
}

#[test]
fn with_no_gap_the_border_grab_widens_the_border_into_the_items() {
    let mut s = Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0),
    );
    s.frame(V);
    let cursor_at = |s: &mut Scenario<IdentityTable>, x: f32| {
        s.input(|g| g.pointer(PointerEvent::moved(x, 0.5)));
        s.frame(V).cursor()
    };
    assert_eq!(cursor_at(&mut s, 9.9), Cursor::Default);
    s.input(|g| g.set_border_grab(0.5));
    assert_eq!(cursor_at(&mut s, 9.6), Cursor::ResizeCol);
    assert_eq!(cursor_at(&mut s, 10.4), Cursor::ResizeCol);
    assert_eq!(cursor_at(&mut s, 9.4), Cursor::Default);
    assert_eq!(cursor_at(&mut s, 10.6), Cursor::Default);
    // The leading edge of the first column is no border.
    assert_eq!(cursor_at(&mut s, 4.2), Cursor::Default);
    // Column 4, the last, ends at x 34: its grab reaches past its end.
    let wide = Viewport { width: 40.0, ..V };
    s.input(|g| g.pointer(PointerEvent::moved(34.4, 0.5)));
    assert_eq!(s.frame(wide).cursor(), Cursor::ResizeCol);
    s.input(|g| g.pointer(PointerEvent::moved(34.6, 0.5)));
    assert_eq!(s.frame(wide).cursor(), Cursor::Default);
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.4, 0.5));
        g.pointer(PointerEvent::moved(12.4, 0.5));
        g.pointer(PointerEvent::up(12.4, 0.5));
    });
    let snap = s.frame(V);
    assert_eq!(snap.cols()[0].size, 8.0);
}

#[test]
fn a_border_press_is_not_a_header_click() {
    let mut s = table();
    s.input(|g| g.set_header_click(Axis::Cols, binnacle_core::HeaderClick::Select));
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::up(10.5, 0.5));
    });
    let snap = s.frame(V);
    assert_eq!(snap.selection_range(), None);
    assert_eq!(snap.focus(Axis::Rows), None);
}

#[test]
fn dragging_back_to_a_restored_size_reports_nothing_and_keeps_it() {
    let mut s = table();
    s.input(|g| g.set_size(Axis::Cols, Key(0), 8.0));
    s.frame(V);
    // Column 0 now spans x 4..12; its border is x 12..13.
    s.input(|g| {
        g.pointer(PointerEvent::down(12.5, 0.5));
        g.pointer(PointerEvent::moved(15.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
        g.pointer(PointerEvent::up(12.5, 0.5));
    });
    assert_eq!(effects(&mut s), vec![]);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(0)), Some(8.0));
    s.input(|g| {
        g.pointer(PointerEvent::down(12.5, 0.5));
        g.pointer(PointerEvent::moved(15.5, 0.5));
        g.action(Action::Cancel);
    });
    assert_eq!(effects(&mut s), vec![]);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(0)), Some(8.0));
}

#[test]
fn a_press_while_resizing_ends_the_resize_where_it_is() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
        // The release was lost; the next press arrives, on column 2 now
        // that column 0 is 8 wide.
        g.pointer(PointerEvent::down(20.0, 3.0));
    });
    assert_eq!(
        effects(&mut s),
        vec![
            Effect::SizeChanged {
                axis: Axis::Cols,
                key: Key(0),
                size: 8.0
            },
            Effect::FocusChanged {
                row: Key(1),
                col: Key(2)
            }
        ]
    );
}

/// Scale probe for a resize drag (the legacy measure rebuilt every item's
/// offset on each pointer move): on a quadrillion rows with thousands of Size
/// Overrides, a drag of thousands of moves, a frame each, finishes at once.
#[test]
fn a_long_resize_drag_on_a_quadrillion_rows_with_thousands_of_overrides_stays_cheap() {
    let n = 1_000_000_000_000_000u64;
    let mut s = Scenario::identity(
        AxisConfig::new(n, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    );
    let v = Viewport::new(30.0, 40.0);
    s.frame(v);
    for i in 0..5_000u64 {
        let key = Key(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) % n);
        s.input(|g| g.set_size(Axis::Rows, key, 1.0 + (i % 3) as f32));
    }
    s.frame(v);
    let started = std::time::Instant::now();
    // Row 0 spans y 1..2 and its border y 2..3.
    s.input(|g| g.pointer(PointerEvent::down(1.0, 2.5)));
    for step in 0..5_000u32 {
        let y = 2.5 + (step % 30) as f32;
        s.input(|g| g.pointer(PointerEvent::moved(1.0, y)));
        assert!(matches!(s.frame(v).mode(), Mode::Resizing { .. }));
    }
    // The last move is 19 down: row 0 is 20 high.
    s.input(|g| g.pointer(PointerEvent::up(1.0, 21.5)));
    assert_eq!(s.frame(v).rows()[0].size, 20.0);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
}
