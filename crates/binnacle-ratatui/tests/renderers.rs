//! Cell Renderers: columns drawn by a renderer into a clipped cell canvas.

use std::time::Duration;

use binnacle_core::{
    Action, Axis, AxisConfig, FocusStep, Grid, Key, Modifiers, PointerEvent, Snapshot, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    Align, BarRenderer, CellCanvas, CellRenderer, Content, RenderContext, SparklineRenderer,
    TextRenderer, Theme, fit_width, render_with,
};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// A table whose columns draw with the given renderers, by column Key.
struct Columns<'a> {
    renderers: Vec<Box<dyn CellRenderer + 'a>>,
}

impl Content for Columns<'_> {
    fn cell(&self, _row: Key, _col: Key) -> String {
        unreachable!("every column has its own renderer")
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }

    fn column_renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        Box::new(&*self.renderers[col.0 as usize])
    }
}

/// The same renderers by row Key instead, as a column-major table wants:
/// each column is a ticker and each row one of its fields.
struct Rows<'a>(Columns<'a>);

impl Content for Rows<'_> {
    fn cell(&self, _row: Key, _col: Key) -> String {
        unreachable!("every row has its own renderer")
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }

    fn renderer(&self, row: Key, col: Key) -> Box<dyn CellRenderer + '_> {
        let _ = col;
        Box::new(&*self.0.renderers[row.0 as usize])
    }
}

/// Runs a host frame over identity sources, leaving rows from `loaded` on
/// unfilled.
fn window(grid: &mut Grid, area: Rect, loaded: u64) {
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let end = match axis {
            Axis::Rows => range.end.min(loaded),
            Axis::Cols => range.end,
        };
        let keys = IdentitySource::new(end).window(range.start..end);
        grid.set_window(axis, range.start, &keys);
    }
}

/// Draws with scrollbars off, as every test here does: they look inside cells.
fn draw(snapshot: &Snapshot, content: &impl Content, ctx: &RenderContext, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    render_with(snapshot, content, ctx, area, &mut buf);
    buf
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

const LOADS: [f64; 4] = [0.0, 0.3, 0.55, 1.0];
const PRICES: [&str; 4] = ["7", "12.5", "300", "4096.25"];
const HISTORY: [[f64; 6]; 4] = [
    [1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
    [6.0, 5.0, 4.0, 3.0, 2.0, 1.0],
    [0.0, 9.0, 0.0, 9.0, 0.0, 9.0],
    [4.0, 4.0, 4.0, 4.0, 4.0, 4.0],
];

/// Right- and centre-aligned text, a sparkline and a bar.
fn ticker() -> Columns<'static> {
    let price = |row: Key, _| PRICES[row.0 as usize % 4].to_string();
    Columns {
        renderers: vec![
            Box::new(TextRenderer::new(price).align(Align::Right)),
            Box::new(TextRenderer::new(price).align(Align::Center)),
            Box::new(SparklineRenderer::new(|row: Key, _| {
                HISTORY[row.0 as usize % 4]
            })),
            Box::new(BarRenderer::new(|row: Key, _| LOADS[row.0 as usize % 4])),
        ],
    }
}

fn ticker_grid() -> Grid {
    Grid::new(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(4, 9.0).header(2.0).gap(1.0),
    )
}

#[test]
fn built_in_renderers_draw_aligned_text_sparklines_and_bars() {
    let area = Rect::new(0, 0, 42, 5);
    let mut grid = ticker_grid();
    window(&mut grid, area, 4);
    let theme = Theme::classic();
    let buf = draw(
        &grid.snapshot(),
        &ticker(),
        &RenderContext::new(&theme).scrollbars(false),
        area,
    );
    insta::assert_snapshot!(text(&buf));
}

#[test]
fn a_part_scrolled_cell_draws_only_its_visible_part() {
    let area = Rect::new(0, 0, 30, 5);
    let mut grid = ticker_grid();
    // Column 0 is scrolled under the row numbers past its fourth cell, and
    // the bar column is cut off by the area's right edge.
    grid.action(Action::Scroll { dx: 4.0, dy: 0.0 });
    window(&mut grid, area, 4);
    let theme = Theme::classic();
    let buf = draw(
        &grid.snapshot(),
        &ticker(),
        &RenderContext::new(&theme).scrollbars(false),
        area,
    );
    insta::assert_snapshot!(text(&buf));
}

/// Writes one letter for each status flag the cell has, then the time.
struct Flags;

impl CellRenderer for Flags {
    fn draw(&self, _row: Key, _col: Key, canvas: &mut CellCanvas) {
        let s = canvas.status();
        let flags: String = [
            (s.focused, 'f'),
            (s.selected, 's'),
            (s.picked, 'p'),
            (s.hovered, 'h'),
        ]
        .iter()
        .filter(|(on, _)| *on)
        .map(|&(_, c)| c)
        .collect();
        let text = format!("{flags}{}", canvas.now().as_secs());
        let style = Style::default().fg(Color::Green);
        canvas.print(0, 0, &text, style);
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        5
    }
}

#[test]
fn a_renderer_sees_the_cell_status_and_the_time_and_focus_is_patched_over_it() {
    let area = Rect::new(0, 0, 24, 6);
    let mut grid = Grid::new(
        AxisConfig::new(10, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(2.0).gap(1.0),
    );
    window(&mut grid, area, 10);
    // Ctrl-click row 3 (line 4) to pick it and focus it in column 0, extend
    // the selection to row 4 and column 1, then hover column 2's header
    // (x 16..22). The picked row shows as picked, not selected.
    grid.pointer(PointerEvent::down(3.0, 4.0).modifiers(Modifiers::CTRL));
    grid.pointer(PointerEvent::up(3.0, 4.0).modifiers(Modifiers::CTRL));
    let extend = |axis| Action::MoveFocus {
        axis,
        by: FocusStep::Item,
        forward: true,
        extend: true,
    };
    grid.action(extend(Axis::Rows));
    grid.action(extend(Axis::Cols));
    grid.pointer(PointerEvent::moved(17.0, 0.0));
    window(&mut grid, area, 10);
    let flags = Columns {
        renderers: vec![Box::new(Flags), Box::new(Flags), Box::new(Flags)],
    };
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme)
        .scrollbars(false)
        .at(Duration::from_secs(7));
    let buf = draw(&grid.snapshot(), &flags, &ctx, area);
    insta::assert_snapshot!(text(&buf));
    // The focused cell is reversed across its width, over the green glyphs.
    let focused = &buf[(9, 5)];
    assert!(focused.modifier.contains(Modifier::REVERSED));
    assert_eq!(focused.fg, Color::Green);
    let reversed = (buf.area.positions())
        .filter(|&p| buf[p].modifier.contains(Modifier::REVERSED))
        .count();
    assert_eq!(reversed, 6);
}

/// Draws an `X` everywhere, so a cell that shows it was drawn by it.
struct Never;

impl CellRenderer for Never {
    fn draw(&self, _row: Key, _col: Key, canvas: &mut CellCanvas) {
        for y in 0..canvas.height() {
            canvas.print(0, y, &"X".repeat(canvas.width() as usize), Style::default());
        }
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        1
    }
}

#[test]
fn a_cell_without_a_key_shows_the_loading_marker_whatever_its_renderer() {
    let area = Rect::new(0, 0, 20, 5);
    let mut grid = Grid::new(
        AxisConfig::new(10, 1.0).header(1.0),
        AxisConfig::new(2, 6.0).header(2.0).gap(1.0),
    );
    window(&mut grid, area, 2);
    let never = Columns {
        renderers: vec![Box::new(Never), Box::new(Never)],
    };
    let theme = Theme::classic();
    let buf = draw(
        &grid.snapshot(),
        &never,
        &RenderContext::new(&theme).scrollbars(false),
        area,
    );
    insta::assert_snapshot!(text(&buf));
}

#[test]
fn fit_width_is_the_widest_preferred_width_or_header() {
    let ticker = ticker();
    let rows: Vec<Key> = (0..4).map(Key).collect();
    // The longest price, "4096.25", and the six-value history.
    assert_eq!(fit_width(&ticker, &rows, Key(0)), 7);
    assert_eq!(fit_width(&ticker, &rows, Key(2)), 6);
    // The bar's default width.
    assert_eq!(fit_width(&ticker, &rows, Key(3)), 10);
    // No rows: the header, "col 1".
    assert_eq!(fit_width(&ticker, &[], Key(1)), 5);
}

/// The ticker's renderers keyed by row, reading their values by column.
fn ticker_on_its_side() -> Rows<'static> {
    let price = |_, col: Key| PRICES[col.0 as usize % 4].to_string();
    Rows(Columns {
        renderers: vec![
            Box::new(TextRenderer::new(price).align(Align::Right)),
            Box::new(TextRenderer::new(price).align(Align::Center)),
            Box::new(SparklineRenderer::new(|_, col: Key| {
                HISTORY[col.0 as usize % 4]
            })),
            Box::new(BarRenderer::new(|_, col: Key| LOADS[col.0 as usize % 4])),
        ],
    })
}

#[test]
fn a_renderer_chosen_per_cell_draws_each_row_its_own_way() {
    let area = Rect::new(0, 0, 42, 5);
    let mut grid = ticker_grid();
    window(&mut grid, area, 4);
    let theme = Theme::classic();
    let buf = draw(
        &grid.snapshot(),
        &ticker_on_its_side(),
        &RenderContext::new(&theme).scrollbars(false),
        area,
    );
    insta::assert_snapshot!(text(&buf));
}

#[test]
fn fit_width_asks_each_cells_own_renderer() {
    let sideways = ticker_on_its_side();
    let rows: Vec<Key> = (0..4).map(Key).collect();
    // Ticker 3: "4096.25" twice, six history values, and the bar's 10.
    assert_eq!(fit_width(&sideways, &rows, Key(3)), 10);
    // Without the bar row: the price, 7.
    assert_eq!(fit_width(&sideways, &rows[..3], Key(3)), 7);
    // Ticker 0's price, "7", is narrower than its sparkline.
    assert_eq!(fit_width(&sideways, &rows[..1], Key(0)), 5);
    assert_eq!(fit_width(&sideways, &rows[..3], Key(0)), 6);
}

#[test]
fn the_default_renderer_is_the_cells_text() {
    struct Plain;
    impl Content for Plain {
        fn cell(&self, row: Key, col: Key) -> String {
            format!("r{}c{}", row.0, col.0)
        }
        fn column_header(&self, _col: Key) -> String {
            "h".into()
        }
    }
    let rows = [Key(3), Key(12)];
    assert_eq!(fit_width(&Plain, &rows, Key(4)), 5);
    let area = Rect::new(0, 0, 20, 3);
    let mut grid = ticker_grid();
    window(&mut grid, area, 4);
    let theme = Theme::classic();
    let no_scrollbars = RenderContext::new(&theme).scrollbars(false);
    let buf = draw(&grid.snapshot(), &Plain, &no_scrollbars, area);
    assert_eq!(text(&buf).lines().nth(1), Some("1 r0c0      r0c1"));
}

/// Flashes when the host says the value changed at second 10.
struct Flashing;

impl CellRenderer for Flashing {
    fn draw(&self, _row: Key, _col: Key, canvas: &mut CellCanvas) {
        canvas.print(0, 0, "v", Style::default());
        canvas.flash(Duration::from_secs(10));
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        1
    }
}

#[test]
fn a_changed_value_flashes_in_the_theme_flash_style_for_a_while() {
    let area = Rect::new(0, 0, 10, 2);
    let mut grid = Grid::new(
        AxisConfig::new(1, 1.0).header(1.0),
        AxisConfig::new(1, 4.0).header(2.0),
    );
    window(&mut grid, area, 1);
    let content = Columns {
        renderers: vec![Box::new(Flashing)],
    };
    let snapshot = grid.snapshot();
    let theme = Theme::classic();
    let bg_at = |ms| {
        let ctx = RenderContext::new(&theme)
            .scrollbars(false)
            .at(Duration::from_millis(ms));
        let buf = draw(&snapshot, &content, &ctx, area);
        (2..6).map(|x| buf[(x, 1)].bg).collect::<Vec<_>>()
    };
    let lit = vec![theme.flash.bg.unwrap(); 4];
    let unlit = vec![Color::Reset; 4];
    assert_eq!(bg_at(9_999), unlit, "before the change");
    assert_eq!(bg_at(10_000), lit);
    assert_eq!(bg_at(10_799), lit);
    assert_eq!(bg_at(10_800), unlit, "after the flash");
}

/// Writes `#` everywhere it can, coloured with the low bytes of its row and
/// column Keys, so a buffer cell shows which cell drew it.
struct Hostile;

impl CellRenderer for Hostile {
    fn draw(&self, row: Key, col: Key, canvas: &mut CellCanvas) {
        let style = Style::default().fg(Color::Rgb(row.0 as u8, col.0 as u8, 7));
        let (w, h) = (canvas.width(), canvas.height());
        let reach = |n: u32| (0..n.saturating_add(4)).chain([u32::MAX / 2, u32::MAX]);
        for y in reach(h) {
            for x in reach(w) {
                canvas.set(x, y, '#', style);
            }
        }
        canvas.print(u32::MAX - 1, 0, "####", style);
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        1
    }
}

/// The whole terminal cells an item's span `[start, end)` covers, clipped to
/// `[min, max)`: rounding up, as the adapter maps part-scrolled items.
fn span(start: f32, end: f32, min: f32, max: u16) -> std::ops::Range<i64> {
    let lo = (start.max(min).ceil() as i64).max(0);
    let hi = (end.ceil() as i64).min(i64::from(max));
    lo..hi.max(lo)
}

/// A hostile renderer in every column changes the buffer only inside each
/// cell's visible part, and reaches all of it.
#[hegel::test(test_cases = 300)]
fn a_hostile_renderer_writes_only_inside_its_cells_visible_part(tc: TestCase) {
    // Quarter cells, so items start and end part-way into terminal cells.
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
    let dim = |max| tc.draw(gs::integers::<u16>().min_value(0).max_value(max));
    let (x, y, w, h) = (dim(3), dim(3), dim(30), dim(16));
    let area = Rect::new(x, y, w, h);
    let loaded = tc.draw(gs::integers::<u64>().min_value(0).max_value(40));
    window(&mut grid, area, loaded);
    let snapshot = grid.snapshot();
    let content = Columns {
        renderers: (0..cols).map(|_| Box::new(Hostile) as _).collect(),
    };
    let theme = Theme::classic();
    // The terminal is larger than the Grid's area, on every side.
    let mut buf = Buffer::empty(Rect::new(0, 0, w + 6, h + 6));
    render_with(
        &snapshot,
        &content,
        &RenderContext::new(&theme).scrollbars(false),
        area,
        &mut buf,
    );

    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    let mut expected = std::collections::HashMap::new();
    for r in snapshot.rows() {
        for c in snapshot.cols() {
            let Some((row, col)) = r.key.zip(c.key) else {
                continue;
            };
            let fg = Color::Rgb(row.0 as u8, col.0 as u8, 7);
            for y in span(r.offset, r.end(), top, area.height) {
                for x in span(c.offset, c.end(), left, area.width) {
                    let at = (area.x + x as u16, area.y + y as u16);
                    expected.insert(at, fg);
                }
            }
        }
    }
    for p in buf.area.positions() {
        let cell = &buf[p];
        let drawn = (cell.symbol() == "#").then_some(cell.fg);
        assert_eq!(drawn, expected.get(&(p.x, p.y)).copied(), "at {p:?}");
    }
}

/// The glyphs drawn in the first body line from x 2 for `width` cells.
fn glyphs(content: &impl Content, width: u16) -> Vec<char> {
    let area = Rect::new(0, 0, width + 2, 2);
    let mut grid = Grid::new(
        AxisConfig::new(1, 1.0).header(1.0),
        AxisConfig::new(1, f32::from(width)).header(2.0),
    );
    window(&mut grid, area, 1);
    let theme = Theme::classic();
    let buf = draw(
        &grid.snapshot(),
        content,
        &RenderContext::new(&theme).scrollbars(false),
        area,
    );
    (2..width + 2)
        .map(|x| buf[(x, 1)].symbol().chars().next().unwrap())
        .collect()
}

const SPARK: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A series and a renderer for it: scaled to a drawn range, or to the
/// series itself. Returns the levels drawn and the scale's ends.
fn sparkline(tc: &TestCase) -> (Vec<f64>, Vec<usize>, (f64, f64)) {
    let finite = || gs::floats::<f64>().allow_nan(false).allow_infinity(false);
    let series = tc.draw(gs::vecs(finite()).min_size(1).max_size(30));
    let range = tc
        .draw(gs::booleans())
        .then(|| (tc.draw(finite()), tc.draw(finite())));
    let levels = {
        let renderer = SparklineRenderer::new(|_, _| &series[..]);
        let renderer = match range {
            Some((min, max)) => renderer.range(min, max),
            None => renderer,
        };
        let content = Columns {
            renderers: vec![Box::new(renderer)],
        };
        glyphs(&content, series.len() as u16)
            .iter()
            .map(|g| {
                SPARK
                    .iter()
                    .position(|s| s == g)
                    .expect("a sparkline glyph")
            })
            .collect()
    };
    let range = range.unwrap_or_else(|| {
        let min = series.iter().copied().fold(f64::INFINITY, f64::min);
        (
            min,
            series.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    });
    (series, levels, range)
}

/// Every value draws one of the eight levels, and a larger value never
/// draws a lower one.
#[hegel::test(test_cases = 300)]
fn sparkline_levels_are_monotone_in_the_value_and_within_eight(tc: TestCase) {
    let (series, levels, _) = sparkline(&tc);
    for (i, a) in series.iter().enumerate() {
        for (j, b) in series.iter().enumerate() {
            if a < b {
                assert!(
                    levels[i] <= levels[j],
                    "{a} at {} above {b} at {}",
                    levels[i],
                    levels[j]
                );
            }
        }
    }
}

/// With a scale of some width, values at or past its ends draw the lowest
/// and highest levels.
#[hegel::test(test_cases = 300)]
fn sparkline_scale_ends_draw_the_lowest_and_highest_levels(tc: TestCase) {
    let (series, levels, (min, max)) = sparkline(&tc);
    tc.assume(min < max);
    for (v, level) in series.iter().zip(&levels) {
        if *v <= min {
            assert_eq!(*level, 0, "{v} in {min}..{max}");
        }
        if *v >= max {
            assert_eq!(*level, 7, "{v} in {min}..{max}");
        }
    }
}

const EIGHTHS: [char; 8] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// A bar fills exactly `floor(fraction * width * 8)` eighths, left to right,
/// with the fraction clamped into `0..=1`.
#[hegel::test(test_cases = 300)]
fn a_bar_fills_floor_fraction_times_width_times_eight_eighths(tc: TestCase) {
    let fraction = tc.draw(gs::floats::<f64>().allow_nan(false));
    let width = tc.draw(gs::integers::<u16>().min_value(1).max_value(40));
    let content = Columns {
        renderers: vec![Box::new(BarRenderer::new(|_, _| fraction))],
    };
    let drawn = glyphs(&content, width);
    let eighths: Vec<usize> = drawn
        .iter()
        .map(|&g| EIGHTHS.iter().position(|&e| e == g).map_or(0, |i| i + 1))
        .collect();
    let expected = (fraction.clamp(0.0, 1.0) * f64::from(width) * 8.0).floor() as usize;
    assert_eq!(
        eighths.iter().sum::<usize>(),
        expected,
        "{fraction} of {width}"
    );
    // Full cells, at most one partial cell, then blanks.
    let partial = eighths.iter().position(|&e| e < 8).unwrap_or(eighths.len());
    assert!(eighths[partial..].iter().skip(1).all(|&e| e == 0));
}

/// Text that fits its cell is drawn whole, where its alignment puts it;
/// longer text keeps its start and ends in `…` in the cell's last column,
/// whatever the alignment.
#[hegel::test(test_cases = 300)]
fn text_too_long_for_its_cell_ends_in_an_ellipsis(tc: TestCase) {
    // One terminal cell a character, as `TextRenderer` counts them; spaces
    // included, so leading and trailing blanks must land where they belong.
    let alphabet = vec!['a', 'Z', '7', '.', ' ', 'é', '…'];
    let text: String = tc
        .draw(gs::vecs(gs::sampled_from(alphabet)).max_size(30))
        .into_iter()
        .collect();
    let width = tc.draw(gs::integers::<u16>().min_value(1).max_value(40));
    let align = tc.draw(gs::sampled_from(vec![
        Align::Left,
        Align::Right,
        Align::Center,
    ]));
    let content = Columns {
        renderers: vec![Box::new(
            TextRenderer::new(|_, _| text.clone()).align(align),
        )],
    };
    let drawn: String = glyphs(&content, width).into_iter().collect();
    let len = text.chars().count();
    let room = usize::from(width);
    let expected = if len <= room {
        let before = match align {
            Align::Left => 0,
            Align::Right => room - len,
            Align::Center => (room - len) / 2,
        };
        format!(
            "{}{text}{}",
            " ".repeat(before),
            " ".repeat(room - len - before)
        )
    } else {
        let kept: String = text.chars().take(room - 1).collect();
        format!("{kept}…")
    };
    assert_eq!(drawn, expected, "{text:?} in {width}, {align:?}");
}
