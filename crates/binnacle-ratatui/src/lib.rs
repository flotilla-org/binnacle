//! binnacle-ratatui: the reference ratatui adapter for the binnacle Grid.
//!
//! Sizes are whole terminal cells. [`render`] draws a Snapshot into a ratatui
//! `Buffer`; the host supplies the text through [`Content`], since the Grid
//! never reads cell values.

mod keymap;

pub use keymap::Keymap;

use binnacle_core::{Action, Axis, Key, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

/// Shown in a cell or header whose Key the Table Source has not filled yet.
const LOADING: &str = "…";

/// Cells one wheel notch scrolls.
const WHEEL_STEP: f32 = 3.0;

/// The scroll Action for a mouse wheel event, if it is one.
pub fn wheel(event: &MouseEvent) -> Option<Action> {
    let (dx, dy) = match event.kind {
        MouseEventKind::ScrollDown => (0.0, WHEEL_STEP),
        MouseEventKind::ScrollUp => (0.0, -WHEEL_STEP),
        MouseEventKind::ScrollRight => (WHEEL_STEP, 0.0),
        MouseEventKind::ScrollLeft => (-WHEEL_STEP, 0.0),
        _ => return None,
    };
    Some(Action::Scroll { dx, dy })
}

/// The host's text for the Keys the Grid places.
pub trait Content {
    fn cell(&self, row: Key, col: Key) -> String;
    fn column_header(&self, col: Key) -> String;
}

/// Draws `snapshot` into `area` of `buf`: column headers along the top band,
/// row numbers (Position plus one) down the left band, then each cell. Text is
/// clipped to its item, to `area`, and for cells to the body past both header
/// bands, so a part-scrolled item shows only its visible part. The focused
/// cell is drawn reversed.
pub fn render(snapshot: &Snapshot, content: &impl Content, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    let header = Style::default().add_modifier(Modifier::BOLD);
    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    // Writes `text` from (x, y), keeping the characters in `[x_min, x_end)` on
    // rows at or below `y_min`, all in cells relative to `area`.
    let mut put = |x: f32, y: f32, x_min: f32, x_end: f32, y_min: f32, text: &str, style| {
        let y = cell(y.max(y_min));
        let (x, x_min, x_end) = (cell(x), cell(x_min), cell(x_end).min(i64::from(area.width)));
        if y >= i64::from(area.height) {
            return;
        }
        for (i, ch) in text.chars().enumerate() {
            let cx = x.saturating_add(i as i64);
            if cx >= x_end {
                break;
            }
            if cx < x_min.max(0) {
                continue;
            }
            let (cx, cy) = (area.x + cx as u16, area.y + y as u16);
            if let Some(c) = buf.cell_mut((cx, cy)) {
                c.set_char(ch).set_style(style);
            }
        }
    };
    for c in snapshot.cols() {
        let label = c
            .key
            .map_or_else(|| LOADING.to_string(), |k| content.column_header(k));
        put(c.offset, 0.0, left, c.end(), 0.0, &label, header);
    }
    for r in snapshot.rows() {
        let number = r.position.saturating_add(1).to_string();
        put(0.0, r.offset, 0.0, left, top, &number, header);
        for c in snapshot.cols() {
            let mut text = match (r.key, c.key) {
                (Some(row), Some(col)) => content.cell(row, col),
                _ => LOADING.to_string(),
            };
            let mut style = Style::default();
            if snapshot.cell_state(r, c).focused {
                // Pad so the highlight fills the cell; `put` clips it.
                let width = usize::from(area.width);
                text = format!("{text:<width$}");
                style = style.add_modifier(Modifier::REVERSED);
            }
            put(c.offset, r.offset, left, c.end(), top, &text, style);
        }
    }
}

/// A viewport-relative coordinate as a whole terminal cell (negative when
/// before the area's edge). Rounding up maps contiguous part-scrolled items to
/// distinct cells, each starting at the first cell it reaches.
fn cell(v: f32) -> i64 {
    if v.is_finite() {
        v.ceil() as i64
    } else if v > 0.0 {
        i64::MAX
    } else {
        i64::MIN
    }
}
