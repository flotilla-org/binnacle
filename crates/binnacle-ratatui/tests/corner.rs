//! The corner grid in the terminal, in the Tiers layout: the caption and
//! the values pseudo-level's name on the top line, each column level's name
//! right-aligned in its tier with an arrow, the row levels' names along the
//! bottom line, and a click on a name finding its level.

use binnacle_core::{
    Axis, AxisConfig, CornerLevel, Form, Grid, Grouping, Key, Level, SortSpec, ViewSpec, Viewport,
};
use binnacle_ratatui::{Content, RenderContext, Theme, corner_level_at, render_with};
use binnacle_testkit::{Group, Scenario, TreeTable};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 44, 6);
const V: Viewport = Viewport {
    width: 44.0,
    height: 6.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const QUARTER: Key = Key(3);

fn rows() -> Vec<Group> {
    vec![Group::groups(
        100,
        "North",
        vec![Group::leaves(110, "Aberdeen", &[0, 1])],
    )]
}

/// Two quarters across, each over two value columns.
fn quarters() -> Vec<Group> {
    vec![
        Group::leaves(200, "Q1", &[10, 11]),
        Group::leaves(201, "Q2", &[12, 13]),
    ]
}

struct Pivot;

impl Content for Pivot {
    fn cell(&self, _: Key, _: Key) -> String {
        "9".into()
    }

    fn column_header(&self, col: Key) -> String {
        match col {
            REGION => "region".into(),
            PORT => "port".into(),
            Key(k) => format!("c{k}"),
        }
    }

    fn row_header(&self, row: Key, position: u64) -> String {
        match row {
            QUARTER => "quarter".into(),
            _ => format!("{}", position + 1),
        }
    }
}

/// Rows by region then port in `form` (level columns 8 and 10 of a 24
/// wide band), columns by `col_levels` (tiers 1 high in a band `top` high),
/// sorted by `sort`.
fn pivot(form: Form, col_levels: Vec<Level>, top: f32, sort: SortSpec) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(2, 1.0).header(top),
        AxisConfig::new(4, 5.0).header(24.0).gap(1.0),
    );
    let table = TreeTable::new(vec![REGION, PORT], 4, rows())
        .cols_by(vec![Level::field(QUARTER), Level::Values], quarters());
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.form = form;
    grouping.col_levels = col_levels;
    let spec = ViewSpec::default()
        .with_grouping(Some(grouping))
        .with_sort(Axis::Rows, sort);
    let mut s = Scenario::new(grid, table);
    s.frame(V);
    s.input(|g| {
        g.set_tiers(Axis::Rows, &[8.0, 10.0]);
        g.set_tiers(Axis::Cols, &[1.0, 1.0]);
        g.request_view(spec);
    });
    s.frame(V);
    s
}

/// The corner's lines as drawn: the band's first `top` lines, its first 24
/// columns.
fn corner(s: &mut Scenario<TreeTable>) -> String {
    let snapshot = s.frame(V);
    let mut buf = Buffer::empty(AREA);
    let theme = Theme::classic();
    render_with(
        &snapshot,
        &Pivot,
        &RenderContext::new(&theme),
        AREA,
        &mut buf,
    );
    let top = snapshot.header(Axis::Rows) as u16;
    (0..top)
        .map(|y| {
            let line: String = (0..24).map(|x| buf[(x, y)].symbol()).collect();
            format!("|{line}|\n")
        })
        .collect()
}

#[test]
fn the_tiers_layout_in_both_forms_with_and_without_column_tiers() {
    let pivoted = vec![Level::field(QUARTER), Level::Values];
    let mut out = String::new();
    for (label, form, col_levels, top) in [
        ("rows in outline form", Form::Outline, vec![], 1.0),
        ("rows in header form", Form::Header, vec![], 1.0),
        (
            "a pivot, rows in outline form",
            Form::Outline,
            pivoted.clone(),
            3.0,
        ),
        (
            "a pivot, rows in header form",
            Form::Header,
            pivoted.clone(),
            3.0,
        ),
    ] {
        let mut s = pivot(form, col_levels, top, SortSpec::unsorted());
        out.push_str(&format!("--- {label}\n{}", corner(&mut s)));
    }
    insta::assert_snapshot!(out);
}

#[test]
fn a_level_its_field_sorts_by_shows_the_sort_mark() {
    let sort = SortSpec::new(vec![binnacle_core::SortColumn::descending(PORT)]);
    let mut s = pivot(Form::Header, vec![], 1.0, sort.clone());
    // The hand-made tree keeps its order, so the Grid hears of the sort as
    // applied straight from the host.
    let applied = s
        .frame(V)
        .applied_view()
        .clone()
        .with_sort(Axis::Rows, sort);
    s.input(|g| g.view_applied(applied, binnacle_core::Totals::unknown()));
    assert_eq!(corner(&mut s), "|region  port ▼    count |\n");
}

#[test]
fn a_click_on_a_name_finds_its_level_and_nowhere_else() {
    let mut s = pivot(
        Form::Header,
        vec![Level::field(QUARTER), Level::Values],
        3.0,
        SortSpec::unsorted(),
    );
    let snapshot = s.frame(V);
    let at = |x, y| corner_level_at(&snapshot, &Pivot, x, y);
    let level = |index, field| CornerLevel { index, field };
    assert_eq!(at(0, 2), Some((Axis::Rows, level(0, Some(REGION)))));
    assert_eq!(at(9, 2), Some((Axis::Rows, level(1, Some(PORT)))));
    assert_eq!(at(14, 2), None, "past the port's name");
    assert_eq!(at(14, 0), Some((Axis::Cols, level(0, Some(QUARTER)))));
    assert_eq!(at(22, 0), None, "the arrow");
    assert_eq!(at(7, 0), Some((Axis::Cols, level(1, None))), "Σ values");
    assert_eq!(at(0, 3), None, "below the corner");
    assert_eq!(at(30, 0), None, "a column header");
}
