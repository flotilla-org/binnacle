//! The tables example draws a recorded Described Table: titled headers,
//! values through the shared formatter, nested columns hidden, the key and
//! state columns pinned leading.

#[path = "../examples/binnacle-tables/table.rs"]
mod table;

mod support;

use std::time::Duration;

use binnacle_core::{Axis, Edge, Grid, Viewport};
use binnacle_format::Timestamp;
use binnacle_ratatui::{RenderContext, Theme, render_with};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use table::Tables;

const AREA: Rect = Rect::new(0, 0, 200, 12);

/// 2026-10-29T00:00:00Z, the day after the capture.
fn origin() -> Timestamp {
    Timestamp::from_unix_secs(1_793_232_000)
}

fn load(name: &str) -> (Tables, Grid) {
    let mut tables = Tables::load(&support::path(name), origin()).unwrap();
    let grid = tables.grid();
    (tables, grid)
}

fn draw(tables: &mut Tables, grid: &mut Grid) -> Buffer {
    let viewport = Viewport::new(AREA.width.into(), AREA.height.into());
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme)
        .scrollbars(false)
        .at(Duration::ZERO);
    tables.fill(grid, viewport);
    tables.fill(grid, viewport);
    let mut buf = Buffer::empty(AREA);
    render_with(&grid.snapshot(), &*tables, &ctx, AREA, &mut buf);
    buf
}

fn lines(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_vessels_watch_draws_titled_headers_and_pins_the_key_and_state_columns() {
    let (mut tables, mut grid) = load("watch-vessels.ndjson");
    let buf = draw(&mut tables, &mut grid);
    let source = &tables.source;
    let pins: Vec<_> = grid.snapshot().pins(Axis::Cols, Edge::Leading).collect();
    let name = source.column_key("metadata.name").unwrap();
    let phase = source.column_key("status.phase").unwrap();
    assert_eq!(pins, [name, phase]);
    let labels = source.column_key("metadata.labels").unwrap();
    assert_eq!(
        grid.snapshot().applied_view().axis(Axis::Cols).hidden,
        [labels],
        "nested columns start hidden"
    );
    assert_eq!(tables.status(), "10 rows");
    insta::assert_snapshot!(lines(&buf));
}

#[test]
fn the_hosts_rows_draw_a_null_as_a_dim_glyph() {
    let (mut tables, mut grid) = load("rows-hosts.ndjson");
    let buf = draw(&mut tables, &mut grid);
    let text = lines(&buf);
    let (y, line) = text.lines().enumerate().nth(1).unwrap();
    let x = line.chars().position(|c| c == '∅').expect("a null");
    assert_eq!(
        buf[(x as u16, y as u16)].style().fg,
        Theme::classic().dim.fg
    );
    insta::assert_snapshot!(text);
}

#[test]
fn sorting_a_time_column_ascending_puts_its_nulls_first() {
    let (mut tables, mut grid) = load("crews.ndjson");
    draw(&mut tables, &mut grid);
    let started = tables.source.column_key("started").unwrap();
    grid.cycle_sort(Axis::Rows, started, false);
    let buf = draw(&mut tables, &mut grid);
    assert_eq!(tables.status(), "6 rows, 3 invalid");
    insta::assert_snapshot!(lines(&buf));
}

#[test]
fn a_table_over_the_cap_says_so() {
    let (tables, _) = load("rows-vessels-page1.ndjson");
    assert_eq!(tables.status(), "5 of more rows");
}

#[test]
fn a_summary_row_draws_blank_where_it_has_no_figure() {
    let (mut tables, mut grid) = load("crews.ndjson");
    draw(&mut tables, &mut grid);
    let phase = tables.source.column_key("phase").unwrap();
    let spec = binnacle_core::ViewSpec::default()
        .with_hidden(
            Axis::Cols,
            grid.snapshot()
                .applied_view()
                .axis(Axis::Cols)
                .hidden
                .clone(),
        )
        .with_grouping(Some(binnacle_core::Grouping::rows_by(phase)));
    grid.request_view(spec);
    let text = lines(&draw(&mut tables, &mut grid));
    let groups: Vec<&str> = (text.lines()).filter(|l| l.starts_with('▾')).collect();
    assert_eq!(groups.len(), 4, "{text}");
    assert!(groups.iter().all(|l| !l.contains('∅')), "{text}");
    insta::assert_snapshot!(text);
}
