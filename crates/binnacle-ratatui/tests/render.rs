//! The adapter's Snapshot renderer, drawn into a `TestBackend`.

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
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
    let (row_source, col_source) = (IdentitySource::new(rows), IdentitySource::new(cols));
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(cols, 8.0).header(5.0),
    );
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = match axis {
            Axis::Rows => row_source.window(range.clone()),
            Axis::Cols => col_source.window(range.clone()),
        };
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
