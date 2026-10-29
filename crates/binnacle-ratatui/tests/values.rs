//! The Value Renderer: the values example's table drawn through the shared
//! formatter, its style hints through the Theme, its relative times moving
//! on with the frame's time.

#[path = "../src/bin/binnacle-values/table.rs"]
mod table;

use std::time::Duration;

use binnacle_core::{Grid, Key, Viewport};
use binnacle_ratatui::{RenderContext, Theme, render_with};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

use table::Values;

const AREA: Rect = Rect::new(0, 0, 230, 9);

fn draw(values: &Values, grid: &mut Grid, theme: &Theme, now: Duration) -> Buffer {
    let viewport = Viewport::new(AREA.width.into(), AREA.height.into());
    values.fill(grid, viewport);
    let mut buf = Buffer::empty(AREA);
    let ctx = RenderContext::new(theme).scrollbars(false).at(now);
    render_with(&grid.snapshot(), values, &ctx, AREA, &mut buf);
    buf
}

fn lines(buf: &Buffer) -> Vec<String> {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect()
}

/// The buffer cells where `text` starts on line `y`.
fn find(buf: &Buffer, y: u16, text: &str) -> u16 {
    let line: Vec<String> = (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect();
    let chars: Vec<String> = text.chars().map(String::from).collect();
    (0..line.len())
        .find(|&x| line[x..].starts_with(&chars))
        .unwrap_or_else(|| panic!("no {text:?} on line {y}: {}", line.concat())) as u16
}

/// The body line of fixture row `row`, under the header band.
fn line(row: u16) -> u16 {
    1 + row
}

#[test]
fn the_values_example_draws_every_type_through_the_formatter() {
    let values = Values::new();
    let mut grid = values.grid();
    let buf = draw(&values, &mut grid, &Theme::classic(), Duration::ZERO);
    insta::assert_snapshot!(lines(&buf).join("\n"));
}

#[test]
fn nulls_draw_dim_and_invalid_values_in_the_warning_style_after_a_mark() {
    let values = Values::new();
    let mut grid = values.grid();
    let theme = Theme::classic();
    let buf = draw(&values, &mut grid, &theme, Duration::ZERO);
    let null = find(&buf, line(2), "∅");
    assert_eq!(buf[(null, line(2))].fg, theme.dim.fg.unwrap());
    let invalid = find(&buf, line(3), "!12x");
    for x in invalid..invalid + 4 {
        assert_eq!(buf[(x, line(3))].fg, theme.warning.fg.unwrap());
    }
    // Plain text and empty text take neither.
    let text = find(&buf, line(0), "Aberdeen");
    assert_eq!(buf[(text, line(0))].fg, Color::Reset);

    let mono = Theme::mono();
    let buf = draw(&values, &mut grid, &mono, Duration::ZERO);
    assert!(
        buf[(find(&buf, line(2), "∅"), line(2))]
            .modifier
            .contains(Modifier::DIM)
    );
}

#[test]
fn numbers_end_aligned_line_up_on_their_decimals() {
    let values = Values::new();
    let mut grid = values.grid();
    let buf = draw(&values, &mut grid, &Theme::classic(), Duration::ZERO);
    let point = |row, text: &str| find(&buf, line(row), text) + text.find('.').unwrap() as u16;
    assert_eq!(point(0, "7.000"), point(6, "1234.567"));
    assert_eq!(point(1, "12.500"), point(2, "-0.250"));
    // Text starts where its column does.
    assert_eq!(
        find(&buf, line(0), "Aberdeen"),
        find(&buf, line(4), "Funchal")
    );
}

#[test]
fn relative_times_move_on_with_the_frames_time() {
    let values = Values::new();
    let mut grid = values.grid();
    let theme = Theme::classic();
    // Row 0 was seen 30 s before the fixture's time.
    let mut at = |secs| {
        lines(&draw(&values, &mut grid, &theme, Duration::from_secs(secs)))[line(0) as usize]
            .clone()
    };
    assert!(at(0).contains("just now"));
    assert!(at(29).contains("just now"));
    assert!(at(30).contains("1m ago"));
    assert!(at(90).contains("2m ago"));
}

#[test]
fn a_value_renderer_wants_its_formatted_width() {
    use binnacle_format::{Value, ValueType};
    use binnacle_ratatui::{CellRenderer, ValueRenderer};
    let renderer = ValueRenderer::new(|_, _| Value::Invalid {
        raw: "12x".into(),
        expected: ValueType::Int,
    });
    assert_eq!(renderer.preferred_width(Key(0), Key(0)), 4);
    let renderer = ValueRenderer::new(|_, _| Value::Int(1536)).unit("By");
    assert_eq!(
        renderer.preferred_width(Key(0), Key(0)),
        "1.5 KiB".len() as u32
    );
}
