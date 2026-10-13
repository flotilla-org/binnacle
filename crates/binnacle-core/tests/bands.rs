//! Seam 1 scenarios for Header Bands: dragging a band edge, the double-click
//! auto-size request, `Cancel`, a 0-thickness band, clamping, and the host's
//! `set_header` and `set_gap`.

use binnacle_core::{Action, Axis, AxisConfig, Cursor, Effect, Mode, PointerEvent, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

/// 20 rows of 1 with a Gap of 1 under a 1-high column header band; 5 columns
/// of 6 with a Gap of 1 after a 4-wide row-number band. The row-number band's
/// edge is its last Gap's width, x 3..4, in the column header band (y 0..1).
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

#[test]
fn dragging_the_row_number_band_edge_resizes_the_band_and_reports_it_on_release() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| g.pointer(PointerEvent::moved(3.5, 0.5)));
    out += &s.step("hover the row-number band's edge: resize cursor", V);
    s.input(|g| g.pointer(PointerEvent::down(3.5, 0.5)));
    out += &s.step("press on it: resizing the band, capturing", V);
    s.input(|g| g.pointer(PointerEvent::moved(6.5, 3.0)));
    out += &s.step("move 3 right: the band is 7 wide, the guide follows", V);
    s.input(|g| g.pointer(PointerEvent::up(6.5, 3.0)));
    out += &s.step("release: BandSizeChanged, idle", V);
    insta::assert_snapshot!(out);
    assert_eq!(s.frame(V).header(Axis::Cols), 7.0);
}

#[test]
fn dragging_the_column_header_band_edge_resizes_it() {
    let mut s = table();
    s.frame(V);
    // The rows' Gap is 1, so the whole 1-high band is its edge in the corner.
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 0.5));
        g.pointer(PointerEvent::moved(1.0, 2.5));
    });
    let mut out = s.step("drag the column header band's edge down 2: 3 high", V);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 2.5)));
    out += &s.step("release", V);
    insta::assert_snapshot!(out);
}

#[test]
fn double_clicking_a_band_edge_requests_band_auto_size() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::up(3.5, 0.5));
        g.pointer(PointerEvent::down(3.5, 0.5).clicks(2));
        g.pointer(PointerEvent::up(3.5, 0.5));
    });
    let mut out = s.step("double-click the row-number band's edge", V);
    s.input(|g| g.set_header(Axis::Cols, 9.0));
    out += &s.step(
        "the host measures the band and answers with set_header(9)",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn cancel_restores_the_starting_thickness_and_reports_nothing() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::moved(8.5, 0.5));
    });
    let mut out = s.step("drag the row-number band's edge 5 right", V);
    s.input(|g| {
        g.action(Action::Cancel);
        g.pointer(PointerEvent::moved(12.5, 0.5));
        g.pointer(PointerEvent::up(12.5, 0.5));
    });
    out += &s.step(
        "Cancel, then move on and release: back to 4, nothing reported",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_drag_clamps_the_band_into_the_axis_size_range() {
    let mut s = Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0).max_size(10.0),
    );
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::moved(-20.0, 0.5));
    });
    let mut out = s.step("drag far left: the band stops at min_size 1", V);
    s.input(|g| g.pointer(PointerEvent::moved(25.0, 0.5)));
    out += &s.step("drag far right: the band stops at max_size 10", V);
    s.input(|g| g.pointer(PointerEvent::up(25.0, 0.5)));
    out += &s.step("release", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_band_of_thickness_zero_has_no_edge() {
    let mut s = table();
    s.input(|g| g.set_header(Axis::Cols, 0.0));
    let mut out = s.step("set_header(Cols, 0): no row-number band", V);
    s.input(|g| g.pointer(PointerEvent::moved(0.0, 0.5)));
    out += &s.step(
        "hover where its edge was: column 0's header, no resize cursor",
        V,
    );
    s.input(|g| {
        g.pointer(PointerEvent::down(0.0, 0.5));
        g.pointer(PointerEvent::moved(5.0, 0.5));
    });
    out += &s.step("drag from there: a header move, never a band resize", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_band_edge_wins_over_a_header_press_near_it() {
    let mut s = table();
    s.input(|g| g.set_border_grab(1.0));
    s.frame(V);
    // The grab widens the edge to x 2..5: into column 0's header at x 4..5.
    s.input(|g| g.pointer(PointerEvent::moved(4.5, 0.5)));
    assert_eq!(s.frame(V).cursor(), Cursor::ResizeCol);
    s.input(|g| g.pointer(PointerEvent::moved(5.5, 0.5)));
    assert_eq!(s.frame(V).cursor(), Cursor::Default);
    s.input(|g| {
        g.pointer(PointerEvent::down(4.5, 0.5));
        g.pointer(PointerEvent::up(4.5, 0.5));
    });
    // No sort click: the press grabbed the band.
    assert_eq!(s.effects(), vec![]);
    s.input(|g| g.pointer(PointerEvent::down(4.5, 0.5)));
    assert_eq!(
        s.frame(V).mode(),
        Mode::ResizingBand {
            axis: Axis::Cols,
            start_thickness: 4.0
        }
    );
}

#[test]
fn a_band_edge_is_grabbed_only_where_the_bands_meet() {
    let mut s = table();
    s.frame(V);
    let cursor_at = |s: &mut Scenario<IdentityTable>, x: f32, y: f32| {
        s.input(|g| g.pointer(PointerEvent::moved(x, y)));
        s.frame(V).cursor()
    };
    assert_eq!(cursor_at(&mut s, 3.5, 0.5), Cursor::ResizeCol);
    // Down the row headers, the band's last cell is a row header.
    assert_eq!(cursor_at(&mut s, 3.5, 1.5), Cursor::Default);
    assert_eq!(cursor_at(&mut s, 2.5, 0.5), Cursor::ResizeRow);
    // With no Gap and no grab, neither band has an edge.
    s.input(|g| {
        g.set_gap(Axis::Rows, 0.0);
        g.set_gap(Axis::Cols, 0.0);
    });
    assert_eq!(cursor_at(&mut s, 3.5, 0.5), Cursor::Default);
    assert_eq!(cursor_at(&mut s, 2.5, 0.5), Cursor::Default);
}

#[test]
fn set_header_restores_a_saved_thickness_and_allows_zero() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::moved(5.5, 0.5));
        g.pointer(PointerEvent::up(5.5, 0.5));
    });
    let saved = s.effects();
    let [Effect::BandSizeChanged { axis, thickness }] = saved[..] else {
        panic!("{saved:?}");
    };
    let mut restored = table();
    restored.input(|g| g.set_header(axis, thickness));
    let snap = restored.frame(V);
    assert_eq!(snap.header(Axis::Cols), 6.0);
    assert!(snap.damage().layout);
    assert_eq!(snap.cols()[0].offset, 6.0);
    restored.input(|g| g.set_header(Axis::Rows, 0.0));
    let snap = restored.frame(V);
    assert!(snap.damage().layout);
    assert_eq!((snap.header(Axis::Rows), snap.rows()[0].offset), (0.0, 0.0));
    // Thicknesses that are not finite and non-negative are ignored.
    restored.input(|g| {
        g.set_header(Axis::Rows, -1.0);
        g.set_header(Axis::Rows, f32::NAN);
    });
    let snap = restored.frame(V);
    assert!(!snap.damage().layout);
    assert_eq!(snap.header(Axis::Rows), 0.0);
}

#[test]
fn set_gap_keeps_the_scroll_anchors_item_in_place() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 4.5 }));
    let mut out = s.step("scroll down 4.5: row 2 is the anchor, half scrolled", V);
    s.input(|g| g.set_gap(Axis::Rows, 0.0));
    out += &s.step(
        "set_gap(Rows, 0): row 2 stays put, rows close up after it",
        V,
    );
    s.input(|g| g.set_gap(Axis::Rows, 2.0));
    out += &s.step("set_gap(Rows, 2): still in place, rows spread out", V);
    insta::assert_snapshot!(out);
}

#[test]
fn set_header_during_a_band_drag_rebases_it_and_cancel_keeps_the_hosts_thickness() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(3.5, 0.5));
        g.pointer(PointerEvent::moved(5.5, 0.5));
        g.set_header(Axis::Cols, 9.0);
        g.pointer(PointerEvent::moved(6.5, 0.5));
    });
    let snap = s.frame(V);
    assert_eq!(
        snap.mode(),
        Mode::ResizingBand {
            axis: Axis::Cols,
            start_thickness: 9.0
        }
    );
    assert_eq!(snap.header(Axis::Cols), 10.0);
    s.input(|g| g.action(Action::Cancel));
    assert_eq!(s.frame(V).header(Axis::Cols), 9.0);
    assert_eq!(s.effects(), vec![]);
}

/// Found by `set_gap_keeps_the_scroll_anchors_item_at_its_offset`: dropping
/// the Gap makes the whole axis fit the body, so the end clamp scrolls back to
/// the start, past a header so thick that f32 placement drops the Strip.
#[test]
fn set_gap_that_lets_the_whole_axis_fit_scrolls_back_to_the_start() {
    let rows = AxisConfig::new(2, 0.5)
        .min_size(0.5)
        .header(8_388_608.0)
        .gap(0.5);
    let mut s = Scenario::identity(rows, AxisConfig::new(1, 1.0));
    let v = Viewport::new(4.0, 8_388_609.0);
    s.frame(v);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 0.25 }));
    let anchor = s.frame(v).scroll_anchor(Axis::Rows);
    assert_eq!((anchor.position, anchor.offset), (0, 0.25));
    s.input(|g| g.set_gap(Axis::Rows, 0.0));
    let snap = s.frame(v);
    let anchor = snap.scroll_anchor(Axis::Rows);
    assert_eq!((anchor.position, anchor.offset), (0, 0.0));
    assert!(snap.thumb(Axis::Rows).size >= 1.0);
}
