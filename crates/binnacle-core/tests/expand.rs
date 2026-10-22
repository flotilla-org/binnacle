//! Seam 1 scenarios for opening and closing groups: the View Edits that
//! change `expand`, pending disclosure marks, the `Collapsed` and `Expanded`
//! Change Notices, and the tree keys of the outline column.

use binnacle_core::{
    Action, Axis, AxisConfig, Depth, EXCEPT_CAP, Edge, Effect, ExceptCapped, Expand, FocusStep,
    Grid, Grouping, Key, Level, Modifiers, PointerEvent, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 20.0,
    height: 13.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const ABERDEEN: Key = Key(110);
const BERGEN: Key = Key(111);
const CADIZ: Key = Key(120);
const EMPTY: Key = Key(130);

/// Six shipments by region, then port: North holds Aberdeen (0 and 3) and
/// Bergen (4), South holds Cadiz (1, 2 and 5) and an empty port, which
/// can't open. Fully open, that is 12 rows.
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

fn spec(expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// The shipments grouped, opened as `expand` says, focus on `row`.
fn grouped(expand: Expand, row: Key) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree()));
    s.frame(V);
    s.input(|g| g.request_view(spec(expand)));
    s.frame(V);
    s.input(|g| g.set_focus(row, Key(1)));
    s.frame(V);
    s
}

fn requests(effects: &[Effect]) -> Vec<(ViewSpec, ViewEdit)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ViewRequested { spec, edit } => Some((spec.clone(), edit.clone())),
            _ => None,
        })
        .collect()
}

fn rows(s: &mut Scenario<TreeTable>) -> Vec<Option<Key>> {
    s.frame(V).rows().iter().map(|i| i.key).collect()
}

#[test]
fn collapsing_a_group_hides_its_items_and_focus_inside_moves_to_its_group_item() {
    let mut s = grouped(Expand::default(), Key(3));
    let mut out = s.step("everything open, focus on shipment 3 in Aberdeen", V);
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    out += &s.step(
        "collapse North: the source answers Collapsed, focus moves to North",
        V,
    );
    s.input(|g| g.expand(Axis::Rows, NORTH).unwrap());
    out += &s.step("expand North: Expanded, focus stays on North", V);
    insta::assert_snapshot!(out);
}

#[test]
fn toggles_edit_the_exceptions_against_the_default_depth() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    let want = Expand {
        depth: Depth::All,
        except: vec![NORTH],
    };
    let edit = ViewEdit::Collapse {
        axis: Axis::Rows,
        key: NORTH,
    };
    assert_eq!(requests(&s.effects()), vec![(spec(want), edit)]);
    s.frame(V);
    s.input(|g| g.toggle(Axis::Rows, NORTH).unwrap());
    let edit = ViewEdit::Expand {
        axis: Axis::Rows,
        key: NORTH,
    };
    assert_eq!(
        requests(&s.effects()),
        vec![(spec(Expand::default()), edit)]
    );

    let mut s = grouped(Expand::to(Depth::Levels(1)), NORTH);
    s.input(|g| g.expand(Axis::Rows, ABERDEEN).unwrap());
    let want = Expand {
        depth: Depth::Levels(1),
        except: vec![ABERDEEN],
    };
    assert_eq!(requests(&s.effects())[0].0, spec(want));
    s.frame(V);
    s.input(|g| g.expand_to(Axis::Rows, Depth::Levels(0)));
    let edit = ViewEdit::ExpandTo {
        axis: Axis::Rows,
        depth: Depth::Levels(0),
    };
    assert_eq!(
        requests(&s.effects()),
        vec![(spec(Expand::to(Depth::Levels(0))), edit)],
        "ExpandTo sets the depth and clears the exceptions"
    );
    assert_eq!(rows(&mut s), vec![Some(NORTH), Some(SOUTH)]);
}

#[test]
fn opening_an_open_group_or_one_that_cant_open_asks_nothing() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| {
        g.expand(Axis::Rows, NORTH).unwrap();
        g.toggle(Axis::Rows, EMPTY).unwrap();
        g.collapse(Axis::Rows, EMPTY).unwrap();
        g.toggle(Axis::Rows, Key(3)).unwrap();
    });
    assert_eq!(requests(&s.effects()), vec![]);
}

#[test]
fn a_pending_toggle_marks_its_group_until_answered_and_a_second_takes_it_back() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| g.collapse(Axis::Rows, SOUTH).unwrap());
    let mut out = s.step_waiting("collapse South, unanswered: its mark is pending", V);
    s.input(|g| g.toggle(Axis::Rows, SOUTH).unwrap());
    out += &s.step_waiting(
        "toggle South again before the answer: open again, nothing pending",
        V,
    );
    out += &s.step("the answers come: South stays open", V);
    insta::assert_snapshot!(out);
}

#[test]
fn hidden_items_keep_their_sizes_and_their_pins_go_dormant_until_shown() {
    let mut s = grouped(Expand::default(), SOUTH);
    s.input(|g| {
        g.set_size(Axis::Rows, Key(3), 2.0);
        g.pin(Axis::Rows, Edge::Leading, Key(4));
    });
    let drawn = |s: &mut Scenario<TreeTable>| -> Vec<Option<Key>> {
        let snap = s.frame(V);
        snap.pinned(Axis::Rows, Edge::Leading)
            .iter()
            .map(|i| i.key)
            .collect()
    };
    assert_eq!(drawn(&mut s), vec![Some(Key(4))]);
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    assert_eq!(drawn(&mut s), vec![], "the pin is dormant");
    assert_eq!(
        s.grid.pins(Axis::Rows, Edge::Leading).collect::<Vec<_>>(),
        vec![Key(4)],
        "but kept"
    );
    assert_eq!(s.grid.size_override(Axis::Rows, Key(3)), Some(2.0));
    s.input(|g| g.expand(Axis::Rows, NORTH).unwrap());
    assert_eq!(drawn(&mut s), vec![Some(Key(4))], "the pin is back");
    assert_eq!(s.grid.size_override(Axis::Rows, Key(3)), Some(2.0));
}

#[test]
fn range_corners_inside_pull_to_the_group_item_and_picks_inside_drop() {
    let mut s = grouped(Expand::default(), Key(3));
    // From shipment 3 in Aberdeen down to Cadiz.
    s.input(|g| {
        for _ in 0..4 {
            g.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: true,
            });
        }
        g.pointer(PointerEvent::down(3.5, 11.5).modifiers(Modifiers::CTRL));
        g.pointer(PointerEvent::up(3.5, 11.5).modifiers(Modifiers::CTRL));
    });
    let mut out = s.step("a range from shipment 3 to Cadiz, and shipment 5 picked", V);
    s.input(|g| g.collapse(Axis::Rows, NORTH).unwrap());
    out += &s.step(
        "collapse North: the corner on shipment 3 pulls to North, the range now North to Cadiz",
        V,
    );
    s.input(|g| g.collapse(Axis::Rows, SOUTH).unwrap());
    out += &s.step(
        "collapse South: the corner on Cadiz pulls to South, the pick of shipment 5 drops",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn the_scroll_anchor_inside_a_collapsed_group_moves_to_its_group_item() {
    let short = Viewport {
        width: 20.0,
        height: 3.0,
    };
    let mut s = grouped(Expand::default(), Key(3));
    s.frame(short);
    s.input(|g| g.scroll_to_item(Axis::Rows, 8));
    let anchor = s.frame(short).scroll_anchor(Axis::Rows);
    assert_eq!(anchor.position, 8, "on shipment 1, in Cadiz");
    s.input(|g| g.collapse(Axis::Rows, SOUTH).unwrap());
    s.frame(short);
    let anchor = s.frame(short).scroll_anchor(Axis::Rows);
    assert_eq!(anchor.position, 5, "on South");
}

#[test]
fn exceptions_are_capped() {
    let groups: Vec<Group> = (0..EXCEPT_CAP as u64 + 1)
        .map(|i| Group::leaves(10_000 + i, "port", &[i]))
        .collect();
    let grid = Grid::new(
        AxisConfig::new(groups.len() as u64, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![PORT], 3, groups));
    let tall = Viewport {
        width: 20.0,
        height: 2100.0,
    };
    let mut grouping = Grouping::rows_by(PORT);
    grouping.rows.expand = Expand::to(Depth::Levels(0));
    s.frame(tall);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.frame(tall);
    for i in 0..EXCEPT_CAP as u64 {
        s.input(|g| g.expand(Axis::Rows, Key(10_000 + i)).unwrap());
    }
    s.frame(tall);
    let last = Key(10_000 + EXCEPT_CAP as u64);
    let mut refused = Ok(());
    s.input(|g| refused = g.expand(Axis::Rows, last));
    assert_eq!(refused, Err(ExceptCapped { cap: EXCEPT_CAP }));
    assert_eq!(requests(&s.effects()), vec![]);
    s.input(|g| g.collapse(Axis::Rows, Key(10_000)).unwrap());
    assert_eq!(
        requests(&s.effects()).len(),
        1,
        "shrinking is never refused"
    );
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
fn left_from_the_first_column_enters_the_outline_column_and_tab_leaves_it() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| g.set_focus(NORTH, Key(0)));
    s.frame(V);
    assert!(!s.frame(V).outline_focused());
    press(&mut s, cols(false));
    let snap = s.frame(V);
    assert!(snap.outline_focused());
    let rect = snap.focus_rect().expect("drawn");
    assert_eq!((rect.x, rect.width), (0.0, 3.0), "the band cell");
    press(&mut s, down());
    assert!(s.frame(V).outline_focused(), "up and down stay in the band");
    press(&mut s, Action::LeaveOutline);
    let snap = s.frame(V);
    assert!(!snap.outline_focused());
    assert_eq!(snap.focus(Axis::Cols).and_then(|f| f.key), Some(Key(0)));
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(ABERDEEN));
}

#[test]
fn the_tree_keys_walk_and_toggle_groups_in_the_outline_column() {
    let mut s = grouped(Expand::to(Depth::Levels(1)), NORTH);
    s.input(|g| g.set_focus(NORTH, Key(0)));
    s.frame(V);
    press(&mut s, cols(false));
    let mut out = s.step("depth 1, in the outline column on North", V);
    press(&mut s, cols(true));
    out += &s.step("Right on an open group steps to its first child", V);
    press(&mut s, cols(true));
    out += &s.step("Right on a closed group opens it", V);
    press(&mut s, cols(false));
    out += &s.step("Left on an open group closes it", V);
    press(&mut s, cols(false));
    out += &s.step("Left on a closed one steps out to its parent", V);
    press(&mut s, Action::Activate);
    out += &s.step("Enter toggles: North closes", V);
    press(&mut s, Action::TypeText("*".into()));
    out += &s.step(
        "* opens North, then the groups under it as they come into view",
        V,
    );
    press(&mut s, Action::TypeText("0".into()));
    out += &s.step("0 collapses everything", V);
    press(&mut s, Action::TypeText("2".into()));
    out += &s.step("2 opens two levels", V);
    press(&mut s, cols(false));
    press(&mut s, cols(false));
    out += &s.step("Left closes North, Left again at the top stays", V);
    press(&mut s, cols(true));
    press(&mut s, cols(true));
    press(&mut s, cols(true));
    out += &s.step(
        "Right opens North, steps to Aberdeen, then to shipment 0",
        V,
    );
    press(&mut s, cols(true));
    out += &s.step("Right on a Leaf leaves the outline column for the body", V);
    insta::assert_snapshot!(out);
}

#[test]
fn typing_outside_the_outline_column_is_typing() {
    let mut s = grouped(Expand::default(), NORTH);
    s.input(|g| g.action(Action::TypeText("2".into())));
    let effects = s.effects();
    assert!(effects.contains(&Effect::TextTyped { text: "2".into() }));
    assert_eq!(requests(&effects), vec![]);
}

#[test]
fn clicking_a_row_header_in_outline_form_focuses_the_outline_column() {
    let mut s = grouped(Expand::default(), Key(3));
    s.click(1.0, 5.5, Modifiers::NONE);
    let snap = s.frame(V);
    assert!(snap.outline_focused());
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(BERGEN));
    s.click(5.0, 5.5, Modifiers::NONE);
    assert!(!s.frame(V).outline_focused(), "a cell click leaves it");
}
