//! The steps a fake plays to the watches open on it, built in Rust or read
//! from a JSON-lines file.

use std::time::Duration;

use binnacle_described::{Field, Row, Wire};
use serde_json::Value as Json;

/// One step of a script. A step names its table; an empty name is the
/// first table served.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
#[allow(clippy::large_enum_variant)]
pub enum Step {
    /// A row added or changed: watches get an `upsert`, or a `delete` if
    /// it left their `where` clauses.
    Upsert { table: String, row: Row },
    /// A row gone, by its primary-key tuple: watches get a `delete`.
    Delete { table: String, key: Vec<Wire> },
    /// Watches get a `resync`, then the table again: `table`, `rows`,
    /// `synced`.
    Resync { table: String },
    /// A field added, every row taking `value` for it: watches get the
    /// new `table`, then a `rows` with every row.
    AddField {
        table: String,
        field: Field,
        value: Wire,
    },
    /// A field removed: watches get the new `table`, then a `rows` with
    /// every row.
    RemoveField { table: String, name: String },
    /// Every watch's stream ends.
    End,
    /// The script waits.
    Sleep(Duration),
    /// The next rows request, watch or read, is answered with this status.
    Fail { status: u16 },
    /// Watches get a line (or CBOR item) that doesn't decode as a message.
    Garbage { table: String },
}

impl Step {
    /// Reads a step from one line of a script:
    ///
    /// ```text
    /// {"step": "upsert", "table": "hosts", "row": {"metadata.name": "a"}}
    /// {"step": "delete", "table": "hosts", "key": ["a"]}
    /// {"step": "resync", "table": "hosts"}
    /// {"step": "add-field", "table": "hosts", "field": {"name": "zone", "type": "string"}, "value": "eu"}
    /// {"step": "remove-field", "table": "hosts", "field": "zone"}
    /// {"step": "end"}
    /// {"step": "sleep", "ms": 500}
    /// {"step": "fail", "status": 500}
    /// {"step": "garbage", "table": "hosts"}
    /// ```
    ///
    /// `table` may be left out for the first table served.
    pub fn from_json_line(line: &str) -> Result<Step, String> {
        let json: Json = serde_json::from_str(line).map_err(|e| format!("{e}: {line}"))?;
        let text = |name: &str| json.get(name).and_then(Json::as_str);
        let table = text("table").unwrap_or_default().to_owned();
        let missing = |what: &str| format!("a step needs `{what}`: {line}");
        Ok(match text("step").ok_or_else(|| missing("step"))? {
            "upsert" => match Wire::from_json(json.get("row").ok_or_else(|| missing("row"))?) {
                Wire::Map(row) => Step::Upsert { table, row },
                _ => return Err(missing("row")),
            },
            "delete" => match Wire::from_json(json.get("key").ok_or_else(|| missing("key"))?) {
                Wire::Array(key) => Step::Delete { table, key },
                _ => return Err(missing("key")),
            },
            "resync" => Step::Resync { table },
            "add-field" => {
                let Some(Json::Object(field)) = json.get("field") else {
                    return Err(missing("field"));
                };
                Step::AddField {
                    table,
                    field: Field::from_properties(field.clone()).map_err(|e| e.to_string())?,
                    value: Wire::from_json(json.get("value").unwrap_or(&Json::Null)),
                }
            }
            "remove-field" => Step::RemoveField {
                table,
                name: text("field").ok_or_else(|| missing("field"))?.to_owned(),
            },
            "end" => Step::End,
            "sleep" => Step::Sleep(Duration::from_millis(
                json.get("ms")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| missing("ms"))?,
            )),
            "fail" => Step::Fail {
                status: (json.get("status").and_then(Json::as_u64))
                    .and_then(|s| u16::try_from(s).ok())
                    .ok_or_else(|| missing("status"))?,
            },
            "garbage" => Step::Garbage { table },
            other => return Err(format!("no step `{other}`: {line}")),
        })
    }
}

/// Reads a script: one step per line, blank lines and `#` comments
/// skipped.
pub fn parse_script(text: &str) -> Result<Vec<Step>, String> {
    (text.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(Step::from_json_line)
        .collect()
}
