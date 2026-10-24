//! Seam 1 scenarios for scrolling: the Scroll Anchor, the three scroll Actions,
//! Size Overrides, content sizes and the Gap, seen through Snapshots.

use std::ops::Range;

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Edge, FocusStep, Grid, Key, Position, ScrollAnchor,
    Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{PumpSource, Scenario, ScenarioSource};

fn table() -> Scenario<binnacle_testkit::IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0),
    )
}

#[test]
fn scroll_moves_the_anchor_and_the_window_follows() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    let mut out = s.step("top", v);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 5.0 }));
    out += &s.step("down 5", v);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 0.5 }));
    out += &s.step("down half a row: a ninth row shows, so the Window grows", v);
    s.input(|g| g.action(Action::Scroll { dx: 3.0, dy: 0.0 }));
    out += &s.step("right 3", v);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 100.0 }));
    out += &s.step("far down: clamped so the last row ends the viewport", v);
    s.input(|g| {
        g.action(Action::Scroll {
            dx: -100.0,
            dy: -100.0,
        })
    });
    out += &s.step("far up and left: back at the origin", v);
    insta::assert_snapshot!(out);
}

#[test]
fn the_snapshot_reports_each_scroll_anchor() {
    let mut s = table();
    s.input(|g| g.action(Action::Scroll { dx: 7.5, dy: 2.25 }));
    let snap = s.frame(Viewport::new(22.0, 4.0));
    assert_eq!(
        snap.scroll_anchor(Axis::Rows),
        ScrollAnchor {
            position: 2,
            offset: 0.25
        }
    );
    assert_eq!(
        snap.scroll_anchor(Axis::Cols),
        ScrollAnchor {
            position: 1,
            offset: 1.5
        }
    );
}

#[test]
fn scroll_by_items_and_to_a_fraction() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    let mut out = String::new();
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: 4,
        })
    });
    out += &s.step("by 4 rows", v);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: -10,
        })
    });
    out += &s.step("back 10 rows: stops at the first", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        })
    });
    out += &s.step("to the middle", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    out += &s.step("to the end", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction: 2.0,
        })
    });
    out += &s.step("columns past the end: clamped", v);
    insta::assert_snapshot!(out);
}

#[test]
fn a_quadrillion_rows_scroll_exactly_at_the_start_middle_and_end() {
    let mut s = Scenario::identity(
        AxisConfig::new(1_000_000_000_000_000, 1.0).header(1.0),
        AxisConfig::new(2, 20.0).header(17.0),
    );
    let v = Viewport::new(57.0, 5.0);
    let mut out = s.step("start", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        })
    });
    out += &s.step("middle", v);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 1.0 }));
    out += &s.step("one row on", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    out += &s.step("end: the last row ends the viewport", v);
    s.input(|g| {
        g.action(Action::ScrollBy {
            axis: Axis::Rows,
            items: -1,
        })
    });
    out += &s.step("one row back", v);
    insta::assert_snapshot!(out);
}

#[test]
fn gaps_sit_between_items() {
    let mut s = Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    );
    let v = Viewport::new(22.0, 6.0);
    let mut out = s.step("Gap 1 on both axes", v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    out += &s.step("end: no trailing Gap after the last row", v);
    insta::assert_snapshot!(out);
}

#[test]
fn size_overrides_follow_keys_and_bound_the_window() {
    let mut s = Scenario::identity(
        AxisConfig::new(20, 2.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    );
    let v = Viewport::new(30.0, 7.0);
    let mut out = s.step("default sizes", v);
    s.input(|g| {
        g.set_size(Axis::Cols, Key(1), 3.0);
        g.set_size(Axis::Rows, Key(1), 1.0);
    });
    out += &s.step("column 1 narrower, row 1 shorter: the Window grows", v);
    s.input(|g| g.set_size(Axis::Rows, Key(1), 2.0));
    out += &s.step("row 1 back to 2: the bound remembers nothing smaller", v);
    insta::assert_snapshot!(out);
}

/// Answers row Windows with a content size per row (odd rows are 2 tall) and a
/// hint that no row is under 1.
struct TallOddRows(binnacle_testkit::IdentityTable);

impl PumpSource for TallOddRows {}

impl ScenarioSource for TallOddRows {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.0.window(axis, range)
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        (axis == Axis::Rows).then(|| {
            let sizes = keys.iter().map(|k| if k.0 % 2 == 1 { 2.0 } else { 1.0 });
            (sizes.collect(), 1.0)
        })
    }
}

#[test]
fn content_sizes_from_the_window_lay_out_below_size_overrides() {
    let table = binnacle_testkit::IdentityTable {
        rows: IdentitySource::new(20),
        cols: IdentitySource::new(2),
    };
    let grid = Grid::new(
        AxisConfig::new(20, 3.0).header(1.0),
        AxisConfig::new(2, 6.0).header(4.0),
    );
    let mut s = Scenario::new(grid, TallOddRows(table));
    let v = Viewport::new(16.0, 8.0);
    let mut out = s.step("first frame: bounded by the default, then content sizes", v);
    out += &s.step("next frame: bounded by the hint", v);
    s.input(|g| g.set_size(Axis::Rows, Key(3), 4.0));
    out += &s.step("row 3 overridden", v);
    insta::assert_snapshot!(out);
}

/// Scale probe: a quadrillion rows with thousands of Size Overrides, a long
/// resize drag and many scroll frames. Nothing here may cost per row or grow
/// faster than the overrides, so it finishes at once; a quadratic or per-row
/// path would hang it.
#[test]
fn a_quadrillion_rows_with_thousands_of_overrides_stay_cheap() {
    let n = 1_000_000_000_000_000u64;
    let mut s = Scenario::identity(
        AxisConfig::new(n, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let v = Viewport::new(80.0, 40.0);
    s.frame(v);
    let started = std::time::Instant::now();
    for i in 0..5_000u64 {
        let key = Key(i.wrapping_mul(0x9E37_79B9_7F4A_7C15) % n);
        s.input(|g| g.set_size(Axis::Rows, key, 1.0 + (i % 3) as f32));
    }
    for step in 0..2_000u64 {
        // A resize drag on a visible row, then a scroll.
        s.input(|g| g.set_size(Axis::Rows, Key(step % 5), 1.0 + (step % 7) as f32));
        let action = match step % 4 {
            0 => Action::Scroll { dx: 0.0, dy: 7.5 },
            1 => Action::ScrollBy {
                axis: Axis::Rows,
                items: 1_000_000_007,
            },
            2 => Action::ScrollTo {
                axis: Axis::Rows,
                fraction: (step as f64) / 2_000.0,
            },
            _ => Action::Scroll { dx: 0.0, dy: -3.0 },
        };
        s.input(|g| g.action(action));
        let snap = s.frame(v);
        assert!(!snap.rows().is_empty());
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
}

/// Found by Hegel: the anchor's own item can be larger than the smallest size,
/// so the Window bound must count it even when stepping by the smallest size
/// would skip it.
#[test]
fn an_anchor_item_larger_than_the_smallest_size_still_shows() {
    let mut s = Scenario::identity(AxisConfig::new(1, 1.0), AxisConfig::new(1, 1.0));
    s.input(|g| g.set_size(Axis::Rows, Key(0), 2.0));
    let v = Viewport::new(1.0, 1.0);
    s.frame(v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    let snap = s.frame(v);
    assert_eq!(
        snap.scroll_anchor(Axis::Rows),
        ScrollAnchor {
            position: 0,
            offset: 1.0
        }
    );
    let rows: Vec<_> = snap.rows().iter().map(|r| (r.position, r.offset)).collect();
    assert_eq!(rows, [(0, -1.0)]);
}

/// Found by Hegel: the clamp is found in f64 and stored as an f32 offset, which
/// must round toward the start or the view shows a sliver past the last item.
#[test]
fn scrolling_to_the_end_never_shows_space_past_the_last_item() {
    let mut s = Scenario::identity(
        AxisConfig::new(2, 0.03125).gap(1.0),
        AxisConfig::new(1, 1.0),
    );
    let v = Viewport::new(1.0, 0.06250003);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    let snap = s.frame(v);
    let last = snap.rows().last().unwrap();
    assert_eq!(last.position, 1);
    assert!(
        last.end() >= snap.extent(Axis::Rows),
        "{last:?} ends before the viewport"
    );
}

/// Found by Hegel: ScrollTo 1 clamps to an end found while the Window sat at
/// the start; once the Window moves to the end, the same end is summed along
/// another path and rounds one f32 step further. The view must stay at the
/// end and the thumb read exactly 1.
#[test]
fn the_end_stays_the_end_when_the_window_moves() {
    let mut s = Scenario::identity(
        AxisConfig::new(0, 1.0),
        AxisConfig::new(1016, 2.3984552e-13).gap(5.463464e-20),
    );
    let v = Viewport::new(2.3984553e-10, 0.0);
    s.frame(v);
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction: 1.0,
        })
    });
    s.frame(v);
    let snap = s.frame(v);
    assert_eq!(snap.thumb(Axis::Cols).fraction, 1.0);
    assert_eq!(snap.cols().last().unwrap().position, 1015);
}

/// Found by Hegel: at a header of 2^21 the f32 step is 0.25, so the 0.09375 of
/// the anchor's item left past the header cannot be drawn. The Strip starts at
/// the next item, after the Gap and that sliver.
#[test]
fn an_anchor_item_too_thin_to_draw_past_the_header_is_skipped() {
    let mut s = Scenario::identity(
        AxisConfig::new(999_999_999_999_000, 1.0)
            .header(2_097_152.0)
            .gap(1.65625),
        AxisConfig::new(0, 1.0),
    );
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 1.0,
        })
    });
    let snap = s.frame(Viewport::new(0.0, 2_097_176.0));
    assert_eq!(
        snap.scroll_anchor(Axis::Rows),
        ScrollAnchor {
            position: 999_999_999_998_990,
            offset: 0.90625
        }
    );
    let first = snap.rows()[0];
    assert_eq!(
        (first.position, first.offset),
        (999_999_999_998_991, 2_097_152.0 + 1.75)
    );
    let last = snap.rows().last().unwrap();
    assert_eq!(last.position, 999_999_999_998_999);
    assert!(last.end() >= snap.extent(Axis::Rows));
}

/// Found by Hegel: past a header of 2^16 only an undrawable sliver of the
/// anchor's item and its Gap fall inside a one-unit body, so the Strip is
/// empty.
#[test]
fn a_body_holding_only_a_sliver_and_a_gap_has_an_empty_strip() {
    let n = 999_999_999_999_000;
    let mut s = Scenario::identity(
        AxisConfig::new(0, 1.0),
        AxisConfig::new(n, 1.0).header(65_536.0).gap(0.996_093_75),
    );
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction: 1.0,
        });
        g.action(Action::Scroll { dx: -1.0, dy: 0.0 });
    });
    let snap = s.frame(Viewport::new(65_537.0, 0.0));
    assert_eq!(
        snap.scroll_anchor(Axis::Cols),
        ScrollAnchor {
            position: n - 2,
            offset: 0.996_093_75
        }
    );
    assert_eq!(snap.cols(), &[]);
}

/// Rows whose content is 12 tall (also the minimum-size hint) under a default
/// of 38.
struct ShortRows;

impl PumpSource for ShortRows {}

impl ScenarioSource for ShortRows {
    fn window(&mut self, _axis: Axis, range: Range<Position>) -> Vec<Key> {
        range.map(Key).collect()
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        (axis == Axis::Rows).then(|| (vec![12.0; keys.len()], 12.0))
    }
}

/// Found by Hegel (at 1e18 times the scale): the end clamp is first placed
/// with default sizes, and once the Window brings smaller content sizes it
/// needs more rows than that Window holds. The Window at the end reaches back
/// far enough for the smallest sizes, so once the minimum-size hint is known
/// the clamped view has every Key and size in one frame.
#[test]
fn the_end_clamp_stays_inside_the_window_when_content_sizes_are_smaller() {
    for height in [89.8, 140.0, 200.0, 260.0] {
        let grid = Grid::new(AxisConfig::new(20, 38.0).gap(13.8), AxisConfig::new(1, 5.0));
        let mut s = Scenario::new(grid, ShortRows);
        let v = Viewport::new(5.0, height);
        // The first frame brings the minimum-size hint, which bounds Windows.
        s.frame(v);
        s.input(|g| {
            g.action(Action::ScrollTo {
                axis: Axis::Rows,
                fraction: 1.0,
            })
        });
        let snap = s.frame(v);
        let anchor = snap.scroll_anchor(Axis::Rows);
        assert!(
            anchor.offset < 12.0 + 13.8,
            "height {height}: {anchor:?} past its item's stride"
        );
        for item in snap.rows() {
            assert_eq!(
                (item.key, item.size),
                (Some(Key(item.position)), 12.0),
                "height {height}"
            );
        }
        assert_eq!(snap.rows().last().unwrap().position, 19);
    }
}

/// Found by Hegel: past a 2048-unit header an f32 offset steps by 2^-12, so
/// each 0.0040097977 item advances the layout by only 0.00390625 (16 steps),
/// and a one-unit body holds 256 of them, not 249. The Window bound shares
/// that stepping with the Strip, so it asks for exactly those 256 plus
/// overscan: what is laid out, no more.
#[test]
fn the_window_holds_what_f32_layout_reaches_past_a_large_header() {
    let size = 0.004_009_797_7;
    let mut s = Scenario::identity(
        AxisConfig::new(999_999_999_999_000, size)
            .min_size(size)
            .max_size(size)
            .header(2048.0),
        AxisConfig::new(1, 1.0),
    );
    let requests = s.grid.begin_frame(Viewport::new(1.0, 2049.0));
    assert_eq!(requests.get(Axis::Rows), Some(0..258));
    let snap = s.frame(Viewport::new(1.0, 2049.0));
    let rows = snap.rows();
    assert_eq!(rows.len(), 256);
    assert_eq!(rows[1].offset - rows[0].offset, 0.003_906_25);
    assert!(rows.iter().all(|r| r.key == Some(Key(r.position))));
}

#[test]
fn revealing_a_position_scrolls_the_least_distance_and_leaves_focus() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    let at = |s: &mut Scenario<_>| s.frame(v).scroll_anchor(Axis::Rows);
    s.input(|g| g.set_focus(Key(0), Key(0)));
    s.frame(v);
    // A pending focus reveal does not take the scroll back.
    s.input(|g| {
        g.set_focus(Key(1), Key(0));
        g.reveal(Axis::Rows, 10);
    });
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 8,
            offset: 0.0
        },
        "below: the row ends the body"
    );
    assert_eq!(s.frame(v).focus(Axis::Rows).map(|f| f.position), Some(1));
    s.input(|g| g.reveal(Axis::Rows, 9));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 8,
            offset: 0.0
        },
        "in view: no scroll"
    );
    s.input(|g| g.reveal(Axis::Rows, 2));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 2,
            offset: 0.0
        },
        "above: the row starts the body"
    );
    s.input(|g| g.reveal(Axis::Rows, 99));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 2,
            offset: 0.0
        },
        "past the end: ignored"
    );
}

#[test]
fn scrolling_to_a_position_puts_it_at_the_start() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    let at = |s: &mut Scenario<_>| s.frame(v).scroll_anchor(Axis::Rows);
    s.input(|g| g.scroll_to_item(Axis::Rows, 10));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 10,
            offset: 0.0
        }
    );
    s.input(|g| g.scroll_to_item(Axis::Rows, 3));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 3,
            offset: 0.0
        },
        "back up"
    );
    s.input(|g| g.scroll_to_item(Axis::Rows, 19));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 17,
            offset: 0.0
        },
        "near the end: clamped so the last row ends the body"
    );
    s.input(|g| g.scroll_to_item(Axis::Rows, 99));
    assert_eq!(
        at(&mut s),
        ScrollAnchor {
            position: 17,
            offset: 0.0
        },
        "past the end: ignored"
    );
}

/// An End Gap leaves room past the last item when scrolled to the end, for
/// an adapter's closing line, on both axes: by a scroll, `ScrollTo`, a
/// reveal of the last item, or a focus move to it. Short of the end the
/// items still reach the viewport's end. A trailing pinned Strip ends the
/// End Gap short of the viewport's end, so the room stays past it.
#[test]
fn an_end_gap_leaves_room_past_the_last_item_at_the_end() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    s.input(|g| {
        g.set_end_gap(Axis::Rows, 1.0);
        g.set_end_gap(Axis::Cols, 1.0);
    });
    let mut out = s.step(
        "an End Gap of 1 on both axes: nothing moves at the start",
        v,
    );
    let snap = s.frame(v);
    assert_eq!(snap.rows().last().unwrap().end(), 4.0, "short of the end");
    s.input(|g| {
        g.action(Action::Scroll {
            dx: 100.0,
            dy: 100.0,
        })
    });
    out += &s.step("far down and right: the last row and column end 1 short", v);
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 3.0))
    );
    assert_eq!(
        snap.cols().last().map(|c| (c.position, c.end())),
        Some((4, 21.0))
    );
    assert_eq!(snap.thumb(Axis::Rows).fraction, 1.0);
    s.input(|g| {
        g.action(Action::Scroll {
            dx: -100.0,
            dy: -100.0,
        });
        g.reveal(Axis::Rows, 19);
    });
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 3.0)),
        "reveal"
    );
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.0,
        });
        g.set_focus(Key(0), Key(0));
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Edge,
            forward: true,
            extend: false,
        });
    });
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 3.0)),
        "focus to the end"
    );
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction: 1.0,
        })
    });
    let snap = s.frame(v);
    assert_eq!(
        snap.cols().last().map(|c| (c.position, c.end())),
        Some((4, 21.0)),
        "ScrollTo"
    );
    s.input(|g| g.pin(Axis::Rows, Edge::Trailing, Key(0)));
    out += &s.step(
        "row 0 pinned bottom: its Strip ends 1 short, and the last scrolling row ends where it starts",
        v,
    );
    let snap = s.frame(v);
    let pinned = snap.pinned(Axis::Rows, Edge::Trailing);
    assert_eq!(pinned.last().map(|r| r.end()), Some(3.0));
    assert_eq!(snap.pinned_region(Axis::Rows, Edge::Trailing), 2.0..3.0);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 2.0))
    );
    s.input(|g| {
        g.unpin(Axis::Rows, Key(0));
        g.set_end_gap(Axis::Rows, 0.0);
    });
    out += &s.step(
        "no End Gap on the rows: the last row ends the viewport again",
        v,
    );
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 4.0))
    );
    insta::assert_snapshot!(out);
}

/// A view at the end stays at the end when the End Gap changes, so a
/// closing line asked for there shows at once.
#[test]
fn an_end_gap_set_at_the_end_keeps_the_view_at_the_end() {
    let mut s = table();
    let v = Viewport::new(22.0, 4.0);
    s.input(|g| {
        g.action(Action::Scroll {
            dx: 100.0,
            dy: 100.0,
        })
    });
    s.frame(v);
    s.input(|g| {
        g.set_end_gap(Axis::Rows, 1.0);
        g.set_end_gap(Axis::Cols, 1.0);
    });
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 3.0))
    );
    assert_eq!(
        snap.cols().last().map(|c| (c.position, c.end())),
        Some((4, 21.0))
    );
    s.input(|g| g.set_end_gap(Axis::Rows, 0.0));
    let snap = s.frame(v);
    assert_eq!(
        snap.rows().last().map(|r| (r.position, r.end())),
        Some((19, 4.0))
    );
}

/// #222: a reorder empties the Window, so the end measured before the Window
/// returns uses default sizes. Here they would all fit, and an anchor at the
/// end must not be pulled back to the start by that estimate.
#[test]
fn an_anchor_at_the_end_stays_there_through_a_reorder() {
    let mut s = table();
    let v = Viewport::new(36.0, 4.0);
    s.input(|g| {
        g.set_size(Axis::Cols, Key(4), 12.0);
        g.action(Action::Scroll { dx: 100.0, dy: 0.0 });
    });
    let end = ScrollAnchor {
        position: 0,
        offset: 4.0,
    };
    assert_eq!(s.frame(v).scroll_anchor(Axis::Cols), end);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Reordered));
    assert_eq!(s.frame(v).scroll_anchor(Axis::Cols), end);
    s.input(|g| g.set_size(Axis::Cols, Key(4), 14.0));
    let snap = s.frame(v);
    assert_eq!(
        (
            snap.scroll_anchor(Axis::Cols),
            snap.cols().last().map(|c| c.end())
        ),
        (
            ScrollAnchor {
                position: 1,
                offset: 0.0
            },
            Some(36.0)
        ),
        "still following the end"
    );
}

/// A regression from #222: an anchor waiting at the end for the Window to
/// reach it still keeps its offset inside its own item once the Window
/// holds that item. Here a scroll to the end before any Window measures the
/// last item at the default size, and the Window then brings a size of 1.
#[test]
fn an_anchor_waiting_at_the_end_keeps_its_offset_inside_its_item() {
    let mut grid = Grid::new(AxisConfig::new(1_000_000, 64.0), AxisConfig::new(0, 1.0));
    grid.action(Action::ScrollTo {
        axis: Axis::Rows,
        fraction: 1.0,
    });
    let v = Viewport::new(5.0, 5.0);
    let requests: Vec<_> = grid.begin_frame(v).into_iter().collect();
    for (axis, range) in requests {
        let keys: Vec<Key> = range.clone().map(Key).collect();
        let sizes = vec![1.0; keys.len()];
        grid.set_window_sized(axis, range.start, &keys, &sizes, 1.0);
    }
    let anchor = grid.snapshot().scroll_anchor(Axis::Rows);
    assert!(anchor.offset < 1.0, "{anchor:?}");
}
