//! The tagged JSON form of a Typed Value, as the shared fixtures write them:
//! `null`, or an object with one tag, such as `{"int": "-42"}`,
//! `{"number": "NaN"}`, `{"datetime": "2026-10-29T14:03:00Z"}` or
//! `{"invalid": "12x", "expected": "int"}`. Integers may be strings, so
//! they survive JavaScript's `JSON.parse` whole.

use serde_json::Value as Json;

use crate::{Timestamp, Value, ValueType};

/// The Typed Value `json` writes, or why it writes none.
pub fn from_tagged_json(json: &Json) -> Result<Value, String> {
    let Json::Object(map) = json else {
        return match json {
            Json::Null => Ok(Value::Null),
            other => Err(format!("not a tagged value: {other}")),
        };
    };
    let text = |key: &str| -> Result<String, String> {
        match map.get(key) {
            Some(Json::String(s)) => Ok(s.clone()),
            other => Err(format!("{key} wants a string, got {other:?}")),
        }
    };
    let parsed = |key: &str| -> Result<String, String> {
        match map.get(key) {
            Some(Json::Number(n)) => Ok(n.to_string()),
            _ => text(key),
        }
    };
    let bad = |key: &str| format!("bad {key}: {:?}", map.get(key));
    if let Some(b) = map.get("bool") {
        return b.as_bool().map(Value::Bool).ok_or_else(|| bad("bool"));
    }
    if map.contains_key("int") {
        return parsed("int")?
            .parse()
            .map(Value::Int)
            .map_err(|_| bad("int"));
    }
    if map.contains_key("uint") {
        return parsed("uint")?
            .parse()
            .map(Value::UInt)
            .map_err(|_| bad("uint"));
    }
    if let Some(n) = map.get("number") {
        let x = match n {
            Json::Number(n) => n.as_f64(),
            Json::String(s) => s.parse().ok(),
            _ => None,
        };
        return x.map(Value::Number).ok_or_else(|| bad("number"));
    }
    if map.contains_key("text") {
        return text("text").map(Value::Text);
    }
    if map.contains_key("bytes") {
        let hex = text("bytes")?;
        return (0..hex.len())
            .step_by(2)
            .map(|i| {
                hex.get(i..i + 2)
                    .and_then(|b| u8::from_str_radix(b, 16).ok())
            })
            .collect::<Option<Vec<u8>>>()
            .map(Value::Bytes)
            .ok_or_else(|| bad("bytes"));
    }
    if map.contains_key("datetime") {
        return Timestamp::parse_rfc3339(&text("datetime")?)
            .map(Value::DateTime)
            .ok_or_else(|| bad("datetime"));
    }
    if map.contains_key("category") {
        let label = map
            .contains_key("label")
            .then(|| text("label"))
            .transpose()?;
        return Ok(Value::Category {
            value: text("category")?,
            label,
        });
    }
    if let Some(nested) = map.get("nested") {
        return Ok(Value::Nested(nested.clone()));
    }
    if map.contains_key("invalid") {
        let expected = ValueType::from_name(&text("expected")?).ok_or_else(|| bad("expected"))?;
        return Ok(Value::Invalid {
            raw: text("invalid")?,
            expected,
        });
    }
    Err(format!("no known tag in {json}"))
}
