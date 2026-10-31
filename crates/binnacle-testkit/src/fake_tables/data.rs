//! A fake table's data: its schema and rows by namespace, from a recorded
//! fixture or generated, and the `where` clauses that narrow them.

use std::collections::BTreeMap;
use std::path::Path;

use binnacle_described::{Decoder, Encoding, Field, Message, Row, Schema, Wire};
use serde_json::json;

/// One table the fake serves: its schema and its rows, which live in one
/// namespace (`flotilla` unless set).
#[derive(Clone, Debug, PartialEq)]
pub struct FakeTable {
    pub name: String,
    pub schema: Schema,
    pub namespace: String,
    /// What its `table` messages name as its life, `durable` by default.
    pub incarnation: Option<String>,
    pub rows: Vec<Row>,
}

impl FakeTable {
    pub fn new(name: impl Into<String>, schema: Schema) -> FakeTable {
        FakeTable {
            name: name.into(),
            schema,
            namespace: "flotilla".into(),
            incarnation: Some("durable".into()),
            rows: Vec::new(),
        }
    }

    pub fn with_rows(mut self, rows: Vec<Row>) -> FakeTable {
        self.rows = rows;
        self
    }

    pub fn in_namespace(mut self, namespace: impl Into<String>) -> FakeTable {
        self.namespace = namespace.into();
        self
    }

    /// A table from a recorded rows or watch stream (`.ndjson` or
    /// `.cborseq`): its `table` message's schema, then its rows, upserts and
    /// deletes in order. Its name is the fixture's: `rows-hosts.ndjson` is
    /// `hosts`.
    pub fn from_fixture(path: impl AsRef<Path>) -> Result<FakeTable, String> {
        let path = path.as_ref();
        let file = (path.file_name().and_then(|n| n.to_str())).unwrap_or_default();
        let stem = file.split('.').next().unwrap_or_default();
        let stem = (stem
            .strip_prefix("rows-")
            .or_else(|| stem.strip_prefix("watch-")))
        .unwrap_or(stem);
        let name = stem.split('-').next().unwrap_or(stem);
        let encoding = match file.rsplit('.').next() {
            Some("cborseq") | Some("cbor") => Encoding::CborSeq,
            _ => Encoding::Ndjson,
        };
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut decoder = Decoder::new(encoding);
        decoder.feed(&bytes);
        let mut messages = Vec::new();
        while let Some(message) = decoder.next() {
            messages.push(message.map_err(|e| format!("{}: {e}", path.display()))?);
        }
        if let Some(last) = decoder.finish() {
            messages.push(last.map_err(|e| format!("{}: {e}", path.display()))?);
        }
        let mut table: Option<FakeTable> = None;
        for message in messages {
            match (message, &mut table) {
                (Message::Table(t), None) => {
                    let mut fake = FakeTable::new(name, t.schema);
                    fake.incarnation = t.incarnation;
                    table = Some(fake);
                }
                (Message::Rows(rows), Some(table)) => {
                    for row in rows.rows {
                        table.upsert(row);
                    }
                }
                (Message::Upsert { row, .. }, Some(table)) => {
                    table.upsert(row);
                }
                (Message::Delete { key }, Some(table)) => {
                    table.delete(&key);
                }
                _ => {}
            }
        }
        table.ok_or_else(|| format!("{}: no table message", path.display()))
    }

    /// A made table of `rows` rows keyed by `id`, with nulls, empty text,
    /// invalid values, datetimes and an object column.
    pub fn generated(name: impl Into<String>, rows: usize) -> FakeTable {
        let field = |properties: serde_json::Value| {
            let serde_json::Value::Object(properties) = properties else {
                unreachable!()
            };
            Field::from_properties(properties).expect("a name and a type")
        };
        let schema = Schema {
            fields: vec![
                field(
                    json!({"name": "id", "type": "integer", "title": "ID", "roles": ["key"], "sortable": true, "constraints": {"required": true}}),
                ),
                field(
                    json!({"name": "name", "type": "string", "title": "Name", "sortable": true, "whereOperators": ["=="]}),
                ),
                field(
                    json!({"name": "phase", "type": "string", "title": "Phase", "roles": ["state"], "whereOperators": ["=="]}),
                ),
                field(
                    json!({"name": "started", "type": "datetime", "title": "Started", "roles": ["time"]}),
                ),
                field(json!({"name": "load", "type": "number", "title": "Load", "unit": "%"})),
                field(json!({"name": "spec", "type": "object", "title": "Spec"})),
            ],
            primary_key: vec!["id".into()],
            foreign_keys: Vec::new(),
        };
        let rows = (0..rows as u64).map(generated_row).collect();
        FakeTable::new(name, schema).with_rows(rows)
    }

    /// Whether the table has a primary key.
    pub fn keyed(&self) -> bool {
        !self.schema.primary_key.is_empty()
    }

    /// The primary-key tuple of `row`.
    pub fn key(&self, row: &Row) -> Vec<Wire> {
        (self.schema.primary_key.iter())
            .map(|name| row.get(name).cloned().unwrap_or(Wire::Null))
            .collect()
    }

    /// Where the row with key tuple `key` is, if the table has it. In a
    /// table with no primary key, `[n]` names the row at place `n`.
    pub fn find(&self, key: &[Wire]) -> Option<usize> {
        if !self.keyed() {
            let [Wire::Int(n)] = key else {
                return None;
            };
            return usize::try_from(*n).ok().filter(|&n| n < self.rows.len());
        }
        self.rows.iter().position(|r| self.key(r) == key)
    }

    /// Sets `row` in place of the row with its key, or adds it at the end.
    /// Returns the row it replaced. A table with no primary key has no row
    /// to replace, so `row` is added.
    pub fn upsert(&mut self, row: Row) -> Option<Row> {
        let found = match self.keyed() {
            true => self.find(&self.key(&row)),
            false => None,
        };
        match found {
            Some(at) => Some(std::mem::replace(&mut self.rows[at], row)),
            None => {
                self.rows.push(row);
                None
            }
        }
    }

    /// Removes the row with key tuple `key` (in a table with no primary
    /// key, `[n]` for the row at place `n`), returning it.
    pub fn delete(&mut self, key: &[Wire]) -> Option<Row> {
        self.find(key).map(|at| self.rows.remove(at))
    }
}

fn generated_row(i: u64) -> Row {
    let mut row = BTreeMap::new();
    row.insert("id".to_owned(), Wire::Int(i.into()));
    let name = match i % 13 {
        0 => Wire::Text(String::new()),
        7 => Wire::Null,
        _ => Wire::Text(format!("row-{i:05}")),
    };
    row.insert("name".to_owned(), name);
    let phase = ["Running", "Stalled", "Done", "Running", "Pending"][(i % 5) as usize];
    let phase = match i % 11 {
        3 => Wire::Null,
        _ => Wire::Text(phase.into()),
    };
    row.insert("phase".to_owned(), phase);
    let started = match i % 17 {
        4 => Wire::Null,
        9 => Wire::Text("soon".into()),
        _ => Wire::Text(rfc3339(
            1_785_000_000 + i as i64 * 61,
            (i * 7_919 % 1_000_000_000) as u32,
        )),
    };
    row.insert("started".to_owned(), started);
    let load = match i % 19 {
        5 => Wire::Null,
        11 => Wire::Text("high".into()),
        _ => Wire::Float((i * 37 % 1000) as f64 / 10.0),
    };
    row.insert("load".to_owned(), load);
    let image = ["rust", "zig", "go"][(i % 3) as usize];
    let spec = Wire::from_json(&json!({"image": image, "replicas": i % 4}));
    row.insert("spec".to_owned(), spec);
    row
}

/// The civil date of a day count since the epoch (Howard Hinnant's
/// algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

fn rfc3339(secs: i64, nanos: u32) -> String {
    let (y, mo, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{nanos:09}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// A `where` clause the fake understands: `.field.path == value`.
#[derive(Clone, Debug, PartialEq)]
pub struct Where {
    path: String,
    value: String,
}

impl Where {
    /// Reads a clause, or says why it can't, as flotillad's 400 does.
    pub fn parse(clause: &str) -> Result<Where, String> {
        let at = clause.len() - clause.trim_start().len();
        let rest = clause.trim_start();
        let Some(rest) = rest.strip_prefix('.') else {
            return Err(format!(
                "expected a field path starting with `.` at byte {} of `$row {clause}`",
                at + 5
            ));
        };
        let Some((path, value)) = rest.split_once("==") else {
            return Err(format!("expected `==` in `$row {clause}`"));
        };
        let path = path.trim();
        let value = value.trim();
        let value = (value.strip_prefix('"').and_then(|v| v.strip_suffix('"'))).unwrap_or(value);
        if path.is_empty() || path.contains(char::is_whitespace) || value.contains("==") {
            return Err(format!("expected `.field == value` in `$row {clause}`"));
        }
        Ok(Where {
            path: path.to_owned(),
            value: value.to_owned(),
        })
    }

    /// Whether `row` has the value at the path: a field by its whole name,
    /// else the longest field the path starts with, then into its object.
    pub fn keeps(&self, row: &Row) -> bool {
        let found = row.get(&self.path).or_else(|| {
            let mut best: Option<(&String, &Wire)> = None;
            for (name, value) in row {
                let inside = self.path.starts_with(name.as_str())
                    && self.path[name.len()..].starts_with('.');
                if inside && best.is_none_or(|(b, _)| name.len() > b.len()) {
                    best = Some((name, value));
                }
            }
            let (name, mut value) = best?;
            for part in self.path[name.len() + 1..].split('.') {
                value = value.as_map()?.get(part)?;
            }
            Some(value)
        });
        match found {
            Some(Wire::Text(text)) => *text == self.value,
            Some(Wire::Null) | None => false,
            Some(other) => {
                let text = other.to_json().to_string();
                text == self.value
            }
        }
    }
}
