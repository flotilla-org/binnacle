//! Seam 1 scenarios for the corner grid: where the two Header Bands' tiers
//! cross, each cell's geometry by (column tier, row tier), each tier's
//! level, and the values pseudo-level marked.

use binnacle_core::{
    Axis, AxisConfig, CornerLevel, Effect, Form, Grid, Grouping, Key, Level, Rect, SortDirection,
    Support, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 40.0,
    height: 13.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const QUARTER: Key = Key(3);

fn rows() -> Vec<Group> {
    vec![
        Group::groups(
            100,
            "North",
            vec![
                Group::leaves(110, "Aberdeen", &[0, 3]),
                Group::leaves(111, "Bergen", &[4]),
            ],
        ),
        Group::groups(101, "South", vec![Group::leaves(120, "Cadiz", &[1, 2, 5])]),
    ]
}

/// Two quarters across, each over two value columns.
fn quarters() -> Vec<Group> {
    vec![
        Group::leaves(200, "Q1", &[10, 11]),
        Group::leaves(201, "Q2", &[12, 13]),
    ]
}

fn grouping(form: Form, col_levels: Vec<Level>) -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.form = form;
    grouping.col_levels = col_levels;
    grouping
}

/// A row Header Band 12 wide and a column Header Band `top` high, the rows
/// grouped by region then port in `form` (level columns 4 and 6 wide in
/// header form), and the columns by `col_levels` (tiers 1 high each).
fn scenario(form: Form, col_levels: Vec<Level>, top: f32) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(top),
        AxisConfig::new(4, 5.0).header(12.0),
    );
    let table = TreeTable::new(vec![REGION, PORT], 4, rows())
        .cols_by(vec![Level::field(QUARTER), Level::Values], quarters());
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    s.input(|g| {
        g.set_tiers(Axis::Rows, &[4.0, 6.0]);
        g.set_tiers(Axis::Cols, &[1.0, 1.0]);
        g.request_view(ViewSpec::default().with_grouping(Some(grouping(form, col_levels))));
    });
    s.frame(V);
    s
}

fn level(index: u32, field: Option<Key>) -> CornerLevel {
    CornerLevel { index, field }
}

/// Each tier of `axis` across the corner: its extent and its levels.
fn tiers(s: &mut Scenario<TreeTable>, axis: Axis) -> Vec<(f32, f32, Vec<CornerLevel>)> {
    let snap = s.frame(V);
    (snap.corner().tiers(axis).iter())
        .map(|t| (t.across.start, t.across.end, t.levels.clone()))
        .collect()
}

#[test]
fn a_plain_table_has_one_corner_cell_naming_no_level() {
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(6, 1.0).header(1.0),
            AxisConfig::new(4, 5.0).header(12.0),
        ),
        TreeTable::new(vec![REGION, PORT], 4, rows()),
    );
    assert_eq!(tiers(&mut s, Axis::Rows), vec![(0.0, 12.0, vec![])]);
    assert_eq!(tiers(&mut s, Axis::Cols), vec![(0.0, 1.0, vec![])]);
    let snap = s.frame(V);
    assert_eq!(
        snap.corner().cell(0, 0),
        Some(Rect {
            x: 0.0,
            y: 0.0,
            width: 12.0,
            height: 1.0
        })
    );
    assert_eq!(snap.corner().cell(1, 0), None);
}

#[test]
fn in_outline_form_the_one_band_column_names_every_row_level_outermost_first() {
    let mut s = scenario(Form::Outline, vec![], 1.0);
    assert_eq!(
        tiers(&mut s, Axis::Rows),
        vec![(
            0.0,
            12.0,
            vec![level(0, Some(REGION)), level(1, Some(PORT))]
        )]
    );
    assert_eq!(
        tiers(&mut s, Axis::Cols),
        vec![(0.0, 1.0, vec![])],
        "with no column levels the corner is one row: its top and its bottom"
    );
}

#[test]
fn in_header_form_each_level_column_names_its_level_and_the_row_headers_column_none() {
    let mut s = scenario(Form::Header, vec![], 1.0);
    assert_eq!(
        tiers(&mut s, Axis::Rows),
        vec![
            (0.0, 4.0, vec![level(0, Some(REGION))]),
            (4.0, 10.0, vec![level(1, Some(PORT))]),
            (10.0, 12.0, vec![]),
        ]
    );
    let snap = s.frame(V);
    assert_eq!(
        snap.corner().cell(0, 1),
        Some(Rect {
            x: 4.0,
            y: 0.0,
            width: 6.0,
            height: 1.0
        })
    );
}

#[test]
fn column_tiers_give_the_corner_a_row_each_above_the_column_headers_row() {
    let mut s = scenario(
        Form::Header,
        vec![Level::field(QUARTER), Level::Values],
        3.0,
    );
    assert_eq!(
        tiers(&mut s, Axis::Cols),
        vec![
            (0.0, 1.0, vec![level(0, Some(QUARTER))]),
            (1.0, 2.0, vec![level(1, None)]),
            (2.0, 3.0, vec![]),
        ],
        "the values pseudo-level is marked by having no field"
    );
    let snap = s.frame(V);
    let corner = snap.corner();
    assert!(corner.tiers(Axis::Cols)[1].levels[0].is_values());
    assert_eq!(
        corner.cell(1, 1),
        Some(Rect {
            x: 4.0,
            y: 1.0,
            width: 6.0,
            height: 1.0
        }),
        "the cell where the values tier crosses the port column"
    );
    assert_eq!(
        corner.cell(2, 2),
        Some(Rect {
            x: 10.0,
            y: 2.0,
            width: 2.0,
            height: 1.0
        }),
        "the bottom row's cell over the row headers"
    );
    assert_eq!(corner.cell(3, 0), None);
    assert_eq!(corner.cell(0, 3), None);
    assert_eq!(
        snap.tiers(Axis::Cols),
        &[0.0..1.0, 1.0..2.0],
        "the corner's rows are the column Header Band's tiers"
    );
}

#[test]
fn the_corner_follows_the_bands_as_they_resize() {
    let mut s = scenario(
        Form::Header,
        vec![Level::field(QUARTER), Level::Values],
        3.0,
    );
    s.input(|g| {
        g.set_header(Axis::Cols, 14.0);
        g.set_header(Axis::Rows, 4.0);
    });
    assert_eq!(
        tiers(&mut s, Axis::Rows),
        vec![
            (0.0, 4.0, vec![level(0, Some(REGION))]),
            (4.0, 10.0, vec![level(1, Some(PORT))]),
            (10.0, 14.0, vec![]),
        ]
    );
    assert_eq!(
        tiers(&mut s, Axis::Cols),
        vec![
            (0.0, 1.0, vec![level(0, Some(QUARTER))]),
            (1.0, 2.0, vec![level(1, None)]),
            (2.0, 4.0, vec![]),
        ]
    );
}

#[test]
fn ungrouping_leaves_one_corner_cell_naming_no_level() {
    let mut s = scenario(Form::Header, vec![], 1.0);
    s.input(|g| g.ungroup(Axis::Rows));
    s.frame(V);
    assert_eq!(tiers(&mut s, Axis::Rows), vec![(0.0, 12.0, vec![])]);
}

#[test]
fn sorting_by_a_level_cycles_the_axis_sort_by_its_field_as_a_header_click_does() {
    let mut s = scenario(Form::Outline, vec![], 1.0);
    s.input(|g| {
        g.cycle_sort(Axis::Rows, PORT, false);
    });
    let requested = s.effects().into_iter().find_map(|e| match e {
        Effect::ViewRequested { spec, edit } => Some((spec, edit)),
        _ => None,
    });
    let (spec, edit) = requested.expect("a View Spec request");
    assert_eq!(
        edit,
        ViewEdit::CycleSort {
            axis: Axis::Rows,
            key: PORT,
            multi: false
        }
    );
    assert_eq!(
        spec.rows.sort.find(PORT),
        Some((0, SortDirection::Ascending))
    );
}

#[test]
fn sorting_by_a_field_the_source_cannot_sort_by_asks_nothing() {
    let mut s = scenario(Form::Outline, vec![], 1.0);
    s.input(|g| {
        g.set_support(Axis::Rows, REGION, Support::default().with_sort(false));
        g.cycle_sort(Axis::Rows, REGION, false);
    });
    let effects = s.effects();
    let asked = (effects.iter()).any(|e| matches!(e, Effect::ViewRequested { .. }));
    assert!(!asked, "{effects:?}");
}
