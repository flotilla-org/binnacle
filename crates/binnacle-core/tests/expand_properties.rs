//! Seam 1 properties for opening and closing groups: a real Grid over a
//! hand-made tree whose groups open as the View Spec's `expand` says.

use binnacle_core::{
    Axis, AxisConfig, Depth, Edge, Expand, Grid, Grouping, Key, Level, Snapshot, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use hegel::TestCase;
use hegel::generators as gs;

const REGION: Key = Key(1);
const PORT: Key = Key(2);

/// A tree of regions and ports, some ports empty, with Leaves keyed from 0
/// and groups from 1000; returns it with every group's Key and every
/// Leaf's.
fn tree(tc: &TestCase) -> (Vec<Group>, Vec<Key>, Vec<Key>) {
    let mut next_leaf = 0;
    let mut next_group = 1000;
    let mut groups = Vec::new();
    let mut leaves = Vec::new();
    let regions = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let mut tree = Vec::new();
    for _ in 0..regions {
        let ports = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
        let mut kids = Vec::new();
        for _ in 0..ports {
            let n = tc.draw(gs::integers::<u64>().max_value(4));
            let keys: Vec<u64> = (next_leaf..next_leaf + n).collect();
            leaves.extend(keys.iter().copied().map(Key));
            next_leaf += n;
            groups.push(Key(next_group));
            kids.push(Group::leaves(next_group, "port", &keys));
            next_group += 1;
        }
        groups.push(Key(next_group));
        tree.push(Group::groups(next_group, "region", kids));
        next_group += 1;
    }
    (tree, groups, leaves)
}

fn spec(expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// Room for every row a tree draws: at most 4 regions of 3 ports of 4.
const ALL: Viewport = Viewport {
    width: 20.0,
    height: 70.0,
};

fn scenario(tree: Vec<Group>, expand: Expand) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(0, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree));
    s.frame(ALL);
    s.input(|g| g.request_view(spec(expand)));
    s.frame(ALL);
    s
}

fn depth(tc: &TestCase) -> Depth {
    match tc.draw(gs::integers::<u32>().max_value(3)) {
        3 => Depth::All,
        n => Depth::Levels(n),
    }
}

/// Every drawn row's Key, pinned or scrolling.
fn drawn(snap: &Snapshot) -> Vec<(u64, Option<Key>)> {
    snap.drawn(Axis::Rows)
        .map(|i| (i.position, i.key))
        .collect()
}

/// Through any sequence of toggles, depths and pins, the Strips draw each
/// row the source shows at its Position, once, and nothing else.
#[hegel::test(test_cases = 300)]
fn the_strips_partition_the_shown_rows_through_any_toggles(tc: TestCase) {
    let (tree, groups, leaves) = tree(&tc);
    let mut s = scenario(tree, Expand::to(depth(&tc)));
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(12));
    for _ in 0..steps {
        match tc.draw(gs::integers::<u8>().max_value(3)) {
            0 | 1 => {
                let key = tc.draw(gs::sampled_from(groups.clone()));
                s.input(|g| g.toggle(Axis::Rows, key).unwrap());
            }
            2 => {
                let depth = depth(&tc);
                s.input(|g| g.expand_to(Axis::Rows, depth));
            }
            _ => {
                if !leaves.is_empty() {
                    let key = tc.draw(gs::sampled_from(leaves.clone()));
                    let edge = tc.draw(gs::sampled_from(vec![Edge::Leading, Edge::Trailing]));
                    s.input(|g| g.pin(Axis::Rows, edge, key));
                }
            }
        }
        // Some toggles are left waiting for their answers.
        if tc.draw(gs::booleans()) {
            s.frame(ALL);
        }
    }
    s.frame(ALL);
    let shown: Vec<Key> = s.source.shown().iter().map(|(k, _)| *k).collect();
    let snap = s.frame(ALL);
    let mut drawn = drawn(&snap);
    drawn.sort();
    let want: Vec<(u64, Option<Key>)> = (0..).zip(shown.iter().map(|&k| Some(k))).collect();
    assert_eq!(drawn, want);
}

/// Closing a group and opening it again brings back focus (on the Group
/// Item if it was inside), every Size Override and every pin.
#[hegel::test(test_cases = 300)]
fn collapsing_then_expanding_restores_focus_sizes_and_pins(tc: TestCase) {
    let (tree, groups, leaves) = tree(&tc);
    let mut s = scenario(tree, Expand::default());
    let shown: Vec<Key> = s.source.shown().iter().map(|(k, _)| *k).collect();
    let focus = tc.draw(gs::sampled_from(shown.clone()));
    s.input(|g| g.set_focus(focus, Key(1)));
    let sized: Vec<(Key, f32)> = tc.draw(gs::vecs(hegel::tuples!(
        gs::sampled_from(shown.clone()),
        gs::floats::<f32>().min_value(1.0).max_value(3.0)
    )));
    let pinned: Vec<Key> = match leaves.is_empty() {
        true => Vec::new(),
        false => tc.draw(gs::vecs(gs::sampled_from(leaves.clone())).max_size(3)),
    };
    s.input(|g| {
        for &(key, size) in &sized {
            g.set_size(Axis::Rows, key, size);
        }
        for &key in &pinned {
            g.pin(Axis::Rows, Edge::Leading, key);
        }
    });
    s.frame(ALL);
    let group = tc.draw(gs::sampled_from(groups));
    let inside =
        (s.source.shown().iter()).any(|(k, o)| *k == focus && o.ancestors.contains(&group));
    let sizes = |s: &Scenario<TreeTable>| -> Vec<Option<f32>> {
        shown
            .iter()
            .map(|&k| s.grid.size_override(Axis::Rows, k))
            .collect()
    };
    let before_sizes = sizes(&s);
    let before_pins: Vec<Key> = s.grid.pins(Axis::Rows, Edge::Leading).collect();
    let before_drawn: Vec<Option<Key>> = (s.frame(ALL).pinned(Axis::Rows, Edge::Leading))
        .iter()
        .map(|i| i.key)
        .collect();

    s.input(|g| g.collapse(Axis::Rows, group).unwrap());
    s.frame(ALL);
    s.input(|g| g.expand(Axis::Rows, group).unwrap());
    s.frame(ALL);
    let snap = s.frame(ALL);
    let row = snap.focus(Axis::Rows).and_then(|f| f.key);
    assert_eq!(row, Some(if inside { group } else { focus }));
    let after_drawn: Vec<Option<Key>> = (snap.pinned(Axis::Rows, Edge::Leading))
        .iter()
        .map(|i| i.key)
        .collect();
    assert_eq!(after_drawn, before_drawn);
    assert_eq!(sizes(&s), before_sizes);
    assert_eq!(
        s.grid.pins(Axis::Rows, Edge::Leading).collect::<Vec<_>>(),
        before_pins
    );
}
