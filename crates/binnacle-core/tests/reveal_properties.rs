//! Seam 1 properties for revealing an item by its Key: a real Grid over an
//! identity Table Source (each Key is its item's Position), checked against
//! a twin Grid revealing the same item by Position.

use binnacle_core::{Action, Align, Axis, AxisConfig, Key, Viewport};
use binnacle_testkit::Scenario;
use hegel::TestCase;
use hegel::generators as gs;

#[hegel::composite]
fn axis_config(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(500));
    let size = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(20.0));
    let gap = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(2.0));
    let header = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(5.0));
    AxisConfig::new(count, size).header(header).gap(gap)
}

/// `reveal_key` scrolls as `reveal` (`Align::Nearest`) or `scroll_to_item`
/// (`Align::Start`) does at the Key's Position, whether the Window holds
/// the Key or the Table Source is asked where it is, and moves no focus. A
/// Key the source lacks scrolls nothing.
#[hegel::test(test_cases = 1000)]
fn revealing_a_key_scrolls_as_revealing_its_position_does(tc: TestCase) {
    let (rows, cols) = (tc.draw(axis_config()), tc.draw(axis_config()));
    let along = |c: AxisConfig| {
        tc.draw(
            gs::floats::<f32>()
                .min_value(c.header + 1.0)
                .max_value(c.header + 100.0),
        )
    };
    let viewport = Viewport::new(along(cols), along(rows));
    let mut by_key = Scenario::identity(rows, cols);
    let mut by_position = Scenario::identity(rows, cols);
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let align = tc.draw(gs::sampled_from(vec![Align::Nearest, Align::Start]));
    let count = [rows.count, cols.count][axis.index()];
    let key = tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(count - 1),
        gs::integers::<u64>().max_value(count + 5),
    ));
    let items = tc.draw(gs::integers::<i64>().min_value(-50).max_value(400));
    let focus = tc.draw(gs::optional(hegel::tuples!(
        gs::integers::<u64>().max_value(rows.count - 1),
        gs::integers::<u64>().max_value(cols.count - 1)
    )));
    for s in [&mut by_key, &mut by_position] {
        s.frame(viewport);
        if let Some((r, c)) = focus {
            s.input(|g| g.set_focus(Key(r), Key(c)));
            s.frame(viewport);
        }
        s.input(|g| g.action(Action::ScrollBy { axis, items }));
        s.frame(viewport);
    }
    let before = Axis::ALL.map(|a| by_key.grid.snapshot().focus(a));
    let unrevealed = by_key.grid.snapshot().scroll_anchor(axis);

    by_key.input(|g| g.reveal_key(axis, Key(key), align));
    if key < count {
        by_position.input(|g| match align {
            Align::Nearest => g.reveal(axis, key),
            Align::Start => g.scroll_to_item(axis, key),
        });
    }
    let want = by_position.frame(viewport).scroll_anchor(axis);
    let snap = by_key.frame(viewport);
    assert_eq!(Axis::ALL.map(|a| snap.focus(a)), before, "focus moved");
    match key < count {
        true => assert_eq!(snap.scroll_anchor(axis), want),
        false => assert_eq!(snap.scroll_anchor(axis), unrevealed),
    }
}
