//! Border Styles: the lines `render_with` draws in the Gaps.

use binnacle_core::{Action, Axis, AxisConfig, Edge, Grid, Key, PointerEvent, StripItem, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    BorderExtent, BorderStyle, BorderWeight, Content, RenderContext, Theme, begin_frame,
    render_with,
};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use std::ops::Range;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }
}

const AREA: Rect = Rect::new(0, 0, 30, 9);

const EXTENTS: [BorderExtent; 4] = [
    BorderExtent::None,
    BorderExtent::Headers,
    BorderExtent::Columns,
    BorderExtent::Grid,
];

const WEIGHTS: [BorderWeight; 3] = [
    BorderWeight::Light,
    BorderWeight::Heavy,
    BorderWeight::Double,
];

/// A table of 5 columns 6 wide, with a 2-line column header band and a
/// 4-wide row-number band, Gaps of 1 on both axes, scrolled part-way into
/// column 0 and two rows down.
fn fixture() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(2.0).gap(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    );
    grid.action(Action::Scroll { dx: 2.0, dy: 4.0 });
    grid
}

fn window(grid: &mut Grid, area: Rect) {
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
}

fn draw_in(grid: &mut Grid, ctx: &RenderContext, area: Rect) -> Buffer {
    window(grid, area);
    let mut buf = Buffer::empty(area);
    render_with(&grid.snapshot(), &Labels, ctx, area, &mut buf);
    buf
}

fn draw(grid: &mut Grid, border: BorderStyle) -> Buffer {
    let theme = Theme::classic();
    draw_in(grid, &RenderContext::new(&theme).border(border), AREA)
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
fn each_extent_draws_its_lines_in_each_weight() {
    let mut out = String::new();
    for extent in EXTENTS {
        for weight in WEIGHTS {
            let style = BorderStyle::new(extent, weight);
            out += &format!("--- {style}\n{}\n", text(&draw(&mut fixture(), style)));
        }
    }
    insta::assert_snapshot!(out);
}

#[test]
fn with_a_row_gap_of_0_grid_draws_only_column_lines() {
    let flat = || {
        let mut grid = fixture();
        grid.set_gap(Axis::Rows, 0.0);
        grid
    };
    let light = |extent| BorderStyle::new(extent, BorderWeight::Light);
    let grid = text(&draw(&mut flat(), light(BorderExtent::Grid)));
    assert_eq!(grid, text(&draw(&mut flat(), light(BorderExtent::Columns))));
    insta::assert_snapshot!(grid);
}

fn describe(style: Style) -> String {
    let colour = |c: Option<Color>| c.map_or("-".to_string(), |c| c.to_string());
    let mut out = format!("fg {} bg {}", colour(style.fg), colour(style.bg));
    if !style.add_modifier.is_empty() {
        out += &format!(" {:?}", style.add_modifier);
    }
    out
}

/// The text, then each cell's style as a letter (`.` for unstyled), then
/// what each letter stands for.
fn styled(buf: &Buffer) -> String {
    let a = buf.area;
    let mut legend: Vec<Style> = Vec::new();
    let mut map = String::new();
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            let style = buf[(x, y)].style();
            if style == Style::default().fg(Color::Reset).bg(Color::Reset) {
                map.push('.');
                continue;
            }
            let i = legend.iter().position(|&s| s == style).unwrap_or_else(|| {
                legend.push(style);
                legend.len() - 1
            });
            map.push(char::from(b'a' + i as u8));
        }
        map.push('\n');
    }
    let legend: String = legend
        .iter()
        .enumerate()
        .map(|(i, &s)| format!("{}: {}\n", char::from(b'a' + i as u8), describe(s)))
        .collect();
    format!("{}\n\n{map}\n{legend}", text(buf))
}

/// Band rules take each Header Band's last cell (x 3 and line 1 here), with
/// the band edge handles in the corner, and join the lines they cross.
#[test]
fn band_rules_take_each_header_band_s_last_cell() {
    let mut out = String::new();
    for extent in EXTENTS {
        let style = BorderStyle::new(extent, BorderWeight::Light).band_rules(true);
        out += &format!("--- {style}\n{}\n", text(&draw(&mut fixture(), style)));
    }
    insta::assert_snapshot!(out);
}

/// Handles (in the header bands, the band edge handles among them) take the
/// Theme's `handle` style; lines in the body take its `border` style.
#[test]
fn handles_and_borders_take_their_theme_styles() {
    let theme = Theme::catppuccin_mocha();
    let style = BorderStyle::new(BorderExtent::Grid, BorderWeight::Light).band_rules(true);
    let ctx = RenderContext::new(&theme).border(style);
    insta::assert_snapshot!(styled(&draw_in(&mut fixture(), &ctx, AREA)));
}

/// A table smaller than the area: 3 columns 6 wide and 3 rows, in the same
/// bands and Gaps as [`fixture`], so the cells past the last column and row
/// are inside the area.
fn small() -> Grid {
    Grid::new(
        AxisConfig::new(3, 1.0).header(2.0).gap(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    )
}

/// `Columns` closes the last column with a line in the cell past its end,
/// and `Grid` the last row too, joined to the band rules.
#[test]
fn columns_and_grid_close_the_last_column_and_row() {
    let mut out = String::new();
    for extent in EXTENTS {
        for weight in WEIGHTS {
            let style = BorderStyle::new(extent, weight);
            out += &format!("--- {style}\n{}\n", text(&draw(&mut small(), style)));
        }
    }
    insta::assert_snapshot!(out);
}

/// A band one cell thick keeps its text: no band rule takes its only cell.
#[test]
fn a_band_too_thin_for_a_rule_keeps_its_text() {
    let mut grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0).gap(1.0),
        AxisConfig::new(3, 6.0).header(1.0).gap(1.0),
    );
    let style = BorderStyle::new(BorderExtent::Grid, BorderWeight::Light);
    insta::assert_snapshot!(text(&draw(&mut grid, style)));
}

/// A resize guide and a drop indicator draw over the borders.
#[test]
fn drags_draw_over_the_borders() {
    type Drag = fn(&mut Grid);
    let drags: [(&str, Drag); 2] = [
        ("resizing column 1", |grid| {
            // Column 1's border Gap is at x 15.
            grid.pointer(PointerEvent::down(15.0, 0.0));
            grid.pointer(PointerEvent::moved(18.0, 3.0));
        }),
        ("moving column 1", |grid| {
            grid.pointer(PointerEvent::down(11.0, 0.0));
            grid.pointer(PointerEvent::moved(24.0, 3.0));
        }),
    ];
    let mut out = String::new();
    for (name, drag) in drags {
        for (extent, weight) in [
            (BorderExtent::Columns, BorderWeight::Heavy),
            (BorderExtent::Grid, BorderWeight::Double),
        ] {
            let style = BorderStyle::new(extent, weight).band_rules(true);
            let mut grid = fixture();
            grid.set_movable(Axis::Cols, true);
            window(&mut grid, AREA);
            drag(&mut grid);
            out += &format!("--- {name}, {style}\n{}\n", text(&draw(&mut grid, style)));
        }
    }
    insta::assert_snapshot!(out);
}

/// The terminal cells an item covers: from the first cell it reaches to the
/// first cell the next item (or Gap) could start in.
fn covers(offset: f32, end: f32, at: i64) -> bool {
    (offset.ceil() as i64..end.ceil() as i64).contains(&at)
}

/// [`fixture`] with column 3 pinned left: it draws after the row numbers,
/// and the scrolling columns start past the Gap after it, part-way into
/// column 0.
fn pinned() -> Grid {
    let mut grid = fixture();
    grid.pin(Axis::Cols, Edge::Leading, Key(3));
    grid
}

/// The pinned edge is `║` in the Gap after the pinned Strip in every Border
/// Style, `┃` under Double, whose rules are `║` already. It caps the band
/// rule with `╥` and the closing rule with `╨`, crosses the rules between
/// rows, and leaves the header's handle alone.
#[test]
fn the_pinned_edge_is_drawn_in_its_gap_in_every_border_style() {
    let mut out = String::new();
    for extent in EXTENTS {
        for weight in WEIGHTS {
            let style = BorderStyle::new(extent, weight);
            out += &format!("--- {style}\n{}\n", text(&draw(&mut pinned(), style)));
        }
    }
    let style = BorderStyle::headers().band_rules(true);
    out += &format!("--- {style}\n{}\n", text(&draw(&mut pinned(), style)));
    for weight in WEIGHTS {
        let mut grid = small();
        grid.pin(Axis::Cols, Edge::Leading, Key(1));
        let style = BorderStyle::new(BorderExtent::Grid, weight);
        out += &format!("--- closed, {style}\n{}\n", text(&draw(&mut grid, style)));
    }
    insta::assert_snapshot!(out);
}

/// [`fixture`] pinned on every edge: column 3 left and column 1 right, row
/// 5 to the top and row 9 to the bottom.
fn pinned_all_round() -> Grid {
    let mut grid = pinned();
    grid.pin(Axis::Cols, Edge::Trailing, Key(1));
    grid.pin(Axis::Rows, Edge::Leading, Key(5));
    grid.pin(Axis::Rows, Edge::Trailing, Key(9));
    grid
}

/// With items pinned at both ends of both axes, the trailing Strips sit
/// flush with the far edges and the scrolling items stop short of them.
/// Each edge is drawn in the Gap beside its Strip: a column edge `║` as
/// before, a row edge `═` (`━` under Double) across the body, capped `╞`
/// on the row band's rule and `╡` on the closing line, crossing column
/// lines with `╪` and column edges with `╬`. No scrollbars, so the far
/// edges show.
#[test]
fn edges_are_drawn_beside_pinned_strips_at_both_ends_of_both_axes() {
    let theme = Theme::classic();
    let draw = |grid: &mut Grid, style| {
        draw_in(
            grid,
            &RenderContext::new(&theme).border(style).scrollbars(false),
            AREA,
        )
    };
    let mut out = String::new();
    for extent in EXTENTS {
        for weight in WEIGHTS {
            let style = BorderStyle::new(extent, weight);
            out += &format!(
                "--- {style}\n{}\n",
                text(&draw(&mut pinned_all_round(), style))
            );
        }
    }
    let style = BorderStyle::headers().band_rules(true);
    out += &format!(
        "--- {style}\n{}\n",
        text(&draw(&mut pinned_all_round(), style))
    );
    for weight in WEIGHTS {
        let mut grid = small();
        grid.pin(Axis::Cols, Edge::Trailing, Key(0));
        grid.pin(Axis::Rows, Edge::Trailing, Key(0));
        let style = BorderStyle::new(BorderExtent::Grid, weight);
        out += &format!("--- closed, {style}\n{}\n", text(&draw(&mut grid, style)));
    }
    insta::assert_snapshot!(out);
}

/// The edge takes each Theme's `pinned_edge` style: the border's in every
/// preset, and its own where a host sets one.
#[test]
fn the_pinned_edge_takes_each_theme_s_style() {
    let mut out = String::new();
    let custom = Theme {
        name: "custom",
        pinned_edge: Style::default().fg(Color::Yellow),
        ..Theme::classic()
    };
    for theme in Theme::presets().into_iter().chain([custom]) {
        let style = BorderStyle::new(BorderExtent::Columns, BorderWeight::Light);
        let ctx = RenderContext::new(&theme).border(style);
        out += &format!(
            "--- {}\n{}\n",
            theme.name,
            styled(&draw_in(&mut pinned(), &ctx, AREA))
        );
    }
    insta::assert_snapshot!(out);
}

/// Every Border Style changes only Gap cells (cells no column or no row
/// covers), the closing lines past the last column and row among them. The
/// band rules are the only exception: they take each Header Band's last
/// cell, which a part-scrolled item may lie under.
#[hegel::test(test_cases = 2000)]
fn lines_are_drawn_only_in_gap_cells(tc: TestCase) {
    // Quarter cells, so items and Gaps start and end part-way into cells.
    let quarters = |lo, hi| tc.draw(gs::integers::<u8>().min_value(lo).max_value(hi)) as f32 / 4.0;
    let size = || quarters(4, 28);
    let band = || quarters(0, 16);
    let gap = || quarters(0, 8);
    let rows = tc.draw(gs::integers::<u64>().min_value(0).max_value(40));
    let cols = tc.draw(gs::integers::<u64>().min_value(0).max_value(8));
    let mut grid = Grid::new(
        AxisConfig::new(rows, size()).header(band()).gap(gap()),
        AxisConfig::new(cols, size()).header(band()).gap(gap()),
    );
    let scroll = || tc.draw(gs::integers::<u16>().min_value(0).max_value(400)) as f32 / 4.0;
    grid.action(Action::Scroll {
        dx: scroll(),
        dy: scroll(),
    });
    for (axis, count) in [(Axis::Rows, rows), (Axis::Cols, cols)] {
        grid.set_min_scrolling(axis, quarters(0, 40));
        grid.set_end_gap(axis, quarters(0, 8));
        for _ in 0..tc.draw(gs::integers::<u8>().max_value(if count > 0 { 3 } else { 0 })) {
            let key = tc.draw(gs::integers::<u64>().max_value(count - 1));
            let edge = tc.draw(gs::sampled_from(Edge::ALL.to_vec()));
            grid.pin(axis, edge, Key(key));
        }
    }
    let dim = |max| tc.draw(gs::integers::<u16>().min_value(0).max_value(max));
    let area = Rect::new(dim(3), dim(3), dim(30), dim(16));
    let extent = tc.draw(gs::sampled_from(EXTENTS.to_vec()));
    let weight = tc.draw(gs::sampled_from(WEIGHTS.to_vec()));
    let style = BorderStyle::new(extent, weight).band_rules(tc.draw(gs::booleans()));
    window(&mut grid, area);
    let snapshot = grid.snapshot();
    // The edge in a colour of its own, to find it.
    let edge = Color::Rgb(1, 2, 3);
    let theme = Theme {
        pinned_edge: Style::default().fg(edge),
        ..Theme::classic()
    };
    let ctx = RenderContext::new(&theme);
    let drawn = |border| {
        // The terminal is larger than the Grid's area, on every side.
        let mut buf = Buffer::empty(Rect::new(0, 0, area.right() + 3, area.bottom() + 3));
        render_with(
            &snapshot,
            &Labels,
            &ctx.clone().border(border),
            area,
            &mut buf,
        );
        buf
    };
    let plain = drawn(BorderStyle::new(BorderExtent::None, weight));
    let lined = drawn(style);
    let band_last = |axis| {
        let thickness = snapshot.header(axis).ceil() as i64;
        (style.draws_band_rules() && thickness >= 2).then_some(thickness - 1)
    };
    let (rule_x, rule_y) = (band_last(Axis::Cols), band_last(Axis::Rows));
    // Each item shows only in its Strip's region: a scrolling item is
    // hidden where the pinned Strips draw, a pinned one where its Strip
    // clips.
    let shown = |axis, at| {
        let clipped = |span: Range<f32>, i: &StripItem| {
            covers(i.offset.max(span.start), i.end().min(span.end), at)
        };
        let pinned = (Edge::ALL.into_iter()).any(|e| {
            (snapshot.pinned(axis, e).iter()).any(|i| clipped(snapshot.pinned_region(axis, e), i))
        });
        pinned || (snapshot.strip(axis).iter()).any(|i| clipped(snapshot.scrolling(axis), i))
    };

    for p in plain.area.positions() {
        let x = i64::from(p.x) - i64::from(area.x);
        if lined[p].fg == edge {
            let y = i64::from(p.y) - i64::from(area.y);
            assert!(area.contains(p), "the edge drew outside the area at {p:?}");
            assert!(
                !shown(Axis::Cols, x) || !shown(Axis::Rows, y),
                "the edge drew over a cell at {p:?}"
            );
        }
        if plain[p] == lined[p] {
            continue;
        }
        assert!(
            area.contains(p),
            "{extent:?} drew outside the area at {p:?}"
        );
        let y = i64::from(p.y - area.y);
        if rule_x == Some(x) || rule_y == Some(y) {
            continue;
        }
        let (in_col, in_row) = (shown(Axis::Cols, x), shown(Axis::Rows, y));
        assert!(!(in_col && in_row), "{extent:?} drew over an item at {p:?}");
    }
}

/// Rows have no Gap, so [`begin_frame`] gives each pinned edge one cell of
/// Edge Gap: a row pinned to the top or bottom costs a line for its `═`
/// rule, as a column with no Gap costs a cell for its `║`.
#[test]
fn begin_frame_gives_pinned_edges_a_cell_where_there_is_no_gap() {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0),
    );
    grid.pin(Axis::Rows, Edge::Leading, Key(4));
    grid.pin(Axis::Rows, Edge::Trailing, Key(9));
    grid.pin(Axis::Cols, Edge::Trailing, Key(0));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme).scrollbars(false);
    let area = Rect::new(0, 0, 30, 8);
    let layout = begin_frame(&mut grid, &ctx, area, |grid, viewport| {
        for (axis, range) in grid.begin_frame(viewport) {
            let keys = IdentitySource::new(range.end).window(range.clone());
            grid.set_window(axis, range.start, &keys);
        }
    });
    let mut buf = Buffer::empty(area);
    render_with(&grid.snapshot(), &Labels, &ctx, layout.grid, &mut buf);
    insta::assert_snapshot!(text(&buf));
}

/// [`begin_frame`] keeps `MIN_SCROLLING` cells of each axis to scroll:
/// pinned Strips longer than the rest clip at their inside ends, the
/// trailing one first, with the edge where they clip; focus on a clipped
/// item slides its Strip to show it.
#[test]
fn pinned_strips_too_long_for_the_area_clip_with_the_edge_where_they_clip() {
    let mut grid = fixture();
    for key in [1, 2] {
        grid.pin(Axis::Cols, Edge::Leading, Key(key));
    }
    grid.pin(Axis::Cols, Edge::Trailing, Key(4));
    for key in 0..5 {
        grid.pin(Axis::Rows, Edge::Leading, Key(key));
    }
    let theme = Theme::classic();
    let style = BorderStyle::new(BorderExtent::Grid, BorderWeight::Light);
    let ctx = RenderContext::new(&theme).scrollbars(false).border(style);
    let frame = |grid: &mut Grid| {
        let layout = begin_frame(grid, &ctx, AREA, |grid, viewport| {
            for (axis, range) in grid.begin_frame(viewport) {
                let keys = IdentitySource::new(range.end).window(range.clone());
                grid.set_window(axis, range.start, &keys);
            }
        });
        let mut buf = Buffer::empty(AREA);
        render_with(&grid.snapshot(), &Labels, &ctx, layout.grid, &mut buf);
        text(&buf)
    };
    let mut out = format!(
        "--- columns 1 and 2 pinned left, 4 right; rows 0 to 4 to the top\n{}\n",
        frame(&mut grid)
    );
    grid.set_focus(Key(4), Key(2));
    out += &format!(
        "--- focus row 4, column 2: both leading Strips slide\n{}\n",
        frame(&mut grid)
    );
    grid.set_focus(Key(6), Key(4));
    out += &format!(
        "--- focus row 6, column 4: the trailing Strip takes room for it\n{}\n",
        frame(&mut grid)
    );
    insta::assert_snapshot!(out);
}

/// Scrolled so that row 1 ends exactly at the column header band's end and
/// column 0 exactly at the row band's end, the first visible row and
/// column still have their lines before them: in the Gap cells that start
/// the body.
#[test]
fn an_item_ending_at_the_header_band_still_has_its_line() {
    let mut out = String::new();
    for extent in [BorderExtent::Columns, BorderExtent::Grid] {
        let style = BorderStyle::new(extent, BorderWeight::Light);
        let mut grid = Grid::new(
            AxisConfig::new(20, 1.0).header(2.0).gap(1.0),
            AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
        );
        grid.action(Action::Scroll { dx: 6.0, dy: 3.0 });
        out += &format!("--- {style}\n{}\n", text(&draw(&mut grid, style)));
    }
    insta::assert_snapshot!(out);
}

/// With `Columns` or `Grid`, the Gap between two rows in the body carries
/// a line reaching into the row band, and the Gap between two columns one
/// reaching into the column band, wherever the scroll leaves the
/// items: an item ending exactly at its Header Band among them.
#[hegel::test(test_cases = 500)]
fn every_gap_cell_between_items_carries_a_line(tc: TestCase) {
    let whole = |lo, hi| tc.draw(gs::integers::<u8>().min_value(lo).max_value(hi)) as f32;
    let rows = tc.draw(gs::integers::<u64>().min_value(2).max_value(30));
    let cols = tc.draw(gs::integers::<u64>().min_value(2).max_value(8));
    let mut grid = Grid::new(
        AxisConfig::new(rows, whole(1, 3))
            .header(whole(1, 3))
            .gap(whole(1, 2)),
        AxisConfig::new(cols, whole(2, 7))
            .header(whole(1, 5))
            .gap(whole(1, 2)),
    );
    let scroll = || whole(0, 60);
    grid.action(Action::Scroll {
        dx: scroll(),
        dy: scroll(),
    });
    let extent = tc.draw(gs::sampled_from(vec![
        BorderExtent::Columns,
        BorderExtent::Grid,
    ]));
    let style = BorderStyle::new(extent, tc.draw(gs::sampled_from(WEIGHTS.to_vec())));
    let buf = draw(&mut grid, style);
    let snapshot = grid.snapshot();
    let blank = |x: u16, y: u16| buf[(x, y)].symbol() == " ";
    // The last line and column hold the scrollbars.
    for (axis, len) in [(Axis::Rows, AREA.height - 1), (Axis::Cols, AREA.width - 1)] {
        // Columns draws handles in the row band too, so rows are lined
        // with either extent.
        let (strip, gap) = (snapshot.strip(axis), snapshot.gap(axis));
        let band = snapshot.header(axis).ceil() as i64;
        for (i, item) in strip.iter().enumerate().filter(|(_, it)| it.position > 0) {
            // The cells of the Gap before `item`. A line takes the first,
            // so a Gap that starts under the Header Band shows none.
            let after = i
                .checked_sub(1)
                .map_or(item.offset - gap, |p| strip[p].end());
            let cells = after.ceil() as i64..(item.offset.ceil() as i64).min(i64::from(len));
            if cells.start < band {
                continue;
            }
            if cells.is_empty() {
                continue;
            }
            let lined = cells.clone().any(|at| match axis {
                Axis::Rows => !blank(0, at as u16),
                Axis::Cols => !blank(at as u16, 0),
            });
            assert!(lined, "no line in the Gap cells {cells:?} on {axis:?}");
        }
    }
}

/// [`begin_frame`] asks for an End Gap of one cell where the Border Style
/// closes the table (after the last column for `Columns` and `Grid`, under
/// the last row for `Grid`), so scrolled to the end of a long table the
/// closing lines show. A trailing pinned Strip keeps that cell past it,
/// and the closing line follows it. Without closing lines no room is kept.
#[test]
fn scrolled_to_the_end_the_closing_lines_show() {
    let theme = Theme::classic();
    let frame_in = |grid: &mut Grid, style: BorderStyle, area: Rect| {
        let ctx = RenderContext::new(&theme).border(style);
        begin_frame(grid, &ctx, area, |grid, viewport| {
            for (axis, range) in grid.begin_frame(viewport) {
                let keys = IdentitySource::new(range.end).window(range.clone());
                grid.set_window(axis, range.start, &keys);
            }
        });
        let mut buf = Buffer::empty(area);
        render_with(&grid.snapshot(), &Labels, &ctx, area, &mut buf);
        text(&buf)
    };
    let frame = |grid: &mut Grid, style| frame_in(grid, style, AREA);
    let at_end = |pin: bool| {
        let mut grid = fixture();
        if pin {
            grid.pin(Axis::Cols, Edge::Trailing, Key(1));
            grid.pin(Axis::Rows, Edge::Trailing, Key(16));
        }
        grid.action(Action::Scroll {
            dx: 1000.0,
            dy: 1000.0,
        });
        grid
    };
    let mut out = String::new();
    for extent in [
        BorderExtent::Headers,
        BorderExtent::Columns,
        BorderExtent::Grid,
    ] {
        let style = BorderStyle::new(extent, BorderWeight::Light);
        let mut grid = at_end(false);
        // Settle the end clamp with the End Gap's room.
        frame(&mut grid, style);
        out += &format!("--- {style}, at the end\n{}\n", frame(&mut grid, style));
    }
    for extent in [BorderExtent::Columns, BorderExtent::Grid] {
        let style = BorderStyle::new(extent, BorderWeight::Light);
        let mut grid = at_end(true);
        frame(&mut grid, style);
        out += &format!(
            "--- {style}, at the end, column 1 pinned right and row 16 to the bottom\n{}\n",
            frame(&mut grid, style)
        );
    }
    // A line taller, row 16 ends exactly at the column header band's end.
    let style = BorderStyle::new(BorderExtent::Grid, BorderWeight::Light);
    let taller = Rect::new(0, 0, 30, 10);
    let mut grid = at_end(false);
    frame_in(&mut grid, style, taller);
    out += &format!(
        "--- {style}, at the end, a line taller: a rule above the first row\n{}\n",
        frame_in(&mut grid, style, taller)
    );
    insta::assert_snapshot!(out);
}
