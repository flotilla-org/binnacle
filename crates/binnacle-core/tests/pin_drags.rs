//! Seam 1 scenarios for dragging headers across the pinned edge: into a
//! pinned Strip pins, within one reorders the pins, out of one unpins (and
//! moves, where the axis can), on movable and unmovable axes alike; the lit
//! pin zones and the drop line's caption while dragging.

use binnacle_core::{
    Axis, AxisConfig, DropTarget, Edge, Effect, Key, Mode, PinZone, PointerEvent, Viewport,
};
use binnacle_testkit::{IdentityTable, Scenario};
use hegel::TestCase;
use hegel::generators as gs;

/// 4 rows of 1 under a 1-high header band; 10 columns of 6 with a Gap of 1
/// after a 4-wide row header band. Column p starts at x `4 + 7p`. Pin zones
/// are 3 wide. Nothing is movable until a test declares it.
fn table() -> Scenario<IdentityTable> {
    let mut s = Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(10, 6.0).header(4.0).gap(1.0),
    );
    s.input(|g| g.set_pin_zone(Axis::Cols, 3.0));
    s
}

const V: Viewport = Viewport {
    width: 39.0,
    height: 5.0,
};

/// The Keys pinned to `edge` of the columns, in drawn order.
fn pinned(s: &mut Scenario<IdentityTable>, edge: Edge) -> Vec<u64> {
    let snap = s.frame(V);
    snap.pinned(Axis::Cols, edge)
        .iter()
        .map(|i| i.key.unwrap().0)
        .collect()
}

/// Presses the header at `from`, drags it to `to` and holds it there.
fn drag_header(s: &mut Scenario<IdentityTable>, from: f32, to: f32) {
    s.frame(V);
    s.input(|g| {
        g.pointer(PointerEvent::down(from, 0.5));
        g.pointer(PointerEvent::moved(to, 0.5));
    });
}

/// The report that the columns' leading pins are now `keys`, in pin order.
fn leading_pins(keys: &[u64]) -> Effect {
    Effect::PinsChanged {
        axis: Axis::Cols,
        edge: Edge::Leading,
        keys: keys.iter().map(|&k| Key(k)).collect(),
    }
}

fn release(s: &mut Scenario<IdentityTable>, at: f32) {
    s.input(|g| g.pointer(PointerEvent::up(at, 0.5)));
}

#[test]
fn dropping_a_header_in_the_empty_leading_zone_pins_it_on_an_unmovable_axis() {
    let mut s = table();
    drag_header(&mut s, 20.0, 5.0);
    let snap = s.frame(V);
    assert!(matches!(snap.mode(), Mode::MovingHeaders { .. }));
    let drop = snap.drop_indicator().expect("a drop line in the zone");
    assert_eq!(
        drop.target,
        DropTarget::Pin {
            edge: Edge::Leading,
            before: None,
            reorder: false
        }
    );
    release(&mut s, 5.0);
    assert_eq!(s.effects(), [leading_pins(&[2])]);
    assert_eq!(pinned(&mut s, Edge::Leading), vec![2]);
}

#[test]
fn dropping_into_a_pinned_strip_pins_before_the_item_whose_first_half_it_is_over() {
    let mut s = table();
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(0));
        g.pin(Axis::Cols, Edge::Leading, Key(1));
    });
    // Pinned 0 at 4..10 and 1 at 11..17; column 4 scrolls at 32..38.
    drag_header(&mut s, 34.0, 13.0);
    let drop = s.frame(V).drop_indicator().unwrap();
    assert_eq!(
        drop.target,
        DropTarget::Pin {
            edge: Edge::Leading,
            before: Some(Key(1)),
            reorder: false
        }
    );
    assert_eq!(drop.offset, 10.5);
    release(&mut s, 13.0);
    assert_eq!(s.effects(), [leading_pins(&[0, 4, 1])]);
    assert_eq!(pinned(&mut s, Edge::Leading), vec![0, 4, 1]);
}

#[test]
fn dragging_a_pin_within_its_strip_reorders_the_pins() {
    let mut s = table();
    s.input(|g| {
        for k in [0, 1, 2] {
            g.pin(Axis::Cols, Edge::Leading, Key(k));
        }
    });
    drag_header(&mut s, 20.0, 5.0);
    let target = s.frame(V).drop_indicator().unwrap().target;
    assert_eq!(
        target,
        DropTarget::Pin {
            edge: Edge::Leading,
            before: Some(Key(0)),
            reorder: true
        }
    );
    release(&mut s, 5.0);
    assert_eq!(s.effects(), [leading_pins(&[2, 0, 1])]);
    assert_eq!(pinned(&mut s, Edge::Leading), vec![2, 0, 1]);
}

#[test]
fn dragging_a_pin_out_on_an_unmovable_axis_unpins_it_back_to_its_own_place() {
    let mut s = table();
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(3)));
    // Column 3 pinned at 4..10; the body starts at 11 with column 0.
    drag_header(&mut s, 6.0, 30.0);
    let drop = s.frame(V).drop_indicator().unwrap();
    assert_eq!(drop.target, DropTarget::Unpin { at: 3 });
    // Its own place: before column 4, after columns 0 to 2 (11 + 3 * 7).
    assert_eq!(drop.offset, 31.5);
    release(&mut s, 30.0);
    assert_eq!(s.effects(), [leading_pins(&[])]);
    assert_eq!(pinned(&mut s, Edge::Leading), Vec::<u64>::new());
}

#[test]
fn an_own_place_out_of_view_puts_the_line_at_the_scrolling_region_s_end() {
    let mut s = table();
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(9)));
    drag_header(&mut s, 6.0, 20.0);
    let drop = s.frame(V).drop_indicator().unwrap();
    assert_eq!(drop.target, DropTarget::Unpin { at: 9 });
    assert_eq!(drop.offset, 38.5);
}

#[test]
fn dragging_a_pin_out_on_a_movable_axis_unpins_it_and_requests_the_move() {
    let mut s = table();
    s.input(|g| {
        g.set_movable(Axis::Cols, true);
        g.pin(Axis::Cols, Edge::Leading, Key(0));
    });
    // The body starts at 11 with column 1; x 30 is in column 3's second half.
    drag_header(&mut s, 6.0, 30.0);
    let target = s.frame(V).drop_indicator().unwrap().target;
    assert_eq!(target, DropTarget::Move { to: 4, unpin: true });
    release(&mut s, 30.0);
    assert_eq!(
        s.effects(),
        vec![
            Effect::MoveRequested {
                axis: Axis::Cols,
                keys: vec![Key(0)],
                to: 4
            },
            leading_pins(&[])
        ]
    );
    assert_eq!(pinned(&mut s, Edge::Leading), Vec::<u64>::new());
}

#[test]
fn a_refused_move_after_an_unpin_leaves_the_item_unpinned_in_its_own_place() {
    let mut s = table();
    s.input(|g| {
        g.set_movable(Axis::Cols, true);
        g.pin(Axis::Cols, Edge::Leading, Key(0));
    });
    drag_header(&mut s, 6.0, 30.0);
    release(&mut s, 30.0);
    // The identity source refuses every move.
    s.pump();
    let snap = s.frame(V);
    assert!(snap.move_refusal().is_some());
    assert_eq!(snap.pending_move(), None);
    assert!(snap.pinned(Axis::Cols, Edge::Leading).is_empty());
}

#[test]
fn an_unpinned_header_over_the_body_of_an_unmovable_axis_drops_nothing() {
    let mut s = table();
    drag_header(&mut s, 20.0, 30.0);
    let snap = s.frame(V);
    assert!(matches!(
        snap.mode(),
        Mode::MovingHeaders {
            target: DropTarget::Stay,
            ..
        }
    ));
    assert_eq!(snap.drop_indicator(), None);
    release(&mut s, 30.0);
    assert_eq!(s.effects(), vec![]);
    assert!(s.frame(V).pinned(Axis::Cols, Edge::Leading).is_empty());
}

#[test]
fn dropping_in_the_empty_trailing_zone_pins_to_the_trailing_edge() {
    let mut s = table();
    drag_header(&mut s, 13.0, 37.0);
    let drop = s.frame(V).drop_indicator().unwrap();
    assert_eq!(
        drop.target,
        DropTarget::Pin {
            edge: Edge::Trailing,
            before: None,
            reorder: false
        }
    );
    release(&mut s, 37.0);
    assert_eq!(pinned(&mut s, Edge::Trailing), vec![1]);
}

#[test]
fn only_empty_pin_zones_light_up_and_only_while_dragging() {
    let mut s = table();
    assert_eq!(s.frame(V).pin_zones(), vec![]);
    drag_header(&mut s, 20.0, 5.0);
    let zone = |edge, offset, hot| PinZone {
        axis: Axis::Cols,
        edge,
        offset,
        size: 3.0,
        hot,
    };
    assert_eq!(
        s.frame(V).pin_zones(),
        vec![
            zone(Edge::Leading, 4.0, true),
            zone(Edge::Trailing, 36.0, false)
        ]
    );
    release(&mut s, 5.0);
    assert_eq!(s.frame(V).pin_zones(), vec![]);
    drag_header(&mut s, 20.0, 30.0);
    assert_eq!(
        s.frame(V).pin_zones(),
        vec![zone(Edge::Trailing, 36.0, false)]
    );
}

#[test]
fn row_headers_drag_into_and_out_of_the_top_strip() {
    let mut s = table();
    s.input(|g| g.set_pin_zone(Axis::Rows, 1.0));
    s.frame(V);
    // Row 2 is at y 3..4; the top zone is y 1..2.
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 3.5));
        g.pointer(PointerEvent::moved(1.0, 1.5));
    });
    let drop = s.frame(V).drop_indicator().unwrap();
    assert_eq!(drop.axis, Axis::Rows);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 1.5)));
    let snap = s.frame(V);
    let top: Vec<_> = snap
        .pinned(Axis::Rows, Edge::Leading)
        .iter()
        .map(|i| i.key)
        .collect();
    assert_eq!(top, vec![Some(Key(2))]);
    // Row 2 pinned at y 1..2; drag it back down into the body.
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 1.5));
        g.pointer(PointerEvent::moved(1.0, 3.5));
    });
    assert_eq!(
        s.frame(V).drop_indicator().unwrap().target,
        DropTarget::Unpin { at: 2 }
    );
    s.input(|g| g.pointer(PointerEvent::up(1.0, 3.5)));
    assert!(s.frame(V).pinned(Axis::Rows, Edge::Leading).is_empty());
}

#[test]
fn dragging_across_the_pinned_edge_walks_through() {
    let mut s = table();
    s.input(|g| g.set_movable(Axis::Cols, true));
    let mut out = s.step("start: nothing pinned, columns movable", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(20.0, 0.5));
        g.pointer(PointerEvent::moved(30.0, 0.5));
    });
    out += &s.step("drag column 2 over the body: both zones lit, a move", V);
    s.input(|g| g.pointer(PointerEvent::moved(5.0, 0.5)));
    out += &s.step("into the leading zone: it is hot, pin left", V);
    s.input(|g| g.pointer(PointerEvent::up(5.0, 0.5)));
    out += &s.step("release: column 2 pinned left", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(6.0, 0.5));
        g.pointer(PointerEvent::moved(30.0, 0.5));
    });
    out += &s.step("drag it out over the body: unpin and move", V);
    s.input(|g| g.pointer(PointerEvent::up(30.0, 0.5)));
    out += &s.step("release: unpinned, the move requested", V);
    insta::assert_snapshot!(out);
}

/// A column header dropped into the leading pinned Strip, empty or not, is
/// drawn just after the drop line it showed, which is at the item boundary
/// nearest the pointer: the line is in the Gap before where it lands, and
/// the other pins keep their order.
#[hegel::test(test_cases = 300)]
fn a_header_dropped_into_the_leading_strip_lands_just_after_its_drop_line(tc: TestCase) {
    let mut s = table();
    let pins: Vec<u64> = tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(9))
            .max_size(3)
            .unique(true),
    );
    s.input(|g| {
        for &k in &pins {
            g.pin(Axis::Cols, Edge::Leading, Key(k));
        }
    });
    let snap = s.frame(V);
    let end = if pins.is_empty() {
        7.0
    } else {
        snap.scrolling(Axis::Cols).start
    };
    // Headers whose middle is past the drag threshold from every drop point.
    let grabbable: Vec<(u64, f32)> = (snap.strip(Axis::Cols).iter())
        .map(|i| (i.key.unwrap().0, i.offset + i.size / 2.0))
        .filter(|&(_, middle)| middle >= end + 1.0 && middle < V.width)
        .collect();
    let (key, from) = tc.draw(gs::sampled_from(grabbable));
    // From the row header band's start: a drop over the band pins too.
    let to = tc.draw(gs::floats::<f32>().min_value(0.0).max_value_exclusive(end));
    drag_header(&mut s, from, to);
    let drop = s.frame(V).drop_indicator().expect("a drop line");
    let DropTarget::Pin {
        edge: Edge::Leading,
        before,
        reorder: false,
    } = drop.target
    else {
        panic!("{drop:?} pins nothing");
    };
    // The line is at the item boundary nearest the pointer: within half an
    // item (3) and half a Gap of it.
    assert!((to - drop.offset).abs() <= 3.5, "{drop:?} far from {to}");
    release(&mut s, to);
    let snap = s.frame(V);
    let strip = snap.pinned(Axis::Cols, Edge::Leading);
    let landed = strip.iter().find(|i| i.key == Some(Key(key))).unwrap();
    assert_eq!(landed.offset - 0.5, drop.offset);
    let mut expected = pins.clone();
    let at = before.map_or(pins.len(), |b| pins.iter().position(|&p| p == b.0).unwrap());
    expected.insert(at, key);
    let keys: Vec<u64> = strip.iter().map(|i| i.key.unwrap().0).collect();
    assert_eq!(keys, expected);
}

/// Any header dropped past the scrolling Strip's far end, into the trailing
/// pinned Strip, its empty pin zone or beyond the viewport, pins there in
/// the order its drop line showed, the line at the item boundary nearest the
/// pointer (or the viewport's end, for a pointer beyond it).
#[hegel::test(test_cases = 300)]
fn a_header_dropped_into_the_trailing_strip_pins_where_its_drop_line_showed(tc: TestCase) {
    let mut s = table();
    let pins: Vec<u64> = tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(9))
            .max_size(3)
            .unique(true),
    );
    s.input(|g| {
        for &k in &pins {
            g.pin(Axis::Cols, Edge::Trailing, Key(k));
        }
    });
    let snap = s.frame(V);
    let start = if pins.is_empty() {
        V.width - 3.0
    } else {
        snap.scrolling(Axis::Cols).end
    };
    let grabbable: Vec<(u64, f32)> = (snap.strip(Axis::Cols).iter())
        .map(|i| (i.key.unwrap().0, i.offset + i.size / 2.0))
        .filter(|&(_, middle)| middle <= start - 1.0 && middle >= 4.0)
        .collect();
    let (key, from) = tc.draw(gs::sampled_from(grabbable));
    let to = tc.draw(
        gs::floats::<f32>()
            .min_value(start)
            .max_value_exclusive(V.width + 10.0),
    );
    drag_header(&mut s, from, to);
    let drop = s.frame(V).drop_indicator().expect("a drop line");
    let DropTarget::Pin {
        edge: Edge::Trailing,
        before,
        reorder: false,
    } = drop.target
    else {
        panic!("{drop:?} pins nothing");
    };
    assert!(
        (to.min(V.width) - drop.offset).abs() <= 3.5,
        "{drop:?} far from {to}"
    );
    release(&mut s, to);
    let mut expected = pins.clone();
    let at = before.map_or(pins.len(), |b| pins.iter().position(|&p| p == b.0).unwrap());
    expected.insert(at, key);
    assert_eq!(pinned(&mut s, Edge::Trailing), expected);
}

/// #197: a Gap widened while a pinned header is dragged out over the
/// scrolling region can leave that region no room, its start rounded a
/// step past its end. The drop line still shows, between the pinned
/// Strips, rather than panic on bounds that cross.
#[test]
fn a_drop_line_shows_when_the_pins_leave_the_scrolling_region_no_room() {
    let mut s = Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(5, 3.0).gap(1.0),
    );
    let v = Viewport::new(11.136542, 5.0);
    s.input(|g| {
        g.set_pin_zone(Axis::Cols, 0.0);
        g.set_end_gap(Axis::Cols, 1.0);
        g.set_size(Axis::Cols, Key(1), 4.0903974);
        g.pin(Axis::Cols, Edge::Trailing, Key(0));
        g.pin(Axis::Cols, Edge::Leading, Key(1));
    });
    s.frame(v);
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, 0.5));
        g.pointer(PointerEvent::moved(5.5, 0.5));
    });
    s.frame(v);
    s.input(|g| g.set_gap(Axis::Cols, 2.0));
    let snap = s.frame(v);
    assert!(matches!(
        snap.mode(),
        Mode::MovingHeaders {
            target: DropTarget::Unpin { .. },
            ..
        }
    ));
    let drop = snap.drop_indicator().expect("a drop line while unpinning");
    // Between the leading Strip's end and the trailing Strip's start, in
    // the middle of their Gaps.
    assert!(
        (4.0903974 + 1.0..=8.090397 - 1.0).contains(&drop.offset),
        "{drop:?}"
    );
}
