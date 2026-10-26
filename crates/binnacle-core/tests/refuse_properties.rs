//! Seam 1 properties for the Actions the Grid refuses: a real Grid over an
//! identity Table Source, put in a random state, then given one Action.

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FindQuery, FocusStep, Key, Modifiers, Snapshot, TextQuery,
    Viewport,
};
use binnacle_testkit::Scenario;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

fn axis() -> impl Generator<Axis> {
    gs::sampled_from(vec![Axis::Rows, Axis::Cols])
}

#[hegel::composite]
fn action(tc: &TestCase) -> Action {
    let focus_step = gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]);
    tc.draw(hegel::one_of!(
        gs::sampled_from(vec![
            Action::TypeText("x".into()),
            Action::Activate,
            Action::BeginEdit,
            Action::Copy,
            Action::SelectAll,
            Action::SelectColumn,
            Action::SelectRow,
            Action::Cancel,
            Action::LeaveOutline,
            Action::FindNext,
            Action::FindPrevious,
            Action::Scroll { dx: 1.0, dy: 2.0 },
        ]),
        hegel::tuples!(axis(), focus_step, gs::booleans(), gs::booleans()).map(
            |(axis, by, forward, extend)| Action::MoveFocus {
                axis,
                by,
                forward,
                extend
            }
        ),
        axis().map(|axis| Action::ScrollTo {
            axis,
            fraction: 0.5
        }),
        axis().map(|axis| Action::ScrollBy { axis, items: 2 }),
    ))
}

#[derive(Clone, Debug)]
enum Setup {
    Action(Action),
    Focus(u64, u64),
    Find(bool),
    Click(f32, f32, bool),
}

#[hegel::composite]
fn setup(tc: &TestCase) -> Setup {
    let at = || gs::floats::<f32>().min_value(0.0).max_value(1.0);
    tc.draw(hegel::one_of!(
        action().map(Setup::Action),
        hegel::tuples!(
            gs::integers::<u64>().max_value(12),
            gs::integers::<u64>().max_value(12)
        )
        .map(|(r, c)| Setup::Focus(r, c)),
        gs::booleans().map(Setup::Find),
        hegel::tuples!(at(), at(), gs::booleans()).map(|(x, y, ctrl)| Setup::Click(x, y, ctrl)),
    ))
}

/// What a refused Action must leave as it was.
fn state(snap: &Snapshot) -> String {
    let axes = Axis::ALL.map(|a| {
        (
            snap.focus(a),
            snap.picked(a).to_vec(),
            snap.scroll_anchor(a),
            snap.band_focused(a),
        )
    });
    format!(
        "{axes:?} {:?} {:?} {:?} {:?}",
        snap.selection_range(),
        snap.find_match(),
        snap.mode(),
        snap.view_pending()
    )
}

/// An Action `refuses` names does nothing: no Effect, and focus,
/// selection, picks, scroll, mode and find stay as they were.
#[hegel::test(test_cases = 1000)]
fn a_refused_action_does_nothing(tc: TestCase) {
    let count = || gs::integers::<u64>().max_value(10);
    let (rows, cols) = (tc.draw(count()), tc.draw(count()));
    let mut s = Scenario::identity(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(cols, 4.0).header(4.0),
    );
    let viewport = Viewport::new(30.0, 8.0);
    s.frame(viewport);
    for step in tc.draw(gs::vecs(setup()).max_size(5)) {
        match step {
            Setup::Action(a) => s.input(|g| g.action(a)),
            Setup::Focus(r, c) => s.input(|g| g.set_focus(Key(r), Key(c))),
            Setup::Find(open) => s.input(|g| {
                g.set_find(open.then(|| FindQuery::new(TextQuery::new("1"))));
            }),
            Setup::Click(x, y, ctrl) => {
                let held = if ctrl {
                    Modifiers::CTRL
                } else {
                    Modifiers::NONE
                };
                s.click(x * viewport.width, y * viewport.height, held);
            }
        }
        s.frame(viewport);
    }
    s.pump();
    let before = state(&s.frame(viewport));
    let action = tc.draw(action());
    if !s.grid.refuses(&action) {
        return;
    }
    s.input(|g| g.action(action));
    let effects: Vec<Effect> = s.effects();
    assert_eq!(effects, [], "a refused Action sent Effects");
    assert_eq!(state(&s.frame(viewport)), before);
}
