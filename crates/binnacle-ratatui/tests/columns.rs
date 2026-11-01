//! Text in terminal columns: wide, combining and joined text drawn through
//! the text renderer into a buffer.

use binnacle_core::{AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    Align, CellRenderer, Content, RenderContext, TextRenderer, Theme, fit_width, render_with,
};
use hegel::TestCase;
use hegel::generators as gs;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// The row header's width, so the cell starts at this column.
const HEADER: u16 = 2;
/// Columns past the cell that must stay blank.
const MARGIN: u16 = 4;

struct One(String, Align);

impl Content for One {
    fn cell(&self, _row: Key, _col: Key) -> String {
        self.0.clone()
    }

    fn column_header(&self, _col: Key) -> String {
        "h".to_owned()
    }

    fn column_renderer(&self, _col: Key) -> Box<dyn CellRenderer + '_> {
        let text = self.0.clone();
        Box::new(TextRenderer::new(move |_, _| text.clone()).align(self.1))
    }
}

/// Draws `text` in one cell `width` wide; the buffer's line of the cell.
fn draw(text: &str, align: Align, width: u16) -> (Buffer, u16) {
    let area = Rect::new(0, 0, HEADER + width + MARGIN, 2);
    let mut grid = Grid::new(
        AxisConfig::new(1, 1.0).header(1.0),
        AxisConfig::new(1, f32::from(width)).header(f32::from(HEADER)),
    );
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end.min(1)).window(range.start..range.end.min(1));
        grid.set_window(axis, range.start, &keys);
    }
    let theme = Theme::classic();
    let mut buf = Buffer::empty(area);
    let content = One(text.to_owned(), align);
    render_with(
        &grid.snapshot(),
        &content,
        &RenderContext::new(&theme).scrollbars(false),
        area,
        &mut buf,
    );
    (buf, 1)
}

/// The symbols on `line` from column `from` to `to`, skipping the cell a
/// wide symbol covers.
fn symbols(buf: &Buffer, line: u16, from: u16, to: u16) -> Vec<String> {
    let mut out = Vec::new();
    let mut x = from;
    while x < to {
        let s = buf[(x, line)].symbol().to_owned();
        x += u16::try_from(s.width().max(1)).unwrap();
        out.push(s);
    }
    out
}

fn drawn(text: &str, align: Align, width: u16) -> String {
    let (buf, line) = draw(text, align, width);
    symbols(&buf, line, HEADER, HEADER + width).concat()
}

#[test]
fn two_adjacent_wide_emoji_both_draw() {
    assert_eq!(drawn("🚢🚢 ok", Align::Left, 8), "🚢🚢 ok ");
}

#[test]
fn wide_text_right_aligned_ends_at_the_right_edge() {
    let (buf, line) = draw("日本語", Align::Right, 10);
    let cells = symbols(&buf, line, HEADER, HEADER + 10);
    assert_eq!(cells.concat(), "    日本語");
    assert_eq!(buf[(HEADER + 4, line)].symbol(), "日");
}

#[test]
fn wide_text_centred_is_centred_by_columns() {
    assert_eq!(drawn("日本", Align::Center, 10), "   日本   ");
}

#[test]
fn a_combining_mark_joins_its_letter() {
    let (buf, line) = draw("e\u{301}x", Align::Left, 4);
    assert_eq!(buf[(HEADER, line)].symbol(), "e\u{301}");
    assert_eq!(buf[(HEADER + 1, line)].symbol(), "x");
}

#[test]
fn text_cut_at_an_odd_width_ends_in_an_ellipsis_never_half_a_glyph() {
    let (buf, line) = draw("日本語", Align::Left, 5);
    assert_eq!(symbols(&buf, line, HEADER, HEADER + 5).concat(), "日本…");
    for x in HEADER + 5..HEADER + 5 + MARGIN {
        assert_eq!(buf[(x, line)].symbol(), " ", "past the cell at {x}");
    }
}

/// Draws straight through the canvas: `日` at the last column of a 3-wide cell.
struct Straddle;

impl CellRenderer for Straddle {
    fn draw(&self, _row: Key, _col: Key, canvas: &mut binnacle_ratatui::CellCanvas) {
        canvas.print(2, 0, "日本", ratatui::style::Style::default());
    }

    fn preferred_width(&self, _row: Key, _col: Key) -> u32 {
        4
    }
}

struct Straddling;

impl Content for Straddling {
    fn cell(&self, _row: Key, _col: Key) -> String {
        String::new()
    }

    fn column_header(&self, _col: Key) -> String {
        "h".to_owned()
    }

    fn column_renderer(&self, _col: Key) -> Box<dyn CellRenderer + '_> {
        Box::new(Straddle)
    }
}

#[test]
fn a_wide_character_straddling_the_right_edge_draws_a_space() {
    let area = Rect::new(0, 0, HEADER + 3 + MARGIN, 2);
    let mut grid = Grid::new(
        AxisConfig::new(1, 1.0).header(1.0),
        AxisConfig::new(1, 3.0).header(f32::from(HEADER)),
    );
    for (axis, range) in grid.begin_frame(Viewport::new(area.width.into(), area.height.into())) {
        let keys = IdentitySource::new(range.end.min(1)).window(range.start..range.end.min(1));
        grid.set_window(axis, range.start, &keys);
    }
    let theme = Theme::classic();
    let mut buf = Buffer::empty(area);
    render_with(
        &grid.snapshot(),
        &Straddling,
        &RenderContext::new(&theme).scrollbars(false),
        area,
        &mut buf,
    );
    assert_eq!(symbols(&buf, 1, HEADER, area.width).concat(), "       ");
}

#[test]
fn auto_size_prefers_the_columns_not_the_chars() {
    let renderer = TextRenderer::new(|_, _| "日本語".to_owned());
    assert_eq!(renderer.preferred_width(Key(0), Key(0)), 6);
    let content = One("日本語".to_owned(), Align::Left);
    assert_eq!(fit_width(&content, &[Key(0)], Key(0)), 6);
}

fn mixed(tc: &TestCase) -> String {
    let pieces = [
        "a",
        "Z",
        " ",
        "日",
        "本",
        "e\u{301}",
        "\u{301}",
        "🚢",
        "👩‍💻",
        "🇬🇧",
    ];
    tc.draw(gs::vecs(gs::sampled_from(pieces.to_vec())).max_size(12))
        .concat()
}

#[hegel::test(test_cases = 500)]
fn nothing_draws_outside_the_cell_and_what_draws_is_a_prefix_of_the_text(tc: TestCase) {
    let text = mixed(&tc);
    // A combining mark with no letter before it has no cell to join.
    tc.assume(text.graphemes(true).next().is_none_or(|g| g.width() > 0));
    let width = tc.draw(gs::integers::<u16>().min_value(1).max_value(20));
    let (buf, line) = draw(&text, Align::Left, width);
    let a = buf.area;
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            let inside = y == line && (HEADER..HEADER + width).contains(&x);
            if !inside && y == line && x >= HEADER + width {
                assert_eq!(buf[(x, y)].symbol(), " ", "({x},{y}) past the cell");
            }
        }
    }
    let shown = symbols(&buf, line, HEADER, HEADER + width).concat();
    let shown = shown.trim_end();
    let shown = shown.strip_suffix('…').unwrap_or(shown);
    let ours: Vec<&str> = shown.graphemes(true).collect();
    let theirs: Vec<&str> = text.graphemes(true).collect();
    assert!(
        theirs.starts_with(&ours),
        "{shown:?} is not a prefix of {text:?} at {width}"
    );
}
