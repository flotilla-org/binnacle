//! Seam 1 scenarios for find on grouped rows: a step to a match in
//! collapsed groups opens each closed ancestor with an `Expand` and focuses
//! the match once answered, the groups staying open as ordinary exceptions;
//! a Group Item's label matches in the outline column.

use binnacle_core::{
    Action, Axis, AxisConfig, Depth, Effect, Expand, FindMatch, FindQuery, Grid, Grouping, Key,
    Level, TextQuery, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 20.0,
    height: 13.0,
};

const REGION: Key = Key(10);
const PORT: Key = Key(11);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const ABERDEEN: Key = Key(110);
const BERGEN: Key = Key(111);
const CADIZ: Key = Key(120);

/// Six shipments by region, then port: North holds Aberdeen (0 and 3) and
/// Bergen (4), South holds Cadiz (1, 2 and 5). The level fields are not
/// among the three columns, as when a source lifts them out of the body.
fn tree() -> Vec<Group> {
    vec![
        Group::groups(
            NORTH.0,
            "North",
            vec![
                Group::leaves(ABERDEEN.0, "Aberdeen", &[0, 3]),
                Group::leaves(BERGEN.0, "Bergen", &[4]),
            ],
        ),
        Group::groups(
            SOUTH.0,
            "South",
            vec![Group::leaves(CADIZ.0, "Cadiz", &[1, 2, 5])],
        ),
    ]
}

fn spec(expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// The shipments grouped, opened as `expand` says, with find on `text`.
fn finding(expand: Expand, text: &str) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let table = TreeTable::new(vec![REGION, PORT], 3, tree());
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    s.input(|g| g.request_view(spec(expand)));
    s.frame(V);
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new(text)))));
    s.frame(V);
    s
}

fn expands(effects: &[Effect]) -> Vec<(Expand, ViewEdit)> {
    (effects.iter())
        .filter_map(|e| match e {
            Effect::ViewRequested { spec, edit } => {
                Some((spec.grouping.clone()?.rows.expand, edit.clone()))
            }
            _ => None,
        })
        .collect()
}

fn expand(axis: Axis, key: Key) -> ViewEdit {
    ViewEdit::Expand { axis, key }
}

#[test]
fn a_step_into_a_collapsed_group_opens_it_then_focuses_the_match() {
    let mut s = finding(Expand::to(Depth::Levels(1)), "r4c1");
    let mut out = s.step("ports closed; find \"r4c1\", in closed Bergen", V);
    s.input(|g| g.action(Action::FindNext));
    out += &s.step(
        "next: Bergen opens by an Expand, then shipment 4 takes focus",
        V,
    );
    insta::assert_snapshot!(out);
    let snapshot = s.grid.snapshot();
    assert_eq!(snapshot.find_match(), Some(FindMatch::new(Key(4), Key(1))));
    let applied = snapshot.applied_view().grouping.clone().unwrap();
    assert_eq!(
        applied.rows.expand,
        Expand {
            depth: Depth::Levels(1),
            except: vec![BERGEN]
        },
        "Bergen stays open as an ordinary exception"
    );
}

#[test]
fn each_closed_ancestor_is_opened_by_its_own_expand_outermost_first() {
    let mut s = finding(Expand::to(Depth::Levels(0)), "r3c0");
    s.input(|g| g.action(Action::FindNext));
    let asked = expands(&s.effects());
    let opened = |except: Vec<Key>| Expand {
        depth: Depth::Levels(0),
        except,
    };
    assert_eq!(
        asked,
        vec![
            (opened(vec![NORTH]), expand(Axis::Rows, NORTH)),
            (opened(vec![NORTH, ABERDEEN]), expand(Axis::Rows, ABERDEEN)),
        ]
    );
    s.frame(V);
    let snapshot = s.frame(V);
    let focus = |axis| snapshot.focus(axis).and_then(|f| f.key);
    assert_eq!(
        (focus(Axis::Rows), focus(Axis::Cols)),
        (Some(Key(3)), Some(Key(0)))
    );
}

#[test]
fn a_match_in_open_groups_asks_for_nothing_more() {
    let mut s = finding(Expand::default(), "r5c2");
    s.input(|g| g.action(Action::FindNext));
    assert_eq!(expands(&s.effects()), vec![]);
    let snapshot = s.frame(V);
    assert_eq!(snapshot.find_match(), Some(FindMatch::new(Key(5), Key(2))));
    assert_eq!(snapshot.focus(Axis::Rows).and_then(|f| f.key), Some(Key(5)));
}

#[test]
fn a_group_label_matches_in_the_outline_column() {
    let mut s = finding(Expand::to(Depth::Levels(0)), "bergen");
    let mut out = s.step("everything closed; find \"bergen\", a port's label", V);
    s.input(|g| g.action(Action::FindNext));
    out += &s.step(
        "next: North opens, and focus takes Bergen's label in the outline column",
        V,
    );
    insta::assert_snapshot!(out);
    let snapshot = s.grid.snapshot();
    assert_eq!(snapshot.find_match(), Some(FindMatch::new(BERGEN, PORT)));
    assert!(snapshot.outline_focused());
    assert_eq!(snapshot.focus(Axis::Rows).and_then(|f| f.key), Some(BERGEN));
}

#[test]
fn a_top_level_label_answered_under_no_group_is_focused_in_the_outline_column() {
    let mut s = finding(Expand::to(Depth::Levels(0)), "south");
    s.input(|g| g.action(Action::FindNext));
    let snapshot = s.frame(V);
    assert_eq!(snapshot.find_match(), Some(FindMatch::new(SOUTH, REGION)));
    assert!(snapshot.outline_focused());
    assert_eq!(snapshot.focus(Axis::Rows).and_then(|f| f.key), Some(SOUTH));
}

#[test]
fn a_cell_match_after_a_label_match_leaves_the_outline_column() {
    let mut s = finding(Expand::to(Depth::Levels(0)), "bergen");
    s.input(|g| g.action(Action::FindNext));
    s.frame(V);
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new("r4c0")))));
    s.input(|g| g.action(Action::FindNext));
    let snapshot = s.frame(V);
    assert_eq!(snapshot.find_match(), Some(FindMatch::new(Key(4), Key(0))));
    assert!(!snapshot.outline_focused());
    let focus = Axis::ALL.map(|a| snapshot.focus(a).and_then(|f| f.key));
    assert_eq!(focus, [Some(Key(4)), Some(Key(0))]);
}

#[test]
fn a_new_query_before_the_groups_open_leaves_focus_alone() {
    let mut s = finding(Expand::to(Depth::Levels(1)), "r4c0");
    s.input(|g| g.set_focus(SOUTH, Key(0)));
    s.frame(V);
    s.input(|g| g.action(Action::FindNext));
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new("zzz")))));
    let snapshot = s.frame(V);
    assert_eq!(snapshot.find_match(), None);
    assert_eq!(snapshot.focus(Axis::Rows).and_then(|f| f.key), Some(SOUTH));
}

#[test]
fn closing_find_while_the_groups_open_drops_the_match() {
    let mut s = finding(Expand::to(Depth::Levels(1)), "r4c0");
    s.input(|g| g.set_focus(SOUTH, Key(0)));
    s.frame(V);
    // The step's answer comes, but not yet the source's to the Expand.
    s.input(|g| {
        g.action(Action::FindNext);
        while g.poll_effect().is_some() {}
        g.find_stepped_within(FindMatch::new(Key(4), Key(0)), [&[NORTH, BERGEN], &[]]);
        g.set_find(None);
    });
    let snapshot = s.frame(V);
    assert_eq!(snapshot.find_match(), None);
    assert_eq!(snapshot.focus(Axis::Rows).and_then(|f| f.key), Some(SOUTH));
    let open = snapshot
        .applied_view()
        .grouping
        .clone()
        .unwrap()
        .rows
        .expand;
    assert_eq!(open.except, vec![BERGEN], "Bergen opened all the same");
}
