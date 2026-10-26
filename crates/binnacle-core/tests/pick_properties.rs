//! Seam 1 properties for picking items by Key: a real Grid over an identity
//! Table Source, its picks checked against a set per axis.

use std::collections::BTreeSet;

use binnacle_core::{Axis, AxisConfig, Effect, Key, Viewport};
use binnacle_testkit::Scenario;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

#[derive(Clone, Debug)]
enum Input {
    Pick(Axis, Key),
    Unpick(Axis, Key),
    Clear(Axis),
}

fn axis() -> impl Generator<Axis> {
    gs::sampled_from(vec![Axis::Rows, Axis::Cols])
}

#[hegel::composite]
fn input(tc: &TestCase) -> Input {
    let key = || gs::integers::<u64>().max_value(30).map(Key);
    tc.draw(hegel::one_of!(
        hegel::tuples!(axis(), key()).map(|(a, k)| Input::Pick(a, k)),
        hegel::tuples!(axis(), key()).map(|(a, k)| Input::Pick(a, k)),
        hegel::tuples!(axis(), key()).map(|(a, k)| Input::Unpick(a, k)),
        axis().map(Input::Clear),
    ))
}

/// After any run of picks, unpicks and clears, each axis's picks are the
/// Keys picked and not since unpicked or cleared, in Key order, and a step
/// that changed them sends one `SelectionChanged` (a step that changed them
/// back sends at most one).
#[hegel::test(test_cases = 1000)]
fn picks_are_the_keys_picked_and_not_since_unpicked_or_cleared(tc: TestCase) {
    let mut s = Scenario::identity(
        AxisConfig::new(40, 1.0).header(1.0),
        AxisConfig::new(40, 4.0).header(4.0),
    );
    let viewport = Viewport::new(30.0, 10.0);
    s.frame(viewport);
    let mut model: [BTreeSet<Key>; 2] = Default::default();
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(5)) {
        let before = model.clone();
        for input in tc.draw(gs::vecs(input()).max_size(6)) {
            match input {
                Input::Pick(axis, key) => {
                    model[axis.index()].insert(key);
                    s.input(|g| g.pick(axis, key));
                }
                Input::Unpick(axis, key) => {
                    model[axis.index()].remove(&key);
                    s.input(|g| g.unpick(axis, key));
                }
                Input::Clear(axis) => {
                    model[axis.index()].clear();
                    s.input(|g| g.clear_picks(axis));
                }
            }
        }
        let changed = s
            .effects()
            .iter()
            .filter(|e| **e == Effect::SelectionChanged)
            .count();
        assert!(changed <= 1, "{changed} SelectionChanged");
        if model != before {
            assert_eq!(changed, 1, "no SelectionChanged");
        }
        let snap = s.frame(viewport);
        for axis in Axis::ALL {
            let want: Vec<Key> = model[axis.index()].iter().copied().collect();
            assert_eq!(snap.picked(axis), want, "{axis:?}");
        }
    }
}
