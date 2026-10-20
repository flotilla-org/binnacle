//! Seam 1 scenarios for pinning: Keys pinned to an axis's leading edge are
//! drawn in their own Strip between the Header Band and the scrolling
//! Strip, which skips them.

use binnacle_core::{Action, Axis, AxisConfig, Edge, Key, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

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
