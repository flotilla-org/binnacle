//! Dragging headers across the pinned edge: the lit pin zones, the drop
//! line (dashed where an unpinned item returns to its own place) and the
//! status line's caption saying what a drop does.

use binnacle_core::{Axis, AxisConfig, Edge, Grid, Key, PointerEvent};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, RenderContext, Theme, begin_frame, render_with, status};
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

const AREA: Rect = Rect::new(0, 0, 40, 6);

/// 20 rows of 1 under a 1-line header band; 8 columns of 6 with a Gap of 1
/// after a 4-wide row-number band, so column p starts at x `4 + 7p`.
fn grid(movable: bool) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(8, 6.0).header(4.0).gap(1.0),
    );
    grid.set_movable(Axis::Cols, movable);
    grid
}

/// One frame through the adapter's `begin_frame`, drawn as text, then a
/// line marking the lit pin zone columns (`z`, `Z` where hot) and the
/// status line.
fn frame(grid: &mut Grid, label: &str) -> String {
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme).scrollbars(false);
    let layout = begin_frame(grid, &ctx, AREA, |grid, viewport| {
        for (axis, range) in grid.begin_frame(viewport) {
            let keys = IdentitySource::new(range.end).window(range.clone());
            grid.set_window(axis, range.start, &keys);
        }
    });
    let mut buf = Buffer::empty(AREA);
    let snapshot = grid.snapshot();
    render_with(&snapshot, &Labels, &ctx, layout.grid, &mut buf);
    let y = AREA.bottom() - 1;
    let zones: String = (AREA.left()..AREA.right())
        .map(|x| match buf[(x, y)].bg {
            bg if bg == theme.pin_zone_hot.bg.unwrap() => 'Z',
            bg if bg == theme.pin_zone.bg.unwrap() => 'z',
            _ => ' ',
        })
        .collect();
    format!(
        "--- {label}\n{}\nzones  {}\nstatus {}\n",
        text(&buf),
        zones.trim_end(),
        status(&snapshot)
    )
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

#[test]
fn a_column_dragged_into_the_lit_zone_is_pinned_and_dragged_out_returns() {
    let mut g = grid(false);
    let mut out = frame(&mut g, "nothing pinned, columns can't move");
    g.pointer(PointerEvent::down(20.0, 0.0));
    g.pointer(PointerEvent::moved(25.0, 0.0));
    out += &frame(
        &mut g,
        "column 2 over the body: both zones lit, nothing to drop",
    );
    g.pointer(PointerEvent::moved(4.0, 0.0));
    out += &frame(&mut g, "over the leading zone: hot, pin left");
    g.pointer(PointerEvent::up(4.0, 0.0));
    out += &frame(&mut g, "released: column 2 pinned left");
    g.pointer(PointerEvent::down(6.0, 0.0));
    g.pointer(PointerEvent::moved(33.0, 0.0));
    out += &frame(&mut g, "dragged out: returns to its place, dashed");
    g.pointer(PointerEvent::up(33.0, 0.0));
    out += &frame(&mut g, "released: unpinned");
    insta::assert_snapshot!(out);
}

#[test]
fn on_a_movable_axis_a_pin_dragged_out_unpins_and_moves_and_pins_reorder() {
    let mut g = grid(true);
    g.pin(Axis::Cols, Edge::Leading, Key(5));
    g.pin(Axis::Cols, Edge::Leading, Key(6));
    let mut out = frame(&mut g, "columns 5 and 6 pinned left, movable");
    g.pointer(PointerEvent::down(13.0, 0.0));
    g.pointer(PointerEvent::moved(5.0, 0.0));
    out += &frame(&mut g, "column 6 dragged before 5: reorder");
    g.pointer(PointerEvent::moved(30.0, 0.0));
    out += &frame(&mut g, "over the body: unpin and move");
    g.pointer(PointerEvent::moved(39.0, 0.0));
    out += &frame(&mut g, "over the trailing zone: pin right");
    insta::assert_snapshot!(out);
}

#[test]
fn a_row_dragged_to_the_bottom_zone_is_pinned_bottom() {
    let mut g = grid(false);
    frame(&mut g, "");
    g.pointer(PointerEvent::down(1.0, 2.0));
    g.pointer(PointerEvent::moved(1.0, 5.0));
    let out = frame(&mut g, "row 1 over the bottom row: pin bottom");
    assert!(out.contains("status pin row 1 bottom"), "{out}");
    g.pointer(PointerEvent::up(1.0, 5.0));
    let snap = g.snapshot();
    let bottom: Vec<_> = snap
        .pinned(Axis::Rows, Edge::Trailing)
        .iter()
        .map(|i| i.key)
        .collect();
    assert_eq!(bottom, [Some(Key(1))]);
}
