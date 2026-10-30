//! Values as they come on the wire, in either encoding, before a field's
//! declared type reads them.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::Value as Json;

/// One value as a message carries it, decoded from JSON or CBOR alike: a
/// row's cell, a primary-key part. CBOR tags are read through.
#[derive(Clone, Debug, PartialEq)]
pub enum Wire {
    Null,
    Bool(bool),
    /// Any integer, signed or not (CBOR reaches past `i64` and `u64`).
    Int(i128),
    Float(f64),
    Text(String),
    /// CBOR only: JSON has no byte strings.
    Bytes(Vec<u8>),
    Array(Vec<Wire>),
    /// A map with text keys, in key order.
    Map(BTreeMap<String, Wire>),
}

impl Wire {
    /// As JSON, for a Nested value or a schema property: an integer past
    /// `u64` and `i64` becomes a float, a NaN or infinity a null, and bytes
    /// an array of their numbers.
    pub fn to_json(&self) -> Json {
        match self {
            Wire::Null => Json::Null,
            Wire::Bool(b) => Json::Bool(*b),
            Wire::Int(i) => match (i64::try_from(*i), u64::try_from(*i)) {
                (Ok(i), _) => Json::from(i),
                (_, Ok(u)) => Json::from(u),
                _ => serde_json::Number::from_f64(*i as f64).map_or(Json::Null, Json::Number),
            },
            Wire::Float(x) => serde_json::Number::from_f64(*x).map_or(Json::Null, Json::Number),
            Wire::Text(s) => Json::String(s.clone()),
            Wire::Bytes(b) => Json::Array(b.iter().map(|&b| Json::from(b)).collect()),
            Wire::Array(items) => Json::Array(items.iter().map(Wire::to_json).collect()),
            Wire::Map(map) => {
                Json::Object(map.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())
            }
        }
    }

    /// From JSON, exactly.
    pub fn from_json(json: &Json) -> Wire {
        match json {
            Json::Null => Wire::Null,
            Json::Bool(b) => Wire::Bool(*b),
            Json::Number(n) => match (n.as_i64(), n.as_u64()) {
                (Some(i), _) => Wire::Int(i.into()),
                (_, Some(u)) => Wire::Int(u.into()),
                _ => Wire::Float(n.as_f64().unwrap_or(f64::NAN)),
            },
            Json::String(s) => Wire::Text(s.clone()),
            Json::Array(items) => Wire::Array(items.iter().map(Wire::from_json).collect()),
            Json::Object(map) => Wire::Map(
                (map.iter())
                    .map(|(k, v)| (k.clone(), Wire::from_json(v)))
                    .collect(),
            ),
        }
    }

    /// What an invalid value shows: text as it is, anything else as JSON.
    pub fn raw_text(&self) -> String {
        match self {
            Wire::Text(s) => s.clone(),
            Wire::Float(x) if !x.is_finite() => x.to_string(),
            other => other.to_json().to_string(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Wire::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Wire>> {
        match self {
            Wire::Map(map) => Some(map),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Wire::Null)
    }
}

impl<'de> Deserialize<'de> for Wire {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Wire, D::Error> {
        deserializer.deserialize_any(WireVisitor)
    }
}

struct WireVisitor;

impl<'de> Visitor<'de> for WireVisitor {
    type Value = Wire;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON or CBOR value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Wire, E> {
        Ok(Wire::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Wire, E> {
        Ok(Wire::Int(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Wire, E> {
        Ok(Wire::Int(v.into()))
    }

    fn visit_i128<E>(self, v: i128) -> Result<Wire, E> {
        Ok(Wire::Int(v))
    }

    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Wire, E> {
        match i128::try_from(v) {
            Ok(v) => Ok(Wire::Int(v)),
            Err(_) => Ok(Wire::Float(v as f64)),
        }
    }

    fn visit_f64<E>(self, v: f64) -> Result<Wire, E> {
        Ok(Wire::Float(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Wire, E> {
        Ok(Wire::Text(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Wire, E> {
        Ok(Wire::Text(v))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Wire, E> {
        Ok(Wire::Bytes(v.to_owned()))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Wire, E> {
        Ok(Wire::Bytes(v))
    }

    fn visit_none<E>(self) -> Result<Wire, E> {
        Ok(Wire::Null)
    }

    fn visit_unit<E>(self) -> Result<Wire, E> {
        Ok(Wire::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Wire, D::Error> {
        Wire::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Wire, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(4096));
        while let Some(item) = seq.next_element()? {
            items.push(item);
        }
        Ok(Wire::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Wire, A::Error> {
        let mut entries = BTreeMap::new();
        while let Some((key, value)) = map.next_entry::<String, Wire>()? {
            entries.insert(key, value);
        }
        Ok(Wire::Map(entries))
    }
}

/// The deterministic CBOR encoding of `value` (RFC 8949, section 4.2.1):
/// integers and lengths in their shortest form, floats in the shortest
/// width that keeps their value, map keys in the bytewise order of their
/// encodings. Equal values encode to the same bytes, whichever encoding
/// they came in.
pub fn deterministic_cbor(value: &Wire) -> Vec<u8> {
    let mut out = Vec::new();
    encode(value, &mut out);
    out
}

fn head(major: u8, n: u64, out: &mut Vec<u8>) {
    let major = major << 5;
    match n {
        0..=23 => out.push(major | n as u8),
        24..=0xff => out.extend([major | 24, n as u8]),
        0x100..=0xffff => {
            out.push(major | 25);
            out.extend((n as u16).to_be_bytes());
        }
        0x1_0000..=0xffff_ffff => {
            out.push(major | 26);
            out.extend((n as u32).to_be_bytes());
        }
        _ => {
            out.push(major | 27);
            out.extend(n.to_be_bytes());
        }
    }
}

fn encode(value: &Wire, out: &mut Vec<u8>) {
    match value {
        Wire::Null => out.push(0xf6),
        Wire::Bool(false) => out.push(0xf4),
        Wire::Bool(true) => out.push(0xf5),
        Wire::Int(i) => {
            let (major, n) = match *i >= 0 {
                true => (0, *i as u128),
                false => (1, (-1 - *i) as u128),
            };
            match u64::try_from(n) {
                Ok(n) => head(major, n, out),
                Err(_) => {
                    // A bignum: tag 2 or 3 over the magnitude's bytes.
                    head(6, 2 + u64::from(major), out);
                    let bytes = n.to_be_bytes();
                    let first = bytes.iter().position(|&b| b != 0).unwrap_or(15);
                    head(2, (16 - first) as u64, out);
                    out.extend(&bytes[first..]);
                }
            }
        }
        Wire::Float(x) => float(*x, out),
        Wire::Text(s) => {
            head(3, s.len() as u64, out);
            out.extend(s.as_bytes());
        }
        Wire::Bytes(b) => {
            head(2, b.len() as u64, out);
            out.extend(b);
        }
        Wire::Array(items) => {
            head(4, items.len() as u64, out);
            for item in items {
                encode(item, out);
            }
        }
        Wire::Map(map) => {
            let mut entries: Vec<(Vec<u8>, &Wire)> = (map.iter())
                .map(|(k, v)| (deterministic_cbor(&Wire::Text(k.clone())), v))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            head(5, entries.len() as u64, out);
            for (key, value) in entries {
                out.extend(key);
                encode(value, out);
            }
        }
    }
}

/// A float in the shortest of half, single and double width that holds it
/// exactly; every NaN is the one half-width quiet NaN.
fn float(x: f64, out: &mut Vec<u8>) {
    if x.is_nan() {
        out.extend([0xf9, 0x7e, 0x00]);
        return;
    }
    if let Some(half) = to_half(x) {
        out.push(0xf9);
        out.extend(half.to_be_bytes());
        return;
    }
    let single = x as f32;
    if f64::from(single) == x {
        out.push(0xfa);
        out.extend(single.to_bits().to_be_bytes());
        return;
    }
    out.push(0xfb);
    out.extend(x.to_bits().to_be_bytes());
}

/// `x` as an IEEE half if that holds it exactly.
fn to_half(x: f64) -> Option<u16> {
    let sign = if x.is_sign_negative() { 0x8000u16 } else { 0 };
    if x == 0.0 {
        return Some(sign);
    }
    if x.is_infinite() {
        return Some(sign | 0x7c00);
    }
    let a = x.abs();
    // Normal halves: 2^-14 to 65504, 10 bits of fraction.
    let exp = a.log2().floor() as i32;
    if (-14..=15).contains(&exp) {
        let mantissa = a / 2f64.powi(exp) - 1.0;
        let bits = mantissa * 1024.0;
        if bits.fract() == 0.0 && (0.0..1024.0).contains(&bits) {
            let half = sign | (((exp + 15) as u16) << 10) | bits as u16;
            return (half_to_f64(half) == x).then_some(half);
        }
        return None;
    }
    // Subnormal halves: multiples of 2^-24 below 2^-14.
    let bits = a * 2f64.powi(24);
    if exp < -14 && bits.fract() == 0.0 && bits < 1024.0 {
        let half = sign | bits as u16;
        return (half_to_f64(half) == x).then_some(half);
    }
    None
}

fn half_to_f64(half: u16) -> f64 {
    let sign = if half & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((half >> 10) & 0x1f);
    let fraction = f64::from(half & 0x3ff);
    sign * match exp {
        0 => fraction * 2f64.powi(-24),
        31 => f64::INFINITY,
        _ => (1.0 + fraction / 1024.0) * 2f64.powi(exp - 15),
    }
}
