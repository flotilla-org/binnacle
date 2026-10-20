//! Border Styles: the lines `render_with` draws in the Gaps.

use binnacle_core::{Action, Axis, AxisConfig, Edge, Grid, Key, PointerEvent, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    BorderExtent, BorderStyle, BorderWeight, Content, RenderContext, Theme, render_with,
};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

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
#[hegel::test(test_cases = 300)]
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
        if count > 0 && tc.draw(gs::booleans()) {
            let key = tc.draw(gs::integers::<u64>().max_value(count - 1));
            grid.pin(axis, Edge::Leading, Key(key));
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
    // A scrolling item is hidden where the pinned Strip draws.
    let shown = |axis, at| {
        let from = snapshot.scrolling(axis).start;
        let pinned = snapshot.pinned(axis, Edge::Leading);
        pinned.iter().any(|i| covers(i.offset, i.end(), at))
            || (snapshot.strip(axis).iter()).any(|i| covers(i.offset.max(from), i.end(), at))
    };

    for p in plain.area.positions() {
        let x = i64::from(p.x) - i64::from(area.x);
        if lined[p].fg == edge {
            assert!(area.contains(p), "the edge drew outside the area at {p:?}");
            assert!(
                !shown(Axis::Cols, x),
                "the edge drew over a column at {p:?}"
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
