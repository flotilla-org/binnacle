//! PROTOTYPE (#45): the minimum wasm-bindgen bridge a web page needs to drive
//! a binnacle Grid. Throwaway. Keys and Positions cross as `f64` (exact below
//! 2^53, which covers the 10^15-row huge source); `NaN` stands for `None`.
//! Cell values never enter wasm: the page fills Windows with Keys only.

use binnacle_core::{
    Action, Axis, Button, Cursor, Effect, EditEnd, FocusStep, Grid, Key, Mode, Modifiers,
    PointerEvent, PointerKind, Viewport,
};
use wasm_bindgen::prelude::*;

fn axis(a: u8) -> Axis {
    if a == 0 { Axis::Rows } else { Axis::Cols }
}

fn axis_code(a: Axis) -> f64 {
    match a {
        Axis::Rows => 0.0,
        Axis::Cols => 1.0,
    }
}

fn opt_key(k: Option<Key>) -> f64 {
    k.map_or(f64::NAN, |k| k.0 as f64)
}

/// The scalar header of the flat Snapshot array. Keep in step with
/// `src/bridge.ts`.
const HEAD: usize = 48;

#[wasm_bindgen]
pub struct WebGrid {
    grid: Grid,
    /// The last Snapshot, flattened: `HEAD` scalars, then 4 numbers per row
    /// item, per column item and per selection rect.
    flat: Vec<f64>,
}

#[wasm_bindgen]
impl WebGrid {
    #[wasm_bindgen(constructor)]
    pub fn new(
        rows: f64,
        row_size: f32,
        col_header: f32,
        cols: f64,
        col_size: f32,
        row_header: f32,
        gap: f32,
    ) -> WebGrid {
        let mut grid = Grid::new(
            binnacle_core::AxisConfig::new(rows as u64, row_size)
                .header(col_header)
                .gap(gap)
                .min_size(row_size),
            binnacle_core::AxisConfig::new(cols as u64, col_size)
                .header(row_header)
                .gap(gap)
                .min_size(24.0),
        );
        for c in 0..cols as u64 {
            grid.set_sortable(Key(c), false);
        }
        WebGrid {
            grid,
            flat: Vec::new(),
        }
    }

    /// Starts a frame. Returns `[rowStart, rowEnd, colStart, colEnd]`, `NaN`
    /// where an axis needs no Window.
    pub fn begin_frame(&mut self, width: f32, height: f32) -> Vec<f64> {
        let req = self.grid.begin_frame(Viewport::new(width, height));
        let mut out = vec![f64::NAN; 4];
        for a in Axis::ALL {
            if let Some(r) = req.get(a) {
                out[a.index() * 2] = r.start as f64;
                out[a.index() * 2 + 1] = r.end as f64;
            }
        }
        out
    }

    pub fn set_window(&mut self, a: u8, start: f64, keys: &[f64]) {
        let keys: Vec<Key> = keys.iter().map(|&k| Key(k as u64)).collect();
        self.grid.set_window(axis(a), start as u64, &keys);
    }

    /// Lays out the frame and returns the Snapshot flattened (see `HEAD`).
    pub fn snapshot(&mut self) -> Vec<f64> {
        let s = self.grid.snapshot();
        let flat = &mut self.flat;
        flat.clear();
        flat.resize(HEAD, f64::NAN);
        let d = s.damage();
        flat[0] = s.generation() as f64;
        flat[1] = (d.layout as u8 | (d.selection as u8) << 1 | (d.positions as u8) << 2) as f64;
        for a in Axis::ALL {
            let i = a.index();
            let anchor = s.scroll_anchor(a);
            let thumb = s.thumb(a);
            flat[2 + i] = s.count(a) as f64;
            flat[4 + i] = s.extent(a) as f64;
            flat[6 + i] = s.header(a) as f64;
            flat[8 + i] = s.gap(a) as f64;
            flat[10 + i * 2] = anchor.position as f64;
            flat[11 + i * 2] = anchor.offset as f64;
            flat[14 + i * 2] = thumb.fraction;
            flat[15 + i * 2] = thumb.size;
            if let Some(f) = s.focus(a) {
                flat[18 + i * 2] = f.position as f64;
                flat[19 + i * 2] = opt_key(f.key);
            }
        }
        if let Some(r) = s.focus_rect() {
            flat[22..26].copy_from_slice(&[r.x as f64, r.y as f64, r.width as f64, r.height as f64]);
        }
        if let Some(r) = s.editor_rect() {
            flat[26..30].copy_from_slice(&[r.x as f64, r.y as f64, r.width as f64, r.height as f64]);
        }
        flat[30] = match s.cursor() {
            Cursor::Default => 0.0,
            Cursor::ResizeCol => 1.0,
            Cursor::ResizeRow => 2.0,
            Cursor::Move => 3.0,
        };
        flat[31] = match s.mode() {
            Mode::Idle => 0.0,
            Mode::DraggingSelection => 1.0,
            Mode::Resizing { .. } => 2.0,
            Mode::ResizingBand { .. } => 3.0,
            Mode::MovingHeaders { .. } => 4.0,
            Mode::Editing { .. } => 5.0,
        };
        flat[32] = s.capturing() as u8 as f64;
        if let Some(g) = s.resize_guide() {
            flat[33] = axis_code(g.axis);
            flat[34] = g.offset as f64;
        }
        if let Some(d) = s.drop_indicator() {
            flat[35] = axis_code(d.axis);
            flat[36] = d.offset as f64;
        }
        if let Some((a, item)) = s.hovered_header() {
            flat[37] = axis_code(a);
            flat[38] = item.position as f64;
        }
        // The ratatui adapter's `animating`: a pending sort on a visible header.
        let sorting = s.headers(Axis::Cols).any(|h| h.sort.pending);
        let locating = Axis::ALL.into_iter().any(|a| s.locating(a));
        flat[39] = (sorting || locating && s.focus_rect().is_some()) as u8 as f64;
        let (rows, cols, sel) = (s.rows(), s.cols(), s.selection_rects());
        flat[40] = rows.len() as f64;
        flat[41] = cols.len() as f64;
        flat[42] = sel.len() as f64;
        // Whole-row or whole-column selection, for header highlight.
        if let Some(r) = s.selection_range() {
            flat[43] = r.rows.start as f64;
            flat[44] = r.rows.end as f64;
            flat[45] = r.cols.start as f64;
            flat[46] = r.cols.end as f64;
        }
        for item in rows.iter().chain(cols) {
            flat.extend_from_slice(&[
                item.position as f64,
                opt_key(item.key),
                item.offset as f64,
                item.size as f64,
            ]);
        }
        for r in sel {
            flat.extend_from_slice(&[r.x as f64, r.y as f64, r.width as f64, r.height as f64]);
        }
        flat.clone()
    }

    /// Drains every queued Effect as a JSON array.
    pub fn poll_effects(&mut self) -> String {
        let mut out = Vec::new();
        while let Some(st) = self.grid.poll_effect() {
            let g = st.generation;
            let k = |k: Key| k.0;
            let a = |a: Axis| if a == Axis::Rows { "rows" } else { "cols" };
            out.push(match st.effect {
                Effect::TextTyped { text } => format!(
                    r#"{{"g":{g},"t":"TextTyped","text":{}}}"#,
                    json_str(&text)
                ),
                Effect::Activated { row, col } => {
                    format!(r#"{{"g":{g},"t":"Activated","row":{},"col":{}}}"#, k(row), k(col))
                }
                Effect::EditRequested { row, col } => format!(
                    r#"{{"g":{g},"t":"EditRequested","row":{},"col":{}}}"#,
                    k(row),
                    k(col)
                ),
                Effect::EditCancelled { row, col } => format!(
                    r#"{{"g":{g},"t":"EditCancelled","row":{},"col":{}}}"#,
                    k(row),
                    k(col)
                ),
                Effect::FocusChanged { row, col } => format!(
                    r#"{{"g":{g},"t":"FocusChanged","row":{},"col":{}}}"#,
                    k(row),
                    k(col)
                ),
                Effect::SelectionChanged => format!(r#"{{"g":{g},"t":"SelectionChanged"}}"#),
                Effect::SortRequested { .. } => format!(r#"{{"g":{g},"t":"SortRequested"}}"#),
                Effect::Locate { axis, key } => format!(
                    r#"{{"g":{g},"t":"Locate","axis":"{}","key":{}}}"#,
                    a(axis),
                    k(key)
                ),
                Effect::SizeChanged { axis, key, size } => format!(
                    r#"{{"g":{g},"t":"SizeChanged","axis":"{}","key":{},"size":{size}}}"#,
                    a(axis),
                    k(key)
                ),
                Effect::AutoSizeRequested { axis, key } => format!(
                    r#"{{"g":{g},"t":"AutoSizeRequested","axis":"{}","key":{}}}"#,
                    a(axis),
                    k(key)
                ),
                Effect::BandSizeChanged { axis, thickness } => format!(
                    r#"{{"g":{g},"t":"BandSizeChanged","axis":"{}","thickness":{thickness}}}"#,
                    a(axis)
                ),
                Effect::BandAutoSizeRequested { axis } => format!(
                    r#"{{"g":{g},"t":"BandAutoSizeRequested","axis":"{}"}}"#,
                    a(axis)
                ),
                Effect::MoveRequested { axis, to, .. } => format!(
                    r#"{{"g":{g},"t":"MoveRequested","axis":"{}","to":{to}}}"#,
                    a(axis)
                ),
            });
        }
        format!("[{}]", out.join(","))
    }

    /// kind: 0 down, 1 move, 2 up. mods: bit 0 shift, 1 ctrl, 2 alt, 3 meta.
    pub fn pointer(&mut self, kind: u8, x: f32, y: f32, button: u8, mods: u8, clicks: u32) {
        self.grid.pointer(PointerEvent {
            kind: match kind {
                0 => PointerKind::Down,
                1 => PointerKind::Move,
                _ => PointerKind::Up,
            },
            x,
            y,
            button: match button {
                0 => Button::Primary,
                1 => Button::Middle,
                _ => Button::Secondary,
            },
            modifiers: Modifiers {
                shift: mods & 1 != 0,
                ctrl: mods & 2 != 0,
                alt: mods & 4 != 0,
                meta: mods & 8 != 0,
            },
            clicks,
        });
    }

    pub fn scroll(&mut self, dx: f32, dy: f32) {
        self.grid.action(Action::Scroll { dx, dy });
    }

    pub fn scroll_to(&mut self, a: u8, fraction: f64) {
        self.grid.action(Action::ScrollTo {
            axis: axis(a),
            fraction,
        });
    }

    /// step: 0 item, 1 page, 2 edge.
    pub fn move_focus(&mut self, a: u8, step: u8, forward: bool, extend: bool) {
        let by = match step {
            0 => FocusStep::Item,
            1 => FocusStep::Page,
            _ => FocusStep::Edge,
        };
        self.grid.action(Action::MoveFocus {
            axis: axis(a),
            by,
            forward,
            extend,
        });
    }

    pub fn activate(&mut self) {
        self.grid.action(Action::Activate);
    }

    pub fn begin_edit(&mut self) {
        self.grid.action(Action::BeginEdit);
    }

    pub fn cancel(&mut self) {
        self.grid.action(Action::Cancel);
    }

    pub fn select_all(&mut self) {
        self.grid.action(Action::SelectAll);
    }

    pub fn type_text(&mut self, text: String) {
        self.grid.action(Action::TypeText(text));
    }

    pub fn edit_started(&mut self, row: f64, col: f64) {
        self.grid.edit_started(Key(row as u64), Key(col as u64));
    }

    pub fn edit_ended(&mut self, commit: bool) {
        self.grid
            .edit_ended(if commit { EditEnd::Commit } else { EditEnd::Cancel });
    }

    pub fn set_size(&mut self, a: u8, key: f64, size: f32) {
        self.grid.set_size(axis(a), Key(key as u64), size);
    }

    pub fn set_header(&mut self, a: u8, thickness: f32) {
        self.grid.set_header(axis(a), thickness);
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
