//! The adapter's Snapshot renderer, drawn into a `TestBackend`.

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, render, wheel};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }
}

fn text(buf: &Buffer) -> String {
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

/// Draws one frame of an identity table into a terminal of `w` x `h` cells,
/// with the Grid placed at `area`.
fn draw(rows: u64, cols: u64, w: u16, h: u16, area: Rect) -> String {
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(cols, 8.0).header(5.0),
    );
    frame(&mut grid, w, h, area)
}

/// Runs one host frame of `grid` over identity sources and draws it.
fn frame(grid: &mut Grid, w: u16, h: u16, area: Rect) -> String {
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let snapshot = grid.snapshot();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| render(&snapshot, &Labels, area, f.buffer_mut()))
        .unwrap();
    text(term.backend().buffer())
}

#[test]
fn draws_headers_and_cells_clipped_to_the_area() {
    insta::assert_snapshot!(draw(100, 10, 30, 5, Rect::new(0, 0, 30, 5)));
}

#[test]
fn draws_at_the_area_offset_and_leaves_the_rest_alone() {
    insta::assert_snapshot!(draw(3, 2, 30, 7, Rect::new(2, 1, 25, 5)));
}

#[test]
fn draws_only_headers_for_an_empty_table() {
    insta::assert_snapshot!(draw(0, 3, 30, 4, Rect::new(0, 0, 30, 4)));
}

#[test]
fn part_scrolled_items_are_clipped_to_the_body() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let area = Rect::new(0, 0, 30, 5);
    grid.action(Action::Scroll { dx: 3.0, dy: 2.0 });
    insta::assert_snapshot!(frame(&mut grid, 30, 5, area));
}

#[test]
fn a_quadrillion_rows_are_exact_at_the_start_middle_and_end() {
    let mut grid = Grid::new(
        AxisConfig::new(1_000_000_000_000_000, 1.0).header(1.0),
        AxisConfig::new(2, 21.0).header(17.0),
    );
    let area = Rect::new(0, 0, 59, 4);
    let mut out = String::new();
    for fraction in [0.0, 0.5, 1.0] {
        grid.action(Action::ScrollTo {
            axis: Axis::Rows,
            fraction,
        });
        out += &format!(
            "--- ScrollTo {fraction}\n{}\n",
            frame(&mut grid, 59, 4, area)
        );
    }
    insta::assert_snapshot!(out);
}

fn mouse(kind: MouseEventKind) -> MouseEvent {
    MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn the_wheel_scrolls_three_cells_and_other_mouse_events_do_not() {
    let scroll = |dx, dy| Some(Action::Scroll { dx, dy });
    assert_eq!(wheel(&mouse(MouseEventKind::ScrollDown)), scroll(0.0, 3.0));
    assert_eq!(wheel(&mouse(MouseEventKind::ScrollUp)), scroll(0.0, -3.0));
    assert_eq!(wheel(&mouse(MouseEventKind::ScrollRight)), scroll(3.0, 0.0));
    assert_eq!(wheel(&mouse(MouseEventKind::ScrollLeft)), scroll(-3.0, 0.0));
    assert_eq!(wheel(&mouse(MouseEventKind::Moved)), None);
}
