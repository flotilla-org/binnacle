//! What only real wasm shows: Keys as `bigint` across the boundary and the
//! count cap surfacing as a thrown JS error. Run with
//! `wasm-pack test --node crates/binnacle-wasm` (`pnpm -C web test` does).
#![cfg(target_arch = "wasm32")]

use binnacle_wasm::{WebAxis, WebGrid};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

fn grid(rows: f64, cols: f64) -> WebGrid {
    WebGrid::new(
        WebAxis::new(rows, 20.0).unwrap(),
        WebAxis::new(cols, 80.0).unwrap(),
    )
}

#[wasm_bindgen_test]
fn keys_past_2_53_cross_exactly() {
    let mut g = grid(2.0, 1.0);
    g.begin_frame(400.0, 200.0);
    g.set_window(0, 0.0, &[u64::MAX, (1 << 53) + 1]);
    g.set_window(1, 0.0, &[7]);
    g.snapshot();
    assert_eq!(&g.snapshot_keys()[..2], &[u64::MAX, (1 << 53) + 1]);
}

#[wasm_bindgen_test]
fn effect_keys_are_bigints() {
    let effects = js_sys::Array::from(&binnacle_wasm::debug_every_effect());
    let focus = effects.iter().find(|e| {
        js_sys::Reflect::get(e, &"type".into()).unwrap() == JsValue::from_str("FocusChanged")
    });
    let row = js_sys::Reflect::get(&focus.unwrap(), &"row".into()).unwrap();
    assert!(row.is_bigint());
    assert_eq!(row, JsValue::from(js_sys::BigInt::from(u64::MAX)));
}

#[wasm_bindgen_test]
fn the_largest_safe_count_is_accepted() {
    assert!(WebAxis::new(9_007_199_254_740_991.0, 20.0).is_ok());
}

#[wasm_bindgen_test]
fn a_count_past_2_53_throws() {
    let err = WebAxis::new(9_007_199_254_740_992.0, 20.0).unwrap_err();
    let message = js_sys::Error::from(JsValue::from(err)).message();
    assert!(String::from(message).contains("2^53"));
}
