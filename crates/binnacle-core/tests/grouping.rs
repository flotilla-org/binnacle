//! Seam 1 scenarios for grouping: the "Group rows by" View Edit, a grouped
//! Table Source's outlines reaching the Snapshot, and ungrouping.

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FocusStep, Grid, Grouping, Key, OpenState, Outline, Support,
    Value, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{GroupedTable, Scenario};

const V: Viewport = Viewport {
    width: 20.0,
    height: 10.0,
};

const REGION: Key = Key(1);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);

/// Six shipments keyed 0 to 5 over three columns; column 1 is the region,
/// North for 0, 3 and 4 and South for 1, 2 and 5.
fn shipments() -> Scenario<GroupedTable> {
    let group = |key, name: &str| {
        (
            key,
            Outline::group(vec![], OpenState::Open, REGION, Value::Text(name.into())),
        )
    };
    let leaf = |key, parent| (Key(key), Outline::leaf(vec![parent]));
    let grouped = vec![
        group(NORTH, "North"),
        leaf(0, NORTH),
        leaf(3, NORTH),
        leaf(4, NORTH),
        group(SOUTH, "South"),
        leaf(1, SOUTH),
        leaf(2, SOUTH),
        leaf(5, SOUTH),
    ];
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    Scenario::new(
        grid,
        GroupedTable::new((0..6).map(Key).collect(), 3, REGION, grouped),
    )
}

/// Support for grouping the rows by a column.
fn groupable() -> Support {
    Support::default().with_group(true)
}

#[test]
fn grouping_rows_by_a_column_brings_outlines_and_ungrouping_takes_them_away() {
    let mut s = shipments();
    let mut out = s.step("a plain table, no outlines", V);
    s.input(|g| {
        g.set_support(Axis::Rows, REGION, groupable());
        g.set_focus(Key(3), Key(2));
    });
    out += &s.step("focus shipment 3 in column 2", V);
    s.input(|g| g.group_by(Axis::Rows, REGION));
    out += &s.step(
        "group the rows by the region: Group Items head their Leaves, the region column leaves, focus stays on shipment 3",
        V,
    );
    s.input(|g| g.ungroup(Axis::Rows));
    out += &s.step("ungroup: the plain rows come back without outlines", V);
    insta::assert_snapshot!(out);
}

#[test]
fn group_by_asks_for_a_grouping_counting_each_group() {
    let mut s = shipments();
    s.frame(V);
    s.input(|g| {
        g.set_support(Axis::Rows, REGION, groupable());
        g.group_by(Axis::Rows, REGION);
    });
    let effects = s.effects();
    let want = ViewSpec::default().with_grouping(Some(Grouping::rows_by(REGION)));
    assert!(effects.contains(&Effect::ViewRequested {
        spec: want,
        edit: ViewEdit::Group {
            axis: Axis::Rows,
            field: REGION,
        },
    }));
}

#[test]
fn group_by_asks_nothing_where_the_source_cant_group_or_already_groups() {
    let mut s = shipments();
    s.frame(V);
    s.input(|g| g.group_by(Axis::Rows, REGION));
    assert_eq!(s.effects(), vec![], "unsupported");
    s.input(|g| {
        g.set_support(Axis::Rows, REGION, groupable());
        g.group_by(Axis::Rows, REGION);
    });
    s.frame(V);
    s.input(|g| g.group_by(Axis::Rows, REGION));
    assert_eq!(s.effects(), vec![], "grouped already");
    s.input(|g| {
        g.ungroup(Axis::Rows);
        g.ungroup(Axis::Rows);
    });
    let requests = s
        .effects()
        .into_iter()
        .filter(|e| matches!(e, Effect::ViewRequested { .. }))
        .count();
    assert_eq!(requests, 1, "ungrouping a plain table asks nothing");
}

#[test]
fn arrows_step_through_group_items_like_any_item() {
    let mut s = shipments();
    s.input(|g| g.set_support(Axis::Rows, REGION, groupable()));
    s.frame(V);
    s.input(|g| {
        g.group_by(Axis::Rows, REGION);
    });
    s.frame(V);
    s.input(|g| g.set_focus(NORTH, Key(0)));
    s.frame(V);
    let mut visited = Vec::new();
    for _ in 0..8 {
        let snap = s.frame(V);
        let row = snap.focus(Axis::Rows).and_then(|f| f.key);
        visited.push(row);
        s.input(|g| {
            g.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: false,
            })
        });
    }
    let want: Vec<Option<Key>> = [NORTH, Key(0), Key(3), Key(4), SOUTH, Key(1), Key(2), Key(5)]
        .into_iter()
        .map(Some)
        .collect();
    assert_eq!(visited, want);
}
