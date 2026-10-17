//! Scrollbars: `render_with` draws one down the right edge and, when the
//! columns overflow, one along the bottom; `Mouse::scrollbar` pages and drags
//! them.

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    BarEvent, Content, Layout, Mouse, RenderContext, Theme, begin_frame, render_with,
};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

fn fill(grid: &mut Grid, viewport: Viewport) {
    for (axis, range) in grid.begin_frame(viewport) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
}

fn table(rows: u64, cols: u64) -> Grid {
    Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(cols, 8.0).header(4.0),
    )
}

/// Runs one host frame of `grid` at `area` and draws it into a buffer one
/// cell larger than `area` on every side.
fn draw(grid: &mut Grid, ctx: &RenderContext, area: Rect) -> (Layout, Buffer) {
    let layout = begin_frame(grid, ctx, area, fill);
    let mut buf = Buffer::empty(Rect::new(0, 0, area.right() + 1, area.bottom() + 1));
    render_with(&grid.snapshot(), &Labels, ctx, area, &mut buf);
    (layout, buf)
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

/// The thumb's cells along a scrollbar: the run of `█` in it.
fn thumb(buf: &Buffer, bar: Rect) -> Vec<u16> {
    bar.positions()
        .enumerate()
        .filter(|(_, p)| buf[*p].symbol() == "█")
        .map(|(i, _)| i as u16)
        .collect()
}

fn press(column: u16, row: u16) -> MouseEvent {
    mouse(MouseEventKind::Down(MouseButton::Left), column, row)
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

const AREA: Rect = Rect::new(1, 1, 30, 8);

#[test]
fn a_table_that_overflows_both_ways_draws_both_scrollbars() {
    let mut grid = table(100, 10);
    grid.action(Action::Scroll { dx: 20.0, dy: 30.0 });
    let theme = Theme::classic();
    let (_, buf) = draw(&mut grid, &RenderContext::new(&theme), AREA);
    insta::assert_snapshot!(text(&buf));
}

#[test]
fn the_vertical_scrollbar_takes_the_right_column_and_the_horizontal_one_only_comes_with_overflow() {
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    let (fits, _) = draw(&mut table(100, 2), &ctx, AREA);
    assert_eq!(fits.grid, Rect::new(1, 1, 29, 8));
    assert_eq!(fits.bar(Axis::Rows), Some(Rect::new(30, 2, 1, 7)));
    assert_eq!(fits.bar(Axis::Cols), None);
    let (overflows, _) = draw(&mut table(100, 10), &ctx, AREA);
    assert_eq!(overflows.grid, Rect::new(1, 1, 29, 7));
    assert_eq!(overflows.bar(Axis::Rows), Some(Rect::new(30, 2, 1, 6)));
    assert_eq!(overflows.bar(Axis::Cols), Some(Rect::new(5, 8, 25, 1)));
}

#[test]
fn a_host_can_turn_the_scrollbars_off() {
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme).scrollbars(false);
    let mut grid = table(100, 10);
    let (layout, buf) = draw(&mut grid, &ctx, AREA);
    assert_eq!(layout.grid, AREA);
    assert_eq!(
        (layout.bar(Axis::Rows), layout.bar(Axis::Cols)),
        (None, None)
    );
    assert!(!text(&buf).contains('█'));
}

#[test]
fn clicking_the_track_pages_and_the_thumb_is_left_to_drag() {
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    let mut grid = table(100, 2);
    grid.action(Action::ScrollTo {
        axis: Axis::Rows,
        fraction: 0.5,
    });
    let (layout, _) = draw(&mut grid, &ctx, AREA);
    let mut m = Mouse::default();
    let bar = layout.bar(Axis::Rows).unwrap();
    // The body is 7 lines tall.
    let below = m.scrollbar(&press(bar.x, bar.bottom() - 1), &grid.snapshot(), &layout);
    assert_eq!(
        below,
        BarEvent::Took(Some(Action::Scroll { dx: 0.0, dy: 7.0 }))
    );
    let above = m.scrollbar(&press(bar.x, bar.y), &grid.snapshot(), &layout);
    assert_eq!(
        above,
        BarEvent::Took(Some(Action::Scroll { dx: 0.0, dy: -7.0 }))
    );
    let body = m.scrollbar(&press(bar.x - 1, bar.y), &grid.snapshot(), &layout);
    assert_eq!(body, BarEvent::Missed);
}

#[test]
fn dragging_the_thumb_to_the_end_scrolls_to_the_end_and_a_release_lets_go() {
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    let mut grid = table(100, 2);
    let (layout, _) = draw(&mut grid, &ctx, AREA);
    let bar = layout.bar(Axis::Rows).unwrap();
    let mut m = Mouse::default();
    let snapshot = grid.snapshot();
    assert_eq!(
        m.scrollbar(&press(bar.x, bar.y), &snapshot, &layout),
        BarEvent::Took(None)
    );
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), 3, 40);
    let end = Action::ScrollTo {
        axis: Axis::Rows,
        fraction: 1.0,
    };
    assert_eq!(
        m.scrollbar(&drag, &snapshot, &layout),
        BarEvent::Took(Some(end))
    );
    let up = mouse(MouseEventKind::Up(MouseButton::Left), 3, 40);
    assert_eq!(m.scrollbar(&up, &snapshot, &layout), BarEvent::Took(None));
    assert_eq!(m.scrollbar(&drag, &snapshot, &layout), BarEvent::Missed);
}

/// On any axis, however long, the thumb is one unbroken run of at least one
/// cell inside its track, at the track's start when scrolled to the start and
/// at its end when scrolled to the end.
#[hegel::test(test_cases = 300)]
fn the_thumb_is_one_run_of_at_least_a_cell_on_any_axis(tc: TestCase) {
    let count = |max| tc.draw(gs::integers::<u64>().min_value(0).max_value(max));
    let huge = tc.draw(gs::booleans());
    let rows = if huge {
        count(1_000_000_000_000_000)
    } else {
        count(200)
    };
    let cols = count(40);
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(cols, 6.0).header(3.0),
    );
    for axis in Axis::ALL {
        let fraction = fraction(&tc);
        grid.action(Action::ScrollTo { axis, fraction });
    }
    let dim = |lo, hi| tc.draw(gs::integers::<u16>().min_value(lo).max_value(hi));
    let area = Rect::new(dim(0, 3), dim(0, 3), dim(0, 40), dim(0, 20));
    let theme = Theme::classic();
    let (layout, buf) = draw(&mut grid, &RenderContext::new(&theme), area);
    let snapshot = grid.snapshot();
    for axis in Axis::ALL {
        let Some(bar) = layout.bar(axis) else {
            continue;
        };
        let cells = thumb(&buf, bar);
        let track = bar.width.max(bar.height);
        assert!(!cells.is_empty(), "{axis:?} has no thumb in {bar:?}");
        let (first, last) = (cells[0], cells[cells.len() - 1]);
        assert_eq!(
            usize::from(last - first) + 1,
            cells.len(),
            "{axis:?}: {cells:?}"
        );
        let t = snapshot.thumb(axis);
        if t.fraction == 0.0 {
            assert_eq!(first, 0, "{axis:?} at the start: {cells:?}");
        }
        if t.fraction == 1.0 {
            assert_eq!(last, track - 1, "{axis:?} at the end: {cells:?}");
        }
    }
}

/// Grabbing the thumb anywhere and dragging it to any cell of the track
/// leaves the grabbed cell under the pointer, as far as the track allows, on
/// either axis.
#[hegel::test(test_cases = 300)]
fn a_dragged_thumb_follows_the_pointer(tc: TestCase) {
    let count = |lo, hi| tc.draw(gs::integers::<u64>().min_value(lo).max_value(hi));
    let rows = if tc.draw(gs::booleans()) {
        1_000_000_000_000_000
    } else {
        count(10, 300)
    };
    let mut grid = table(rows, count(5, 60));
    let axis = tc.draw(gs::sampled_from(Axis::ALL.to_vec()));
    grid.action(Action::ScrollTo {
        axis,
        fraction: fraction(&tc),
    });
    let dim = |lo, hi| tc.draw(gs::integers::<u16>().min_value(lo).max_value(hi));
    let area = Rect::new(0, 0, dim(12, 60), dim(4, 30));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    let (layout, buf) = draw(&mut grid, &ctx, area);
    // A short table has no column scrollbar.
    let Some(bar) = layout.bar(axis) else {
        return;
    };
    let length = bar.width.max(bar.height);
    let cells = thumb(&buf, bar);
    let grab = tc.draw(gs::sampled_from(cells.clone()));
    let to = tc.draw(gs::integers::<u16>().min_value(0).max_value(length - 1));
    let at = |i: u16| match axis {
        Axis::Rows => (bar.x, bar.y + i),
        Axis::Cols => (bar.x + i, bar.y),
    };
    let mut m = Mouse::default();
    let snapshot = grid.snapshot();
    let (x, y) = at(grab);
    assert_eq!(
        m.scrollbar(&press(x, y), &snapshot, &layout),
        BarEvent::Took(None)
    );
    let (x, y) = at(to);
    let drag = mouse(MouseEventKind::Drag(MouseButton::Left), x, y);
    let BarEvent::Took(Some(action)) = m.scrollbar(&drag, &snapshot, &layout) else {
        panic!("the drag was not taken");
    };
    grid.action(action);
    let (_, buf) = draw(&mut grid, &ctx, area);
    let moved = thumb(&buf, bar);
    let len = cells.len() as u16;
    let expected = to.saturating_sub(grab - cells[0]).min(length - len);
    assert_eq!(
        moved[0], expected,
        "grabbed at {grab}, dragged to {to}: {moved:?}"
    );
}

/// Any scroll fraction, often exactly an end.
fn fraction(tc: &TestCase) -> f64 {
    if tc.draw(gs::booleans()) {
        tc.draw(gs::sampled_from(vec![0.0, 1.0]))
    } else {
        tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0))
    }
}
