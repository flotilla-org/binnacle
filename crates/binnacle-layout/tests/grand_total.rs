//! A Layout keeps each axis's Grand Total pinned or not, as the user left it:
//! restoring requests the grouping again, which pins a Grand Total placed
//! after by default, and the saved pin then puts it back.

use binnacle_core::{
    Axis, AxisConfig, Edge, GRAND_TOTAL, Grid, Grouping, Key, OpenState, Outline, Placement, Value,
    ViewSpec, Viewport,
};
use binnacle_layout::{IdTable, Layout};
use binnacle_testkit::{GroupedTable, Scenario};

const V: Viewport = Viewport {
    width: 20.0,
    height: 10.0,
};
const REGION: Key = Key(1);

fn table() -> Scenario<GroupedTable> {
    let north = Key(100);
    let items = vec![
        (
            north,
            Outline::group(vec![], OpenState::Open, REGION, Value::Text("North".into())),
        ),
        (Key(0), Outline::leaf(vec![north])),
        (Key(1), Outline::leaf(vec![north])),
        (GRAND_TOTAL, Outline::grand_total()),
    ];
    let grid = Grid::new(
        AxisConfig::new(2, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(
        grid,
        GroupedTable::new(vec![Key(0), Key(1)], 3, REGION, items),
    );
    s.frame(V);
    s
}

fn ids() -> IdTable<String, String> {
    IdTable::new([], [("region".to_owned(), REGION)])
}

fn by_region() -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.rows.grand_total = Placement::After;
    ViewSpec::default().with_grouping(Some(grouping))
}

fn trailing(s: &mut Scenario<GroupedTable>) -> Vec<Key> {
    s.frame(V).pins(Axis::Rows, Edge::Trailing).collect()
}

fn saved(pinned: bool) -> Layout<String, String> {
    let mut s = table();
    s.input(|g| g.request_view(by_region()));
    if !pinned {
        s.input(|g| g.unpin(Axis::Rows, GRAND_TOTAL));
    }
    s.frame(V);
    let mut layout = None;
    s.input(|g| layout = Some(Layout::read(g, &ids())));
    let layout = layout.unwrap();
    let pins = layout.pins.as_ref().expect("pins are read");
    assert_eq!(pins.rows.grand_total, Some(pinned));
    layout
}

fn restored(layout: &Layout<String, String>) -> Vec<Key> {
    let mut s = table();
    s.input(|g| {
        layout.restore(g, &ids());
    });
    trailing(&mut s)
}

#[test]
fn a_layout_keeps_an_unpinned_grand_total_unpinned() {
    assert_eq!(restored(&saved(false)), vec![]);
}

#[test]
fn a_layout_keeps_a_pinned_grand_total_pinned() {
    assert_eq!(restored(&saved(true)), vec![GRAND_TOTAL]);
}

#[test]
fn the_saved_pin_survives_a_json_round_trip() {
    let layout = saved(false);
    let again = Layout::<String, String>::from_json(&layout.to_json()).unwrap();
    assert_eq!(restored(&again), vec![]);
}

#[test]
fn a_layout_that_says_nothing_of_the_grand_total_restores_it_pinned() {
    let mut layout = saved(false);
    let pins = layout.pins.as_mut().unwrap();
    pins.rows.grand_total = None;
    assert_eq!(restored(&layout), vec![GRAND_TOTAL]);
    let json = String::from_utf8(saved(true).to_json()).unwrap();
    let old = json
        .replace(r#","grandTotal":true"#, "")
        .replace(r#","grandTotal":false"#, "");
    let layout = Layout::<String, String>::from_json(old.as_bytes()).unwrap();
    assert_eq!(layout.pins.as_ref().unwrap().rows.grand_total, None);
    assert_eq!(restored(&layout), vec![GRAND_TOTAL]);
}

#[test]
fn an_old_layout_restored_over_a_grouped_grid_leaves_the_grand_total_pinned() {
    let mut old = saved(true);
    old.pins.as_mut().unwrap().rows.grand_total = None;
    let mut s = table();
    s.input(|g| g.request_view(by_region()));
    assert_eq!(trailing(&mut s), vec![GRAND_TOTAL]);
    s.input(|g| {
        old.restore(g, &ids());
    });
    assert_eq!(trailing(&mut s), vec![GRAND_TOTAL]);
}
