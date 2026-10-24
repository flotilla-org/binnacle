//! Seam 1 scenarios for header form: each row level a tier of the row
//! Header Band, each group one spanning cell over its items, merged by
//! ancestor Key, broken at pinned edges, with its label kept in view, and
//! the tree keys stepping between the level columns.

use binnacle_core::{
    Action, Axis, AxisConfig, Depth, Edge, Effect, Expand, FocusStep, Form, Grid, Grouping, Key,
    Level, Modifiers, OpenState, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 30.0,
    height: 13.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const ABERDEEN: Key = Key(110);
const BERGEN: Key = Key(111);
const CADIZ: Key = Key(120);

/// Six shipments by region, then port: North holds Aberdeen (0 and 3) and
/// Bergen (4), South holds Cadiz (1, 2 and 5).
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

fn spec(expand: Expand, form: Form) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    grouping.rows.form = form;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// The shipments grouped in header form, with a Subtotal after each open
/// group, opened as `expand` says; level columns 4 and 6 wide in a band of
/// 12, focus on `row`.
fn grouped(expand: Expand, row: Key) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(12.0),
    );
    let table = TreeTable::new(vec![REGION, PORT], 3, tree()).subtotals_after();
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    s.input(|g| {
        g.set_tiers(Axis::Rows, &[4.0, 6.0]);
        g.request_view(spec(expand, Form::Header));
    });
    s.frame(V);
    s.input(|g| g.set_focus(row, Key(1)));
    s.frame(V);
    s
}

fn subtotal(group: Key) -> Key {
    TreeTable::subtotal(group)
}

fn press(s: &mut Scenario<TreeTable>, action: Action) {
    s.input(|g| g.action(action));
    s.frame(V);
}

fn cols(forward: bool) -> Action {
    Action::MoveFocus {
        axis: Axis::Cols,
        by: FocusStep::Item,
        forward,
        extend: false,
    }
}

fn down() -> Action {
    Action::MoveFocus {
        axis: Axis::Rows,
        by: FocusStep::Item,
        forward: true,
        extend: false,
    }
}

#[test]
fn each_group_is_one_span_per_tier_and_each_row_header_starts_past_its_spans() {
    let mut s = grouped(Expand::default(), Key(3));
    let out = s.step("everything open in header form", V);
    insta::assert_snapshot!(out);
    let snap = s.frame(V);
    assert_eq!(snap.tiers(Axis::Rows), &[0.0..4.0, 4.0..10.0]);
    let spans: Vec<(u32, Key, u64, u64)> = (snap.spans(Axis::Rows).iter())
        .map(|s| (s.tier, s.key, s.first, s.last))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0, NORTH, 0, 4),
            (0, SOUTH, 6, 9),
            (1, ABERDEEN, 0, 1),
            (1, BERGEN, 3, 3),
            (1, CADIZ, 6, 8),
        ],
        "an open group's own Subtotal sits outside its span"
    );
    let header = |key| {
        let h = snap.headers(Axis::Rows).find(|h| h.key == Some(key));
        h.map(|h| (h.rect.x, h.rect.width))
    };
    assert_eq!(header(Key(0)), Some((10.0, 2.0)), "a Leaf: past both tiers");
    assert_eq!(
        header(subtotal(ABERDEEN)),
        Some((4.0, 8.0)),
        "a port's Subtotal: labelled from the port column"
    );
    assert_eq!(
        header(subtotal(NORTH)),
        Some((0.0, 12.0)),
        "a region's Subtotal: labelled from the region column"
    );
}

#[test]
fn a_closed_group_keeps_its_subtotal_under_its_span() {
    let mut s = grouped(Expand::to(Depth::Levels(1)), NORTH);
    let snap = s.frame(V);
    let spans: Vec<(u32, Key, OpenState, u64, u64)> = (snap.spans(Axis::Rows).iter())
        .map(|s| (s.tier, s.key, s.open, s.first, s.last))
        .collect();
    assert_eq!(
        spans,
        vec![
            (0, NORTH, OpenState::Open, 0, 1),
            (0, SOUTH, OpenState::Open, 3, 3),
            (1, ABERDEEN, OpenState::Closed, 0, 0),
            (1, BERGEN, OpenState::Closed, 1, 1),
            (1, CADIZ, OpenState::Closed, 3, 3),
        ]
    );
    let rows: Vec<Option<Key>> = snap.rows().iter().map(|i| i.key).collect();
    assert_eq!(
        rows,
        [
            subtotal(ABERDEEN),
            subtotal(BERGEN),
            subtotal(NORTH),
            subtotal(CADIZ),
            subtotal(SOUTH)
        ]
        .map(Some)
    );
}

#[test]
fn a_span_keeps_its_label_in_view_and_marks_where_it_is_cut() {
    let short = Viewport {
        width: 30.0,
        height: 4.0,
    };
    let mut s = grouped(Expand::default(), Key(0));
    s.frame(short);
    s.input(|g| g.scroll_to_item(Axis::Rows, 1));
    let out = s.step(
        "scrolled to shipment 3: North and Aberdeen go on above",
        short,
    );
    insta::assert_snapshot!(out);
    let snap = s.frame(short);
    let north = snap.spans(Axis::Rows)[0];
    assert_eq!(north.key, NORTH);
    assert!(north.cut_before && north.cut_after);
    assert_eq!((north.visible.y, north.visible.height), (1.0, 3.0));
}

#[test]
fn a_span_breaks_in_two_at_a_pinned_edge_each_part_labelled() {
    let mut s = grouped(Expand::default(), Key(0));
    s.input(|g| g.pin(Axis::Rows, Edge::Leading, Key(0)));
    let out = s.step("shipment 0 pinned: Aberdeen and North break in two", V);
    insta::assert_snapshot!(out);
    let snap = s.frame(V);
    let aberdeen: Vec<(u64, u64, bool, bool)> = (snap.spans(Axis::Rows).iter())
        .filter(|s| s.key == ABERDEEN)
        .map(|s| (s.first, s.last, s.cut_before, s.cut_after))
        .collect();
    assert_eq!(aberdeen, vec![(0, 0, false, true), (1, 1, true, false)]);
}

#[test]
fn closing_a_group_in_header_form_moves_focus_inside_to_its_subtotal() {
    let mut s = grouped(Expand::default(), Key(3));
    s.input(|g| g.collapse(Axis::Rows, ABERDEEN).unwrap());
    let mut out = s.step(
        "collapse Aberdeen: Collapsed, focus moves to Aberdeen's Subtotal",
        V,
    );
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    out += &s.step("collapse North: focus moves to North's Subtotal", V);
    insta::assert_snapshot!(out);
    let snap = s.frame(V);
    assert_eq!(
        snap.focus(Axis::Rows).and_then(|f| f.key),
        Some(subtotal(NORTH))
    );
}

#[test]
fn a_closed_group_with_no_subtotal_placed_is_refiltered() {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(12.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree()));
    s.frame(V);
    s.input(|g| g.request_view(spec(Expand::default(), Form::Header)));
    s.frame(V);
    s.input(|g| g.collapse(Axis::Rows, ABERDEEN).unwrap());
    let out = s.step("collapse Aberdeen with no Subtotals placed: Refiltered", V);
    assert!(out.contains("Refiltered(5)"), "{out}");
    let rows: Vec<Option<Key>> = s.frame(V).rows().iter().map(|i| i.key).collect();
    assert_eq!(
        rows[0],
        Some(subtotal(ABERDEEN)),
        "the Subtotal stands for it"
    );
}

#[test]
fn switching_the_form_is_a_view_edit() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| g.set_form(Axis::Rows, Form::Outline));
    let edits: Vec<ViewEdit> = (s.effects().into_iter())
        .filter_map(|e| match e {
            Effect::ViewRequested { edit, .. } => Some(edit),
            _ => None,
        })
        .collect();
    assert_eq!(
        edits,
        vec![ViewEdit::SetForm {
            axis: Axis::Rows,
            form: Form::Outline
        }]
    );
    let snap = s.frame(V);
    assert!(snap.tiers(Axis::Rows).is_empty());
    assert!(snap.spans(Axis::Rows).is_empty());
    s.input(|g| g.set_form(Axis::Rows, Form::Outline));
    assert!(s.effects().is_empty(), "already in outline form");
}

#[test]
fn the_tree_keys_step_between_level_columns_in_header_form() {
    let mut s = grouped(Expand::to(Depth::Levels(1)), subtotal(ABERDEEN));
    s.input(|g| g.set_focus(subtotal(ABERDEEN), Key(0)));
    s.frame(V);
    press(&mut s, cols(false));
    let mut out = s.step(
        "depth 1, Left from the first column: the port column, on Aberdeen's span",
        V,
    );
    press(&mut s, cols(true));
    out += &s.step("Right on a closed span opens it", V);
    press(&mut s, cols(true));
    out += &s.step("Right on an open span at the last level enters the body", V);
    press(&mut s, cols(false));
    press(&mut s, cols(false));
    out += &s.step(
        "Left back into the band, Left on open Aberdeen closes it",
        V,
    );
    press(&mut s, cols(false));
    out += &s.step("Left on a closed span steps to the region column", V);
    press(&mut s, cols(false));
    out += &s.step("Left on open North closes it", V);
    press(&mut s, Action::Activate);
    out += &s.step("Enter toggles North open", V);
    press(&mut s, cols(true));
    out += &s.step("Right on open North steps to the port column", V);
    press(&mut s, down());
    out += &s.step("Down stays in the port column", V);
    press(&mut s, Action::LeaveOutline);
    out += &s.step("Tab leaves the band for the body", V);
    insta::assert_snapshot!(out);
}

#[test]
fn clicking_a_level_column_focuses_that_tier() {
    let mut s = grouped(Expand::default(), Key(3));
    s.click(5.0, 2.5, Modifiers::NONE);
    let snap = s.frame(V);
    assert!(snap.outline_focused());
    assert_eq!(snap.focused_tier(), 1);
    let rect = snap.focus_rect().expect("drawn");
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (4.0, 1.0, 6.0, 2.0),
        "the focus is drawn on Aberdeen's whole span"
    );
    s.click(1.0, 2.5, Modifiers::NONE);
    assert_eq!(s.frame(V).focused_tier(), 0);
}

#[test]
fn opening_a_span_whose_subtotal_leads_the_view_shows_what_it_opened() {
    let short = Viewport {
        width: 30.0,
        height: 4.0,
    };
    let mut s = grouped(Expand::to(Depth::Levels(1)), subtotal(BERGEN));
    s.frame(short);
    s.input(|g| g.scroll_to_item(Axis::Rows, 1));
    let first = |s: &mut Scenario<TreeTable>| s.frame(short).rows().first().and_then(|i| i.key);
    assert_eq!(first(&mut s), Some(subtotal(BERGEN)));
    s.input(|g| g.expand(Axis::Rows, BERGEN).unwrap());
    s.frame(short);
    assert_eq!(
        first(&mut s),
        Some(Key(4)),
        "Bergen's shipment, above its Subtotal"
    );
    let focus = s.frame(short).focus(Axis::Rows).and_then(|f| f.key);
    assert_eq!(focus, Some(subtotal(BERGEN)), "focus stays on the Subtotal");
}

#[test]
fn closing_a_group_that_leads_the_view_keeps_what_stands_for_it_in_view() {
    let short = Viewport {
        width: 30.0,
        height: 4.0,
    };
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(12.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree()));
    s.frame(short);
    s.input(|g| g.request_view(spec(Expand::default(), Form::Header)));
    s.frame(short);
    // Aberdeen's second shipment leads the view, its first above.
    s.input(|g| g.scroll_to_item(Axis::Rows, 1));
    let first = |s: &mut Scenario<TreeTable>| s.frame(short).rows().first().and_then(|i| i.key);
    assert_eq!(first(&mut s), Some(Key(3)));
    s.input(|g| g.collapse(Axis::Rows, ABERDEEN).unwrap());
    let out = s.step("collapse Aberdeen, scrolled into it: Refiltered", short);
    assert!(out.contains("Refiltered(5)"), "{out}");
    assert_eq!(
        first(&mut s),
        Some(subtotal(ABERDEEN)),
        "the Subtotal standing for Aberdeen leads the view, not Bergen"
    );
}
