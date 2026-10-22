//! Seam 1 scenarios for Summary Items as the user meets them: selecting a
//! whole group from its Group Item's header, copying what is shown, pinning
//! a Group Item, and the drill-down an activated Summary Item's cell
//! carries.

// Runs of Positions are Vecs of Ranges, often of one.
#![allow(clippy::single_range_in_vec_init)]

use binnacle_core::{
    Action, Axis, AxisConfig, CellRange, Comparison, Condition, DrillDown, Edge, Effect, Expand,
    Grid, Grouping, Key, Level, Modifiers, PointerEvent, Scalar, Value, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 20.0,
    height: 15.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const WEIGHT: Key = Key(3);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const ABERDEEN: Key = Key(110);
const BERGEN: Key = Key(111);
const CADIZ: Key = Key(120);
const EMPTY: Key = Key(130);

/// Six shipments by region, then port. Fully open, the rows are:
///
/// ```text
///  0 North        6 South
///  1   Aberdeen   7   Cadiz
///  2     0        8     1
///  3     3        9     2
///  4   Bergen    10     5
///  5     4       11   Empty
/// ```
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
            vec![
                Group::leaves(CADIZ.0, "Cadiz", &[1, 2, 5]),
                Group::leaves(EMPTY.0, "Empty", &[]),
            ],
        ),
    ]
}

fn grouping(expand: Expand) -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    grouping
}

fn grouped_by(grouping: Grouping) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(4, 4.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 4, tree()));
    s.frame(V);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.frame(V);
    s
}

fn grouped(expand: Expand) -> Scenario<TreeTable> {
    grouped_by(grouping(expand))
}

/// The middle of row `p`'s header, in the outline column.
fn header(p: u64) -> (f32, f32) {
    (1.0, 1.5 + p as f32)
}

/// The middle of the cell at row `p` and column `c`.
fn cell(p: u64, c: u64) -> (f32, f32) {
    (5.0 + 4.0 * c as f32, 1.5 + p as f32)
}

fn click(s: &mut Scenario<TreeTable>, (x, y): (f32, f32), modifiers: Modifiers) {
    s.click(x, y, modifiers);
    s.frame(V);
}

fn range(s: &mut Scenario<TreeTable>) -> Option<CellRange> {
    s.frame(V).selection_range().cloned()
}

fn rows_selected(rows: std::ops::Range<u64>) -> Option<CellRange> {
    Some(CellRange {
        rows: vec![rows],
        cols: vec![0..4],
    })
}

fn focused_row(s: &mut Scenario<TreeTable>) -> Option<Key> {
    s.frame(V).focus(Axis::Rows).and_then(|f| f.key)
}

#[test]
fn clicking_a_group_items_header_selects_the_group_through_its_last_descendant() {
    let mut s = grouped(Expand::default());
    click(&mut s, header(0), Modifiers::NONE);
    assert_eq!(range(&mut s), rows_selected(0..6), "North and all under it");
    assert_eq!(focused_row(&mut s), Some(NORTH));
    click(&mut s, header(7), Modifiers::NONE);
    assert_eq!(range(&mut s), rows_selected(7..11), "Cadiz and its Leaves");
    click(&mut s, header(11), Modifiers::NONE);
    assert_eq!(
        range(&mut s),
        rows_selected(11..12),
        "an empty group is itself"
    );
    click(&mut s, header(3), Modifiers::NONE);
    assert_eq!(range(&mut s), rows_selected(3..4), "a Leaf is its row");
}

#[test]
fn a_closed_group_selects_its_group_item_alone() {
    let mut s = grouped(Expand::default());
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    s.frame(V);
    click(&mut s, header(0), Modifiers::NONE);
    assert_eq!(range(&mut s), rows_selected(0..1));
}

#[test]
fn shift_clicking_a_group_item_extends_by_whole_groups() {
    let mut s = grouped(Expand::default());
    click(&mut s, header(1), Modifiers::NONE);
    assert_eq!(range(&mut s), rows_selected(1..4), "Aberdeen");
    click(&mut s, header(7), Modifiers::SHIFT);
    assert_eq!(range(&mut s), rows_selected(1..11), "on through Cadiz");
    assert_eq!(focused_row(&mut s), Some(CADIZ));

    click(&mut s, header(7), Modifiers::NONE);
    click(&mut s, header(4), Modifiers::SHIFT);
    assert_eq!(
        range(&mut s),
        rows_selected(4..11),
        "back over Bergen, keeping Cadiz whole"
    );
    click(&mut s, header(0), Modifiers::SHIFT);
    assert_eq!(range(&mut s), rows_selected(0..11), "and over North");
}

#[test]
fn a_range_covers_summary_items_between_its_corners() {
    let mut s = grouped(Expand::default());
    click(&mut s, cell(3, 1), Modifiers::NONE);
    click(&mut s, cell(8, 2), Modifiers::SHIFT);
    assert_eq!(
        range(&mut s),
        Some(CellRange {
            rows: vec![3..9],
            cols: vec![1..3],
        })
    );
}

fn copies(s: &mut Scenario<TreeTable>) -> Vec<Effect> {
    s.input(|g| g.action(Action::Copy));
    s.effects()
        .into_iter()
        .filter(|e| matches!(e, Effect::CopyRequested { .. }))
        .collect()
}

fn copy(rows: Vec<std::ops::Range<u64>>, cols: Vec<std::ops::Range<u64>>) -> Effect {
    Effect::CopyRequested {
        rows,
        cols,
        row_labels: true,
        col_labels: false,
    }
}

#[test]
fn copy_lists_what_is_shown_leaving_out_collapsed_groups() {
    let mut s = grouped(Expand::default());
    s.input(|g| g.collapse(Axis::Rows, ABERDEEN).unwrap());
    s.frame(V);
    // North, Aberdeen (closed), Bergen, 4, South, Cadiz, 1, 2, 5, Empty.
    click(&mut s, cell(0, 0), Modifiers::NONE);
    click(&mut s, cell(5, 1), Modifiers::SHIFT);
    assert_eq!(copies(&mut s), vec![copy(vec![0..6], vec![0..2])]);

    s.input(|g| g.action(Action::SelectAll));
    assert_eq!(copies(&mut s), vec![copy(vec![0..10], vec![0..4])]);

    s.input(|g| g.expand(Axis::Rows, ABERDEEN).unwrap());
    s.frame(V);
    assert_eq!(
        copies(&mut s),
        vec![copy(vec![0..12], vec![0..4])],
        "Aberdeen's Leaves are back"
    );
}

#[test]
fn copy_without_a_range_copies_the_focused_cell() {
    let mut s = grouped(Expand::default());
    click(&mut s, cell(4, 2), Modifiers::NONE);
    assert_eq!(copies(&mut s), vec![copy(vec![4..5], vec![2..3])]);
}

#[test]
fn copy_lists_pinned_rows_where_they_are_drawn() {
    let mut s = grouped(Expand::default());
    s.input(|g| g.pin(Axis::Rows, Edge::Leading, CADIZ));
    s.frame(V);
    click(&mut s, cell(3, 0), Modifiers::NONE);
    s.input(|g| g.action(Action::SelectAll));
    assert_eq!(
        copies(&mut s),
        vec![copy(vec![7..8, 0..7, 8..12], vec![0..4])],
        "Cadiz first, as it is drawn"
    );
}

#[test]
fn copy_on_a_plain_table_has_no_labels_and_nothing_without_focus() {
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(6, 1.0).header(1.0),
            AxisConfig::new(4, 4.0).header(3.0),
        ),
        TreeTable::new(vec![REGION, PORT], 4, tree()),
    );
    s.frame(V);
    assert_eq!(copies(&mut s), vec![], "nothing focused");
    click(&mut s, cell(1, 1), Modifiers::NONE);
    click(&mut s, cell(2, 3), Modifiers::SHIFT);
    assert_eq!(
        copies(&mut s),
        vec![Effect::CopyRequested {
            rows: vec![1..3],
            cols: vec![1..4],
            row_labels: false,
            col_labels: false,
        }]
    );
}

#[test]
fn pinning_a_group_item_pins_that_row_alone() {
    let mut s = grouped(Expand::default());
    s.input(|g| g.pin(Axis::Rows, Edge::Leading, NORTH));
    let snap = s.frame(V);
    let pinned: Vec<Option<Key>> = (snap.pinned(Axis::Rows, Edge::Leading).iter())
        .map(|i| i.key)
        .collect();
    assert_eq!(pinned, vec![Some(NORTH)]);
    let scrolling: Vec<Option<Key>> = snap.rows().iter().map(|i| i.key).collect();
    assert_eq!(scrolling.len(), 11, "everything under North still scrolls");
    assert!(scrolling.contains(&Some(ABERDEEN)) && scrolling.contains(&Some(Key(0))));
}

#[test]
fn a_pin_inside_a_collapsed_group_goes_dormant_and_returns() {
    let mut s = grouped(Expand::default());
    s.input(|g| {
        g.pin(Axis::Rows, Edge::Trailing, BERGEN);
        g.pin(Axis::Rows, Edge::Trailing, SOUTH);
    });
    let pinned = |s: &mut Scenario<TreeTable>| -> Vec<Option<Key>> {
        (s.frame(V).pinned(Axis::Rows, Edge::Trailing).iter())
            .map(|i| i.key)
            .collect()
    };
    assert_eq!(pinned(&mut s), vec![Some(BERGEN), Some(SOUTH)]);
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    assert_eq!(pinned(&mut s), vec![Some(SOUTH)], "Bergen's pin is dormant");
    let kept: Vec<Key> = s.grid.pins(Axis::Rows, Edge::Trailing).collect();
    assert_eq!(kept, vec![BERGEN, SOUTH]);
    s.input(|g| g.expand(Axis::Rows, NORTH).unwrap());
    assert_eq!(pinned(&mut s), vec![Some(BERGEN), Some(SOUTH)]);
}

fn activations(s: &mut Scenario<TreeTable>) -> Vec<Effect> {
    s.effects()
        .into_iter()
        .filter(|e| matches!(e, Effect::Activated { .. }))
        .collect()
}

fn is(field: Key, value: &str) -> Condition {
    Condition::one_of(field, vec![Value::Text(value.into())])
}

fn sliced() -> Grouping {
    let mut g = grouping(Expand::default());
    g.slicers = vec![Condition::compare(
        WEIGHT,
        Comparison::Greater(Scalar::Number(100.0)),
    )];
    g
}

#[test]
fn activating_a_group_items_cell_carries_its_path_after_the_slicers() {
    let mut s = grouped_by(sliced());
    let heavy = sliced().slicers;
    s.input(|g| g.set_focus(BERGEN, Key(2)));
    s.frame(V);
    s.input(|g| g.action(Action::Activate));
    let want = |conditions: Vec<Condition>| Effect::Activated {
        row: BERGEN,
        col: Key(2),
        drill_down: Some(DrillDown::new(conditions)),
    };
    let mut path = heavy.clone();
    path.extend([is(REGION, "North"), is(PORT, "Bergen")]);
    assert_eq!(activations(&mut s), vec![want(path)]);

    let (x, y) = cell(6, 1);
    s.input(|g| {
        g.pointer(PointerEvent::down(x, y).clicks(2));
        g.pointer(PointerEvent::up(x, y));
    });
    let mut path = heavy;
    path.push(is(REGION, "South"));
    assert_eq!(
        activations(&mut s),
        vec![Effect::Activated {
            row: SOUTH,
            col: Key(1),
            drill_down: Some(DrillDown::new(path)),
        }],
        "a double-click on South"
    );
}

#[test]
fn activating_a_leaf_or_a_plain_tables_cell_carries_no_drill_down() {
    let mut s = grouped(Expand::default());
    s.input(|g| {
        g.set_focus(Key(3), Key(0));
        g.action(Action::Activate);
    });
    assert_eq!(
        activations(&mut s),
        vec![Effect::Activated {
            row: Key(3),
            col: Key(0),
            drill_down: None,
        }]
    );
    s.input(|g| {
        g.request_view(ViewSpec::default());
        g.set_focus(Key(1), Key(0));
    });
    s.frame(V);
    s.input(|g| g.action(Action::Activate));
    assert!(matches!(
        activations(&mut s).as_slice(),
        [Effect::Activated {
            drill_down: None,
            ..
        }]
    ));
}

#[test]
fn enter_in_the_outline_column_toggles_rather_than_activating() {
    let mut s = grouped(Expand::default());
    click(&mut s, header(4), Modifiers::NONE);
    s.input(|g| g.action(Action::Activate));
    assert_eq!(activations(&mut s), vec![]);
}
