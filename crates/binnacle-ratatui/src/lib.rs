//! binnacle-ratatui: the reference ratatui adapter for the binnacle Grid.
//!
//! Sizes are whole terminal cells. [`render`] draws a Snapshot into a ratatui
//! `Buffer`; the host supplies the text through [`Content`], since the Grid
//! never reads cell values.

use binnacle_core::{Axis, Key, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

/// Shown in a cell or header whose Key the Table Source has not filled yet.
const LOADING: &str = "…";

/// The host's text for the Keys the Grid places.
pub trait Content {
    fn cell(&self, row: Key, col: Key) -> String;
    fn column_header(&self, col: Key) -> String;
}

/// Draws `snapshot` into `area` of `buf`: column headers along the top band,
/// row numbers (Position plus one) down the left band, then each cell. Text is
/// clipped to its item and to `area`.
pub fn render(snapshot: &Snapshot, content: &impl Content, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    let header = Style::default().add_modifier(Modifier::BOLD);
    let row_band = snapshot.header(Axis::Cols);
    let mut put = |x: f32, y: f32, end: f32, text: &str, style: Style| {
        let (Some(x), Some(y), Some(end)) = (cell(x), cell(y), cell(end)) else {
            return;
        };
        let y = area.y.saturating_add(y);
        let end = area.x.saturating_add(end).min(area.right());
        if y >= area.bottom() {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let x = area
                .x
                .saturating_add(x)
                .saturating_add(i.try_into().unwrap_or(u16::MAX));
            if x >= end {
                break;
            }
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(ch).set_style(style);
            }
        }
    };
    for c in snapshot.cols() {
        let label = c
            .key
            .map_or_else(|| LOADING.to_string(), |k| content.column_header(k));
        put(c.offset, 0.0, c.end(), &label, header);
    }
    for r in snapshot.rows() {
        put(
            0.0,
            r.offset,
            row_band,
            &(r.position + 1).to_string(),
            header,
        );
        for c in snapshot.cols() {
            let text = match (r.key, c.key) {
                (Some(row), Some(col)) => content.cell(row, col),
                _ => LOADING.to_string(),
            };
            put(c.offset, r.offset, c.end(), &text, Style::default());
        }
    }
}

/// A viewport-relative coordinate as a whole terminal cell, if it is on screen.
fn cell(v: f32) -> Option<u16> {
    (v.is_finite() && v >= 0.0).then(|| v.floor().min(f32::from(u16::MAX)) as u16)
}
