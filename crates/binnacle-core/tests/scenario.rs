//! Seam 1 scenarios: a real Grid driven through the frame loop, each Snapshot
//! rendered as text and compared with a stored snapshot.

use std::ops::Range;

use binnacle_core::{Action, Axis, AxisConfig, Effect, Grid, Key, Position, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{IdentityTable, Scenario, ScenarioSource};

#[test]
fn identity_table_first_frame() {
    let mut s = Scenario::identity(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0),
    );
    insta::assert_snapshot!(s.step("first frame", Viewport::new(22.0, 4.0)));
}

#[test]
fn steady_frames_reuse_the_window_and_report_no_damage() {
    let mut s = Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0),
    );
    let mut out = String::new();
    out += &s.step("first frame", Viewport::new(22.0, 4.0));
    out += &s.step("same viewport", Viewport::new(22.0, 4.0));
    out += &s.step("taller: Window grows", Viewport::new(22.0, 8.0));
    out += &s.step(
        "shorter again: Window still covers",
        Viewport::new(22.0, 4.0),
    );
    insta::assert_snapshot!(out);
}

#[test]
fn effects_drain_in_order_stamped_with_the_generation_they_were_based_on() {
    let mut s = Scenario::identity(AxisConfig::new(2, 1.0), AxisConfig::new(2, 4.0));
    let viewport = Viewport::new(8.0, 2.0);
    let typed = |t: &str| Effect::TextTyped { text: t.into() };
    let drain = |s: &mut Scenario<_>| -> Vec<_> {
        s.pump()
            .into_iter()
            .map(|e| (e.generation, e.effect))
            .collect()
    };

    s.frame(viewport);
    s.input(|g| g.action(Action::TypeText("a".into())));
    s.input(|g| g.action(Action::TypeText("b".into())));
    assert_eq!(drain(&mut s), vec![(1, typed("a")), (1, typed("b"))]);
    assert!(s.grid.poll_effect().is_none());

    s.frame(viewport);
    s.input(|g| g.action(Action::TypeText("c".into())));
    assert_eq!(drain(&mut s), vec![(2, typed("c"))]);
}

#[test]
fn empty_tables_give_an_empty_snapshot() {
    let mut out = String::new();
    for (rows, cols) in [(0, 3), (5, 0), (0, 0)] {
        let mut s = Scenario::identity(
            AxisConfig::new(rows, 1.0).header(1.0),
            AxisConfig::new(cols, 6.0).header(4.0),
        );
        out += &s.step(
            &format!("{rows} rows, {cols} cols"),
            Viewport::new(22.0, 4.0),
        );
    }
    insta::assert_snapshot!(out);
}

/// Fills only the first two Positions of each requested row Window.
struct SlowRows(IdentityTable);

impl ScenarioSource for SlowRows {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        let mut keys = self.0.window(axis, range);
        if axis == Axis::Rows {
            keys.truncate(2);
        }
        keys
    }
}

#[test]
fn positions_left_unfilled_have_no_key() {
    let table = IdentityTable {
        rows: IdentitySource::new(5),
        cols: IdentitySource::new(2),
    };
    let grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(2, 6.0).header(4.0),
    );
    let mut s = Scenario::new(grid, SlowRows(table));
    insta::assert_snapshot!(s.step("rows 2.. unfilled", Viewport::new(16.0, 5.0)));
}

#[test]
fn layout_ends_when_items_are_too_small_to_advance_an_f32_offset() {
    // 3e-6 is below half the f32 spacing at 100, so 100.0 + 3e-6 == 100.0.
    // Only a host that lowers the size range can get there.
    let mut s = Scenario::identity(
        AxisConfig::new(1_000_000_000_000_000, 3e-6)
            .min_size(3e-6)
            .header(100.0),
        AxisConfig::new(1, 1.0),
    );
    let snap = s.frame(Viewport::new(1.0, 100.002));
    assert_eq!(snap.rows(), &[]);
}

/// An empty axis needs no Keys, wherever the host last filled its Window, so
/// steady frames never ask for an empty Window.
#[test]
fn an_empty_axis_never_requests_a_window() {
    let mut grid = Grid::new(AxisConfig::new(0, 1.0), AxisConfig::new(3, 4.0));
    let viewport = Viewport::new(12.0, 5.0);
    grid.set_window(Axis::Rows, 3, &[]);
    let requests = grid.begin_frame(viewport);
    assert_eq!(requests.get(Axis::Rows), None);
}

/// Found by #25: a tiny Size Override made the Window bound step at that size
/// and ask for 17M column Positions. Overrides are clamped into the size
/// range, so the Window stays near the viewport's width over `min_size`.
#[test]
fn a_tiny_size_override_cannot_blow_up_the_window() {
    let mut grid = Grid::new(
        AxisConfig::new(10, 1.0),
        AxisConfig::new(2_147_483_647, 3.37),
    );
    grid.set_size(Axis::Cols, Key(4), 9.9e-23);
    let requests = grid.begin_frame(Viewport::new(80.0, 10.0));
    let cols = requests.get(Axis::Cols).unwrap();
    assert!(cols.end - cols.start <= 80 + 7, "asked for {cols:?}");
}
