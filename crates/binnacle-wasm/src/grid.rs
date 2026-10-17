//! The Grid as a wasm-bindgen class. Each input is one typed method: a new
//! Action or setting is a few lines here plus its call in `web/src`.

use binnacle_core::{Action, AxisConfig, Grid, Key, PointerEvent, Viewport};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::codes::{self, axis, focus_step, position};
use crate::effect::WireStamped;
use crate::snapshot::{Encoded, encode};
use crate::testing;

/// The serializer every value crossing to JS goes through: `u64` as `bigint`,
/// maps as plain objects, `None` as `null`.
pub(crate) fn to_js<T: Serialize + ?Sized>(value: &T) -> JsValue {
    let serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_large_number_types_as_bigints(true)
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true);
    value
        .serialize(&serializer)
        .unwrap_or_else(|e| wasm_bindgen::throw_str(&e.to_string()))
}

/// One axis's settings for [`WebGrid::new`], mirroring [`AxisConfig`].
#[wasm_bindgen]
#[derive(Clone, Copy, Debug)]
pub struct WebAxis(AxisConfig);

#[wasm_bindgen]
impl WebAxis {
    /// Rejects a count that is not a whole number up to 2^53 - 1.
    #[wasm_bindgen(constructor)]
    pub fn new(count: f64, default_size: f32) -> Result<WebAxis, JsError> {
        let count = codes::count(count).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(WebAxis(AxisConfig::new(count, default_size)))
    }

    pub fn header(self, thickness: f32) -> WebAxis {
        WebAxis(self.0.header(thickness))
    }

    pub fn gap(self, gap: f32) -> WebAxis {
        WebAxis(self.0.gap(gap))
    }

    pub fn min_size(self, size: f32) -> WebAxis {
        WebAxis(self.0.min_size(size))
    }

    pub fn max_size(self, size: f32) -> WebAxis {
        WebAxis(self.0.max_size(size))
    }
}

#[wasm_bindgen]
pub struct WebGrid {
    grid: Grid,
    encoded: Encoded,
}

#[wasm_bindgen]
impl WebGrid {
    #[wasm_bindgen(constructor)]
    pub fn new(rows: WebAxis, cols: WebAxis) -> WebGrid {
        WebGrid {
            grid: Grid::new(rows.0, cols.0),
            encoded: Encoded::default(),
        }
    }

    /// Starts a frame. Returns `[rowStart, rowEnd, colStart, colEnd]`, `NaN`
    /// for an axis whose Window already covers what it needs.
    pub fn begin_frame(&mut self, width: f32, height: f32) -> Vec<f64> {
        let requests = self.grid.begin_frame(Viewport::new(width, height));
        let mut out = vec![f64::NAN; 4];
        for (a, range) in requests {
            out[a.index() * 2] = range.start as f64;
            out[a.index() * 2 + 1] = range.end as f64;
        }
        out
    }

    pub fn set_window(&mut self, a: u8, start: f64, keys: &[u64]) {
        let keys: Vec<Key> = keys.iter().map(|&k| Key(k)).collect();
        self.grid.set_window(axis(a), position(start), &keys);
    }

    /// Lays out the frame and returns its numbers; [`WebGrid::snapshot_keys`]
    /// returns its Keys.
    pub fn snapshot(&mut self) -> Vec<f64> {
        encode(&self.grid.snapshot(), &mut self.encoded);
        self.encoded.numbers.clone()
    }

    /// The Keys of the Snapshot last returned by [`WebGrid::snapshot`].
    pub fn snapshot_keys(&self) -> Vec<u64> {
        self.encoded.keys.clone()
    }

    /// Drains every queued Effect as an array of tagged objects.
    pub fn poll_effects(&mut self) -> JsValue {
        let effects: Vec<WireStamped> =
            std::iter::from_fn(|| self.grid.poll_effect().map(WireStamped::from)).collect();
        to_js(&effects)
    }

    pub fn set_header(&mut self, a: u8, thickness: f32) {
        self.grid.set_header(axis(a), thickness);
    }

    pub fn set_gap(&mut self, a: u8, gap: f32) {
        self.grid.set_gap(axis(a), gap);
    }

    pub fn set_size(&mut self, a: u8, key: u64, size: f32) {
        self.grid.set_size(axis(a), Key(key), size);
    }

    pub fn set_sortable(&mut self, key: u64, sortable: bool) {
        self.grid.set_sortable(Key(key), sortable);
    }

    /// A Change Notice on axis `a`; see [`codes::notice`].
    pub fn notify(&mut self, a: u8, code: u8, first: f64, second: f64) {
        self.grid
            .notify(axis(a), codes::notice(code, first, second));
    }

    /// The Sort Spec the Table Source now applies; see [`codes::sort_spec`].
    pub fn sort_applied(&mut self, keys: &[u64], directions: &[u8]) {
        self.grid.sort_applied(codes::sort_spec(keys, directions));
    }

    /// The answer to `Locate`; see [`codes::located`].
    pub fn located(&mut self, a: u8, key: u64, code: u8, position: f64) {
        self.grid
            .located(axis(a), Key(key), codes::located(code, position));
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

    /// A raw pointer event: `kind` and `button` as [`codes::pointer_kind`] and
    /// [`codes::button`], `modifiers` as [`codes::modifiers`], `clicks` the
    /// widget's own count.
    pub fn pointer(&mut self, kind: u8, x: f32, y: f32, button: u8, modifiers: u8, clicks: u32) {
        self.grid.pointer(PointerEvent {
            kind: codes::pointer_kind(kind),
            x,
            y,
            button: codes::button(button),
            modifiers: codes::modifiers(modifiers),
            clicks,
        });
    }

    pub fn set_border_grab(&mut self, grab: f32) {
        self.grid.set_border_grab(grab);
    }

    pub fn set_focus(&mut self, row: u64, col: u64) {
        self.grid.set_focus(Key(row), Key(col));
    }

    pub fn clear_size(&mut self, a: u8, key: u64) {
        self.grid.clear_size(axis(a), Key(key));
    }

    pub fn move_focus(&mut self, a: u8, by: u8, forward: bool, extend: bool) {
        self.grid.action(Action::MoveFocus {
            axis: axis(a),
            by: focus_step(by),
            forward,
            extend,
        });
    }

    pub fn select_all(&mut self) {
        self.grid.action(Action::SelectAll);
    }

    pub fn select_column(&mut self) {
        self.grid.action(Action::SelectColumn);
    }

    pub fn cancel(&mut self) {
        self.grid.action(Action::Cancel);
    }

    pub fn activate(&mut self) {
        self.grid.action(Action::Activate);
    }

    pub fn begin_edit(&mut self) {
        self.grid.action(Action::BeginEdit);
    }

    /// The widget opened its editor on the cell with `row` and `col`.
    pub fn edit_started(&mut self, row: u64, col: u64) {
        self.grid.edit_started(Key(row), Key(col));
    }

    /// The widget closed its editor; `end` as [`codes::edit_end`].
    pub fn edit_ended(&mut self, end: u8) {
        self.grid.edit_ended(codes::edit_end(end));
    }

    pub fn scroll_by(&mut self, a: u8, items: f64) {
        self.grid.action(Action::ScrollBy {
            axis: axis(a),
            items: items as i64,
        });
    }

    /// Test only: a Grid in one of the states [`testing::SCENARIOS`] names.
    pub fn debug_scenario(name: &str) -> WebGrid {
        WebGrid {
            grid: testing::scenario(name),
            encoded: Encoded::default(),
        }
    }

    /// Test only: lays out the frame like [`WebGrid::snapshot`] and returns
    /// `{ numbers, keys, dump }`, the dump built from the same Snapshot.
    pub fn debug_frame(&mut self) -> JsValue {
        let s = self.grid.snapshot();
        encode(&s, &mut self.encoded);
        #[derive(Serialize)]
        struct Frame<'a> {
            numbers: &'a [f64],
            keys: &'a [u64],
            dump: testing::Dump,
        }
        to_js(&Frame {
            numbers: &self.encoded.numbers,
            keys: &self.encoded.keys,
            dump: testing::dump(&s),
        })
    }
}

/// Test only: the names [`WebGrid::debug_scenario`] takes.
#[wasm_bindgen]
pub fn debug_scenarios() -> Vec<String> {
    testing::SCENARIOS.map(String::from).to_vec()
}

/// Test only: one of every Effect, as [`WebGrid::poll_effects`] returns them.
#[wasm_bindgen]
pub fn debug_every_effect() -> JsValue {
    to_js(&testing::every_effect())
}
