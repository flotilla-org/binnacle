//! The shared formatter for `valueRenderer` in `web/src/renderers/value.ts`.
//! A Typed Value crosses as TypeScript writes it (`TypedValue` in
//! `web/src/format.ts`) and is decoded here, so the web formats exactly as
//! Rust does.

use binnacle_format::{
    Align, FormatContext, FormatOptions, Formatted, Role, TimeStyle, Timestamp, Value, ValueType,
    Zone, format, format_stable,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use crate::grid::to_js;

/// A column's `FormatOptions` as TypeScript writes them.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct WireOptions {
    time: Option<String>,
    decimals: Option<u8>,
    null_glyph: Option<String>,
    plain: bool,
}

#[derive(Serialize)]
struct WireFormatted {
    text: String,
    /// `"start"` or `"end"`.
    align: &'static str,
    /// The style hints' bits: 1 dim, 2 warning, 4 pending.
    style: u8,
}

/// One column's unit, role and options, kept so each cell formats without
/// decoding them again.
#[wasm_bindgen]
pub struct WebColumnFormat {
    unit: Option<String>,
    role: Role,
    options: FormatOptions,
}

#[wasm_bindgen]
impl WebColumnFormat {
    /// `role` as the server names it (`"time"`); `options` as
    /// `FormatOptions` in TypeScript.
    #[wasm_bindgen(constructor)]
    pub fn new(
        unit: Option<String>,
        role: Option<String>,
        options: JsValue,
    ) -> Result<WebColumnFormat, JsError> {
        let wire: WireOptions = if options.is_undefined() || options.is_null() {
            WireOptions::default()
        } else {
            serde_wasm_bindgen::from_value(options).map_err(|e| JsError::new(&e.to_string()))?
        };
        let time = match wire.time.as_deref() {
            Some("relative") => Some(TimeStyle::Relative),
            Some("absolute") => Some(TimeStyle::Absolute),
            _ => None,
        };
        Ok(WebColumnFormat {
            unit,
            role: role.map_or(Role::None, |r| Role::from_name(&r)),
            options: FormatOptions {
                time,
                decimals: wire.decimals,
                null_glyph: wire.null_glyph,
                plain: wire.plain,
            },
        })
    }

    /// `value` formatted at `now_ms` (milliseconds since the Unix epoch,
    /// fractions kept), its dates in `zone` (seconds east of UTC at a time
    /// in ms; the runtime's local zone when absent): `{ text, align, style }`.
    pub fn format(&self, value: &JsValue, now_ms: f64, zone: Option<js_sys::Function>) -> JsValue {
        let zone = JsZone(zone);
        let ctx = FormatContext::new(millis(now_ms), &zone);
        let value = decode(value);
        wire(format(
            &value,
            self.unit.as_deref(),
            self.role,
            &self.options,
            &ctx,
        ))
    }

    /// The stable text, as [`format`](Self::format) gives the rest: what
    /// search, copy and assistive tech read, its dates absolute.
    #[wasm_bindgen(js_name = formatStable)]
    pub fn format_stable(
        &self,
        value: &JsValue,
        now_ms: f64,
        zone: Option<js_sys::Function>,
    ) -> JsValue {
        let zone = JsZone(zone);
        let ctx = FormatContext::new(millis(now_ms), &zone);
        let value = decode(value);
        wire(format_stable(
            &value,
            self.unit.as_deref(),
            self.role,
            &self.options,
            &ctx,
        ))
    }

    /// Whether the column's dates draw relative to now, so its cells redraw
    /// as the clock moves.
    #[wasm_bindgen(getter)]
    pub fn relative(&self) -> bool {
        self.options.relative_dates(self.role)
    }
}

fn wire(f: Formatted) -> JsValue {
    to_js(&WireFormatted {
        text: f.text,
        align: match f.align {
            Align::Start => "start",
            Align::End => "end",
        },
        style: f.style.bits(),
    })
}

/// A zone from JavaScript: a function from ms since the Unix epoch to
/// seconds east of UTC, or the runtime's local zone. A function that throws
/// or gives no number counts as UTC.
pub(crate) struct JsZone(pub(crate) Option<js_sys::Function>);

impl Zone for JsZone {
    fn offset_at(&self, at: Timestamp) -> i32 {
        let ms = JsValue::from_f64(at.unix_nanos() as f64 / 1e6);
        let secs = match &self.0 {
            Some(zone) => zone
                .call1(&JsValue::NULL, &ms)
                .ok()
                .and_then(|s| s.as_f64()),
            None => Some(-js_sys::Date::new(&ms).get_timezone_offset() * 60.0),
        };
        secs.filter(|s| s.is_finite()).map_or(0, |s| s as i32)
    }
}

/// The fewest decimals, at most 3, that show every finite number in
/// `numbers`: a column's `decimals`.
#[wasm_bindgen]
pub fn fewest_decimals(numbers: &[f64]) -> u8 {
    binnacle_format::fewest_decimals(numbers.iter().copied())
}

fn millis(ms: f64) -> Timestamp {
    Timestamp::from_unix_nanos(if ms.is_finite() {
        (ms * 1e6) as i128
    } else {
        0
    })
}

/// A JavaScript value as JSON reads it, as `JSON.stringify` would write it
/// but for a `bigint`, which is an exact integer: `None` for one past
/// `i64::MIN..=u64::MAX`, or for anything JSON has no form for.
fn js_to_json(value: &JsValue) -> Option<serde_json::Value> {
    use serde_json::Value as Json;
    if value.is_null() {
        return Some(Json::Null);
    }
    if let Some(b) = value.as_bool() {
        return Some(Json::Bool(b));
    }
    if let Some(s) = value.as_string() {
        return Some(Json::String(s));
    }
    if let Some(x) = value.as_f64() {
        // A number JSON cannot hold is null, as `JSON.stringify` writes it.
        return Some(match x.is_finite() {
            false => Json::Null,
            true if x.fract() == 0.0 && x.abs() < 9.007_199_254_740_992e15 => Json::from(x as i64),
            true => serde_json::Number::from_f64(x).map_or(Json::Null, Json::Number),
        });
    }
    if value.is_bigint() {
        let big = js_sys::BigInt::from(value.clone());
        let text = String::from(big.to_string(10).ok()?);
        return match (text.parse::<i64>(), text.parse::<u64>()) {
            (Ok(i), _) => Some(Json::from(i)),
            (_, Ok(u)) => Some(Json::from(u)),
            _ => None,
        };
    }
    if let Some(items) = value.dyn_ref::<js_sys::Array>() {
        return (items.iter())
            .map(|item| match item.is_undefined() || item.is_function() {
                true => Some(Json::Null),
                false => js_to_json(&item),
            })
            .collect::<Option<Vec<_>>>()
            .map(Json::Array);
    }
    if value.is_object() && !value.is_function() {
        let mut fields = serde_json::Map::new();
        for key in js_sys::Object::keys(value.unchecked_ref::<js_sys::Object>()).iter() {
            let item = js_sys::Reflect::get(value, &key).ok()?;
            if item.is_undefined() || item.is_function() {
                continue;
            }
            fields.insert(key.as_string()?, js_to_json(&item)?);
        }
        return Some(Json::Object(fields));
    }
    None
}

fn field(object: &JsValue, key: &str) -> Option<JsValue> {
    let key = JsValue::from_str(key);
    js_sys::Reflect::has(object, &key)
        .unwrap_or(false)
        .then(|| js_sys::Reflect::get(object, &key).ok())
        .flatten()
}

/// `value` as text: a bigint's digits, a string itself, anything else its
/// JSON.
fn raw(value: &JsValue) -> String {
    if value.is_bigint() {
        let digits = js_sys::BigInt::from(value.clone()).to_string(10);
        return digits.map(String::from).unwrap_or_default();
    }
    value
        .as_string()
        .or_else(|| js_sys::JSON::stringify(value).ok()?.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}

fn invalid(value: &JsValue, expected: ValueType) -> Value {
    Value::Invalid {
        raw: raw(value),
        expected,
    }
}

/// A `TypedValue` as Rust's Value. Anything else is invalid text.
fn decode(value: &JsValue) -> Value {
    if value.is_null() || value.is_undefined() {
        return Value::Null;
    }
    if let Some(b) = value.as_bool() {
        return Value::Bool(b);
    }
    if let Some(x) = value.as_f64() {
        return Value::Number(x);
    }
    if value.is_bigint() {
        return i64::try_from(value.clone())
            .map(Value::Int)
            .or_else(|_| u64::try_from(value.clone()).map(Value::UInt))
            .unwrap_or_else(|_| invalid(value, ValueType::Int));
    }
    if let Some(s) = value.as_string() {
        return Value::Text(s);
    }
    if let Some(date) = value.dyn_ref::<js_sys::Date>() {
        let ms = date.get_time();
        return match ms.is_finite() {
            true => Value::DateTime(millis(ms)),
            false => Value::Invalid {
                raw: "Invalid Date".to_owned(),
                expected: ValueType::DateTime,
            },
        };
    }
    if let Some(bytes) = value.dyn_ref::<js_sys::Uint8Array>() {
        return Value::Bytes(bytes.to_vec());
    }
    if !value.is_object() {
        return invalid(value, ValueType::Text);
    }
    if let Some(category) = field(value, "category").and_then(|c| c.as_string()) {
        let label = field(value, "label").and_then(|l| l.as_string());
        return Value::Category {
            value: category,
            label,
        };
    }
    if let Some(nested) = field(value, "nested") {
        return match js_to_json(&nested) {
            Some(json) => Value::Nested(json),
            None => invalid(&nested, ValueType::Nested),
        };
    }
    if let Some(raw) = field(value, "invalid").and_then(|r| r.as_string()) {
        let expected = field(value, "expected")
            .and_then(|e| e.as_string())
            .and_then(|e| ValueType::from_name(&e))
            .unwrap_or(ValueType::Text);
        return Value::Invalid { raw, expected };
    }
    if let Some(nanos) = field(value, "instant").filter(JsValue::is_bigint)
        && let Ok(nanos) = raw(&nanos).parse::<i128>()
    {
        return Value::DateTime(Timestamp::from_unix_nanos(nanos));
    }
    invalid(value, ValueType::Text)
}
