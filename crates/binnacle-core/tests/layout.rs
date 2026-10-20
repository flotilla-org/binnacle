//! Seam 1 scenarios and properties for the parts of a saved layout the Grid
//! holds: pins set as a whole and reported by `PinsChanged`, and the Size
//! Overrides listed per axis.

use binnacle_core::{Axis, AxisConfig, Edge, Effect, Grid, Key, Snapshot, Viewport};
use binnacle_testkit::{IdentityTable, KeyedTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// 4 rows of 1 under a 1-high header band; 10 columns of 6 with a Gap of 1
/// after a 4-wide row header band.
fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(10, 6.0).header(4.0).gap(1.0),
    )
}

const V: Viewport = Viewport {
    width: 46.0,
    height: 5.0,
};

fn pins(s: &Snapshot, axis: Axis) -> String {
    let list = |edge| {
        let keys: Vec<String> = s.pins(axis, edge).map(|k| k.0.to_string()).collect();
        keys.join(" ")
    };
    format!(
        "pins {axis:?}: leading [{}], trailing [{}]\n",
        list(Edge::Leading),
        list(Edge::Trailing)
    )
}

#[test]
fn set_pins_replaces_an_edges_pins_in_order_and_each_change_is_reported() {
    let mut s = table();
    let mut out = s.step("start", V);
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(1));
        g.pin(Axis::Cols, Edge::Trailing, Key(7));
    });
    out += &s.step("pin 1 left and 7 right: one PinsChanged per edge", V);
    out += &pins(&s.grid.snapshot(), Axis::Cols);
    s.input(|g| {
        g.set_pins(
            Axis::Cols,
            Edge::Leading,
            &[Key(7), Key(3), Key(3), Key(40)],
        )
    });
    out += &s.step(
        "set the leading pins to 7, 3, 3, 40: 7 leaves the trailing edge, 1 unpins, 3 counts once, 40 is located, gone, and sleeps",
        V,
    );
    out += &pins(&s.grid.snapshot(), Axis::Cols);
    s.input(|g| g.set_pins(Axis::Cols, Edge::Leading, &[Key(7), Key(3), Key(40)]));
    out += &s.step("the same pins again: nothing to report", V);
    insta::assert_snapshot!(out);
}

#[test]
fn a_freeze_reports_its_pins_once() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.freeze(Axis::Cols, Key(2)));
    let pinned: Vec<Effect> = (s.effects().into_iter())
        .filter(|e| matches!(e, Effect::PinsChanged { .. }))
        .collect();
    assert_eq!(
        pinned,
        [Effect::PinsChanged {
            axis: Axis::Cols,
            edge: Edge::Leading,
            keys: vec![Key(0), Key(1), Key(2)],
        }]
    );
}

#[test]
fn size_overrides_are_listed_per_axis_in_key_order_whether_or_not_shown() {
    let mut s = table();
    s.frame(V);
    s.input(|g| {
        g.set_size(Axis::Cols, Key(5), 9.0);
        g.set_size(Axis::Cols, Key(2), 3.0);
        g.set_size(Axis::Cols, Key(7), 4.0);
        g.clear_size(Axis::Cols, Key(7));
        g.set_size(Axis::Rows, Key(40), 2.0);
    });
    let listed =
        |s: &Scenario<IdentityTable>, axis| s.grid.size_overrides(axis).collect::<Vec<_>>();
    assert_eq!(listed(&s, Axis::Cols), [(Key(2), 3.0), (Key(5), 9.0)]);
    assert_eq!(listed(&s, Axis::Rows), [(Key(40), 2.0)]);
}

/// What a step of [`pins_changed_always_names_the_pins_the_snapshot_holds`] does.
#[derive(Clone, Debug)]
enum Op {
    Pin(u64, Edge),
    Unpin(u64),
    SetPins(Edge, Vec<u64>),
    Freeze(u64),
    Remove(u64),
    Insert(u64),
}

#[hegel::composite]
fn op(tc: &TestCase) -> Op {
    let key = || gs::integers::<u64>().max_value(24);
    let edge = || gs::sampled_from(vec![Edge::Leading, Edge::Trailing]);
    tc.draw(hegel::one_of!(
        hegel::tuples!(key(), edge()).map(|(k, e)| Op::Pin(k, e)),
        key().map(Op::Unpin),
        hegel::tuples!(edge(), gs::vecs(key()).max_size(6)).map(|(e, ks)| Op::SetPins(e, ks)),
        hegel::tuples!(edge(), gs::vecs(key()).max_size(6)).map(|(e, ks)| Op::SetPins(e, ks)),
        key().map(Op::Freeze),
        gs::integers::<u64>().max_value(20).map(Op::Remove),
        gs::integers::<u64>().max_value(20).map(Op::Insert),
    ))
}

/// Through pins, unpins, freezes, `set_pins` and items leaving and
/// arriving on either axis, the last `PinsChanged` drained for each edge
/// names the Keys the Snapshot lists as pinned there, dormant ones
/// included, and `set_pins` leaves exactly its Keys (each once, in order)
/// on its edge and none of them on the other.
#[hegel::test(test_cases = 300)]
fn pins_changed_always_names_the_pins_the_snapshot_holds(tc: TestCase) {
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(20));
    let along = AxisConfig::new(count, 2.0).header(1.0);
    let across = AxisConfig::new(3, 2.0).header(1.0);
    let (grid, table) = match axis {
        Axis::Rows => (Grid::new(along, across), KeyedTable::new(count, 3)),
        Axis::Cols => (Grid::new(across, along), KeyedTable::new(3, count)),
    };
    let mut s = Scenario::new(grid, table);
    let v = Viewport::new(30.0, 30.0);
    s.frame(v);
    let mut heard: [Vec<Key>; 2] = Default::default();
    for op in tc.draw(gs::vecs(op()).min_size(1).max_size(10)) {
        let set = if let Op::SetPins(edge, keys) = &op {
            Some((*edge, keys.clone()))
        } else {
            None
        };
        match op {
            Op::Pin(k, edge) => s.input(|g| g.pin(axis, edge, Key(k))),
            Op::Unpin(k) => s.input(|g| g.unpin(axis, Key(k))),
            Op::SetPins(edge, keys) => {
                let keys: Vec<Key> = keys.into_iter().map(Key).collect();
                s.input(|g| g.set_pins(axis, edge, &keys));
            }
            Op::Freeze(k) => s.input(|g| g.freeze(axis, Key(k))),
            Op::Remove(at) => {
                let notice = s.source.remove(axis, at, 1);
                s.input(|g| g.notify(axis, notice));
            }
            Op::Insert(at) => {
                let notice = s.source.insert(axis, at, 1);
                s.input(|g| g.notify(axis, notice));
            }
        }
        if let Some((edge, keys)) = set {
            let mut once: Vec<Key> = Vec::new();
            for k in keys.into_iter().map(Key) {
                if !once.contains(&k) {
                    once.push(k);
                }
            }
            let snap = s.grid.snapshot();
            assert_eq!(snap.pins(axis, edge).collect::<Vec<_>>(), once);
            let other = match edge {
                Edge::Leading => Edge::Trailing,
                Edge::Trailing => Edge::Leading,
            };
            assert!(snap.pins(axis, other).all(|k| !once.contains(&k)));
        }
        for effect in s.effects() {
            if let Effect::PinsChanged {
                axis: a,
                edge,
                keys,
            } = effect
            {
                assert_eq!(a, axis, "PinsChanged on the other axis");
                heard[edge.index()] = keys;
            }
        }
        // A freeze waiting for its Window finishes on a frame.
        s.frame(v);
        for effect in s.effects() {
            if let Effect::PinsChanged { edge, keys, .. } = effect {
                heard[edge.index()] = keys;
            }
        }
        let snap = s.grid.snapshot();
        for edge in Edge::ALL {
            let pins: Vec<Key> = snap.pins(axis, edge).collect();
            assert_eq!(
                heard[edge.index()],
                pins,
                "{edge:?} pins against the last PinsChanged"
            );
        }
    }
}
