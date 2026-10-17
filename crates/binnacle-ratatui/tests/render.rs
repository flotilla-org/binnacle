//! The adapter's Snapshot renderer, drawn into a `TestBackend`.

use binnacle_core::{
    Action, Axis, AxisConfig, FocusStep, Grid, Key, SortColumn, SortSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    Content, RenderContext, Theme, render, render_editor, render_with, status, wheel,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

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
    text(&frame_buffer(grid, w, h, area))
}

fn frame_buffer(grid: &mut Grid, w: u16, h: u16, area: Rect) -> Buffer {
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let snapshot = grid.snapshot();
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| render(&snapshot, &Labels, area, f.buffer_mut()))
        .unwrap();
    term.backend().buffer().clone()
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

#[test]
fn the_focused_cell_is_drawn_reversed_across_its_width() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let down = Action::MoveFocus {
        axis: Axis::Rows,
        by: FocusStep::Item,
        forward: true,
        extend: false,
    };
    grid.action(down.clone());
    grid.action(down);
    let buf = frame_buffer(&mut grid, 30, 5, Rect::new(0, 0, 30, 5));
    let reversed: Vec<(u16, u16)> = buf
        .area
        .positions()
        .filter(|&p| buf[p].modifier.contains(Modifier::REVERSED))
        .map(|p| (p.x, p.y))
        .collect();
    // Row 1 sits on line 2, under the header line; column 0 spans x 5..13.
    assert_eq!(reversed, (5..13).map(|x| (x, 2)).collect::<Vec<_>>());
    assert!(text(&buf).lines().nth(2).unwrap().starts_with("2    r1c0"));
}

#[test]
fn selected_cells_get_a_background_across_their_width_and_the_hovered_header_is_underlined() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let extend = |axis| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward: true,
        extend: true,
    };
    grid.action(extend(Axis::Rows));
    grid.action(extend(Axis::Rows));
    grid.action(extend(Axis::Cols));
    // Hover column 2's header (x 23..31).
    grid.pointer(binnacle_core::PointerEvent::moved(24.0, 0.0));
    let buf = frame_buffer(&mut grid, 30, 5, Rect::new(0, 0, 30, 5));
    let with = |f: &dyn Fn(&ratatui::buffer::Cell) -> bool| -> Vec<(u16, u16)> {
        buf.area
            .positions()
            .filter(|&p| f(&buf[p]))
            .map(|p| (p.x, p.y))
            .collect()
    };
    // Rows 0 and 1 (lines 1 and 2), columns 0 and 1 (x 5..13 and 14..22);
    // the focused cell, row 1 column 1, is drawn reversed instead.
    let selected: Vec<(u16, u16)> = [(5..13, 1), (14..22, 1), (5..13, 2)]
        .into_iter()
        .flat_map(|(xs, y)| xs.map(move |x| (x, y)))
        .collect();
    assert_eq!(with(&|c| c.bg == Color::DarkGray), selected);
    let focused: Vec<(u16, u16)> = (14..22).map(|x| (x, 2)).collect();
    assert_eq!(with(&|c| c.modifier.contains(Modifier::REVERSED)), focused);
    // The scrollbar takes the last column.
    let hovered: Vec<(u16, u16)> = (23..29).map(|x| (x, 0)).collect();
    assert_eq!(
        with(&|c| c.modifier.contains(Modifier::UNDERLINED)),
        hovered
    );
}

#[test]
fn column_headers_show_the_applied_sort_and_pending_requests() {
    let mut grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(4, 8.0).header(5.0).gap(1.0),
    );
    let area = Rect::new(0, 0, 41, 2);
    let mut out = String::new();
    grid.sort_applied(SortSpec::new(vec![SortColumn::descending(Key(1))]));
    out += &format!(
        "--- column 1 descending\n{}\n",
        frame(&mut grid, 41, 2, area)
    );
    grid.sort_applied(SortSpec::new(vec![
        SortColumn::descending(Key(1)),
        SortColumn::ascending(Key(3)),
    ]));
    out += &format!(
        "--- then column 3 ascending\n{}\n",
        frame(&mut grid, 41, 2, area)
    );
    // Click column 0's header with Shift: a request, not yet answered.
    let shift = binnacle_core::Modifiers::SHIFT;
    grid.pointer(binnacle_core::PointerEvent::down(6.0, 0.0).modifiers(shift));
    grid.pointer(binnacle_core::PointerEvent::up(6.0, 0.0).modifiers(shift));
    out += &format!(
        "--- shift-click column 0, unanswered\n{}\n",
        frame(&mut grid, 41, 2, area)
    );
    insta::assert_snapshot!(out);
}

#[test]
fn column_header_gaps_are_drawn_as_handles_and_a_resize_draws_its_guide() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let area = Rect::new(0, 0, 32, 4);
    let mut out = format!("--- idle\n{}\n", frame(&mut grid, 32, 4, area));
    // Column 0 spans x 5..13; its border is the Gap at x 13.
    grid.pointer(binnacle_core::PointerEvent::down(13.0, 0.0));
    grid.pointer(binnacle_core::PointerEvent::moved(16.0, 0.0));
    out += &format!("--- dragged 3 right\n{}\n", frame(&mut grid, 32, 4, area));
    grid.pointer(binnacle_core::PointerEvent::up(16.0, 0.0));
    out += &format!("--- released\n{}\n", frame(&mut grid, 32, 4, area));
    insta::assert_snapshot!(out);
}

#[test]
fn a_column_move_draws_its_drop_indicator() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let area = Rect::new(0, 0, 32, 4);
    frame(&mut grid, 32, 4, area);
    // Column 0 spans x 5..13 and column 1 x 14..22; dropping past column 1's
    // middle puts the indicator on the Gap at x 22.
    grid.pointer(binnacle_core::PointerEvent::down(8.0, 0.0));
    grid.pointer(binnacle_core::PointerEvent::moved(19.0, 0.0));
    let out = frame(&mut grid, 32, 4, area);
    insta::assert_snapshot!(out);
}

#[test]
fn the_status_line_names_the_mode_and_hints_at_borders() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let mut status_after = |events: &[binnacle_core::PointerEvent]| {
        for &e in events {
            grid.pointer(e);
        }
        for (axis, range) in grid.begin_frame(Viewport::new(30.0, 8.0)) {
            let keys = IdentitySource::new(range.end).window(range.clone());
            grid.set_window(axis, range.start, &keys);
        }
        status(&grid.snapshot())
    };
    use binnacle_core::PointerEvent as P;
    assert_eq!(status_after(&[]), "");
    assert_eq!(
        status_after(&[P::moved(13.0, 0.0)]),
        "drag to resize the column, double-click to fit"
    );
    assert_eq!(
        status_after(&[P::moved(1.0, 2.0)]),
        "drag to resize the row, double-click to fit"
    );
    assert_eq!(
        status_after(&[P::down(13.0, 0.0), P::moved(15.0, 0.0)]),
        "resizing column 0: 10"
    );
    assert_eq!(status_after(&[P::up(15.0, 0.0), P::moved(7.0, 1.0)]), "");
    assert_eq!(
        status_after(&[P::down(7.0, 1.0), P::moved(16.0, 3.0)]),
        "selecting"
    );
    assert_eq!(
        status_after(&[P::up(16.0, 3.0), P::down(9.0, 0.0), P::moved(20.0, 0.0)]),
        "moving column 0 to 2"
    );
}

#[test]
fn the_editor_draws_its_text_over_the_edited_cell_ending_in_a_caret() {
    let mut grid = Grid::new(
        AxisConfig::new(100, 1.0).header(1.0),
        AxisConfig::new(10, 8.0).header(5.0).gap(1.0),
    );
    let area = Rect::new(0, 0, 30, 5);
    frame(&mut grid, 30, 5, area);
    grid.edit_started(Key(2), Key(1));
    grid.action(Action::Scroll { dx: 3.0, dy: 0.0 });
    for (axis, range) in grid.begin_frame(Viewport::new(30.0, 5.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let snapshot = grid.snapshot();
    let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
    term.draw(|f| {
        render(&snapshot, &Labels, area, f.buffer_mut());
        render_editor(&snapshot, "a long name", area, f.buffer_mut());
    })
    .unwrap();
    let buf = term.backend().buffer().clone();
    insta::assert_snapshot!(text(&buf));
    // Column 1 now starts at x 11: the editor fills its 8 cells.
    assert_eq!(buf[(11, 3)].bg, Color::Yellow);
    assert_eq!(buf[(18, 3)].symbol(), "▏");
    assert_ne!(buf[(19, 3)].bg, Color::Yellow);
    assert_eq!(status(&snapshot), "editing row 2 column 1");
}

/// Labels its rows with their Keys, as a host whose rows have an identity.
struct Keyed;

impl Content for Keyed {
    fn cell(&self, row: Key, col: Key) -> String {
        Labels.cell(row, col)
    }

    fn column_header(&self, col: Key) -> String {
        Labels.column_header(col)
    }

    fn row_header(&self, row: Key, _position: u64) -> String {
        format!("#{}", row.0)
    }
}

/// Under a sort the row header shows the host's label for each row's Key,
/// which stays with the row; a row still loading shows its Position plus
/// one.
#[test]
fn row_headers_show_the_host_s_label_for_each_row_s_key() {
    let mut grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(2, 6.0).header(4.0),
    );
    let area = Rect::new(0, 0, 18, 7);
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys: Vec<Key> = match axis {
            // Sorted descending, and the last row not loaded yet.
            Axis::Rows => range.clone().rev().skip(1).map(Key).collect(),
            Axis::Cols => range.clone().map(Key).collect(),
        };
        grid.set_window(axis, range.start, &keys);
    }
    let snapshot = grid.snapshot();
    let mut buf = Buffer::empty(area);
    render(&snapshot, &Keyed, area, &mut buf);
    insta::assert_snapshot!(text(&buf));
}

/// Headers and row labels too long for their header end in `…`, which a
/// sort marker follows.
#[test]
fn header_text_too_long_for_its_header_ends_in_an_ellipsis() {
    struct Long;
    impl Content for Long {
        fn cell(&self, row: Key, col: Key) -> String {
            Labels.cell(row, col)
        }
        fn column_header(&self, col: Key) -> String {
            format!("column number {}", col.0)
        }
        fn row_header(&self, row: Key, _position: u64) -> String {
            format!("row {}", row.0)
        }
    }
    let mut grid = Grid::new(
        AxisConfig::new(2, 1.0).header(1.0),
        AxisConfig::new(2, 8.0).header(4.0).gap(1.0),
    );
    grid.sort_applied(SortSpec::new(vec![SortColumn::ascending(Key(1))]));
    let area = Rect::new(0, 0, 22, 3);
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let mut buf = Buffer::empty(area);
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme).scrollbars(false);
    render_with(&grid.snapshot(), &Long, &ctx, area, &mut buf);
    let lines: Vec<String> = text(&buf).lines().map(String::from).collect();
    assert_eq!(lines[0], "    column …│column…▲│");
    assert_eq!(lines[1], "row…r0c0     r0c1");
}
