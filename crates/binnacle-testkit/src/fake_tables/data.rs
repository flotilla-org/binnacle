//! A fake table's data: its schema and rows by namespace, from a recorded
//! fixture or generated, and the `where` clauses that narrow them.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;

use binnacle_described::{Decoder, Encoding, Field, FieldType, Message, Row, Schema, Wire};
use rigging_condition::{
    CheckError, Checked, Condition as Clause, Lookup, Path as ConditionPath, Row as ConditionRow,
    Schema as ConditionSchema, Timestamp, Type, Value as ConditionValue,
};
use serde_json::json;

/// The `whereOperators` flotillad lists for an integer, number, string or
/// datetime field.
pub const ORDERED: [&str; 8] = ["==", "!=", "<", "<=", ">", ">=", "in", "has"];

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
                    json!({"name": "id", "type": "integer", "title": "ID", "roles": ["key"], "sortable": true, "constraints": {"required": true}, "whereOperators": ORDERED}),
                ),
                field(
                    json!({"name": "name", "type": "string", "title": "Name", "sortable": true, "whereOperators": ORDERED}),
                ),
                field(
                    json!({"name": "phase", "type": "string", "title": "Phase", "roles": ["state"], "whereOperators": ORDERED}),
                ),
                field(
                    json!({"name": "started", "type": "datetime", "title": "Started", "roles": ["time"], "whereOperators": ORDERED}),
                ),
                field(
                    json!({"name": "load", "type": "number", "title": "Load", "unit": "%", "whereOperators": ORDERED}),
                ),
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

/// A `where` clause in the condition language flotillad speaks
/// (`rigging-condition`), checked against a table's fields: the rows it
/// keeps are those it is true for.
#[derive(Clone, Debug)]
pub struct Where {
    checked: Checked<Clause>,
    /// The fields a clause may read, by path.
    fields: Vec<(ConditionPath, Field)>,
}

/// Why a clause was refused with a 400: its error code and message.
pub type Refused = (&'static str, String);

impl Where {
    /// Reads canonical clause text against `schema`'s fields, or says why
    /// it can't, as flotillad's 400 does: `invalid_filter` for text that
    /// does not parse, `unknown_column` or `type_mismatch` for a clause
    /// that does not check.
    pub fn parse(clause: &str, schema: &Schema) -> Result<Where, Refused> {
        let parsed = Clause::parse_strict(clause).map_err(|e| ("invalid_filter", e.to_string()))?;
        let fields = declared(schema);
        let mut declared = ConditionSchema::new();
        for (path, field) in &fields {
            if let Some(ty) = condition_type(field) {
                // A field that conflicts with another is left out.
                let _ = declared.insert_field(path.clone(), ty, field.nullable());
            }
        }
        let checked = Checked::new(parsed, &declared).map_err(|e| {
            let code = match e {
                CheckError::UnknownField(_) => "unknown_column",
                CheckError::TypeMismatch { .. }
                | CheckError::OrderingNotAllowed { .. }
                | CheckError::InvalidLiteral { .. } => "type_mismatch",
                _ => "invalid_filter",
            };
            (code, e.to_string())
        })?;
        Ok(Where { checked, fields })
    }

    /// Whether the clause is true for `row`. A value that does not match
    /// its field's type is missing, as a typed store can't hold it.
    pub fn keeps(&self, row: &Row) -> bool {
        let cells = Cells {
            row,
            fields: &self.fields,
        };
        self.checked.evaluate(&cells).is_true()
    }
}

/// The fields of `schema` a clause may read, by path: each of a type a
/// condition reads, named by its name split at each dot, or as one segment
/// where another such field's name is a dotted prefix of it.
fn declared(schema: &Schema) -> Vec<(ConditionPath, Field)> {
    let readable: Vec<&Field> = (schema.fields.iter())
        .filter(|f| condition_type(f).is_some())
        .collect();
    let mut fields = Vec::new();
    for field in &readable {
        let name = field.name.as_str();
        let prefixed = (readable.iter()).any(|o| {
            name.strip_prefix(o.name.as_str())
                .is_some_and(|r| r.starts_with('.'))
        });
        let path = match prefixed {
            true => ConditionPath::new([name]),
            false => ConditionPath::new(name.split('.')),
        };
        if let Ok(path) = path {
            fields.push((path, (*field).clone()));
        }
    }
    fields
}

/// The condition type of `field`'s values: a string with categories or
/// `constraints.enum` is an enum. `None` for one a condition can't read.
fn condition_type(field: &Field) -> Option<Type> {
    let listed = (field.property("constraints"))
        .and_then(|c| c.get("enum"))
        .and_then(serde_json::Value::as_array)
        .map(|values| values.iter().filter_map(|v| v.as_str().map(String::from)));
    Some(match field.field_type {
        FieldType::String => match (&field.categories, listed) {
            (Some(categories), _) => Type::enumeration(categories.iter().map(|c| c.value.clone())),
            (None, Some(listed)) => Type::enumeration(listed),
            (None, None) => Type::String,
        },
        FieldType::Other(ref name) if name == "address" => Type::String,
        FieldType::Integer => Type::Int,
        FieldType::Number => Type::Number,
        FieldType::Boolean => Type::Bool,
        FieldType::Datetime => Type::Timestamp,
        _ => return None,
    })
}

/// An RFC 3339 datetime to the microsecond it falls in, as a store holds it:
/// a fraction past six digits is cut.
fn micros(text: &str) -> Option<Timestamp> {
    let cut = match text.find('.') {
        Some(dot) => {
            let digits = text[dot + 1..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            let kept = digits.min(6);
            format!("{}{}", &text[..dot + 1 + kept], &text[dot + 1 + digits..])
        }
        None => text.to_owned(),
    };
    Timestamp::parse_rfc3339(&cut).ok()
}

/// One row as a clause reads it.
struct Cells<'a> {
    row: &'a Row,
    fields: &'a [(ConditionPath, Field)],
}

impl ConditionRow for Cells<'_> {
    fn get(&self, path: &ConditionPath) -> Lookup<'_> {
        let Some((_, field)) = self.fields.iter().find(|(p, _)| p == path) else {
            return Lookup::Absent;
        };
        let value = match (&field.field_type, self.row.get(&field.name)) {
            (FieldType::Integer, Some(Wire::Int(i))) => {
                i64::try_from(*i).ok().map(ConditionValue::Int)
            }
            (FieldType::Number, Some(Wire::Float(x))) => Some(ConditionValue::Number(*x)),
            (FieldType::Number, Some(Wire::Int(i))) => Some(ConditionValue::Number(*i as f64)),
            (FieldType::Boolean, Some(Wire::Bool(b))) => Some(ConditionValue::Bool(*b)),
            (FieldType::Datetime, Some(Wire::Text(text))) => {
                micros(text).map(ConditionValue::Timestamp)
            }
            (FieldType::String | FieldType::Other(_), Some(Wire::Text(text))) => {
                Some(ConditionValue::String(text.clone()))
            }
            _ => None,
        };
        match value {
            Some(value) => Lookup::Present(Cow::Owned(value)),
            None => Lookup::Absent,
        }
    }
}
