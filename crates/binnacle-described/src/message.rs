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
    /// How many rows match the Scope and the `where` clauses (`count`),
    /// or about how many, and how many the Scope alone (`total`), which an
    /// older server leaves out.
    Count {
        count: u64,
        total: Option<u64>,
        estimate: bool,
    },
    /// A field's most frequent values with how many rows hold each, a null
    /// for an absent one, over the Scope and the `where` clauses that read
    /// another field. A watch sends again only the entries that changed, a
    /// count of 0 for one gone. `more`: there are more values than these.
    Values {
        field: String,
        values: Vec<(Wire, u64)>,
        more: bool,
        sequence: Option<u64>,
    },
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
    /// The sequence the table is described as of, when the message gives
    /// it.
    pub sequence: Option<u64>,
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
    /// The sequence the page is read as of, when the message gives it.
    pub sequence: Option<u64>,
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
        let mut seen = std::collections::HashSet::new();
        if !fields.iter().all(|f| seen.insert(f.name.as_str())) {
            return Err(DecodeError::schema("a schema names a field twice"));
        }
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

    /// The schema as JSON, under the wire's names (`fields`, `primaryKey`,
    /// `foreignKeys`), each field written from its properties, so those
    /// this crate does not model survive.
    pub fn to_json(&self) -> Json {
        let fields = (self.fields.iter())
            .map(|f| Json::Object(f.properties.clone()))
            .collect();
        let texts =
            |names: &[String]| Json::Array(names.iter().cloned().map(Json::String).collect());
        let foreign_keys = (self.foreign_keys.iter())
            .map(|fk| {
                serde_json::json!({
                    "fields": texts(&fk.fields),
                    "reference": {
                        "resource": fk.resource.clone().unwrap_or_default(),
                        "fields": texts(&fk.reference_fields),
                    },
                })
            })
            .collect();
        let mut object = Map::new();
        object.insert("fields".into(), Json::Array(fields));
        object.insert("primaryKey".into(), texts(&self.primary_key));
        object.insert("foreignKeys".into(), Json::Array(foreign_keys));
        Json::Object(object)
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
                    sequence: sequence(&map),
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
                    sequence: sequence(&map),
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
                total: match map.get("total") {
                    // A negative total is none: the Totals fall back to the rows held.
                    Some(Wire::Int(n)) => u64::try_from(*n).ok(),
                    _ => None,
                },
                estimate: matches!(map.get("estimate"), Some(Wire::Bool(true))),
            },
            "values" => {
                let field = match map.get("field") {
                    Some(Wire::Text(field)) => field.clone(),
                    _ => return Err(DecodeError::message("a values message has no field")),
                };
                let values = match map.remove("values") {
                    Some(Wire::Array(values)) => values,
                    _ => return Err(DecodeError::message("a values message has no values")),
                };
                let values = (values.into_iter())
                    .map(|entry| {
                        let Wire::Map(mut entry) = entry else {
                            return Err(DecodeError::message("a value is not an object"));
                        };
                        let count = match entry.get("count") {
                            Some(Wire::Int(n)) => u64::try_from(*n).unwrap_or(0),
                            _ => return Err(DecodeError::message("a value has no count")),
                        };
                        Ok((entry.remove("value").unwrap_or(Wire::Null), count))
                    })
                    .collect::<Result<Vec<_>, DecodeError>>()?;
                Message::Values {
                    field,
                    values,
                    more: matches!(map.get("more"), Some(Wire::Bool(true))),
                    sequence: sequence(&map),
                }
            }
            _ => Message::Other {
                kind,
                body: Wire::Map(map),
            },
        };
        Ok(message)
    }

    /// The message as a value, under the wire's names: what
    /// [`Message::from_wire`] reads back.
    pub fn to_wire(&self) -> Wire {
        let mut map = BTreeMap::new();
        let mut put = |name: &str, value: Wire| {
            map.insert(name.to_owned(), value);
        };
        let kind = match self {
            Message::Table(table) => {
                put("schema", Wire::from_json(&table.schema.to_json()));
                if let Some(incarnation) = &table.incarnation {
                    put("incarnation", Wire::Text(incarnation.clone()));
                }
                if let Some(name) = &table.table {
                    put("table", Wire::Text(name.clone()));
                }
                if let Some(sequence) = table.sequence {
                    put("sequence", Wire::Int(sequence.into()));
                }
                "table"
            }
            Message::Rows(rows) => {
                let page = rows.rows.iter().cloned().map(Wire::Map).collect();
                put("rows", Wire::Array(page));
                put("cursor", rows.cursor.clone().map_or(Wire::Null, Wire::Text));
                put("partial", Wire::Bool(rows.partial));
                if let Some(sequence) = rows.sequence {
                    put("sequence", Wire::Int(sequence.into()));
                }
                "rows"
            }
            Message::Synced { sequence } | Message::Progress { sequence } => {
                if let Some(sequence) = sequence {
                    put("sequence", Wire::Int((*sequence).into()));
                }
                match self {
                    Message::Synced { .. } => "synced",
                    _ => "progress",
                }
            }
            Message::Upsert { key, row } => {
                put("key", Wire::Array(key.clone()));
                put("row", Wire::Map(row.clone()));
                "upsert"
            }
            Message::Delete { key } => {
                put("key", Wire::Array(key.clone()));
                "delete"
            }
            Message::Resync => "resync",
            Message::Count {
                count,
                total,
                estimate,
            } => {
                put("count", Wire::Int((*count).into()));
                if let Some(total) = total {
                    put("total", Wire::Int((*total).into()));
                }
                put("estimate", Wire::Bool(*estimate));
                "count"
            }
            Message::Values {
                field,
                values,
                more,
                sequence,
            } => {
                put("field", Wire::Text(field.clone()));
                let values = (values.iter())
                    .map(|(value, count)| {
                        let mut entry = BTreeMap::new();
                        entry.insert("value".to_owned(), value.clone());
                        entry.insert("count".to_owned(), Wire::Int((*count).into()));
                        Wire::Map(entry)
                    })
                    .collect();
                put("values", Wire::Array(values));
                put("more", Wire::Bool(*more));
                if let Some(sequence) = sequence {
                    put("sequence", Wire::Int((*sequence).into()));
                }
                "values"
            }
            Message::Other { kind, body } => {
                match body {
                    Wire::Map(body) => map.extend(body.clone()),
                    other => put("body", other.clone()),
                }
                kind
            }
        };
        map.insert("type".into(), Wire::Text(kind.to_owned()));
        Wire::Map(map)
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

/// Encodes `message` as one line of an `application/x-ndjson` rows stream,
/// its line break included. JSON has no byte strings, NaN or integers past
/// `u64` and `i64`: such values are written as [`Wire::to_json`] writes
/// them.
pub fn encode_json_line(message: &Message) -> Vec<u8> {
    let mut line = serde_json::to_vec(&message.to_wire().to_json()).expect("JSON");
    line.push(b'\n');
    line
}

/// Encodes `message` as one item of an `application/cbor-seq` rows stream,
/// in deterministic CBOR.
pub fn encode_cbor_item(message: &Message) -> Vec<u8> {
    crate::wire::deterministic_cbor(&message.to_wire())
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

/// The longest item a [`Decoder`] holds by default: 16 MiB.
pub const DEFAULT_MAX_ITEM: usize = 16 << 20;

/// How deep a CBOR item may nest before it is an error. ciborium refuses
/// far shallower ones, so this only bounds the walk's own stack.
const MAX_DEPTH: usize = 1024;

/// An incremental decoder for a rows stream: the host feeds it bytes as
/// they come, in pieces of any size, and takes each whole message out.
/// Each byte is looked at a bounded number of times however the stream is
/// cut: newline-delimited JSON is scanned once for line breaks, and a CBOR
/// item's heads are walked once, keeping its place across feeds, before
/// the whole item is decoded once.
///
/// A line or CBOR item longer than the maximum ([`DEFAULT_MAX_ITEM`]
/// unless [`Decoder::with_max_item`] sets it) is an error, and the rest of
/// the stream is dropped until [`Decoder::finish`]: a CBOR head that
/// declares a length past it fails at once.
#[derive(Clone, Debug)]
pub struct Decoder {
    encoding: Encoding,
    max_item: usize,
    buffer: Vec<u8>,
    /// Where the bytes not yet taken start.
    start: usize,
    /// Where the search for the next line break, or the walk of the CBOR
    /// item at `start`, goes on from: for CBOR the next head, which lies
    /// past the bytes fed while a string is cut short.
    scanned: usize,
    /// The CBOR containers open at `scanned`, innermost last, with how
    /// many of their items are still to begin; `None` for one that runs
    /// to a break.
    open: Vec<Option<u64>>,
    /// The items still to begin in all, each at least a byte.
    owed: u128,
    /// The last head was a tag, whose item goes on at `scanned`.
    tagged: bool,
    /// The CBOR item's heads are all walked: it ends at `scanned`.
    walked: bool,
    /// An item went past the maximum: the rest of the stream is dropped.
    dropped: bool,
    read: u64,
}

impl Decoder {
    pub fn new(encoding: Encoding) -> Decoder {
        Decoder {
            encoding,
            max_item: DEFAULT_MAX_ITEM,
            buffer: Vec::new(),
            start: 0,
            scanned: 0,
            open: Vec::new(),
            owed: 0,
            tagged: false,
            walked: false,
            dropped: false,
            read: 0,
        }
    }

    /// The decoder with `bytes` as the longest item it holds.
    pub fn with_max_item(mut self, bytes: usize) -> Decoder {
        self.max_item = bytes;
        self
    }

    /// The longest item the decoder holds.
    pub fn max_item(&self) -> usize {
        self.max_item
    }

    /// How many bytes the decoder has read so far, counting each time it
    /// reads one: in scanning for line breaks, in walking CBOR heads and in
    /// decoding items. A measure of the work done, which stays within a
    /// small multiple of the bytes fed.
    pub fn bytes_read(&self) -> u64 {
        self.read
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        if self.dropped {
            return;
        }
        if self.start > 0 && self.start * 2 >= self.buffer.len() {
            self.buffer.drain(..self.start);
            self.scanned = self.scanned.saturating_sub(self.start);
            self.start = 0;
        }
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole message fed, `None` until one is. A message that
    /// fails to decode is dropped from the buffer and its error returned;
    /// after an item past the maximum, every message is `None`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Result<Message, DecodeError>> {
        match self.encoding {
            Encoding::Ndjson => loop {
                let from = self.scanned.max(self.start);
                let found = self.buffer[from..].iter().position(|&b| b == b'\n');
                self.read += found.map_or(self.buffer.len() - from, |at| at + 1) as u64;
                let Some(at) = found else {
                    self.scanned = self.buffer.len();
                    if self.buffer.len() - self.start > self.max_item {
                        return Some(Err(self.drop_rest()));
                    }
                    return None;
                };
                let line = self.start..from + at;
                self.start = from + at + 1;
                self.scanned = self.start;
                if line.len() > self.max_item {
                    return Some(Err(self.drop_rest()));
                }
                let line = &self.buffer[line];
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                self.read += line.len() as u64;
                return Some(decode_json_line(line));
            },
            Encoding::CborSeq => {
                if self.start == self.buffer.len() {
                    return None;
                }
                match self.walk() {
                    Ok(None) => None,
                    Ok(Some(end)) => {
                        let item = self.start..end;
                        self.restart(end);
                        self.read += item.len() as u64;
                        Some(decode_cbor_item(&self.buffer[item]))
                    }
                    Err(e) => {
                        self.restart(self.buffer.len());
                        Some(Err(e))
                    }
                }
            }
        }
    }

    /// Walks the CBOR item at `start` from where the walk stopped: where
    /// it ends once it is whole, `None` while it is cut short.
    fn walk(&mut self) -> Result<Option<usize>, DecodeError> {
        while !self.walked {
            let at = self.scanned;
            let Some(&initial) = self.buffer.get(at) else {
                return Ok(None);
            };
            let (major, info) = (initial >> 5, initial & 0x1f);
            let width = match info {
                0..=23 | 31 => 0,
                24 => 1,
                25 => 2,
                26 => 4,
                27 => 8,
                _ => return Err(DecodeError::message("CBOR: a reserved head")),
            };
            let Some(argument) = self.buffer.get(at + 1..at + 1 + width) else {
                return Ok(None);
            };
            let argument = match info {
                0..=23 => u64::from(info),
                _ => (argument.iter()).fold(0, |n, &b| n << 8 | u64::from(b)),
            };
            self.read += 1 + width as u64;
            self.scanned = at + 1 + width;
            let indefinite = info == 31;
            if !self.tagged
                && !(major == 7 && indefinite)
                && let Some(Some(left)) = self.open.last_mut()
            {
                *left -= 1;
                self.owed -= 1;
            }
            self.tagged = major == 6;
            // Whether the head ends an item, the bytes of its string, and
            // the items it holds.
            let (whole, payload, items) = match (major, indefinite) {
                (0 | 1 | 6, true) => {
                    return Err(DecodeError::message("CBOR: an indefinite integer or tag"));
                }
                (7, true) => match self.open.pop() {
                    Some(None) => (true, 0, 0),
                    _ => return Err(DecodeError::message("CBOR: a break outside its item")),
                },
                (0 | 1 | 7, false) => (true, 0, 0),
                (2 | 3, false) => (true, u128::from(argument), 0),
                // The chunks of an indefinite string are walked as items;
                // ciborium refuses one of another type when it decodes.
                (2..=5, true) => {
                    self.open.push(None);
                    (false, 0, 0)
                }
                (4 | 5, false) => {
                    let items = u128::from(argument) * u128::from(major - 3);
                    (items == 0, 0, items)
                }
                // A tag: the item after it stands in its place.
                _ => (false, 0, 0),
            };
            let least = (self.scanned - self.start) as u128 + payload + self.owed + items;
            if least > self.max_item as u128 {
                return Err(self.drop_rest());
            }
            if items > 0 {
                self.open.push(Some(items as u64));
                self.owed += items;
            }
            if self.open.len() > MAX_DEPTH {
                return Err(DecodeError::message("CBOR: an item nested too deep"));
            }
            self.scanned += payload as usize;
            if whole {
                loop {
                    match self.open.last() {
                        None => {
                            self.walked = true;
                            break;
                        }
                        Some(Some(0)) => {
                            self.open.pop();
                        }
                        Some(_) => break,
                    }
                }
            }
        }
        Ok((self.scanned <= self.buffer.len()).then_some(self.scanned))
    }

    /// Starts on the next item at `at`.
    fn restart(&mut self, at: usize) {
        (self.start, self.scanned, self.owed) = (at, at, 0);
        (self.tagged, self.walked) = (false, false);
        self.open.clear();
    }

    /// An item went past the maximum: its bytes and the rest of the
    /// stream are dropped.
    fn drop_rest(&mut self) -> DecodeError {
        self.dropped = true;
        self.buffer = Vec::new();
        self.restart(0);
        DecodeError {
            message: format!("an item is longer than {} bytes", self.max_item),
        }
    }

    /// The stream ended: the last message, if one was left without its
    /// line break, or an error if the bytes left are not a whole message.
    /// The decoder is then ready for a new stream, past the maximum or not.
    pub fn finish(&mut self) -> Option<Result<Message, DecodeError>> {
        let rest = std::mem::take(&mut self.buffer).split_off(self.start);
        self.restart(0);
        self.dropped = false;
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
