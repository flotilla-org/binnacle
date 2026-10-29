//! The small UCUM registry: the codes the formatter scales or writes out its
//! own way. Any other code is a literal suffix.

use crate::format::{fewest_decimals, number, one_line};

/// A number as it came: integers stay exact.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Num {
    Int(i128),
    Float(f64),
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(x) => x,
        }
    }

    /// Integers as digits; floats to `decimals`, or as few as show them.
    pub(crate) fn digits(self, decimals: Option<u8>) -> String {
        match self {
            Num::Int(i) => i.to_string(),
            Num::Float(x) => number(x, decimals.unwrap_or_else(|| fewest_decimals([x]))),
        }
    }
}

/// `n` written with `unit`, a UCUM code.
pub(crate) fn with_unit(n: Num, unit: &str, decimals: Option<u8>) -> String {
    match unit {
        "By" => bytes(n.as_f64()),
        "s" => duration(n.as_f64()),
        "ms" => duration(n.as_f64() / 1000.0),
        "%" => format!("{}%", n.digits(decimals)),
        "1" | "" => n.digits(decimals),
        // A UCUM annotation, `{widgets}`, shows as its text.
        _ => {
            let label = unit
                .strip_prefix('{')
                .and_then(|u| u.strip_suffix('}'))
                .unwrap_or(unit);
            format!("{} {}", n.digits(decimals), one_line(label))
        }
    }
}

const BYTE_UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

/// A size in bytes, scaled by 1024 up to TiB: `512 B`, `1.5 KiB`.
pub(crate) fn bytes(n: f64) -> String {
    if !n.is_finite() {
        return format!("{} B", number(n, 0));
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let (mut v, mut unit) = (n.abs(), 0);
    while v >= 1024.0 && unit < BYTE_UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{sign}{} B", number(v, fewest_decimals([v])))
    } else {
        format!("{sign}{v:.1} {}", BYTE_UNITS[unit])
    }
}

/// A duration in seconds: `350 ms`, `4.2 s`, `1m 2s`, `1h 2m`, `2d 3h`.
pub(crate) fn duration(secs: f64) -> String {
    if !secs.is_finite() {
        return format!("{} s", number(secs, 0));
    }
    let sign = if secs < 0.0 { "-" } else { "" };
    let s = secs.abs();
    if s < 1.0 {
        let ms = s * 1000.0;
        return format!("{sign}{} ms", number(ms, fewest_decimals([ms])));
    }
    if s < 60.0 {
        return format!("{sign}{} s", number(s, fewest_decimals([s]).min(1)));
    }
    // Saturates for durations past u64 seconds, which still draw.
    let whole = s as u64;
    let (big, small, units) = match whole {
        0..3600 => (whole / 60, whole % 60, ["m", "s"]),
        3600..86_400 => (whole / 3600, whole % 3600 / 60, ["h", "m"]),
        _ => (whole / 86_400, whole % 86_400 / 3600, ["d", "h"]),
    };
    format!("{sign}{big}{} {small}{}", units[0], units[1])
}
