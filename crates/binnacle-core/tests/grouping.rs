//! Seam 1 scenarios for grouping: the "Group rows by" View Edit, a grouped
//! Table Source's outlines reaching the Snapshot, and ungrouping.

use binnacle_core::{
    Action, Axis, AxisConfig, Edge, Effect, FocusStep, GRAND_TOTAL, Grid, Grouping, ItemKind, Key,
    Level, OpenState, Outline, Placement, Support, Value, ViewEdit, ViewSpec, Viewport,
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

const PORT: Key = Key(2);

/// The six shipments by region, then port, each level with a Subtotal
/// after its group, and the Grand Total after everything: 19 items, more
/// than the viewport shows.
/// A port's Key, name and shipments.
type Port = (Key, &'static str, &'static [u64]);

fn nested() -> Scenario<GroupedTable> {
    let text = |name: &str| Value::Text(name.into());
    let mut items = Vec::new();
    let regions: [(Key, &str, &[Port]); 2] = [
        (
            NORTH,
            "North",
            &[(Key(110), "Aberdeen", &[0, 3]), (Key(111), "Bergen", &[4])],
        ),
        (SOUTH, "South", &[(Key(120), "Cadiz", &[1, 2, 5])]),
    ];
    let mut subtotal = 200;
    for (region, name, ports) in regions {
        items.push((
            region,
            Outline::group(vec![], OpenState::Open, REGION, text(name)),
        ));
        for &(port, port_name, leaves) in ports {
            let up = vec![region];
            items.push((
                port,
                Outline::group(up, OpenState::Open, PORT, text(port_name)),
            ));
            for &leaf in leaves {
                items.push((Key(leaf), Outline::leaf(vec![region, port])));
            }
            subtotal += 1;
            let sub = Outline::subtotal(vec![region, port], PORT, text(port_name));
            items.push((Key(subtotal), sub));
        }
        subtotal += 1;
        let sub = Outline::subtotal(vec![region], REGION, text(name));
        items.push((Key(subtotal), sub));
    }
    items.push((GRAND_TOTAL, Outline::grand_total()));
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    Scenario::new(
        grid,
        GroupedTable::new((0..6).map(Key).collect(), 3, REGION, items),
    )
}

/// Rows by region, then port, Subtotals after each, the Grand Total placed
/// as `total` says.
fn by_region_and_port(total: Placement) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![
        Level::Field {
            field: REGION,
            subtotal: Placement::After,
        },
        Level::Field {
            field: PORT,
            subtotal: Placement::After,
        },
    ];
    grouping.rows.grand_total = total;
    ViewSpec::default().with_grouping(Some(grouping))
}

fn trailing_pins(s: &mut Scenario<GroupedTable>) -> Vec<Key> {
    s.frame(V).pins(Axis::Rows, Edge::Trailing).collect()
}

#[test]
fn a_grand_total_placed_after_is_pinned_by_default_and_drawn_as_one() {
    let mut s = nested();
    s.frame(V);
    s.input(|g| g.request_view(by_region_and_port(Placement::After)));
    let mut out = s.step(
        "region, then port, Subtotals after: the Grand Total is pinned at the bottom, outside the Window",
        V,
    );
    let snap = s.frame(V);
    let pinned = snap.pinned(Axis::Rows, Edge::Trailing);
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].key, Some(GRAND_TOTAL));
    let outline = snap.outline(Axis::Rows, pinned[0].position);
    assert_eq!(outline.map(|o| o.kind), Some(ItemKind::GrandTotal));
    s.input(|g| g.unpin(Axis::Rows, GRAND_TOTAL));
    out += &s.step("unpinned: it scrolls with the rest", V);
    insta::assert_snapshot!(out);
}

#[test]
fn an_unpinned_grand_total_stays_unpinned_until_placed_after_again() {
    let mut s = nested();
    s.frame(V);
    s.input(|g| g.request_view(by_region_and_port(Placement::After)));
    assert_eq!(trailing_pins(&mut s), vec![GRAND_TOTAL]);
    s.input(|g| g.unpin(Axis::Rows, GRAND_TOTAL));
    let mut other = by_region_and_port(Placement::After);
    if let Some(Level::Field { subtotal, .. }) =
        other.grouping.as_mut().unwrap().row_levels.get_mut(1)
    {
        *subtotal = Placement::Before;
    }
    s.input(|g| g.request_view(other));
    assert_eq!(
        trailing_pins(&mut s),
        vec![],
        "another spec, still placed after"
    );
    s.input(|g| g.request_view(by_region_and_port(Placement::Before)));
    assert_eq!(trailing_pins(&mut s), vec![], "placed before");
    s.input(|g| g.request_view(by_region_and_port(Placement::After)));
    assert_eq!(
        trailing_pins(&mut s),
        vec![GRAND_TOTAL],
        "placed after again"
    );
    s.input(|g| g.request_view(by_region_and_port(Placement::Off)));
    assert_eq!(trailing_pins(&mut s), vec![], "off: the default pin goes");
    s.input(|g| g.request_view(by_region_and_port(Placement::After)));
    s.input(|g| g.request_view(ViewSpec::default()));
    assert_eq!(trailing_pins(&mut s), vec![], "ungrouped");
}
