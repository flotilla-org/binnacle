//! Seam 1 scenarios for scrolling: the Scroll Anchor, the three scroll Actions,
//! Size Overrides, content sizes and the Gap, seen through Snapshots.

use std::ops::Range;

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Position, ScrollAnchor, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{Scenario, ScenarioSource};

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
