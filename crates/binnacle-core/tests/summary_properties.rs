//! Seam 1 properties for Summary Items as ordinary items: focus steps
//! through them in drawn order, a Group Item's header selects its group,
//! copy lists what is drawn, and activation names the group's path.

// Runs of Positions are Vecs of Ranges, often of one.
#![allow(clippy::single_range_in_vec_init)]

use binnacle_core::{
    Action, Axis, AxisConfig, Condition, DrillDown, Edge, Effect, Expand, FocusStep, Grid,
    Grouping, ItemKind, Key, Level, Modifiers, Snapshot, Value, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use hegel::TestCase;
use hegel::generators as gs;

const REGION: Key = Key(1);
const PORT: Key = Key(2);

const REGIONS: [&str; 4] = ["North", "South", "East", "West"];
const PORTS: [&str; 3] = ["Aberdeen", "Bergen", "Cadiz"];

/// A tree of regions and ports, some ports empty, with Leaves keyed from 0
/// and groups from 1000; returns it with every group's Key.
fn tree(tc: &TestCase) -> (Vec<Group>, Vec<Key>) {
    let mut next_leaf = 0;
    let mut next_group = 1000;
    let mut groups = Vec::new();
    let regions = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    let mut tree = Vec::new();
    for region in REGIONS.into_iter().take(regions) {
        let ports = tc.draw(gs::integers::<usize>().min_value(1).max_value(3));
        let mut kids = Vec::new();
        for port in PORTS.into_iter().take(ports) {
            let n = tc.draw(gs::integers::<u64>().max_value(4));
            let keys: Vec<u64> = (next_leaf..next_leaf + n).collect();
            next_leaf += n;
            groups.push(Key(next_group));
            kids.push(Group::leaves(next_group, port, &keys));
            next_group += 1;
        }
        groups.push(Key(next_group));
        tree.push(Group::groups(next_group, region, kids));
        next_group += 1;
    }
    (tree, groups)
}

/// Room for every row a tree draws: at most 4 regions of 3 ports of 4.
const ALL: Viewport = Viewport {
    width: 20.0,
    height: 70.0,
};

/// The tree grouped, opened to a drawn depth with a few groups toggled,
/// under a few slicers.
fn scenario(tc: &TestCase) -> Scenario<TreeTable> {
    let (tree, groups) = tree(tc);
    let grid = Grid::new(
        AxisConfig::new(0, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree));
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    let depth = tc.draw(gs::integers::<u32>().max_value(2));
    let except = tc.draw(gs::vecs(gs::sampled_from(groups)).max_size(3));
    grouping.rows.expand = Expand {
        depth: binnacle_core::Depth::Levels(depth),
        except,
    };
    let slicer = gs::tuples2(
        gs::integers::<u64>().max_value(2),
        gs::sampled_from(REGIONS.to_vec()),
    );
    grouping.slicers = (tc.draw(gs::vecs(slicer).max_size(2)).into_iter())
        .map(|(col, v)| Condition::one_of(Key(col), vec![Value::Text(v.into())]))
        .collect();
    s.frame(ALL);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.frame(ALL);
    s
}

/// Pins a few of the rows shown, at either edge.
fn pin_some(tc: &TestCase, s: &mut Scenario<TreeTable>) {
    let shown: Vec<Key> = s.source.shown().iter().map(|(k, _)| *k).collect();
    let pins = tc.draw(gs::vecs(gs::sampled_from(shown)).max_size(3));
    for key in pins {
        let edge = tc.draw(gs::sampled_from(vec![Edge::Leading, Edge::Trailing]));
        s.input(|g| g.pin(Axis::Rows, edge, key));
    }
    s.frame(ALL);
}

fn drawn(snap: &Snapshot) -> Vec<Key> {
    snap.drawn(Axis::Rows).filter_map(|i| i.key).collect()
}

fn step(forward: bool, by: FocusStep) -> Action {
    Action::MoveFocus {
        axis: Axis::Rows,
        by,
        forward,
        extend: false,
    }
}

/// Down from the first row visits every drawn row once, Summary Items
/// included, in drawn order, and Up comes back the same way.
#[hegel::test(test_cases = 200)]
fn arrows_visit_every_drawn_item_in_drawn_order(tc: TestCase) {
    let mut s = scenario(&tc);
    pin_some(&tc, &mut s);
    let order = drawn(&s.frame(ALL));
    s.input(|g| g.set_focus(order[0], Key(1)));
    s.frame(ALL);
    let focused = |s: &mut Scenario<TreeTable>| s.frame(ALL).focus(Axis::Rows).and_then(|f| f.key);
    let mut visited = vec![focused(&mut s).unwrap()];
    for _ in 1..order.len() {
        s.input(|g| g.action(step(true, FocusStep::Item)));
        visited.extend(focused(&mut s));
    }
    assert_eq!(visited, order);
    let mut back = vec![focused(&mut s).unwrap()];
    for _ in 1..order.len() {
        s.input(|g| g.action(step(false, FocusStep::Item)));
        back.extend(focused(&mut s));
    }
    back.reverse();
    assert_eq!(back, order);
}

/// Home and End land on the first and last drawn items, whatever they are.
#[hegel::test(test_cases = 100)]
fn home_and_end_land_on_the_first_and_last_drawn_items(tc: TestCase) {
    let mut s = scenario(&tc);
    pin_some(&tc, &mut s);
    let order = drawn(&s.frame(ALL));
    let start = tc.draw(gs::sampled_from(order.clone()));
    s.input(|g| g.set_focus(start, Key(1)));
    s.frame(ALL);
    s.input(|g| g.action(step(true, FocusStep::Edge)));
    let end = s.frame(ALL).focus(Axis::Rows).and_then(|f| f.key);
    assert_eq!(end, order.last().copied());
    s.input(|g| g.action(step(false, FocusStep::Edge)));
    let home = s.frame(ALL).focus(Axis::Rows).and_then(|f| f.key);
    assert_eq!(home, order.first().copied());
}

/// Clicking a Group Item's header selects its row and every row under it,
/// and nothing else, across every column.
#[hegel::test(test_cases = 200)]
fn a_group_items_header_selects_exactly_its_group(tc: TestCase) {
    let mut s = scenario(&tc);
    let shown = s.source.shown().to_vec();
    let groups: Vec<usize> = (0..shown.len())
        .filter(|&i| shown[i].1.kind == ItemKind::Group)
        .collect();
    let i = tc.draw(gs::sampled_from(groups));
    let key = shown[i].0;
    s.click(1.0, 1.5 + i as f32, Modifiers::NONE);
    let snap = s.frame(ALL);
    let range = snap.selection_range().expect("a range").clone();
    let want: Vec<u64> = (0..shown.len() as u64)
        .filter(|&p| {
            let (k, o) = &shown[p as usize];
            *k == key || o.ancestors.contains(&key)
        })
        .collect();
    let got: Vec<u64> = range.rows.iter().flat_map(|r| r.clone()).collect();
    assert_eq!(got, want);
    assert_eq!(range.cols, vec![0..3]);
}

/// A click on one Group Item's header and a Shift-click on another's
/// select both groups whole and every row drawn between them.
#[hegel::test(test_cases = 200)]
fn shift_clicking_group_items_selects_whole_groups(tc: TestCase) {
    let mut s = scenario(&tc);
    let shown = s.source.shown().to_vec();
    let groups: Vec<usize> = (0..shown.len())
        .filter(|&i| shown[i].1.kind == ItemKind::Group)
        .collect();
    let first = tc.draw(gs::sampled_from(groups.clone()));
    let second = tc.draw(gs::sampled_from(groups));
    // The last Position of the group whose Group Item is at `i`.
    let end = |i: usize| {
        let key = shown[i].0;
        (i..shown.len())
            .take_while(|&p| p == i || shown[p].1.ancestors.contains(&key))
            .last()
            .unwrap() as u64
    };
    s.click(1.0, 1.5 + first as f32, Modifiers::NONE);
    s.click(1.0, 1.5 + second as f32, Modifiers::SHIFT);
    let snap = s.frame(ALL);
    let range = snap.selection_range().expect("a range").clone();
    let got: Vec<u64> = range.rows.iter().flat_map(|r| r.clone()).collect();
    let low = first.min(second) as u64;
    let high = end(first).max(end(second));
    assert_eq!(got, (low..=high).collect::<Vec<_>>());
    assert_eq!(range.cols, vec![0..3]);
}

/// Copying everything lists every drawn row once, in drawn order, pins
/// where they are drawn, and every column.
#[hegel::test(test_cases = 200)]
fn copy_lists_the_drawn_rows_in_drawn_order(tc: TestCase) {
    let mut s = scenario(&tc);
    pin_some(&tc, &mut s);
    let order = drawn(&s.frame(ALL));
    s.input(|g| {
        g.set_focus(order[0], Key(0));
        g.action(Action::SelectAll);
        g.action(Action::Copy);
    });
    let copies: Vec<Effect> = (s.effects().into_iter())
        .filter(|e| matches!(e, Effect::CopyRequested { .. }))
        .collect();
    let [Effect::CopyRequested { rows, cols, .. }] = copies.as_slice() else {
        panic!("one copy, got {copies:?}");
    };
    let shown: Vec<Key> = s.source.shown().iter().map(|(k, _)| *k).collect();
    let copied: Vec<Key> = (rows.iter().flat_map(|r| r.clone()))
        .map(|p| shown[p as usize])
        .collect();
    assert_eq!(copied, order);
    assert_eq!(cols, &vec![0..3]);
}

/// Activating any Summary Item's cell names the slicers, then its groups
/// from the outermost, each by its own value; a Leaf's names nothing.
#[hegel::test(test_cases = 200)]
fn activation_names_the_path_of_a_summary_item(tc: TestCase) {
    let mut s = scenario(&tc);
    let slicers = s
        .frame(ALL)
        .applied_view()
        .grouping
        .as_ref()
        .unwrap()
        .slicers
        .clone();
    let shown = s.source.shown().to_vec();
    let (key, outline) = tc.draw(gs::sampled_from(shown.clone()));
    s.input(|g| g.set_focus(key, Key(2)));
    s.frame(ALL);
    s.input(|g| g.action(Action::Activate));
    let drill_down = s.effects().into_iter().find_map(|e| match e {
        Effect::Activated { drill_down, .. } => Some(drill_down),
        _ => None,
    });
    let want = (outline.kind == ItemKind::Group).then(|| {
        // The groups on the way down to `key`: its ancestors, then itself.
        let groups = outline.ancestors.iter().chain([&key]);
        let named = groups.map(|g| {
            let (_, o) = shown.iter().find(|(k, _)| k == g).expect("shown");
            let group = o.group.as_ref().expect("a Group Item's group");
            Condition::one_of(group.field, vec![group.value.clone()])
        });
        DrillDown::new(slicers.iter().cloned().chain(named).collect())
    });
    assert_eq!(drill_down, Some(want));
}
