//! Seam 1 scenarios for opening and closing column groups: the same
//! `expand` state, View Edits and Change Notices as rows, a closed column
//! span showing its total column under it, and the tree keys in the column
//! Header Band's tiers.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Expand, FocusStep, Grid, Grouping, Key, Level,
    OpenState, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 60.0,
    height: 8.0,
};

const QUARTER: Key = Key(3);
const Q1: Key = Key(200);
const Q2: Key = Key(201);
const Q3: Key = Key(202);

/// Quarters over their months: Q1 holds columns 10 and 11, Q2 12 and 13,
/// Q3 14 alone.
fn tree() -> Vec<Group> {
    vec![
        Group::leaves(Q1.0, "Q1", &[10, 11]),
        Group::leaves(Q2.0, "Q2", &[12, 13]),
        Group::leaves(Q3.0, "Q3", &[14]),
    ]
}

fn total(group: Key) -> Key {
    TreeTable::subtotal(group)
}

fn spec(expand: Expand) -> ViewSpec {
    let mut grouping = Grouping::default();
    grouping.col_levels = vec![Level::field(QUARTER)];
    grouping.cols.expand = expand;
    ViewSpec::default().with_grouping(Some(grouping))
}

/// Six plain rows; columns by quarter with a total column after each open
/// quarter, 5 wide after a 4-wide row band; a column band 2 high, the
/// quarters' tier 1 high. Focus on row 0 and `col`.
fn scenario(expand: Expand, col: Key) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(2.0),
        AxisConfig::new(0, 5.0).header(4.0),
    );
    let table = TreeTable::by_cols(6, vec![Level::field(QUARTER)], tree()).col_subtotals_after();
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    s.input(|g| {
        g.set_tiers(Axis::Cols, &[1.0]);
        g.request_view(spec(expand));
    });
    s.frame(V);
    s.input(|g| g.set_focus(Key(0), col));
    s.frame(V);
    s
}

fn columns(s: &mut Scenario<TreeTable>) -> Vec<Option<Key>> {
    s.frame(V).cols().iter().map(|i| i.key).collect()
}

fn press(s: &mut Scenario<TreeTable>, action: Action) {
    s.input(|g| g.action(action));
    s.frame(V);
}

fn step(axis: Axis, forward: bool) -> Action {
    Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward,
        extend: false,
    }
}

#[test]
fn collapsing_a_quarter_shows_its_total_column_under_its_span() {
    let mut s = scenario(Expand::default(), Key(11));
    s.input(|g| g.collapse(Axis::Cols, Q1).unwrap());
    let out = s.step("collapse Q1", V);
    assert!(
        out.contains(&format!("{:?}", ChangeNotice::Collapsed { at: 0, n: 2 })),
        "{out}"
    );
    assert_eq!(
        columns(&mut s),
        [total(Q1), Key(12), Key(13), total(Q2), Key(14), total(Q3)].map(Some)
    );
    let snap = s.frame(V);
    let q1 = (snap.spans(Axis::Cols).iter())
        .find(|sp| sp.key == Q1)
        .expect("Q1 keeps its span");
    assert_eq!((q1.first, q1.last, q1.open), (0, 0, OpenState::Closed));
    assert_eq!(
        snap.focus(Axis::Cols).and_then(|f| f.key),
        Some(total(Q1)),
        "focus inside moves to the total column"
    );
    s.input(|g| g.expand(Axis::Cols, Q1).unwrap());
    let out = s.step("expand Q1", V);
    assert!(
        out.contains(&format!("{:?}", ChangeNotice::Expanded { at: 0, n: 2 })),
        "{out}"
    );
    assert_eq!(
        columns(&mut s)[..3],
        [Key(10), Key(11), total(Q1)].map(Some)
    );
}

const YEAR: Key = Key(6);
const Y2024: Key = Key(300);
const Y2025: Key = Key(301);

/// Years over quarters over their months: 2024 holds Q1 (10, 11) and Q2
/// (12, 13), 2025 holds Q3 (14).
fn years() -> Vec<Group> {
    vec![
        Group::groups(
            Y2024.0,
            "2024",
            vec![
                Group::leaves(Q1.0, "Q1", &[10, 11]),
                Group::leaves(Q2.0, "Q2", &[12, 13]),
            ],
        ),
        Group::groups(Y2025.0, "2025", vec![Group::leaves(Q3.0, "Q3", &[14])]),
    ]
}

/// Six plain rows; columns by year then quarter, a total column after
/// each open group; a column band 3 high, each tier 1. Focus on row 0 and
/// `col`.
fn by_years(col: Key) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(3.0),
        AxisConfig::new(0, 5.0).header(4.0),
    );
    let levels = vec![Level::field(YEAR), Level::field(QUARTER)];
    let table = TreeTable::by_cols(6, levels.clone(), years()).col_subtotals_after();
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    let mut grouping = Grouping::default();
    grouping.col_levels = levels;
    s.input(|g| {
        g.set_tiers(Axis::Cols, &[1.0, 1.0]);
        g.request_view(ViewSpec::default().with_grouping(Some(grouping)));
    });
    s.frame(V);
    s.input(|g| g.set_focus(Key(0), col));
    s.frame(V);
    s
}

/// Where focus is: in the column tiers (and which), else in the body; and
/// the focused column.
fn focus(s: &mut Scenario<TreeTable>) -> (Option<u32>, Option<Key>) {
    let snap = s.frame(V);
    let tier = snap.band_focused(Axis::Cols).then(|| snap.focused_tier());
    (tier, snap.focus(Axis::Cols).and_then(|f| f.key))
}

/// Whether the View Spec the Table Source applied opens the column group
/// `key`, at `level`.
fn opens(s: &mut Scenario<TreeTable>, level: u32, key: Key) -> bool {
    let snap = s.frame(V);
    let spec = snap.applied_view();
    spec.grouping
        .as_ref()
        .unwrap()
        .cols
        .expand
        .opens(level, key)
}

#[test]
fn up_from_the_first_row_enters_the_column_tiers_where_the_tree_keys_act_on_the_columns() {
    let mut s = by_years(Key(11));
    press(&mut s, step(Axis::Rows, false));
    assert_eq!(
        focus(&mut s),
        (Some(1), Some(Key(11))),
        "the innermost tier"
    );
    let rect = s.frame(V).focus_rect().expect("drawn");
    assert_eq!(
        (rect.x, rect.y, rect.width, rect.height),
        (4.0, 1.0, 10.0, 1.0),
        "the focus is drawn on Q1's span over its months"
    );
    assert!(!s.frame(V).band_focused(Axis::Rows));

    press(&mut s, step(Axis::Cols, false));
    assert!(!opens(&mut s, 1, Q1), "Left on open Q1 closes it");
    assert_eq!(focus(&mut s), (Some(1), Some(total(Q1))));
    press(&mut s, step(Axis::Cols, true));
    assert!(opens(&mut s, 1, Q1), "Right on closed Q1 opens it");
    press(&mut s, step(Axis::Cols, false));
    press(&mut s, step(Axis::Cols, false));
    assert_eq!(
        focus(&mut s),
        (Some(0), Some(total(Q1))),
        "Left on closed Q1 steps a tier out"
    );
    press(&mut s, Action::Activate);
    assert!(!opens(&mut s, 0, Y2024), "Enter toggles 2024");
    assert_eq!(focus(&mut s), (Some(0), Some(total(Y2024))));
    press(&mut s, Action::TypeText("*".into()));
    assert!(
        opens(&mut s, 0, Y2024) && opens(&mut s, 1, Q1),
        "* opens everything under 2024"
    );
    press(&mut s, step(Axis::Cols, true));
    assert_eq!(
        focus(&mut s).0,
        Some(1),
        "Right on open 2024 steps a tier in"
    );
    press(&mut s, Action::TypeText("0".into()));
    let cols: Vec<Option<Key>> = s.frame(V).cols().iter().map(|i| i.key).collect();
    assert_eq!(
        cols,
        [total(Y2024), total(Y2025)].map(Some),
        "0 closes every column group"
    );
    press(&mut s, Action::TypeText("2".into()));
    assert_eq!(s.frame(V).cols().len(), 10, "2 opens both levels");
    press(&mut s, step(Axis::Rows, false));
    assert_eq!(focus(&mut s).0, Some(0), "Up steps a tier out");
    press(&mut s, step(Axis::Rows, false));
    assert_eq!(focus(&mut s).0, Some(0), "and stops at the outermost");
    press(&mut s, step(Axis::Rows, true));
    assert_eq!(focus(&mut s).0, None, "Down leaves for the body");
    assert_eq!(
        s.frame(V).focus(Axis::Rows).and_then(|f| f.key),
        Some(Key(0))
    );
}

#[test]
fn up_from_a_row_past_the_first_moves_up_and_plain_columns_have_no_tiers() {
    let mut s = by_years(Key(11));
    s.input(|g| g.set_focus(Key(2), Key(11)));
    press(&mut s, step(Axis::Rows, false));
    assert_eq!(focus(&mut s).0, None);
    assert_eq!(
        s.frame(V).focus(Axis::Rows).and_then(|f| f.key),
        Some(Key(1))
    );
    s.input(|g| g.request_view(ViewSpec::default()));
    s.frame(V);
    s.input(|g| g.set_focus(Key(0), Key(1)));
    press(&mut s, step(Axis::Rows, false));
    assert_eq!(focus(&mut s).0, None, "ungrouped columns have no tiers");
}

#[test]
fn tab_leaves_the_column_tiers_and_a_body_click_too() {
    let mut s = by_years(Key(11));
    press(&mut s, step(Axis::Rows, false));
    press(&mut s, Action::LeaveOutline);
    assert_eq!(focus(&mut s), (None, Some(Key(11))));
    press(&mut s, step(Axis::Rows, false));
    s.click(30.0, 5.5, binnacle_core::Modifiers::NONE);
    assert_eq!(focus(&mut s).0, None);
}

#[test]
fn clicking_a_column_span_focuses_its_tier() {
    let mut s = by_years(Key(11));
    s.click(6.0, 0.5, binnacle_core::Modifiers::NONE);
    let snap = s.frame(V);
    assert!(snap.band_focused(Axis::Cols));
    assert_eq!(snap.focused_tier(), 0);
}
