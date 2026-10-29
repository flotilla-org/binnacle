//! binnacle-format: Typed Values and the one formatter every host draws them
//! with, so a ratatui table, the web widget, a C renderer and a CLI printer
//! show the same text for the same value, unit and role.
//!
//! Pure and sans-IO: the host injects the clock and the zone through
//! [`FormatContext`], and the crate knows nothing of the Grid. What search,
//! filters, copy and assistive tech read is [`format_stable`]'s text.

pub mod fixture;
mod format;
mod json;
mod time;
mod units;

pub use format::{
    Align, FormatContext, FormatOptions, Formatted, NULL_GLYPH, PLAIN_NULL_GLYPH, Role, Style,
    TimeStyle, fewest_decimals, format, format_stable,
};
pub use json::from_tagged_json;
pub use time::{FixedOffset, Timestamp, Zone};

/// A Typed Value: one cell's value with its kind, as a described table
/// gives it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    UInt(u64),
    Number(f64),
    Text(String),
    Bytes(Vec<u8>),
    /// A UTC instant.
    DateTime(Timestamp),
    /// One of a known set: `value` is what compares, `label` what shows.
    Category {
        value: String,
        label: Option<String>,
    },
    /// A JSON tree.
    Nested(serde_json::Value),
    /// A value the source could not read as the kind its column expects:
    /// `raw` is what it got.
    Invalid {
        raw: String,
        expected: ValueType,
    },
}

impl Value {
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Null => ValueType::Null,
            Value::Bool(_) => ValueType::Bool,
            Value::Int(_) => ValueType::Int,
            Value::UInt(_) => ValueType::UInt,
            Value::Number(_) => ValueType::Number,
            Value::Text(_) => ValueType::Text,
            Value::Bytes(_) => ValueType::Bytes,
            Value::DateTime(_) => ValueType::DateTime,
            Value::Category { .. } => ValueType::Category,
            Value::Nested(_) => ValueType::Nested,
            Value::Invalid { .. } => ValueType::Invalid,
        }
    }
}

/// Names each kind of [`Value`]. Each has a fixed, non-zero
/// [`ValueType::code`] for a C host's kinds, so zero is never a kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ValueType {
    Null = 1,
    Bool = 2,
    Int = 3,
    UInt = 4,
    Number = 5,
    Text = 6,
    Bytes = 7,
    DateTime = 8,
    Category = 9,
    Nested = 10,
    Invalid = 11,
}

impl ValueType {
    pub const ALL: [ValueType; 11] = [
        ValueType::Null,
        ValueType::Bool,
        ValueType::Int,
        ValueType::UInt,
        ValueType::Number,
        ValueType::Text,
        ValueType::Bytes,
        ValueType::DateTime,
        ValueType::Category,
        ValueType::Nested,
        ValueType::Invalid,
    ];

    /// Its name, lower case: `"int"`, `"datetime"` and so on.
    pub fn name(self) -> &'static str {
        match self {
            ValueType::Null => "null",
            ValueType::Bool => "bool",
            ValueType::Int => "int",
            ValueType::UInt => "uint",
            ValueType::Number => "number",
            ValueType::Text => "text",
            ValueType::Bytes => "bytes",
            ValueType::DateTime => "datetime",
            ValueType::Category => "category",
            ValueType::Nested => "nested",
            ValueType::Invalid => "invalid",
        }
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(code: u8) -> Option<ValueType> {
        ValueType::ALL.into_iter().find(|t| t.code() == code)
    }

    pub fn from_name(name: &str) -> Option<ValueType> {
        ValueType::ALL.into_iter().find(|t| t.name() == name)
    }
}
