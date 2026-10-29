//! The messages of a described table's rows stream, and their decoding from
//! newline-delimited JSON or a CBOR sequence.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{Map, Value as Json};

use crate::wire::Wire;

/// One row as a message carries it: each field's value by field name. A
/// field the row leaves out is null.
pub type Row = BTreeMap<String, Wire>;

/// One message of a rows stream, by its `type`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Message {
    /// The table's description, which comes first and again after a resync.
    Table(TableMessage),
    /// A page of rows.
    Rows(RowsMessage),
    /// The rows so far are the table as of `sequence`.
    Synced { sequence: Option<u64> },
    /// A row added or changed, by its primary-key tuple.
    Upsert { key: Vec<Wire>, row: Row },
    /// A row gone, by its primary-key tuple.
    Delete { key: Vec<Wire> },
    /// A heartbeat.
    Progress { sequence: Option<u64> },
    /// Start again: a fresh `table`, `rows` and `synced` follow.
    Resync,
    /// How many rows the table has, or about how many.
    Count { count: u64, estimate: bool },
    /// A message of a type this crate does not know, kept whole.
    Other { kind: String, body: Wire },
}

/// A `table` message.
#[derive(Clone, Debug, PartialEq)]
pub struct TableMessage {
    pub schema: Schema,
    /// Which life of the table this is: `durable` for one that outlives
    /// the daemon.
    pub incarnation: Option<String>,
    /// The table's name, when the message gives it.
    pub table: Option<String>,
}

/// A `rows` message.
#[derive(Clone, Debug, PartialEq)]
pub struct RowsMessage {
    pub rows: Vec<Row>,
    /// Where the next page starts; `None` when there is none. Paging stops
    /// on this, never on `partial`.
    pub cursor: Option<String>,
    /// This response is not the whole table (more pages, an offset or a
    /// cursor was given). Not a reason to fetch again.
    pub partial: bool,
}

/// A Frictionless-style table schema.
#[derive(Clone, Debug, PartialEq)]
pub struct Schema {
    pub fields: Vec<Field>,
    /// The names of the fields that identify a row.
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
}

/// The type a field declares for its values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Datetime,
    Object,
    Array,
    /// A type this crate does not read: its values are taken as they come.
    Other(String),
}

impl FieldType {
    pub fn from_name(name: &str) -> FieldType {
        match name {
            "string" => FieldType::String,
            "integer" => FieldType::Integer,
            "number" => FieldType::Number,
            "boolean" => FieldType::Boolean,
            "datetime" => FieldType::Datetime,
            "object" => FieldType::Object,
            "array" => FieldType::Array,
            other => FieldType::Other(other.to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            FieldType::String => "string",
            FieldType::Integer => "integer",
            FieldType::Number => "number",
            FieldType::Boolean => "boolean",
            FieldType::Datetime => "datetime",
            FieldType::Object => "object",
            FieldType::Array => "array",
            FieldType::Other(name) => name,
        }
    }
}

/// One of a field's categories: the value that compares, and the label
/// that shows, if any.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Category {
    pub value: String,
    pub label: Option<String>,
}

/// One field of a schema: a column. The properties this crate reads have
/// their own accessors; [`Field::property`] reads any of them by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: FieldType,
    pub title: Option<String>,
    /// What the values mean: `key`, `time`, `state` and others.
    pub roles: Vec<String>,
    pub sortable: Option<bool>,
    pub where_operators: Vec<String>,
    /// `constraints.required`.
    pub required: Option<bool>,
    pub categories: Option<Vec<Category>>,
    /// A UCUM code.
    pub unit: Option<String>,
    properties: Map<String, Json>,
}

impl Field {
    /// A field with only a name and a type.
    pub fn new(name: impl Into<String>, field_type: FieldType) -> Field {
        let name = name.into();
        let mut properties = Map::new();
        properties.insert("name".into(), Json::String(name.clone()));
        properties.insert("type".into(), Json::String(field_type.name().into()));
        Field::from_properties(properties).expect("a name and a type")
    }

    /// Reads a field from its JSON object.
    pub fn from_properties(properties: Map<String, Json>) -> Result<Field, DecodeError> {
        let text = |name: &str| {
            properties
                .get(name)
                .and_then(Json::as_str)
                .map(String::from)
        };
        let texts = |name: &str| -> Vec<String> {
            (properties.get(name).and_then(Json::as_array).into_iter())
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        };
        let name = text("name").ok_or_else(|| DecodeError::schema("a field has no name"))?;
        let field_type = FieldType::from_name(text("type").as_deref().unwrap_or("any"));
        let required = (properties.get("constraints"))
            .and_then(|c| c.get("required"))
            .and_then(Json::as_bool);
        let categories = (properties.get("categories").and_then(Json::as_array))
            .map(|items| items.iter().map(category).collect());
        Ok(Field {
            name,
            field_type,
            title: text("title"),
            roles: texts("roles"),
            sortable: properties.get("sortable").and_then(Json::as_bool),
            where_operators: texts("whereOperators"),
            required,
            categories,
            unit: text("unit"),
            properties,
        })
    }

    /// Any property the field has, as sent: `additionalProperties`,
    /// `items` and those this crate reads too.
    pub fn property(&self, name: &str) -> Option<&Json> {
        self.properties.get(name)
    }

    pub fn properties(&self) -> &Map<String, Json> {
        &self.properties
    }

    /// Whether its values may be null: unless `constraints.required` says
    /// they may not.
    pub fn nullable(&self) -> bool {
        self.required != Some(true)
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }

    /// The header a host shows: the title, else the name.
    pub fn header(&self) -> &str {
        self.title.as_deref().unwrap_or(&self.name)
    }
}

/// A category as Frictionless lists one: a bare value, or an object with a
/// `value` and maybe a `label`.
fn category(item: &Json) -> Category {
    let text = |v: &Json| match v {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    };
    match item.get("value") {
        Some(value) => Category {
            value: text(value),
            label: item.get("label").and_then(Json::as_str).map(String::from),
        },
        None => Category {
            value: text(item),
            label: None,
        },
    }
}

/// A foreign key: fields of this table that name rows of another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignKey {
    pub fields: Vec<String>,
    /// The table referred to, `None` (Frictionless's `""`) for this one.
    pub resource: Option<String>,
    /// Its fields, one for each of `fields`.
    pub reference_fields: Vec<String>,
}

/// One name or a list of them, as Frictionless allows.
fn names(json: Option<&Json>) -> Vec<String> {
    match json {
        Some(Json::String(s)) => vec![s.clone()],
        Some(Json::Array(items)) => (items.iter())
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        _ => Vec::new(),
    }
}

impl Schema {
    /// Reads a schema from its JSON object.
    pub fn from_json(json: &Json) -> Result<Schema, DecodeError> {
        let object = json
            .as_object()
            .ok_or_else(|| DecodeError::schema("a schema is not an object"))?;
        let fields = (object.get("fields").and_then(Json::as_array))
            .ok_or_else(|| DecodeError::schema("a schema has no fields"))?;
        let fields = (fields.iter())
            .map(|f| match f {
                Json::Object(properties) => Field::from_properties(properties.clone()),
                _ => Err(DecodeError::schema("a field is not an object")),
            })
            .collect::<Result<Vec<Field>, DecodeError>>()?;
        let foreign_keys = (object
            .get("foreignKeys")
            .and_then(Json::as_array)
            .into_iter())
        .flatten()
        .map(|fk| {
            let reference = fk.get("reference");
            ForeignKey {
                fields: names(fk.get("fields")),
                resource: (reference.and_then(|r| r.get("resource")))
                    .and_then(Json::as_str)
                    .filter(|r| !r.is_empty())
                    .map(String::from),
                reference_fields: names(reference.and_then(|r| r.get("fields"))),
            }
        })
        .collect();
        Ok(Schema {
            fields,
            primary_key: names(object.get("primaryKey")),
            foreign_keys,
        })
    }

    /// Reads a schema from JSON bytes, as `GET /tables/{t}` answers with
    /// `application/json`.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Schema, DecodeError> {
        let wire: Wire = serde_json::from_slice(bytes).map_err(DecodeError::json)?;
        Schema::from_json(&wire.to_json())
    }

    /// Reads a schema from one CBOR item, as `GET /tables/{t}` answers with
    /// `application/cbor`.
    pub fn from_cbor_bytes(bytes: &[u8]) -> Result<Schema, DecodeError> {
        Schema::from_json(&cbor_item(bytes)?.to_json())
    }
}

impl Message {
    /// Reads a message from its decoded value.
    pub fn from_wire(wire: Wire) -> Result<Message, DecodeError> {
        let Wire::Map(mut map) = wire else {
            return Err(DecodeError::message("a message is not an object"));
        };
        let kind = match map.get("type") {
            Some(Wire::Text(kind)) => kind.clone(),
            _ => return Err(DecodeError::message("a message has no type")),
        };
        let sequence = |map: &BTreeMap<String, Wire>| match map.get("sequence") {
            Some(Wire::Int(n)) => u64::try_from(*n).ok(),
            _ => None,
        };
        let key = |map: &mut BTreeMap<String, Wire>| match map.remove("key") {
            Some(Wire::Array(key)) => Ok(key),
            _ => Err(DecodeError::message("a key is not an array")),
        };
        let message = match kind.as_str() {
            "table" => {
                let schema = map
                    .get("schema")
                    .ok_or_else(|| DecodeError::message("a table message has no schema"))?;
                Message::Table(TableMessage {
                    schema: Schema::from_json(&schema.to_json())?,
                    incarnation: map
                        .get("incarnation")
                        .and_then(Wire::as_str)
                        .map(String::from),
                    table: map.get("table").and_then(Wire::as_str).map(String::from),
                })
            }
            "rows" => {
                let rows = match map.remove("rows") {
                    Some(Wire::Array(rows)) => rows,
                    _ => return Err(DecodeError::message("a rows message has no rows")),
                };
                let rows = (rows.into_iter())
                    .map(|row| match row {
                        Wire::Map(row) => Ok(row),
                        _ => Err(DecodeError::message("a row is not an object")),
                    })
                    .collect::<Result<Vec<Row>, DecodeError>>()?;
                let cursor = match map.get("cursor") {
                    Some(Wire::Text(cursor)) => Some(cursor.clone()),
                    None | Some(Wire::Null) => None,
                    Some(_) => return Err(DecodeError::message("a cursor is not text")),
                };
                Message::Rows(RowsMessage {
                    rows,
                    cursor,
                    partial: matches!(map.get("partial"), Some(Wire::Bool(true))),
                })
            }
            "synced" => Message::Synced {
                sequence: sequence(&map),
            },
            "upsert" => Message::Upsert {
                key: key(&mut map)?,
                row: match map.remove("row") {
                    Some(Wire::Map(row)) => row,
                    _ => return Err(DecodeError::message("an upsert has no row")),
                },
            },
            "delete" => Message::Delete {
                key: key(&mut map)?,
            },
            "progress" => Message::Progress {
                sequence: sequence(&map),
            },
            "resync" => Message::Resync,
            "count" => Message::Count {
                count: match map.get("count") {
                    Some(Wire::Int(n)) => u64::try_from(*n).unwrap_or(0),
                    _ => return Err(DecodeError::message("a count message has no count")),
                },
                estimate: matches!(map.get("estimate"), Some(Wire::Bool(true))),
            },
            _ => Message::Other {
                kind,
                body: Wire::Map(map),
            },
        };
        Ok(message)
    }

    /// Reads a message from JSON, as a host that decoded it already has it.
    pub fn from_json(json: &Json) -> Result<Message, DecodeError> {
        Message::from_wire(Wire::from_json(json))
    }
}

/// Decodes one line of an `application/x-ndjson` rows stream, its line
/// break left off or not.
pub fn decode_json_line(line: &[u8]) -> Result<Message, DecodeError> {
    let wire: Wire = serde_json::from_slice(line).map_err(DecodeError::json)?;
    Message::from_wire(wire)
}

/// Decodes one item of an `application/cbor-seq` rows stream: `item` holds
/// exactly one.
pub fn decode_cbor_item(item: &[u8]) -> Result<Message, DecodeError> {
    Message::from_wire(cbor_item(item)?)
}

fn cbor_item(bytes: &[u8]) -> Result<Wire, DecodeError> {
    let mut reader = bytes;
    let wire: Wire = ciborium::from_reader(&mut reader).map_err(DecodeError::cbor)?;
    match reader.is_empty() {
        true => Ok(wire),
        false => Err(DecodeError::message("bytes after the CBOR item")),
    }
}

/// How a rows stream is encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// `application/x-ndjson`: one JSON object per line.
    Ndjson,
    /// `application/cbor-seq`: one CBOR item after another.
    CborSeq,
}

impl Encoding {
    pub fn content_type(self) -> &'static str {
        match self {
            Encoding::Ndjson => "application/x-ndjson",
            Encoding::CborSeq => "application/cbor-seq",
        }
    }

    /// The encoding a response's `content-type` names, parameters and case
    /// aside.
    pub fn from_content_type(content_type: &str) -> Option<Encoding> {
        let media = content_type.split(';').next()?.trim().to_ascii_lowercase();
        match media.as_str() {
            "application/x-ndjson" => Some(Encoding::Ndjson),
            "application/cbor-seq" => Some(Encoding::CborSeq),
            _ => None,
        }
    }
}

/// An incremental decoder for a rows stream: the host feeds it bytes as
/// they come, in pieces of any size, and takes each whole message out.
#[derive(Clone, Debug)]
pub struct Decoder {
    encoding: Encoding,
    buffer: Vec<u8>,
}

impl Decoder {
    pub fn new(encoding: Encoding) -> Decoder {
        Decoder {
            encoding,
            buffer: Vec::new(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole message fed, `None` until one is. A message that
    /// fails to decode is dropped from the buffer and its error returned.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Result<Message, DecodeError>> {
        match self.encoding {
            Encoding::Ndjson => loop {
                let end = self.buffer.iter().position(|&b| b == b'\n')?;
                let line: Vec<u8> = self.buffer.drain(..=end).collect();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                return Some(decode_json_line(&line));
            },
            Encoding::CborSeq => {
                if self.buffer.is_empty() {
                    return None;
                }
                let mut reader = &self.buffer[..];
                match ciborium::from_reader::<Wire, _>(&mut reader) {
                    Ok(wire) => {
                        let used = self.buffer.len() - reader.len();
                        self.buffer.drain(..used);
                        Some(Message::from_wire(wire))
                    }
                    Err(ciborium::de::Error::Io(e))
                        if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                    {
                        None
                    }
                    Err(e) => {
                        self.buffer.clear();
                        Some(Err(DecodeError::cbor(e)))
                    }
                }
            }
        }
    }

    /// The stream ended: the last message, if one was left without its
    /// line break, or an error if the bytes left are not a whole message.
    pub fn finish(&mut self) -> Option<Result<Message, DecodeError>> {
        let rest = std::mem::take(&mut self.buffer);
        if rest.iter().all(u8::is_ascii_whitespace) {
            return None;
        }
        Some(match self.encoding {
            Encoding::Ndjson => decode_json_line(&rest),
            Encoding::CborSeq => Err(DecodeError::message("the stream ended inside an item")),
        })
    }
}

/// Why bytes are not a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodeError {
    message: String,
}

impl DecodeError {
    fn json(e: serde_json::Error) -> DecodeError {
        DecodeError {
            message: format!("JSON: {e}"),
        }
    }

    fn cbor<E: fmt::Debug>(e: ciborium::de::Error<E>) -> DecodeError {
        DecodeError {
            message: format!("CBOR: {e:?}"),
        }
    }

    fn message(message: &str) -> DecodeError {
        DecodeError {
            message: message.into(),
        }
    }

    fn schema(message: &str) -> DecodeError {
        DecodeError::message(message)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DecodeError {}
