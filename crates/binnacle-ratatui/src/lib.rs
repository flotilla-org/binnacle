//! binnacle-ratatui: the reference ratatui adapter for the binnacle Grid.
//!
//! Sizes are whole terminal cells. [`render`] draws a Snapshot into a ratatui
//! `Buffer`; the host supplies the text through [`Content`], since the Grid
//! never reads cell values.

mod keymap;
mod mouse;

pub use keymap::Keymap;
pub use mouse::Mouse;

use binnacle_core::{Action, Axis, Cursor, HeaderSort, Key, Mode, Snapshot, SortDirection};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

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
/// cell is drawn reversed, other selected cells on a dark grey background,
/// and the header under the pointer underlined. Header Gaps are drawn as
/// lines, the handles a resize drag grabs, and a resize in progress draws its
/// guide line across the area in yellow, a header move its drop indicator in
/// cyan. A sorted column's header ends in its direction (and place, in a
/// multi-column sort), and `~` marks a sort change still pending.
pub fn render(snapshot: &Snapshot, content: &impl Content, area: Rect, buf: &mut Buffer) {
    let area = area.intersection(buf.area);
    let header = Style::default().add_modifier(Modifier::BOLD);
    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    // Pads `text` so a style fills its item; `put` clips it.
    let pad = |text: String| format!("{text:<width$}", width = usize::from(area.width));
    let header_style = |hovered| {
        if hovered {
            header.add_modifier(Modifier::UNDERLINED)
        } else {
            header
        }
    };
    let multi = snapshot.applied_sort().columns().len() > 1;
    for h in snapshot.headers(Axis::Cols) {
        let label = h
            .key
            .map_or_else(|| LOADING.to_string(), |k| content.column_header(k));
        let marker = sort_marker(h.sort, multi);
        // The marker keeps the header's last cells; the label gets the rest.
        let (start, end) = (h.rect.x, h.rect.x + h.rect.width);
        let width = usize::try_from(cell(end).saturating_sub(cell(start))).unwrap_or(0);
        let room = width.saturating_sub(marker.chars().count());
        let text = if marker.is_empty() {
            pad(label)
        } else {
            format!("{label:<room$.room$}{marker}")
        };
        let style = header_style(h.hovered);
        put(buf, area, (start, 0.0), (left, end, 0.0), &text, style);
    }
    for (r, h) in snapshot.rows().iter().zip(snapshot.headers(Axis::Rows)) {
        let number = r.position.saturating_add(1).to_string();
        let style = header_style(h.hovered);
        put(
            buf,
            area,
            (0.0, r.offset),
            (0.0, left, top),
            &pad(number),
            style,
        );
        for c in snapshot.cols() {
            let mut text = match (r.key, c.key) {
                (Some(row), Some(col)) => content.cell(row, col),
                _ => LOADING.to_string(),
            };
            let state = snapshot.cell_state(r, c);
            let mut style = Style::default();
            if state.focused {
                text = pad(text);
                style = style.add_modifier(Modifier::REVERSED);
            } else if state.selected {
                text = pad(text);
                style = style.bg(Color::DarkGray);
            }
            let clip = (left, c.end(), top);
            put(buf, area, (c.offset, r.offset), clip, &text, style);
        }
    }
    let mut mark = |x: i64, y: i64, ch: char, style: Style| {
        if (0..i64::from(area.width)).contains(&x) && (0..i64::from(area.height)).contains(&y) {
            let (x, y) = (area.x + x as u16, area.y + y as u16);
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(ch).set_style(style);
            }
        }
    };
    let handle = Style::default().fg(Color::DarkGray);
    if snapshot.gap(Axis::Cols) > 0.0 {
        for c in snapshot.cols().iter().filter(|c| c.end() >= left) {
            for y in 0..cell(top) {
                mark(cell(c.end()), y, '│', handle);
            }
        }
    }
    if snapshot.gap(Axis::Rows) > 0.0 {
        for r in snapshot.rows().iter().filter(|r| r.end() >= top) {
            for x in 0..cell(left) {
                mark(x, cell(r.end()), '─', handle);
            }
        }
    }
    if let Some(drop) = snapshot.drop_indicator() {
        let style = Style::default().fg(Color::Cyan);
        // The middle of a Gap rounds down onto the Gap's cell.
        let at = drop.offset.floor() as i64;
        match drop.axis {
            Axis::Cols => {
                for y in 0..i64::from(area.height) {
                    mark(at, y, '┃', style);
                }
            }
            Axis::Rows => {
                for x in 0..i64::from(area.width) {
                    mark(x, at, '━', style);
                }
            }
        }
    }
    if let Some(guide) = snapshot.resize_guide() {
        let style = Style::default().fg(Color::Yellow);
        let at = cell(guide.offset);
        match guide.axis {
            Axis::Cols if at >= cell(left) => {
                for y in 0..i64::from(area.height) {
                    mark(at, y, '│', style);
                }
            }
            Axis::Rows if at >= cell(top) => {
                for x in 0..i64::from(area.width) {
                    mark(x, at, '─', style);
                }
            }
            _ => {}
        }
    }
}

/// Draws a minimal inline editor over the edited cell, if it is visible:
/// `text` in black on yellow across the cell, ending in a `▏` caret, showing
/// its end when it is too long. Clipped like [`render`]'s cells. Call it
/// after [`render`]; the host keeps the text and its own key handling.
pub fn render_editor(snapshot: &Snapshot, text: &str, area: Rect, buf: &mut Buffer) {
    let Some(rect) = snapshot.editor_rect() else {
        return;
    };
    let area = area.intersection(buf.area);
    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    let (x0, x1) = (cell(rect.x), cell(rect.x + rect.width));
    let y = cell(rect.y);
    let width = usize::try_from(x1 - x0).unwrap_or(0);
    if width == 0 || y < cell(top) || y >= i64::from(area.height) {
        return;
    }
    let chars: Vec<char> = text.chars().collect();
    let shown = &chars[chars.len().saturating_sub(width - 1)..];
    let line: String = shown.iter().chain(['▏'].iter()).collect();
    let style = Style::default().fg(Color::Black).bg(Color::Yellow);
    let line = format!("{line:<width$}");
    let clip = (left, f32::INFINITY, top);
    put(buf, area, (rect.x, rect.y), clip, &line, style);
}

/// Writes `text` from `(x, y)` into `area` of `buf`, keeping the characters
/// in `[x_min, x_end)` on rows at or below `y_min`, all in cells relative to
/// `area`.
fn put(
    buf: &mut Buffer,
    area: Rect,
    (x, y): (f32, f32),
    (x_min, x_end, y_min): (f32, f32, f32),
    text: &str,
    style: Style,
) {
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
}

/// A status line for `snapshot`: the gesture in progress, or a hint when the
/// pointer is over a header border. A terminal cannot change the pointer's
/// shape, so this stands in for the cursor hint. Empty when there is nothing
/// to say.
pub fn status(snapshot: &Snapshot) -> String {
    let noun = |axis| match axis {
        Axis::Rows => "row",
        Axis::Cols => "column",
    };
    match snapshot.mode() {
        Mode::Resizing { axis, key, .. } => match snapshot.resize_guide() {
            Some(guide) => format!("resizing {} {}: {}", noun(axis), key.0, guide.size),
            None => format!("resizing {} {}", noun(axis), key.0),
        },
        Mode::DraggingSelection => "selecting".to_string(),
        Mode::Editing { row, col } => format!("editing row {} column {}", row.0, col.0),
        Mode::MovingHeaders { axis, to, .. } => {
            let keys: Vec<String> = snapshot
                .moving_keys()
                .iter()
                .map(|k| k.0.to_string())
                .collect();
            let plural = if keys.len() == 1 { "" } else { "s" };
            format!("moving {}{plural} {} to {to}", noun(axis), keys.join(","))
        }
        Mode::Idle => {
            let axis = match snapshot.cursor() {
                Cursor::ResizeCol => Axis::Cols,
                Cursor::ResizeRow => Axis::Rows,
                Cursor::Default | Cursor::Move => return String::new(),
            };
            format!("drag to resize the {}, double-click to fit", noun(axis))
        }
    }
}

/// How a column header shows the sort: `▲` ascending or `▼` descending, with
/// the column's place when several columns sort, then `~` while a request
/// that changes it is unanswered.
fn sort_marker(sort: HeaderSort, multi: bool) -> String {
    let mut marker = String::new();
    if let Some((direction, order)) = sort.applied {
        marker.push(match direction {
            SortDirection::Ascending => '▲',
            SortDirection::Descending => '▼',
        });
        if multi {
            marker += &(order + 1).to_string();
        }
    }
    if sort.pending {
        marker.push('~');
    }
    marker
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
