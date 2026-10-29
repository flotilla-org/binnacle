//! An ID as JSON, as the schema and the C API name items.

use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

/// An item's ID as JSON: a string or a number, or an array of strings,
/// numbers, booleans and `null`s, such as the values of a primary key. Two
/// IDs are the same when their JSON values are equal.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct JsonId(Value);

impl JsonId {
    /// `value` as an ID, if it has an ID's shape.
    pub fn new(value: Value) -> Option<Self> {
        let scalar = |v: &Value| !matches!(v, Value::Array(_) | Value::Object(_));
        let id = match &value {
            Value::String(_) | Value::Number(_) => true,
            Value::Array(items) => items.iter().all(scalar),
            _ => false,
        };
        id.then_some(JsonId(value))
    }

    /// The ID `json` is the text of, if it is one.
    pub fn from_json(json: &[u8]) -> Option<Self> {
        serde_json::from_slice(json).ok().and_then(JsonId::new)
    }

    pub fn value(&self) -> &Value {
        &self.0
    }
}

impl From<&str> for JsonId {
    fn from(id: &str) -> Self {
        JsonId(Value::String(id.to_owned()))
    }
}

impl fmt::Display for JsonId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for JsonId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for JsonId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        JsonId::new(value)
            .ok_or_else(|| de::Error::custom("an ID is a string, a number or an array of scalars"))
    }
}

impl JsonSchema for JsonId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Id".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "An item's ID: a string or a number, or an array of strings, numbers, booleans and nulls, such as the values of a primary key. A Group Item's or Summary Item's ID is whatever the Table Source names it by.",
            "anyOf": [
                { "type": "string" },
                { "type": "number" },
                { "type": "array", "items": { "type": ["string", "number", "boolean", "null"] } }
            ]
        })
    }
}
