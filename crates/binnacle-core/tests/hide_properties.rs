//! Seam 1 properties for hiding: a real Grid over an In-Memory Table Source
//! of random records, driven through the frame loop and the shared pump with
//! hides and shows on both axes mixed with clicks, focus moves, selections,
//! resizes, filters and find steps. Once every answer is in, Positions are
//! gapless and never reach a hidden item, Size Overrides of hidden items
//! stay, and nothing a host copies or find lands on is hidden.

use std::collections::BTreeMap;

use binnacle_core::{
    Action, Axis, AxisConfig, Condition, Effect, FindQuery, FocusStep, Grid, HIDE_CAP, Key,
    Modifiers, PointerEvent, Snapshot, TextQuery, TextSupport, ViewSpec, Viewport,
};
use binnacle_memory::MemorySource;
use binnacle_testkit::{Record, RecordTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const ROW_HEADER: f32 = 3.0;
const COL_WIDTH: f32 = 4.0;

#[derive(Clone, Debug)]
enum Input {
    Hide(Axis, Vec<u64>),
    Show(Axis, Vec<u64>),
    /// A press and release at fractions of the viewport.
    Click(f32, f32, Modifiers),
    Action(Action),
    /// Sets the size of the item with this Key, if the axis has it.
    Size(Axis, u64, f32),
    /// A condition on the rows: column `c` holds text containing `t`, or
    /// clears the conditions with `None`.
    Condition(Option<(u64, String)>),
    Find(String),
}

#[hegel::composite]
fn axis(tc: &TestCase) -> Axis {
    tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))
}

/// From one too narrow for a column to more than the table needs.
#[hegel::composite]
fn viewport(tc: &TestCase) -> Viewport {
    Viewport::new(
        tc.draw(gs::floats::<f32>().min_value(1.0).max_value(40.0)),
        tc.draw(gs::floats::<f32>().min_value(1.0).max_value(14.0)),
    )
}

#[hegel::composite]
fn fraction(tc: &TestCase) -> f32 {
    tc.draw(gs::floats::<f32>().min_value(0.0).max_value(0.999))
}

/// Keys of an item on either axis (some past the last).
#[hegel::composite]
fn keys(tc: &TestCase, n: usize) -> Vec<u64> {
    tc.draw(
        gs::vecs(gs::integers::<u64>().max_value(n as u64))
            .min_size(1)
            .max_size(3),
    )
}

#[hegel::composite]
fn input(tc: &TestCase, rows: usize, cols: usize) -> Input {
    let n = rows.max(cols);
    let modifiers = gs::sampled_from(vec![Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL]);
    let step = gs::sampled_from(vec![FocusStep::Item, FocusStep::Item, FocusStep::Edge]);
    let move_focus = hegel::tuples!(axis(), step, gs::booleans(), gs::weighted_booleans(0.3)).map(
        |(axis, by, forward, extend)| {
            Input::Action(Action::MoveFocus {
                axis,
                by,
                forward,
                extend,
            })
        },
    );
    let text = || gs::sampled_from(vec!["1".to_string(), "2".to_string(), "-".to_string()]);
    tc.draw(hegel::one_of!(
        hegel::tuples!(axis(), keys(n)).map(|(a, k)| Input::Hide(a, k)),
        hegel::tuples!(axis(), keys(n)).map(|(a, k)| Input::Hide(a, k)),
        hegel::tuples!(axis(), keys(n)).map(|(a, k)| Input::Show(a, k)),
        hegel::tuples!(fraction(), fraction(), modifiers).map(|(x, y, m)| Input::Click(x, y, m)),
        move_focus,
        gs::sampled_from(vec![
            Input::Action(Action::SelectAll),
            Input::Action(Action::SelectColumn),
            Input::Action(Action::FindNext),
        ]),
        hegel::tuples!(
            axis(),
            gs::integers::<u64>().max_value(n as u64),
            gs::floats::<f32>().min_value(1.0).max_value(6.0)
        )
        .map(|(a, k, size)| Input::Size(a, k, size)),
        gs::optional(hegel::tuples!(
            gs::integers::<u64>().max_value(cols as u64 - 1),
            text()
        ))
        .map(Input::Condition),
        text().map(Input::Find),
    ))
}

struct Host {
    s: Scenario<RecordTable>,
    viewport: Viewport,
    /// The Size Overrides set, by axis and Key.
    sizes: [BTreeMap<Key, f32>; 2],
}

impl Host {
    fn new(rows: Vec<Record>, cols: usize, viewport: Viewport) -> Host {
        let mut grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 1.0).header(1.0),
            AxisConfig::new(cols as u64, COL_WIDTH)
                .header(ROW_HEADER)
                .gap(1.0),
        );
        for axis in Axis::ALL {
            grid.set_hide_support(axis, true);
        }
        grid.set_find_support(Some(TextSupport::new(false)));
        let source = RecordTable::new(rows, cols, 0..cols as u64);
        Host {
            s: Scenario::new(grid, source),
            viewport,
            sizes: Default::default(),
        }
    }

    fn input(&mut self, input: Input) {
        let v = self.viewport;
        let g = &mut self.s.grid;
        match input {
            Input::Hide(axis, keys) => {
                let keys: Vec<Key> = keys.into_iter().map(Key).collect();
                // The default cap is far past these tables.
                g.hide(axis, &keys).unwrap();
            }
            Input::Show(axis, keys) => g.show(axis, &keys.into_iter().map(Key).collect::<Vec<_>>()),
            Input::Click(x, y, m) => {
                let (x, y) = (x * v.width, y * v.height);
                g.pointer(PointerEvent::down(x, y).modifiers(m));
                g.pointer(PointerEvent::up(x, y).modifiers(m));
            }
            Input::Action(action) => g.action(action),
            Input::Size(axis, key, size) => {
                g.set_size(axis, Key(key), size);
                self.sizes[axis.index()].insert(Key(key), size);
            }
            Input::Condition(None) => {
                let spec = g.snapshot().applied_view().clone();
                for c in spec.rows.filter.conditions.conditions() {
                    g.remove_condition(Axis::Rows, c);
                }
            }
            Input::Condition(Some((c, text))) => {
                g.add_condition(Axis::Rows, Condition::text(Key(c), TextQuery::new(text)));
            }
            Input::Find(text) => g.set_find(Some(FindQuery::new(TextQuery::new(text)))),
        }
    }

    /// Runs frames until no answer is due, so the Grid has settled.
    fn settle(&mut self) {
        for _ in 0..4 {
            self.s.frame(self.viewport);
        }
    }
}

#[hegel::composite]
fn records(tc: &TestCase, cols: usize) -> Vec<Record> {
    let value = gs::optional(gs::integers::<i64>().min_value(-2).max_value(12));
    let record = gs::vecs(value).min_size(cols).max_size(cols);
    tc.draw(gs::vecs(record).min_size(1).max_size(10))
}

/// Drives a Host through random inputs, calling `check` with the source,
/// the input, the View Spec the source applied and the Snapshot once each
/// input has settled.
fn drive(
    tc: &TestCase,
    mut check: impl FnMut(&MemorySource<Record>, &Input, &ViewSpec, &mut Snapshot),
) {
    let cols = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let rows = tc.draw(records(cols));
    let n_rows = rows.len();
    let mut host = Host::new(rows, cols, tc.draw(viewport()));
    host.settle();
    for input in tc.draw(gs::vecs(input(n_rows, cols)).max_size(12)) {
        host.input(input.clone());
        host.settle();
        let source = &host.s.source.source;
        check(
            source,
            &input,
            &source.view_spec(),
            &mut host.s.grid.snapshot(),
        );
    }
}

#[hegel::test(test_cases = 300)]
fn positions_stay_gapless_and_never_reach_a_hidden_item(tc: TestCase) {
    drive(&tc, |source, input, applied, snap| {
        for axis in Axis::ALL {
            let hidden = &applied.axis(axis).hidden;
            assert_eq!(snap.hidden(axis), hidden.as_slice(), "{input:?}");
            // The Strip is a run of Positions below the count, each with the
            // Key the source has there, none of them hidden.
            assert_eq!(snap.count(axis), source.count(axis));
            let strip = snap.strip(axis);
            for (a, b) in strip.iter().zip(strip.iter().skip(1)) {
                assert_eq!(a.position + 1, b.position, "{axis:?} {input:?}");
            }
            for item in strip {
                assert!(item.position < snap.count(axis));
                let at = source.window(axis, item.position..item.position + 1);
                assert_eq!(item.key, at.first().copied(), "{axis:?} {input:?}");
                assert!(item.key.is_none_or(|k| !hidden.contains(&k)));
            }
            if let Some(f) = snap.focus(axis) {
                assert!(f.key.is_none_or(|k| !hidden.contains(&k)), "focus {f:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 300)]
fn nothing_copied_or_found_is_hidden(tc: TestCase) {
    drive(&tc, |source, input, applied, snap| {
        // What a host copies: the range's items, and the picked rows.
        if let Some(range) = snap.selection_range() {
            for axis in Axis::ALL {
                let hidden = &applied.axis(axis).hidden;
                for run in range.span(axis) {
                    let keys = source.window(axis, run.clone());
                    assert!(keys.iter().all(|k| !hidden.contains(k)), "{input:?}");
                }
            }
        }
        for axis in Axis::ALL {
            for k in snap.picked(axis) {
                assert!(!applied.axis(axis).hidden.contains(k), "picked {k:?}");
            }
        }
        if let Some(m) = snap.find_match() {
            for axis in Axis::ALL {
                assert!(!applied.axis(axis).hidden.contains(&m.key(axis)), "{m:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 300)]
fn size_overrides_stay_hidden_or_not_until_set_again(tc: TestCase) {
    let cols = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let rows = tc.draw(records(cols));
    let n_rows = rows.len();
    let mut host = Host::new(rows, cols, tc.draw(viewport()));
    host.settle();
    for input in tc.draw(gs::vecs(input(n_rows, cols)).max_size(12)) {
        host.input(input.clone());
        host.settle();
        for axis in Axis::ALL {
            for (&key, &size) in &host.sizes[axis.index()] {
                let kept = host.s.grid.size_override(axis, key);
                assert_eq!(kept, Some(size), "{axis:?} {key:?} {input:?}");
            }
        }
    }
}

#[hegel::test(test_cases = 200)]
fn hiding_then_showing_restores_the_view_and_the_size_overrides(tc: TestCase) {
    let cols = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let rows = tc.draw(records(cols));
    let n_rows = rows.len();
    let mut host = Host::new(rows, cols, tc.draw(viewport()));
    host.settle();
    let axis = tc.draw(axis());
    let n = [n_rows, cols][axis.index()];
    let sized = tc.draw(keys(n));
    for &k in &sized {
        host.input(Input::Size(axis, k, 2.5 + k as f32));
    }
    host.settle();
    // The view keeps its place on the item at its top, so the Strip may
    // start elsewhere; the items come back in their places.
    let strip = |host: &mut Host| {
        let count = host.s.grid.snapshot().count(axis);
        (count, host.s.source.source.window(axis, 0..count))
    };
    let before = strip(&mut host);
    let hidden = tc.draw(keys(n));
    host.input(Input::Hide(axis, hidden.clone()));
    host.settle();
    host.input(Input::Show(axis, hidden));
    host.settle();
    assert_eq!(strip(&mut host), before);
    for &k in &sized {
        let size = host.s.grid.size_override(axis, Key(k));
        assert_eq!(size, Some(2.5 + k as f32));
    }
}

#[hegel::test(test_cases = 200)]
fn a_hide_past_the_cap_changes_nothing_and_one_within_it_hides_every_item_named(tc: TestCase) {
    let cols = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let rows = tc.draw(records(cols));
    let n_rows = rows.len();
    let mut host = Host::new(rows, cols, tc.draw(viewport()));
    // Small caps, so the few Keys there are can pass them; a cap past
    // HIDE_CAP is held to it.
    let cap = || {
        hegel::one_of!(
            gs::integers::<usize>().max_value(3),
            gs::integers::<usize>()
                .min_value(HIDE_CAP - 2)
                .max_value(usize::MAX)
        )
    };
    let declared = [tc.draw(cap()), tc.draw(cap())];
    for axis in Axis::ALL {
        host.s.grid.set_hide_cap(axis, declared[axis.index()]);
    }
    host.settle();
    let caps = declared.map(|c| c.min(HIDE_CAP));
    for axis in Axis::ALL {
        assert_eq!(host.s.grid.snapshot().hide_cap(axis), caps[axis.index()]);
    }
    let step = hegel::tuples!(axis(), gs::booleans(), keys(n_rows.max(cols)));
    for (axis, hide, keys) in tc.draw(gs::vecs(step).max_size(8)) {
        let keys: Vec<Key> = keys.into_iter().map(Key).collect();
        let before = host.s.grid.snapshot().hidden(axis).to_vec();
        if !hide {
            host.s.grid.show(axis, &keys);
            host.settle();
            continue;
        }
        let hid = host.s.grid.hide(axis, &keys);
        let asked = host
            .s
            .effects()
            .iter()
            .any(|e| matches!(e, Effect::ViewRequested { .. }));
        host.settle();
        let snap = host.s.grid.snapshot();
        let after = snap.hidden(axis);
        let cap = caps[axis.index()];
        assert!(after.len() <= cap, "{after:?} past the cap {cap}");
        // Refused exactly when the Keys named and those hidden together pass
        // the cap.
        let mut all = before.clone();
        for &k in &keys {
            if !all.contains(&k) {
                all.push(k);
            }
        }
        assert_eq!(hid.is_err(), all.len() > cap, "{all:?} against {cap}");
        match hid {
            Err(capped) => {
                assert_eq!(capped.cap, cap);
                assert!(!asked, "a refused hide asks for nothing");
                assert_eq!(after, before.as_slice());
            }
            Ok(()) => {
                let count = snap.count(axis);
                let shown = host.s.source.source.window(axis, 0..count);
                assert!(
                    keys.iter().all(|k| !shown.contains(k)),
                    "{keys:?} in {shown:?}"
                );
            }
        }
    }
}
