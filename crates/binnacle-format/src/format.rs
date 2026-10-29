//! The formatter: a Typed Value, its column's unit, role and options in, one
//! line of text with an alignment and style hints out.

use std::borrow::Cow;
use std::ops::{BitOr, BitOrAssign};

use crate::time::{absolute, relative};
use crate::units::{Num, bytes, with_unit};
use crate::{Timestamp, Value};

/// What a null shows unless the options say otherwise.
pub const NULL_GLYPH: &str = "∅";
/// What a null shows in plain-text mode.
pub const PLAIN_NULL_GLYPH: &str = "-";
/// The most characters a Nested value's JSON shows, its `…` included.
const NESTED_MAX: usize = 48;
/// How many bytes a Bytes value's hex prefix shows.
const HEX_PREFIX: usize = 3;
/// Past this a Number is written with an exponent.
const EXPONENT_FROM: f64 = 1e15;
/// The most decimals [`fewest_decimals`] picks.
const MAX_DECIMALS: u8 = 3;

/// One formatted value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Formatted {
    /// One line: never a line break.
    pub text: String,
    pub align: Align,
    pub style: Style,
}

/// Where the text sits in its cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Align {
    #[default]
    Start,
    End,
}

/// Style hints, as bit flags, that each host maps to its own Theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Style(u8);

impl Style {
    pub const NONE: Style = Style(0);
    /// Drawn quietly: a null.
    pub const DIM: Style = Style(1);
    /// Drawn with a visible mark: an invalid value.
    pub const WARNING: Style = Style(2);
    /// Not settled yet.
    pub const PENDING: Style = Style(4);

    pub fn bits(self) -> u8 {
        self.0
    }

    /// Unknown bits are dropped.
    pub fn from_bits(bits: u8) -> Style {
        Style(bits & 0b111)
    }

    pub fn contains(self, other: Style) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Style {
    type Output = Style;

    fn bitor(self, rhs: Style) -> Style {
        Style(self.0 | rhs.0)
    }
}

impl BitOrAssign for Style {
    fn bitor_assign(&mut self, rhs: Style) {
        self.0 |= rhs.0;
    }
}

/// What a column's values mean, as the server's described table names it.
/// More roles will come, so match it with a wildcard.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Role {
    #[default]
    None,
    /// When something happened: dates draw relative to now.
    Time,
}

impl Role {
    /// The role a server names; a name this crate does not know is `None`.
    pub fn from_name(name: &str) -> Role {
        match name {
            "time" => Role::Time,
            _ => Role::None,
        }
    }
}

/// How a column's dates draw, whatever its role.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeStyle {
    /// `3m ago`, `in 2h`.
    Relative,
    /// `2026-10-29 14:03`, in local time.
    Absolute,
}

/// A column's choices. The default follows the role.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FormatOptions {
    /// Relative or absolute dates; `None` draws a `Time` column's relative
    /// and any other's absolute.
    pub time: Option<TimeStyle>,
    /// Decimals for every Number in the column, usually from
    /// [`fewest_decimals`] over its numbers; `None` gives each the fewest
    /// that show it.
    pub decimals: Option<u8>,
    /// What a null shows, in place of [`NULL_GLYPH`].
    pub null_glyph: Option<String>,
    /// Plain-text mode, for output with no styles to see: a null is `-`.
    pub plain: bool,
}

/// What the host injects so the formatter stays pure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FormatContext {
    /// The time relative dates count from.
    pub now: Timestamp,
    /// The local zone, in seconds east of UTC, absolute dates draw in.
    pub utc_offset: i32,
}

/// Formats `value` of a column with `unit` (a UCUM code), `role` and
/// `options`.
pub fn format(
    value: &Value,
    unit: Option<&str>,
    role: Role,
    options: &FormatOptions,
    ctx: &FormatContext,
) -> Formatted {
    let start = |text: Cow<'_, str>, style| Formatted {
        text: text.into_owned(),
        align: Align::Start,
        style,
    };
    let end = |text: String| Formatted {
        text,
        align: Align::End,
        style: Style::NONE,
    };
    let numeric = |n: Num| match unit {
        Some(unit) => with_unit(n, unit, options.decimals),
        None => n.digits(options.decimals),
    };
    match value {
        Value::Null => {
            let glyph = match (&options.null_glyph, options.plain) {
                (Some(glyph), _) => one_line(glyph),
                (None, true) => Cow::Borrowed(PLAIN_NULL_GLYPH),
                (None, false) => Cow::Borrowed(NULL_GLYPH),
            };
            start(glyph, Style::DIM)
        }
        Value::Text(text) => start(one_line(text), Style::NONE),
        Value::Invalid { raw, .. } => start(one_line(raw), Style::WARNING),
        Value::Int(i) => end(numeric(Num::Int(i128::from(*i)))),
        Value::UInt(u) => end(numeric(Num::Int(i128::from(*u)))),
        Value::Number(x) => end(numeric(Num::Float(*x))),
        Value::DateTime(at) => {
            let relative_time = match options.time {
                Some(style) => style == TimeStyle::Relative,
                None => role == Role::Time,
            };
            end(if relative_time {
                relative(*at, ctx.now)
            } else {
                absolute(*at, ctx.utc_offset)
            })
        }
        Value::Bool(b) => start(Cow::Borrowed(if *b { "✓" } else { "" }), Style::NONE),
        Value::Category { value, label } => {
            start(one_line(label.as_deref().unwrap_or(value)), Style::NONE)
        }
        Value::Bytes(b) => {
            let mut text = bytes(b.len() as f64);
            if !b.is_empty() {
                text.push(' ');
                for byte in b.iter().take(HEX_PREFIX) {
                    text.push_str(&format!("{byte:02x}"));
                }
                if b.len() > HEX_PREFIX {
                    text.push('…');
                }
            }
            start(Cow::Owned(text), Style::NONE)
        }
        Value::Nested(json) => {
            let text = json_prefix(json, NESTED_MAX);
            start(Cow::Owned(cut(&one_line(&text), NESTED_MAX)), Style::NONE)
        }
    }
}

/// The fewest decimals, at most 3, that show every finite number in
/// `numbers` without loss: what a column's [`FormatOptions::decimals`]
/// usually is, so its numbers line up on their decimal point.
pub fn fewest_decimals(numbers: impl IntoIterator<Item = f64>) -> u8 {
    numbers
        .into_iter()
        .filter(|x| x.is_finite())
        .map(|x| {
            if x.abs() >= EXPONENT_FROM {
                return 0;
            }
            (0..MAX_DECIMALS)
                .find(|&d| {
                    let d = usize::from(d);
                    format!("{x:.d$}").parse::<f64>() == Ok(x)
                })
                .unwrap_or(MAX_DECIMALS)
        })
        .max()
        .unwrap_or(0)
}

/// `x` to `decimals` places. NaN is `NaN` and the infinities `∞` and `-∞`;
/// past 1e15 it takes an exponent, `1.5e20`. A value that rounds to zero
/// loses its sign.
pub(crate) fn number(x: f64, decimals: u8) -> String {
    if x.is_nan() {
        return "NaN".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "∞" } else { "-∞" }.to_owned();
    }
    if x.abs() >= EXPONENT_FROM {
        return format!("{x:e}");
    }
    let d = usize::from(decimals);
    let text = format!("{x:.d$}");
    match text.strip_prefix('-') {
        Some(unsigned) if unsigned.bytes().all(|c| c == b'0' || c == b'.') => unsigned.to_owned(),
        _ => text,
    }
}

/// `text` on one line: each line break (`\r\n` counting as one) becomes
/// `↵`, a tab a space, and any other control character `�`.
pub(crate) fn one_line(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) && !text.contains(['\u{2028}', '\u{2029}']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('↵');
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => out.push('↵'),
            '\t' => out.push(' '),
            c if c.is_control() => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// The start of `json`'s text, long enough to hold more than `chars`
/// characters when it has them, without writing the rest of a large tree.
fn json_prefix(json: &serde_json::Value, chars: usize) -> String {
    struct Prefix(Vec<u8>, usize);
    impl std::io::Write for Prefix {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let room = self.1 - self.0.len();
            self.0.extend_from_slice(&buf[..buf.len().min(room)]);
            match buf.len() > room {
                true => Err(std::io::ErrorKind::WriteZero.into()),
                false => Ok(buf.len()),
            }
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut prefix = Prefix(Vec::new(), (chars + 1) * 4);
    let _ = serde_json::to_writer(&mut prefix, json);
    let bytes = prefix.0;
    match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_owned(),
        Err(e) => String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned(),
    }
}

/// `text` cut to `max` characters, ending in `…` when cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max - 1).collect();
    out.push('…');
    out
}
