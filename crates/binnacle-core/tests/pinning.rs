//! Seam 1 scenarios for pinning: Keys pinned to an axis's leading edge are
//! drawn in their own Strip between the Header Band and the scrolling
//! Strip, which skips them.

use binnacle_core::{
    Action, Axis, AxisConfig, Edge, FocusStep, Grid, Key, Modifiers, PointerEvent, SortColumn,
    SortSpec, ViewSpec, Viewport,
};
use binnacle_testkit::{ColumnMajorTable, IdentityTable, KeyedTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// 4 rows of 1 under a 1-high header band; 10 columns of 6 with a Gap of 1
/// after a 4-wide row header band. Column p starts at x `4 + 7p`.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(10, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 32.0,
    height: 5.0,
};

#[test]
fn a_pinned_column_stays_at_the_left_while_the_body_scrolls() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(2)));
    out += &s.step("pin column 2: it leads, the body skips it", V);
    s.input(|g| g.action(Action::Scroll { dx: 10.0, dy: 0.0 }));
    out += &s.step("scroll right 10: only the body moves", V);
    s.input(|g| g.unpin(Axis::Cols, Key(2)));
    out += &s.step("unpin: back in its place, the anchor kept", V);
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(2));
        g.action(Action::Scroll { dx: 100.0, dy: 0.0 });
    });
    out += &s.step(
        "pin again and go far right: the scroll extent leaves it out",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn clicks_and_borders_land_on_the_pinned_and_the_scrolling_strips() {
    let mut s = table();
    s.input(|g| {
        g.set_border_grab(0.5);
        g.pin(Axis::Cols, Edge::Leading, Key(5));
    });
    s.frame(V);
    s.input(|g| g.action(Action::Scroll { dx: 10.0, dy: 0.0 }));
    let mut out = s.step("column 5 pinned, the body scrolled right 10", V);
    s.click(6.0, 2.5, Modifiers::NONE);
    out += &s.step("click the pinned column's cell: row 1, column 5", V);
    s.click(15.0, 3.5, Modifiers::NONE);
    out += &s.step("click the scrolling Strip: row 2, column 2", V);
    s.click(10.5, 2.5, Modifiers::NONE);
    out += &s.step("click the edge Gap: the pinned column's Gap, nothing", V);
    s.input(|g| g.pointer(PointerEvent::moved(10.5, 0.5)));
    out += &s.step("hover the pinned header's border: a resize cursor", V);
    s.input(|g| {
        g.pointer(PointerEvent::down(10.5, 0.5));
        g.pointer(PointerEvent::moved(12.5, 0.5));
    });
    out += &s.step(
        "drag it 2 right: the pinned column widens, the body follows",
        V,
    );
    s.input(|g| g.pointer(PointerEvent::up(12.5, 0.5)));
    out += &s.step("release: SizeChanged", V);
    insta::assert_snapshot!(out);
}

/// Six records as columns over three fields: field 0 holds
/// `[5, 13, 8, 31, 9, 23]` and sorts, field 1 the record's index times 10.
/// Fields of 1 under a 1-high header band, records of 4 with a Gap of 1
/// after a 4-wide row header band.
fn records() -> Scenario<ColumnMajorTable> {
    let field0 = [5, 13, 8, 31, 9, 23];
    let records = (0..6)
        .map(|i| vec![Some(field0[i]), Some(i as i64 * 10)])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(2, 1.0).header(1.0),
        AxisConfig::new(6, 4.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, ColumnMajorTable::new(records, 2, [0]))
}

const NARROW: Viewport = Viewport {
    width: 24.0,
    height: 3.0,
};

#[test]
fn a_pin_follows_its_key_through_a_sort_and_sleeps_while_filtered_out() {
    let mut s = records();
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(4)));
    let mut out = s.step("record 4 pinned", NARROW);
    s.input(|g| {
        g.request_view(ViewSpec::default().with_sort(
            Axis::Cols,
            SortSpec::new(vec![SortColumn::descending(Key(0))]),
        ))
    });
    out += &s.step(
        "the records sorted by field 0, descending: record 4 (9) is Position 3 now, still pinned",
        NARROW,
    );
    let notice = s.source.filter(|r| r[0].is_some_and(|v| v > 10));
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step(
        "filter to field 0 over 10: record 4 has gone, its pin sleeps",
        NARROW,
    );
    let notice = s.source.filter(|_| true);
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step(
        "the filter cleared: record 4 is back in its pinned Strip",
        NARROW,
    );
    insta::assert_snapshot!(out);
}

/// What a step of [`every_shown_item_is_drawn_exactly_once`] does.
#[derive(Clone, Debug)]
enum Op {
    Pin(u64, Edge),
    Unpin(u64),
    Scroll(f32),
    /// Any sort: the items in a new order, given as a permutation.
    Sort(Vec<usize>),
    Remove(u64),
    Resize(u64, f32),
    Length(f32),
    EdgeGap(f32),
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let key = || gs::integers::<u64>().max_value(30);
    let edge = || gs::sampled_from(vec![Edge::Leading, Edge::Trailing]);
    tc.draw(hegel::one_of!(
        hegel::tuples!(key(), edge()).map(|(k, e)| Op::Pin(k, e)),
        hegel::tuples!(key(), edge()).map(|(k, e)| Op::Pin(k, e)),
        key().map(Op::Unpin),
        gs::floats::<f32>()
            .min_value(-40.0)
            .max_value(40.0)
            .map(Op::Scroll),
        gs::permutations((0..30).collect::<Vec<usize>>()).map(Op::Sort),
        gs::integers::<u64>().max_value(30).map(Op::Remove),
        hegel::tuples!(key(), gs::floats::<f32>().min_value(1.0).max_value(12.0))
            .map(|(k, s)| Op::Resize(k, s)),
        gs::floats::<f32>()
            .min_value(4.0)
            .max_value(60.0)
            .map(Op::Length),
        gs::sampled_from(vec![0.0f32, 1.0, 2.5]).map(Op::EdgeGap),
    ))
}

/// Through pins on both edges, unpins, scrolls, sorts, removals, resizes
/// and Edge Gaps on either axis, each pinned Strip draws its pinned Keys in pin order at
/// their Positions (the trailing one ending at the viewport's end while
/// the pins fit), and the scrolling Strip draws every other item between
/// its ends once, in order.
#[hegel::test(test_cases = 500)]
fn every_shown_item_is_drawn_exactly_once(tc: TestCase) {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let count = tc.draw(gs::integers::<u64>().max_value(25));
    let gap = tc.draw(gs::sampled_from(vec![0.0f32, 1.0]));
    let size = tc.draw(gs::sampled_from(vec![1.0f32, 3.0, 6.0]));
    let header = tc.draw(gs::sampled_from(vec![0.0f32, 4.0]));
    let along = AxisConfig::new(count, size).header(header).gap(gap);
    let across = AxisConfig::new(3, 1.0).header(1.0);
    let (grid, table) = match axis {
        Axis::Rows => (Grid::new(along, across), KeyedTable::new(count, 3)),
        Axis::Cols => (Grid::new(across, along), KeyedTable::new(3, count)),
    };
    let mut s = Scenario::new(grid, table);
    let mut pins: [Vec<u64>; 2] = Default::default();
    let mut length = 30.0;
    for op in tc.draw(gs::vecs(op()).min_size(1).max_size(12)) {
        match op {
            Op::Pin(k, edge) => {
                pins.iter_mut().for_each(|p| p.retain(|&x| x != k));
                pins[edge.index()].push(k);
                s.input(|g| g.pin(axis, edge, Key(k)));
            }
            Op::Unpin(k) => {
                pins.iter_mut().for_each(|p| p.retain(|&x| x != k));
                s.input(|g| g.unpin(axis, Key(k)));
            }
            Op::Scroll(d) => s.input(|g| {
                let (dx, dy) = match axis {
                    Axis::Rows => (0.0, d),
                    Axis::Cols => (d, 0.0),
                };
                g.action(Action::Scroll { dx, dy })
            }),
            Op::Sort(order) => {
                let keys = &mut s.source.axes[axis.index()];
                let sorted: Vec<Key> = (order.iter())
                    .filter_map(|&i| keys.get(i).copied())
                    .collect();
                *keys = sorted;
                s.input(|g| g.notify(axis, binnacle_core::ChangeNotice::Reordered));
            }
            Op::Remove(at) => {
                let notice = s.source.remove(axis, at, 1);
                s.input(|g| g.notify(axis, notice));
            }
            Op::Resize(k, size) => s.input(|g| g.set_size(axis, Key(k), size)),
            Op::Length(l) => length = l,
            Op::EdgeGap(g) => s.input(|grid| grid.set_edge_gap(axis, g)),
        }
        // Answer every Locate a sort asked, as a host would by its frame.
        s.frame(Viewport::new(length, length));
        let keys = s.source.axes[axis.index()].clone();
        let snap = s.frame(Viewport::new(length, length));
        let position = |k: u64| keys.iter().position(|&x| x == Key(k)).map(|p| p as u64);
        let mut pinned_at = Vec::new();
        for edge in Edge::ALL {
            // Each pinned Strip draws each of its pinned Keys the table has,
            // in pin order, at its Position.
            let strip = snap.pinned(axis, edge);
            let drawn: Vec<(Option<Key>, u64)> =
                strip.iter().map(|i| (i.key, i.position)).collect();
            let expected: Vec<(Option<Key>, u64)> = (pins[edge.index()].iter())
                .filter_map(|&k| Some((Some(Key(k)), position(k)?)))
                .collect();
            assert_eq!(drawn, expected, "{edge:?} pinned Strip");
            pinned_at.extend(expected.iter().map(|&(_, p)| p));
            for pair in strip.windows(2) {
                // The trailing Strip is laid out back from the far end, so
                // its sums round the other way.
                let (at, want) = (pair[1].offset, pair[0].end() + gap);
                let slack = if edge == Edge::Trailing {
                    4.0 * f32::EPSILON * at.abs().max(1.0)
                } else {
                    0.0
                };
                assert!((at - want).abs() <= slack, "{edge:?} {strip:?}");
            }
            if edge == Edge::Trailing
                && let (Some(first), Some(last)) = (strip.first(), strip.last())
                && first.offset >= header
            {
                let slack = 4.0 * f32::EPSILON * length.max(1.0);
                assert!(
                    (last.end() - length).abs() <= slack,
                    "trailing Strip {strip:?}"
                );
            }
        }
        // The scrolling Strip draws the Positions between its ends that are
        // not pinned, each once, in order, side by side.
        let strip = snap.strip(axis);
        for item in strip {
            assert!(!pinned_at.contains(&item.position), "{item:?} is pinned");
            assert_eq!(item.key, Some(keys[item.position as usize]), "{item:?}");
        }
        for pair in strip.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            assert!(
                (a.position + 1..b.position).all(|p| pinned_at.contains(&p)),
                "{a:?} then {b:?} leaves out an item that is not pinned"
            );
            assert_eq!(b.offset, a.end() + gap, "{a:?} then {b:?}");
        }
        // It starts at the scrolling region and reaches its end, or the
        // last item that is not pinned.
        let region = snap.scrolling(axis);
        if let (Some(first), Some(last)) = (strip.first(), strip.last()) {
            assert!(first.offset <= region.start + gap && first.end() > region.start);
            let last_shown = (0..keys.len() as u64)
                .rev()
                .find(|p| !pinned_at.contains(p));
            assert!(
                last.end() + gap >= region.end || Some(last.position) == last_shown,
                "{last:?} stops short of {region:?}"
            );
        } else if region.end - region.start > gap {
            // A region no longer than the Gap may lie wholly in one.
            assert!(
                keys.len() == pinned_at.len(),
                "nothing scrolls in {region:?} though some items are not pinned"
            );
        }
    }
}

/// Columns 0 and 1 pinned leave a scrolling region one wide at 8..9; with
/// the body scrolled by 3, column 2 ends at 8 and column 3 starts at 9, so
/// the region shows only the Gap between them and no column is drawn there.
#[test]
fn a_scrolling_region_no_wider_than_the_gap_may_draw_no_item() {
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(3, 1.0).header(1.0),
            AxisConfig::new(9, 3.0).gap(1.0),
        ),
        KeyedTable::new(3, 9),
    );
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(0));
        g.pin(Axis::Cols, Edge::Leading, Key(1));
    });
    s.frame(Viewport::new(9.0, 4.0));
    s.input(|g| g.action(Action::Scroll { dx: 3.0, dy: 0.0 }));
    let snap = s.frame(Viewport::new(9.0, 4.0));
    assert_eq!(snap.pinned(Axis::Cols, Edge::Leading).len(), 2);
    assert_eq!(snap.scrolling(Axis::Cols), 8.0..9.0);
    assert!(snap.cols().is_empty(), "{:?}", snap.cols());
}

/// 12 rows of 1 under a 1-high header band, columns as in [`table`].
fn tall() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(12, 1.0).header(1.0),
        AxisConfig::new(10, 6.0).header(4.0).gap(1.0),
    )
}

const SHORT: Viewport = Viewport {
    width: 40.0,
    height: 7.0,
};

#[test]
fn trailing_pins_sit_flush_with_the_far_edge_in_pin_order() {
    let mut s = tall();
    let mut out = s.step("start", SHORT);
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Trailing, Key(1));
        g.pin(Axis::Cols, Edge::Trailing, Key(7));
    });
    out += &s.step(
        "pin columns 1 then 7 right: drawn in that order, 7 ending at the edge",
        SHORT,
    );
    s.input(|g| {
        g.pin(Axis::Rows, Edge::Leading, Key(5));
        g.pin(Axis::Rows, Edge::Trailing, Key(0));
    });
    out += &s.step("pin row 5 to the top and row 0 to the bottom", SHORT);
    s.input(|g| {
        g.action(Action::Scroll {
            dx: 100.0,
            dy: 100.0,
        })
    });
    out += &s.step(
        "scroll to the far corner: the last scrolling items end where the trailing Strips begin",
        SHORT,
    );
    s.input(|g| g.pin(Axis::Cols, Edge::Leading, Key(1)));
    out += &s.step("pin column 1 left: it leaves the trailing Strip", SHORT);
    insta::assert_snapshot!(out);
}

#[test]
fn a_trailing_pinned_column_is_clicked_and_resized_from_its_leading_border() {
    let mut s = tall();
    s.input(|g| {
        g.set_border_grab(0.5);
        g.pin(Axis::Cols, Edge::Trailing, Key(7));
    });
    let mut out = s.step("column 7 pinned right, from 34 to the edge at 40", SHORT);
    s.click(36.0, 2.5, Modifiers::NONE);
    out += &s.step("click its cell: row 1, column 7", SHORT);
    s.click(33.5, 2.5, Modifiers::NONE);
    out += &s.step("click the edge Gap before it: nothing", SHORT);
    s.input(|g| g.pointer(PointerEvent::moved(33.5, 0.5)));
    out += &s.step("hover the Gap before its header: a resize cursor", SHORT);
    s.input(|g| {
        g.pointer(PointerEvent::down(33.5, 0.5));
        g.pointer(PointerEvent::moved(31.5, 0.5));
    });
    out += &s.step(
        "drag it 2 left: column 7 widens to 8 from the edge, the guide at its start",
        SHORT,
    );
    s.input(|g| g.pointer(PointerEvent::up(31.5, 0.5)));
    out += &s.step("release: SizeChanged", SHORT);
    insta::assert_snapshot!(out);
}

/// A Grid over a [`KeyedTable`] of `count` items along `axis` (sizes 3 with a
/// Gap of 1 across, 1 down) and 4 along the other, both under header bands,
/// with `leading` and `trailing` pinned to `axis`'s edges, in that order.
fn pinned_keyed(axis: Axis, count: u64, leading: &[u64], trailing: &[u64]) -> Scenario<KeyedTable> {
    let (rows, cols) = match axis {
        Axis::Rows => (
            AxisConfig::new(count, 1.0),
            AxisConfig::new(4, 3.0).gap(1.0),
        ),
        Axis::Cols => (
            AxisConfig::new(4, 1.0),
            AxisConfig::new(count, 3.0).gap(1.0),
        ),
    };
    let grid = Grid::new(rows.header(1.0), cols.header(4.0));
    let (r, c) = match axis {
        Axis::Rows => (count, 4),
        Axis::Cols => (4, count),
    };
    let mut s = Scenario::new(grid, KeyedTable::new(r, c));
    s.input(|g| {
        for &k in leading {
            g.pin(axis, Edge::Leading, Key(k));
        }
        for &k in trailing {
            g.pin(axis, Edge::Trailing, Key(k));
        }
    });
    s
}

/// The Keys of `axis` in drawn order, from the model: the leading pins, the
/// rest in the table's order, then the trailing pins.
fn drawn_order(keys: &[Key], leading: &[u64], trailing: &[u64]) -> Vec<Key> {
    let pinned = |k: &Key| leading.contains(&k.0) || trailing.contains(&k.0);
    (leading.iter().map(|&k| Key(k)))
        .chain(keys.iter().copied().filter(|k| !pinned(k)))
        .chain(trailing.iter().map(|&k| Key(k)))
        .collect()
}

/// Up to `n` distinct Keys below `count`, none of them in `taken`.
fn some_keys(tc: &TestCase, count: u64, n: usize, taken: &[u64]) -> Vec<u64> {
    let keys = tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(count - 1))
            .max_size(n)
            .unique(true),
    );
    keys.into_iter().filter(|k| !taken.contains(k)).collect()
}

/// From the first drawn item, arrow keys step through every drawn item in
/// drawn order, on either axis, each shown as it is focused, and back
/// again.
#[hegel::test(test_cases = 300)]
fn arrow_keys_visit_every_drawn_item_in_drawn_order(tc: TestCase) {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(20));
    let leading = some_keys(&tc, count, 3, &[]);
    let trailing = some_keys(&tc, count, 3, &leading);
    let mut s = pinned_keyed(axis, count, &leading, &trailing);
    let viewport = Viewport::new(40.0, 10.0);
    s.frame(viewport);
    let expected = drawn_order(&s.source.axes[axis.index()], &leading, &trailing);
    let step = |forward| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward,
        extend: false,
    };
    let check = |s: &mut Scenario<KeyedTable>, i: usize| {
        let snap = s.frame(viewport);
        let focus = snap.focus(axis).expect("focus placed");
        assert_eq!(focus.key, Some(expected[i]), "item {i} of {expected:?}");
        assert!(snap.focus_rect().is_some(), "item {i} focused out of view");
    };
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis,
            by: FocusStep::Edge,
            forward: false,
            extend: false,
        })
    });
    check(&mut s, 0);
    for i in 1..expected.len() {
        s.input(|g| g.action(step(true)));
        check(&mut s, i);
    }
    s.input(|g| g.action(step(true)));
    check(&mut s, expected.len() - 1);
    for i in (0..expected.len() - 1).rev() {
        s.input(|g| g.action(step(false)));
        check(&mut s, i);
    }
}

/// Extending the range with arrow keys from any drawn item covers exactly
/// the items drawn between its corners, by the model's drawn order, on
/// either axis: no item drawn between them is left out, and none outside
/// them is in.
#[hegel::test(test_cases = 300)]
fn the_range_covers_what_is_drawn_between_its_corners(tc: TestCase) {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(20));
    let leading = some_keys(&tc, count, 3, &[]);
    let trailing = some_keys(&tc, count, 3, &leading);
    let mut s = pinned_keyed(axis, count, &leading, &trailing);
    let viewport = Viewport::new(40.0, 10.0);
    s.frame(viewport);
    let keys = s.source.axes[axis.index()].clone();
    let expected = drawn_order(&keys, &leading, &trailing);
    let last = expected.len() - 1;
    let start = tc.draw(gs::integers::<usize>().max_value(last));
    let steps = tc.draw(
        gs::integers::<i64>()
            .min_value(-(last as i64))
            .max_value(last as i64),
    );
    let (forward, by) = (steps > 0, steps.unsigned_abs());
    let step = |forward, extend| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward,
        extend,
    };
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis,
            by: FocusStep::Edge,
            forward: false,
            extend: false,
        });
        for _ in 0..start {
            g.action(step(true, false));
        }
        for _ in 0..by {
            g.action(step(forward, true));
        }
    });
    let head = (start as i64 + steps).clamp(0, last as i64) as usize;
    let snap = s.frame(viewport);
    assert_eq!(snap.focus(axis).and_then(|f| f.key), Some(expected[head]));
    let (lo, hi) = (start.min(head), start.max(head));
    let position = |k: Key| keys.iter().position(|&x| x == k).unwrap() as u64;
    match snap.selection_range() {
        None => assert_eq!(lo, hi, "no range from {start} to {head}"),
        Some(range) => {
            for (i, &k) in expected.iter().enumerate() {
                assert_eq!(
                    range.covers(axis, position(k)),
                    (lo..=hi).contains(&i),
                    "item {i} ({k:?}) of {expected:?}, range {range:?}"
                );
            }
        }
    }
}

#[test]
fn freeze_up_to_here_pins_from_the_start_and_is_capped_at_what_fits() {
    let mut s = tall();
    s.frame(SHORT);
    s.input(|g| g.pin(Axis::Cols, Edge::Trailing, Key(9)));
    let mut out = s.step("column 9 pinned right: 4 to 32 scrolls", SHORT);
    s.input(|g| g.freeze(Axis::Cols, Key(2)));
    out += &s.step("freeze up to column 2: columns 0 to 2 pinned left", SHORT);
    s.input(|g| g.freeze(Axis::Cols, Key(0)));
    out += &s.step(
        "freeze up to column 0, a pin: the pins after it are unpinned",
        SHORT,
    );
    s.input(|g| g.freeze(Axis::Cols, Key(9)));
    out += &s.step("freeze up to column 9, pinned right: nothing", SHORT);
    s.input(|g| g.freeze(Axis::Cols, Key(7)));
    out += &s.step(
        "freeze up to column 7: only 1 and 2 fit beside columns 0 and 9, leaving room to scroll",
        SHORT,
    );
    s.input(|g| g.freeze(Axis::Rows, Key(3)));
    out += &s.step("freeze up to row 3", SHORT);
    insta::assert_snapshot!(out);
}

#[test]
fn freeze_far_from_the_start_waits_for_the_keys_from_the_start() {
    let mut s = Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(100, 6.0).header(4.0).gap(1.0),
    );
    s.frame(V);
    s.input(|g| g.scroll_to_item(Axis::Cols, 50));
    let mut out = s.step("scrolled to column 50", V);
    s.input(|g| g.freeze(Axis::Cols, Key(51)));
    out += &s.step(
        "freeze up to column 51: columns 0 to 2 fit, fetched with a Window from the start",
        V,
    );
    insta::assert_snapshot!(out);
}

/// "Freeze up to here" on a drawn item pins, after the leading pins, every
/// item not pinned already from the start of the axis to it, in the
/// table's order, but no more than fit before the trailing Strip with room
/// left for an item to scroll; on a leading pin it unpins the pins after
/// it, on a trailing pin it does nothing. Wherever the axis was scrolled.
#[hegel::test(test_cases = 300)]
fn freeze_up_to_here_pins_from_the_start_as_far_as_fits(tc: TestCase) {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(60));
    let leading = some_keys(&tc, count, 2, &[]);
    let trailing = some_keys(&tc, count, 2, &leading);
    let mut s = pinned_keyed(axis, count, &leading, &trailing);
    let viewport = Viewport::new(40.0, 10.0);
    s.frame(viewport);
    let scroll = tc.draw(gs::integers::<u64>().max_value(count - 1));
    s.input(|g| g.scroll_to_item(axis, scroll));
    let snap = s.frame(viewport);
    let drawn: Vec<Key> = snap.drawn(axis).filter_map(|i| i.key).collect();
    let key = tc.draw(gs::sampled_from(drawn));
    s.input(|g| g.freeze(axis, key));
    s.frame(viewport);
    let keys = s.source.axes[axis.index()].clone();
    let snap = s.frame(viewport);
    // The model's geometry: items of `size` with a Gap of 1 across, 0 down.
    let (size, gap, header, extent) = match axis {
        Axis::Rows => (1.0f32, 0.0f32, 1.0f32, 10.0f32),
        Axis::Cols => (3.0, 1.0, 4.0, 40.0),
    };
    let expected: Vec<u64> = if let Some(i) = leading.iter().position(|&k| k == key.0) {
        leading[..=i].to_vec()
    } else if trailing.contains(&key.0) {
        leading.clone()
    } else {
        let stride = size + gap;
        let mut offset = header + stride * leading.len() as f32;
        let end = extent - stride * trailing.len() as f32;
        let mut frozen = leading.clone();
        for k in keys.iter().take_while(|&&k| k != key).chain([&key]) {
            if leading.contains(&k.0) || trailing.contains(&k.0) {
                continue;
            }
            if offset + stride + size > end {
                break;
            }
            offset += stride;
            frozen.push(k.0);
        }
        frozen
    };
    let pinned = |edge| -> Vec<u64> {
        (snap.pinned(axis, edge).iter())
            .filter_map(|i| i.key.map(|k| k.0))
            .collect()
    };
    assert_eq!(pinned(Edge::Leading), expected, "freeze up to {key:?}");
    assert_eq!(pinned(Edge::Trailing), trailing);
}

/// An Edge Gap wider than the axis's Gap spaces the pinned Strips from the
/// scrolling one only, as ratatui gives rows with no Gap a line for the
/// edge's `═` rule. Freeze leaves it room too.
#[test]
fn an_edge_gap_spaces_the_pinned_strips_from_the_scrolling_one() {
    let mut s = tall();
    let mut out = s.step("start", SHORT);
    s.input(|g| {
        g.set_edge_gap(Axis::Rows, 1.0);
        g.pin(Axis::Rows, Edge::Leading, Key(5));
        g.pin(Axis::Rows, Edge::Leading, Key(7));
        g.pin(Axis::Rows, Edge::Trailing, Key(0));
    });
    out += &s.step(
        "an Edge Gap of 1 on the rows, which have none, and rows 5 and 7 pinned top and 0 bottom: a line after the top Strip and before the bottom one, none between 5 and 7",
        SHORT,
    );
    s.input(|g| g.set_edge_gap(Axis::Cols, 0.5));
    out += &s.step(
        "an Edge Gap smaller than the columns' Gap changes nothing",
        SHORT,
    );
    s.input(|g| {
        g.unpin(Axis::Rows, Key(5));
        g.unpin(Axis::Rows, Key(7));
        g.unpin(Axis::Rows, Key(0));
        g.freeze(Axis::Rows, Key(5));
    });
    out += &s.step(
        "freeze up to row 5: rows 0 to 3 fit, leaving the Edge Gap and a row to scroll",
        SHORT,
    );
    insta::assert_snapshot!(out);
}
