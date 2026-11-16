//! The row Header Band keeps a blank cell at its end, so a row's label
//! never runs into the first cell: a grouped table's two-digit numbers at
//! depth 1, and an ungrouped table's numbers that fill a narrow band, cut
//! with an ellipsis. `row_band_fit` sizes the band to the widest label with
//! its indent and mark, and that cell.

use binnacle_core::{
    Axis, AxisConfig, Grid, Key, OpenState, Outline, Support, Value, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, RenderContext, Theme, render_with, row_band_fit};
use binnacle_testkit::{GroupedTable, Scenario};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const REGION: Key = Key(1);
const NORTH: Key = Key(1 << 63);

struct Plain;

impl Content for Plain {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "region"][col.0 as usize].into()
    }

    fn row_header(&self, _: Key, position: u64) -> String {
        (position + 1).to_string()
    }

    fn group_label(&self, _: Key, _: &Outline) -> String {
        "N".into()
    }
}

/// `rows` leaves under one open group, in a band `band` cells wide.
fn grouped(rows: u64, band: f32) -> Scenario<GroupedTable> {
    let mut items = vec![(
        NORTH,
        Outline::group(vec![], OpenState::Open, REGION, Value::Text("N".into())),
    )];
    items.extend((0..rows).map(|i| (Key(i), Outline::leaf(vec![NORTH]))));
    let grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(2, 6.0).header(band),
    );
    let mut s = Scenario::new(
        grid,
        GroupedTable::new((0..rows).map(Key).collect(), 2, REGION, items),
    );
    s.input(|g| g.set_support(Axis::Rows, REGION, Support::default().with_group(true)));
    s.frame(viewport(rows));
    s.input(|g| {
        g.request_view(
            ViewSpec::default().with_grouping(Some(binnacle_core::Grouping::rows_by(REGION))),
        )
    });
    s.frame(viewport(rows));
    s
}

fn viewport(rows: u64) -> Viewport {
    Viewport {
        width: 30.0,
        height: rows as f32 + 3.0,
    }
}

fn draw(s: &mut Scenario<GroupedTable>, rows: u64) -> String {
    s.frame(viewport(rows));
    let area = Rect::new(0, 0, 30, rows as u16 + 3);
    let mut buf = Buffer::empty(area);
    let theme = Theme::classic();
    let snapshot = s.grid.snapshot();
    render_with(
        &snapshot,
        &Plain,
        &RenderContext::new(&theme),
        area,
        &mut buf,
    );
    (0..area.height)
        .map(|y| {
            let line: String = (0..area.width).map(|x| buf[(x, y)].symbol()).collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_grouped_row_number_of_two_digits_keeps_a_blank_cell_before_the_first_cell() {
    let mut s = grouped(12, 7.0);
    let text = draw(&mut s, 12);
    let ten = text.lines().find(|l| l.contains("10")).unwrap();
    assert!(ten.starts_with("    10 r"), "{text}");
    insta::assert_snapshot!(text);
}

#[test]
fn a_band_too_narrow_for_the_label_cuts_it_with_an_ellipsis_short_of_the_cell() {
    let mut s = grouped(12, 6.0);
    let text = draw(&mut s, 12);
    let ten = text.lines().find(|l| l.contains("r9c0")).unwrap();
    assert!(ten.starts_with("    … r"), "{text}");
}

#[test]
fn the_band_fits_the_widest_label_with_its_indent_mark_and_a_blank_cell() {
    let fit = |rows| {
        let mut s = grouped(rows, 20.0);
        s.frame(viewport(rows));
        row_band_fit(&s.grid.snapshot(), &Plain)
    };
    // Two of indent, two of mark, the digits, one blank; the group is the
    // first row, so the leaves number from 2.
    assert_eq!(fit(8), Some(6.0));
    assert_eq!(fit(9), Some(7.0));
}

#[test]
fn an_ungrouped_number_that_fills_the_band_is_cut_with_an_ellipsis_never_touching() {
    let mut grid = Grid::new(
        AxisConfig::new(12, 1.0).header(1.0),
        AxisConfig::new(2, 6.0).header(2.0),
    );
    let area = Rect::new(0, 0, 30, 15);
    for (axis, range) in grid.begin_frame(Viewport::new(30.0, 15.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let mut buf = Buffer::empty(area);
    let theme = Theme::classic();
    render_with(
        &grid.snapshot(),
        &Plain,
        &RenderContext::new(&theme),
        area,
        &mut buf,
    );
    let line = |y: u16| -> String { (0..area.width).map(|x| buf[(x, y)].symbol()).collect() };
    // A band of 2: one cell for the number, then the blank.
    assert!(line(9).starts_with("9 r"), "{}", line(9));
    assert!(line(10).starts_with("… r"), "{}", line(10));
    assert!(line(12).starts_with("… r"), "{}", line(12));
}
