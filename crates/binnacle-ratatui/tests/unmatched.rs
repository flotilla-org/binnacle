//! Unmatched rows in the ratatui adapter: a row the host's Table Source
//! keeps after the user's edit though the filter drops it is drawn in the
//! Theme's `unmatched` style, under the focus and selection.

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, RenderContext, Theme, render_with};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

/// Cells read `r<row>c<col>`; the rows in `unmatched` are unmatched.
struct Rows(Vec<u64>);

impl Content for Rows {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }

    fn unmatched(&self, axis: Axis, key: Key) -> bool {
        axis == Axis::Rows && self.0.contains(&key.0)
    }
}

/// 4 rows by 3 columns of 6, focus on row 2.
fn draw(theme: &Theme, unmatched: Vec<u64>) -> Buffer {
    let mut grid = Grid::new(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(3.0),
    );
    for (axis, range) in grid.begin_frame(Viewport::new(21.0, 5.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.pointer(binnacle_core::PointerEvent::down(5.0, 3.5));
    grid.pointer(binnacle_core::PointerEvent::up(5.0, 3.5));
    for (axis, range) in grid.begin_frame(Viewport::new(21.0, 5.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let ctx = RenderContext::new(theme).scrollbars(false);
    let area = Rect::new(0, 0, 21, 5);
    let mut buf = Buffer::empty(area);
    render_with(&grid.snapshot(), &Rows(unmatched), &ctx, area, &mut buf);
    buf
}

fn describe(style: Style) -> String {
    let colour = |c: Option<Color>| c.map_or("-".to_string(), |c| c.to_string());
    let mut out = format!("fg {} bg {}", colour(style.fg), colour(style.bg));
    if !style.add_modifier.is_empty() {
        out += &format!(" {:?}", style.add_modifier);
    }
    out
}

/// The text, then each cell's style as a letter, then what each letter
/// stands for.
fn styled(buf: &Buffer) -> String {
    let a = buf.area;
    let mut legend: Vec<Style> = Vec::new();
    let (mut text, mut map) = (String::new(), String::new());
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            text += buf[(x, y)].symbol();
            let style = buf[(x, y)].style();
            let i = legend.iter().position(|&s| s == style).unwrap_or_else(|| {
                legend.push(style);
                legend.len() - 1
            });
            map.push(char::from(b'a' + i as u8));
        }
        text = text.trim_end().to_string() + "\n";
        map.push('\n');
    }
    let legend: String = legend
        .iter()
        .enumerate()
        .map(|(i, &s)| format!("{}: {}\n", char::from(b'a' + i as u8), describe(s)))
        .collect();
    format!("{text}\n{map}\n{legend}")
}

#[test]
fn an_unmatched_row_is_drawn_dimmed_under_the_focus() {
    let theme = Theme::classic();
    let buf = draw(&theme, vec![1, 2]);
    insta::assert_snapshot!(styled(&buf));
    let dim = theme.unmatched.fg.unwrap();
    // Row 1 is unmatched; row 0 is not.
    assert_eq!(buf[(4, 2)].fg, dim);
    assert_ne!(buf[(4, 1)].fg, dim);
    // Row 2's cells are dimmed, and its focused cell still shows the focus.
    assert_eq!(buf[(10, 3)].fg, dim);
    assert!(buf[(4, 3)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn the_chords_preset_re_applies_the_filter_with_alt_l_or_ctrl_alt_l() {
    use binnacle_ratatui::{Control, Controls, Preset};
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let controls = Controls::preset(Preset::Chords);
    let at = |modifiers| controls.control(&KeyEvent::new(KeyCode::Char('l'), modifiers));
    assert_eq!(at(KeyModifiers::ALT), Some(Control::Reapply));
    assert_eq!(
        at(KeyModifiers::CONTROL | KeyModifiers::ALT),
        Some(Control::Reapply)
    );
    assert_eq!(at(KeyModifiers::CONTROL), Some(Control::QuickFilter));
}
