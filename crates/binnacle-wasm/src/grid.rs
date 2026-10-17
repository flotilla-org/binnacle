//! The Grid as a wasm-bindgen class. Each input is one typed method: a new
//! Action or setting is a few lines here plus its call in `web/src`.

use binnacle_core::{Action, AxisConfig, Grid, Key, Viewport};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::codes::{self, axis, position};
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

    pub fn scroll(&mut self, dx: f32, dy: f32) {
        self.grid.action(Action::Scroll { dx, dy });
    }

    pub fn scroll_to(&mut self, a: u8, fraction: f64) {
        self.grid.action(Action::ScrollTo {
            axis: axis(a),
            fraction,
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
