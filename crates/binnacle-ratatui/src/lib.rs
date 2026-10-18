//! binnacle-ratatui: the reference ratatui adapter for the binnacle Grid.
//!
//! Sizes are whole terminal cells. [`render`] draws a Snapshot into a ratatui
//! `Buffer` in a [`Theme`]'s palette; the host supplies the text through [`Content`], since the Grid
//! never reads cell values.

mod border;
mod checklist;
mod context;
mod filter;
mod find;
mod keymap;
mod mouse;
mod renderer;
mod scrollbar;
mod shimmer;
mod theme;

pub use border::{BorderExtent, BorderStyle, BorderWeight};
pub use context::RenderContext;
pub use filter::{
    ColumnFilter, Control, Controls, Preset, QuickFilter, render_conditions, row_count,
};
pub use find::{FindBar, FindStatus};
pub use keymap::Keymap;
pub use mouse::Mouse;
pub use renderer::{
    Align, BAR, BarRenderer, CellCanvas, CellRenderer, CellStatus, FLASH, SPARK, SparklineRenderer,
    TextRenderer, fit_width, flashing,
};
use renderer::{char_count, fit};
pub use scrollbar::{BarEvent, Layout, begin_frame, layout};
pub use shimmer::Shimmer;
pub use theme::{ColorSupport, Theme};

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

/// The host's text for the Keys the Grid places, and how each column draws.
pub trait Content {
    fn cell(&self, row: Key, col: Key) -> String;
    fn column_header(&self, col: Key) -> String;

    /// The row header label for the row with Key `row` at `position`:
    /// Position plus one unless overridden. A row whose Key the Table
    /// Source has not filled yet always shows Position plus one.
    fn row_header(&self, row: Key, position: u64) -> String {
        let _ = row;
        position.saturating_add(1).to_string()
    }

    /// Whether the cell matches the Find Query in force, as the host's Table
    /// Source says for the cells shown: no cell unless overridden.
    fn marked(&self, row: Key, col: Key) -> bool {
        let _ = (row, col);
        false
    }

    /// Column `col`'s Cell Renderer: left-aligned [`Content::cell`] text
    /// unless overridden.
    fn renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        let _ = col;
        Box::new(TextRenderer::new(move |row, col| self.cell(row, col)))
    }
}

/// Whether anything [`render_with`] draws from `snapshot` moves with time:
/// the Shimmer on a visible header whose requested sort is not applied yet,
/// or on the visible focused cell while its Locate is pending. A host
/// redraws on a timer only while this is true (or its own data ticks).
pub fn animating(snapshot: &Snapshot) -> bool {
    let sorting = snapshot.headers(Axis::Cols).any(|h| h.sort.pending);
    sorting || locating(snapshot) && snapshot.focus_rect().is_some()
}

/// Whether the focused cell is waiting for a Locate on either axis.
fn locating(snapshot: &Snapshot) -> bool {
    Axis::ALL.into_iter().any(|a| snapshot.locating(a))
}

/// Draws `snapshot` into `area` of `buf` in the `classic` Theme with
/// header-only lines: [`render_with`] for a host that picks neither.
pub fn render(snapshot: &Snapshot, content: &impl Content, area: Rect, buf: &mut Buffer) {
    let theme = Theme::classic();
    render_with(snapshot, content, &RenderContext::new(&theme), area, buf);
}

/// Draws `snapshot` into `area` of `buf` as `ctx` says: column headers along
/// the top band, row header labels ([`Content::row_header`]) down the left
/// band, then
/// each cell. Text is clipped to its item, to `area`, and for cells to the
/// body past both header bands, so a part-scrolled item shows only its
/// visible part. Gaps are drawn as `ctx.border` says (by default the header
/// band's Gaps only, the handles a resize drag grabs); a resize in progress
/// draws its guide line across the area, a header move its drop indicator,
/// both over the borders. Unless `ctx` turns them off, scrollbars take the
/// area's right column and, when the columns overflow, its bottom line (see
/// [`layout`]; [`begin_frame`] gives the Grid the Viewport left over). A sorted column's header ends in its
/// direction (and place, in a multi-column sort), and `~` marks a sort
/// change still pending. Every style comes from the Theme. A Shimmer at
/// `ctx.now` sweeps across the headers of a pending sort and across the
/// focused cell while its Locate is pending (see [`animating`]).
pub fn render_with(
    snapshot: &Snapshot,
    content: &impl Content,
    ctx: &RenderContext,
    area: Rect,
    buf: &mut Buffer,
) {
    let theme = &ctx.theme.for_colors(ctx.colors);
    let area = area.intersection(buf.area);
    // The sweep keeps its pace across the whole area, scrollbars or not.
    let sweep = Shimmer::new(ctx, area.width);
    let bars = layout(snapshot, ctx, area);
    scrollbar::draw(snapshot, &bars, theme, buf);
    let area = bars.grid;
    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    // Pads `text` so a style fills its item; `put` clips it.
    let pad = |text: String| format!("{text:<width$}", width = usize::from(area.width));
    let band = |style: Style, hovered: bool| {
        let style = if theme.invert_headers {
            style.add_modifier(Modifier::REVERSED)
        } else {
            style
        };
        if hovered {
            style.patch(theme.header_hovered)
        } else {
            style
        }
    };
    let multi = snapshot.applied_view().rows.sort.columns().len() > 1;
    let area_end = i64::from(area.width);
    for h in snapshot.headers(Axis::Cols) {
        let label = h
            .key
            .map_or_else(|| LOADING.to_string(), |k| content.column_header(k));
        let requested = snapshot.requested_view().map(|v| &v.rows.filter.conditions);
        let filtered = match (h.filter.pending, h.key, requested) {
            (true, Some(key), Some(conditions)) => conditions.on(key).next().is_some(),
            _ => h.filter.filtered,
        };
        let marker = header_marker(h.sort, filtered, h.filter.pending, multi);
        // The marker keeps the header's last cells; the label gets the rest.
        let (start, end) = (h.rect.x, h.rect.x + h.rect.width);
        let width = usize::try_from(cell(end).saturating_sub(cell(start))).unwrap_or(0);
        let room = width.saturating_sub(marker.chars().count());
        let style = band(theme.header, h.hovered);
        let clip = (left, end, 0.0);
        let label = fit(&label, u32::try_from(room).unwrap_or(u32::MAX));
        if marker.is_empty() {
            put(
                buf,
                area,
                (start, 0.0),
                clip,
                &pad(label.into_owned()),
                style,
            );
        } else {
            let label = format!("{label:<room$}");
            put(buf, area, (start, 0.0), clip, &label, style);
            let at = start + room as f32;
            let style = style.patch(theme.sort_marker);
            put(buf, area, (at, 0.0), clip, &marker, style);
        }
        if h.key.is_none() {
            let style = style.patch(theme.loading);
            put(buf, area, (start, 0.0), clip, LOADING, style);
        }
        if h.sort.pending {
            let (x0, x1) = (cell(start).max(cell(left)), cell(end).min(area_end));
            shimmer(buf, area, (x0..x1, 0), &sweep, 0);
        }
    }
    let locating = locating(snapshot);
    let picked = snapshot.picked();
    let renderers: Vec<_> = (snapshot.cols().iter())
        .map(|c| c.key.map(|k| content.renderer(k)))
        .collect();
    let hovered_col = match snapshot.hovered_header() {
        Some((Axis::Cols, c)) => Some(c.position),
        _ => None,
    };
    for (r, h) in snapshot.rows().iter().zip(snapshot.headers(Axis::Rows)) {
        let number = r.key.map_or_else(
            || r.position.saturating_add(1).to_string(),
            |k| content.row_header(k, r.position),
        );
        let number = fit(&number, u32::try_from(cell(left)).unwrap_or(0)).into_owned();
        let style = band(theme.row_number, h.hovered);
        put(
            buf,
            area,
            (0.0, r.offset),
            (0.0, left, top),
            &pad(number),
            style,
        );
        let mut row_style = theme.body;
        if let Some(zebra) = theme.zebra.filter(|_| r.position % 2 == 1) {
            row_style = row_style.patch(zebra);
            let blank = " ".repeat(usize::from(area.width));
            for y in cell(r.offset.max(top))..cell(r.end()) {
                let clip = (left, f32::INFINITY, top);
                put(buf, area, (0.0, y as f32), clip, &blank, row_style);
            }
        }
        let in_picked = r.key.is_some_and(|k| picked.contains(&k));
        let row_hovered = h.hovered;
        for (c, renderer) in snapshot.cols().iter().zip(&renderers) {
            let state = snapshot.cell_state(r, c);
            let status = CellStatus {
                focused: state.focused,
                selected: state.selected && !in_picked,
                picked: in_picked,
                hovered: row_hovered || hovered_col == Some(c.position),
            };
            let over = if status.focused {
                theme.focus
            } else if status.picked {
                theme.picks
            } else if status.selected {
                theme.selection
            } else {
                Style::default()
            };
            let (x0, y0) = (cell(c.offset), cell(r.offset));
            let (x1, y1) = (cell(c.end()), cell(r.end()));
            let clip = |lo: i64, hi: i64, min: i64, max: u16| {
                let lo = lo.max(min).max(0).min(i64::from(max));
                (lo, hi.min(i64::from(max)).max(lo))
            };
            let (vx0, vx1) = clip(x0, x1, cell(left), area.width);
            let (vy0, vy1) = clip(y0, y1, cell(top), area.height);
            let visible = Rect::new(
                area.x + vx0 as u16,
                area.y + vy0 as u16,
                (vx1 - vx0) as u16,
                (vy1 - vy0) as u16,
            );
            let size = |lo: i64, hi: i64| u32::try_from(hi - lo).unwrap_or(0);
            let mut canvas = CellCanvas {
                buf,
                origin: (i64::from(area.x) + x0, i64::from(area.y) + y0),
                size: (size(x0, x1), size(y0, y1)),
                visible,
                status,
                theme,
                now: ctx.now,
                colors: ctx.colors,
            };
            let blank = " ".repeat(visible.width.into());
            for y in canvas.visible_y() {
                let x = canvas.visible_x().start;
                canvas.print(x, y, &blank, row_style);
            }
            match (r.key, c.key, renderer) {
                (Some(row), Some(col), Some(renderer)) => {
                    renderer.draw(row, col, &mut canvas);
                    if state.current_match {
                        canvas.patch(theme.find_current);
                    } else if content.marked(row, col) {
                        canvas.patch(theme.find_match);
                    }
                    canvas.patch(over);
                    if state.current_match {
                        let (x, y) = (canvas.visible_x(), canvas.visible_y());
                        let style = theme.body.patch(theme.find_current).patch(over);
                        if x.end > x.start && y.end > y.start {
                            canvas.print(x.end - 1, y.start, "◂", style);
                        }
                    }
                }
                _ => {
                    let y = canvas.visible_y().start;
                    canvas.patch(over);
                    canvas.print(0, y, LOADING, row_style.patch(over).patch(theme.loading));
                }
            }
            if state.focused && locating {
                let (x0, x1) = (cell(c.offset).max(cell(left)), cell(c.end()).min(area_end));
                let pulse = Shimmer::new(ctx, c.size.ceil().clamp(0.0, 65535.0) as u16);
                let y = cell(r.offset.max(top));
                shimmer(buf, area, (x0..x1, y), &pulse, cell(c.offset));
            }
        }
    }
    if Axis::ALL.into_iter().any(|a| snapshot.count(a) == 0) {
        empty(ctx.empty, theme, (left, top), area, buf);
    }
    border::draw(snapshot, ctx.border, theme, area, buf);
    let mut mark = |x: i64, y: i64, ch: char, style: Style| {
        if (0..i64::from(area.width)).contains(&x) && (0..i64::from(area.height)).contains(&y) {
            let (x, y) = (area.x + x as u16, area.y + y as u16);
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(ch).set_style(style);
            }
        }
    };
    if let Some(drop) = snapshot.drop_indicator() {
        let style = theme.drop_indicator;
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
        let style = theme.guide;
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

/// Writes `message` centred in the body, the part of `area` past the header
/// bands that end at `(left, top)`, on its middle line.
fn empty(message: &str, theme: &Theme, (left, top): (f32, f32), area: Rect, buf: &mut Buffer) {
    let (x0, y0) = (cell(left).max(0), cell(top).max(0));
    let (width, height) = (i64::from(area.width) - x0, i64::from(area.height) - y0);
    if width <= 0 || height <= 0 {
        return;
    }
    let message = fit(message, u32::try_from(width).unwrap_or(u32::MAX));
    let room = width - i64::from(char_count(&message));
    let at = ((x0 + room / 2) as f32, (y0 + (height - 1) / 2) as f32);
    let style = theme.body.patch(theme.loading);
    put(buf, area, at, (left, f32::INFINITY, top), &message, style);
}

/// Patches `sweep` over the cells `xs` of line `y` in `area` (relative to
/// it), each by its place from `origin`.
fn shimmer(
    buf: &mut Buffer,
    area: Rect,
    (xs, y): (std::ops::Range<i64>, i64),
    sweep: &Shimmer,
    origin: i64,
) {
    if !(0..i64::from(area.height)).contains(&y) {
        return;
    }
    for x in xs.start.max(0)..xs.end.min(i64::from(area.width)) {
        let at = u16::try_from(x - origin).unwrap_or(u16::MAX);
        if let Some(c) = buf.cell_mut((area.x + x as u16, area.y + y as u16)) {
            c.set_style(sweep.style(at));
        }
    }
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
        Mode::ResizingBand { axis, .. } => {
            let band = match axis {
                Axis::Rows => "column header",
                Axis::Cols => "row-number",
            };
            format!("resizing the {band} band: {}", snapshot.header(axis))
        }
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

/// How a column header shows its conditions and the sort: `▿` while the
/// column is filtered (or will be, once a pending request is answered), `▲`
/// ascending or `▼` descending, with the column's place when several columns
/// sort, then `~` while a request that changes either is unanswered.
fn header_marker(sort: HeaderSort, filtered: bool, filter_pending: bool, multi: bool) -> String {
    let mut marker = String::new();
    if filtered {
        marker.push('▿');
    }
    if let Some((direction, order)) = sort.applied {
        marker.push(match direction {
            SortDirection::Ascending => '▲',
            SortDirection::Descending => '▼',
        });
        if multi {
            marker += &(order + 1).to_string();
        }
    }
    if sort.pending || filter_pending {
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
