//! The Described Table source: a Table Source over the rows a described
//! table's stream carries, each parsed once against its field's type.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::ops::Range;
use std::rc::Rc;

use binnacle_core::{
    Aggregate, Axis, ChangeNotice, Compared, Distinct, DistinctQuery, FindDirection, FindMatch,
    FindQuery, Grid, Key, Located, Position, SortColumn, SortSpec, Support, TextSupport, Totals,
    ViewSpec,
};
use binnacle_format::{
    FixedOffset, FormatContext, FormatOptions, Role, Timestamp, Value, ValueType, Zone,
    format_stable,
};
use binnacle_group::{GroupedMemory, Summary, Window};
use binnacle_memory::{FindStep, MemorySource, Moved, PumpSource, TypedKey, Viewed};

use crate::keys::StableKeys;
use crate::message::{Category, Field, FieldType, Message, Row, RowsMessage, Schema};
use crate::scope::{HostRequest, ROW_CAP, Scope, WatchRequest};
use crate::wire::{Wire, deterministic_cbor};

/// One row's Typed Values, in column order.
pub type Values = Vec<Value>;

/// Where a column's values refer to, from the schema's `foreignKeys`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    /// The table, `None` for this one.
    pub resource: Option<String>,
    pub field: String,
}

/// One column of a Described Table: its field, as the schema describes it.
#[derive(Clone, Debug)]
pub struct Column {
    key: Key,
    field: Field,
    expected: ValueType,
    role: Role,
    reference: Option<Reference>,
    mismatches: u64,
}

impl Column {
    pub fn key(&self) -> Key {
        self.key
    }

    pub fn field(&self) -> &Field {
        &self.field
    }

    pub fn name(&self) -> &str {
        &self.field.name
    }

    pub fn title(&self) -> Option<&str> {
        self.field.title.as_deref()
    }

    /// What its header shows: the title, else the name.
    pub fn header(&self) -> &str {
        self.field.header()
    }

    pub fn field_type(&self) -> &FieldType {
        &self.field.field_type
    }

    /// The kind of Typed Value its values are read as.
    pub fn value_type(&self) -> ValueType {
        self.expected
    }

    pub fn unit(&self) -> Option<&str> {
        self.field.unit.as_deref()
    }

    pub fn nullable(&self) -> bool {
        self.field.nullable()
    }

    pub fn roles(&self) -> &[String] {
        &self.field.roles
    }

    /// The role the formatter draws its values with.
    pub fn role(&self) -> Role {
        self.role
    }

    pub fn categories(&self) -> Option<&[Category]> {
        self.field.categories.as_deref()
    }

    pub fn reference(&self) -> Option<&Reference> {
        self.reference.as_ref()
    }

    /// Whether its values are JSON trees: an object or array field.
    pub fn nested(&self) -> bool {
        self.expected == ValueType::Nested
    }

    /// How many of its values did not match its type and are Invalid.
    pub fn mismatches(&self) -> u64 {
        self.mismatches
    }
}

/// What a source did with a message.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Received {
    /// Applied: the Grid needs these Change Notices, in order.
    Applied(Vec<(Axis, ChangeNotice)>),
    /// Decoded but not yet handled by this source: live changes and
    /// resyncs, and the stream's end. The source is as it was.
    Unhandled(Unhandled),
}

/// What [`Received::Unhandled`] holds.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Unhandled {
    /// An `upsert`, `delete`, `resync` or unknown message.
    Message(Message),
    /// The stream ended.
    Ended,
}

/// How far the rows have come.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Status {
    /// The `table` message has come.
    pub described: bool,
    /// A `synced` has come: the rows are the table as of then.
    pub synced: bool,
    /// The first `rows` message had a cursor: the table has more rows than
    /// [`ROW_CAP`], and only those that came are shown ("10,000 of more").
    pub partial: bool,
    /// The table's row count, if a `count` message gave it, and whether it
    /// is an estimate.
    pub count: Option<(u64, bool)>,
    /// Which life of the table the `table` message named: `durable` for
    /// one that outlives the daemon.
    pub incarnation: Option<String>,
    /// The sequence the last `synced` gave.
    pub sequence: Option<u64>,
}

/// The Described Table source. Built from a [`Scope`], it asks its host to
/// open a watch ([`DescribedSource::poll_request`]), takes the messages
/// the host feeds back ([`DescribedSource::receive`]) and answers the Grid
/// as an In-Memory Table Source over the rows (sort, filter, find,
/// Distinct Values, hiding and grouping), through [`PumpSource`].
///
/// Each row is parsed once, as it comes, into Typed Values in column order
/// (see [`DescribedSource::cell`]). A row's Key is interned from the
/// deterministic CBOR of its primary-key tuple and a column's from its
/// field's name; the interner lives as long as the source, so a row or
/// column that leaves and comes back keeps its Key, and as a Key is a hash
/// of its identity, every source gives the same row the same Key, barring a
/// hash collision. A table with no primary key keys each row by where it
/// came in the stream instead, so its Keys hold only within one source.
///
/// Sorting is the Sort Order with an invalid tier between nulls and typed
/// values. Rows that tie on every term of a sort, or within a group, break
/// the tie on the primary key, ascending, as a final term the applied View
/// Spec does not show; the Key is the last resort. Filters, find and
/// Distinct Values read each value's Stable Text, never its drawn text; a
/// null is a blank and an empty text is not.
pub struct DescribedSource {
    table: String,
    scope: Scope,
    requests: VecDeque<HostRequest>,
    zone: Rc<dyn Zone>,
    schema: Option<Schema>,
    columns: Vec<Column>,
    column_at: HashMap<Key, usize>,
    /// The primary key's field names.
    primary: Vec<String>,
    rows: Vec<Values>,
    row_keys: Vec<Key>,
    row_at: HashMap<Key, usize>,
    row_ids: StableKeys,
    column_ids: StableKeys,
    grouped: GroupedMemory<Values>,
    /// The View Spec last asked for, applied again when the rows change.
    spec: ViewSpec,
    status: Status,
    /// How many `rows` messages came since the `table`.
    pages: u64,
    mismatches: u64,
}

impl DescribedSource {
    /// A source for the rows of `table` within `scope`, its dates' Stable
    /// Text in UTC until [`DescribedSource::zone`] sets another.
    pub fn new(table: impl Into<String>, scope: Scope) -> DescribedSource {
        let table = table.into();
        let request = WatchRequest {
            table: table.clone(),
            scope: scope.clone(),
            limit: ROW_CAP,
        };
        DescribedSource {
            table,
            scope,
            requests: VecDeque::from([HostRequest::OpenWatch(request)]),
            zone: Rc::new(FixedOffset(0)),
            schema: None,
            columns: Vec::new(),
            column_at: HashMap::new(),
            primary: Vec::new(),
            rows: Vec::new(),
            row_keys: Vec::new(),
            row_at: HashMap::new(),
            row_ids: StableKeys::default(),
            column_ids: StableKeys::default(),
            grouped: GroupedMemory::new(MemorySource::new(0, 0)),
            spec: ViewSpec::default(),
            status: Status::default(),
            pages: 0,
            mismatches: 0,
        }
    }

    /// This source with its dates' Stable Text (what filters and find
    /// match) in `zone`, as the host draws them.
    pub fn zone(mut self, zone: impl Zone + 'static) -> DescribedSource {
        self.zone = Rc::new(zone);
        self.rebuild();
        self
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// The next thing the source asks of its host: first, to open the
    /// watch.
    pub fn poll_request(&mut self) -> Option<HostRequest> {
        self.requests.pop_front()
    }

    /// Takes one message from the stream.
    pub fn receive(&mut self, message: Message) -> Received {
        match message {
            Message::Table(table) => {
                let notices = self.describe(table.schema);
                self.status.incarnation = table.incarnation;
                Received::Applied(notices)
            }
            Message::Rows(rows) => Received::Applied(self.take_rows(rows)),
            Message::Synced { sequence } => {
                self.status.synced = true;
                self.status.sequence = sequence;
                Received::Applied(Vec::new())
            }
            Message::Progress { .. } => Received::Applied(Vec::new()),
            Message::Count { count, estimate } => {
                self.status.count = Some((count, estimate));
                Received::Applied(Vec::new())
            }
            other => Received::Unhandled(Unhandled::Message(other)),
        }
    }

    /// The stream ended.
    pub fn ended(&mut self) -> Received {
        Received::Unhandled(Unhandled::Ended)
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// The columns, in the schema's order.
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    pub fn column(&self, col: Key) -> Option<&Column> {
        self.column_at.get(&col).map(|&c| &self.columns[c])
    }

    /// Any property of column `col`'s field, as the schema sent it.
    pub fn property(&self, col: Key, name: &str) -> Option<&serde_json::Value> {
        self.column(col)?.field.property(name)
    }

    /// The Typed Value where row `row` meets column `col`.
    pub fn cell(&self, row: Key, col: Key) -> Option<&Value> {
        let r = *self.row_at.get(&row)?;
        let c = *self.column_at.get(&col)?;
        self.rows[r].get(c)
    }

    /// How many rows came, filtered or not.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The base rows' Keys, in the order they came.
    pub fn row_keys(&self) -> &[Key] {
        &self.row_keys
    }

    /// How many values did not match their field's type, in every column.
    pub fn mismatches(&self) -> u64 {
        self.mismatches
    }

    /// The Key a primary-key tuple names, if a row with it came.
    pub fn row_key(&self, tuple: &[Wire]) -> Option<Key> {
        self.row_ids.key(&identity(tuple))
    }

    /// The identity row Key `row` was interned from: the deterministic CBOR
    /// of its primary-key tuple.
    pub fn row_identity(&self, row: Key) -> Option<&[u8]> {
        self.row_ids.identity(row)
    }

    /// The Key of the column for field `name`, if the schema has it.
    pub fn column_key(&self, name: &str) -> Option<Key> {
        let key = self.column_ids.key(name.as_bytes())?;
        self.column_at.contains_key(&key).then_some(key)
    }

    /// How many items `axis` shows now.
    pub fn count(&self, axis: Axis) -> u64 {
        self.grouped.count(axis)
    }

    /// The Keys at `range` on `axis`, and their outlines if it is grouped.
    pub fn window(&mut self, axis: Axis, range: Range<Position>) -> Window {
        self.grouped.window(&self.rows, axis, range)
    }

    /// The Summary Item with `key`, while the rows are grouped.
    pub fn summary(&self, key: Key) -> Option<Summary<'_, binnacle_group::Figure>> {
        self.grouped.summary(key)
    }

    /// Each axis's count before any filter.
    pub fn totals(&self) -> Totals {
        self.grouped.totals()
    }

    /// The aggregates a value over column `col` may take while grouped.
    pub fn aggregates(&self, col: Key) -> Vec<Aggregate> {
        self.grouped.aggregates(Axis::Rows, col)
    }

    /// Declares to `grid` what each column supports (sorting, filters,
    /// grouping and aggregates), what each axis hides, and find.
    pub fn declare(&mut self, grid: &mut Grid) {
        for c in 0..self.columns.len() {
            let key = self.columns[c].key;
            grid.set_support(Axis::Rows, key, self.support(Axis::Rows, key));
            grid.set_aggregates(Axis::Rows, key, self.aggregates(key));
        }
        for axis in Axis::ALL {
            grid.set_hide_support(axis, self.hide_support(axis));
            grid.set_quick_filter_support(axis, self.quick_filter(axis));
        }
        grid.set_find_support(self.find_support());
    }

    /// A `table` message: the columns are its fields now, and the rows
    /// start again.
    fn describe(&mut self, schema: Schema) -> Vec<(Axis, ChangeNotice)> {
        let reference = |name: &str| {
            schema.foreign_keys.iter().find_map(|fk| {
                let at = fk.fields.iter().position(|f| f == name)?;
                Some(Reference {
                    resource: fk.resource.clone(),
                    field: fk.reference_fields.get(at)?.clone(),
                })
            })
        };
        self.columns = (schema.fields.iter())
            .map(|field| Column {
                key: self.column_ids.intern(field.name.as_bytes()),
                field: field.clone(),
                expected: expected(field),
                role: (field.roles.iter())
                    .map(|r| Role::from_name(r))
                    .find(|r| *r != Role::None)
                    .unwrap_or_default(),
                reference: reference(&field.name),
                mismatches: 0,
            })
            .collect();
        self.column_at = (self.columns.iter().enumerate())
            .map(|(i, c)| (c.key, i))
            .collect();
        self.primary = schema.primary_key.clone();
        self.schema = Some(schema);
        self.rows.clear();
        self.row_keys.clear();
        self.row_at.clear();
        self.mismatches = 0;
        self.pages = 0;
        self.status = Status {
            described: true,
            ..Status::default()
        };
        self.rebuild();
        vec![
            (Axis::Cols, ChangeNotice::Reset(self.count(Axis::Cols))),
            (Axis::Rows, ChangeNotice::Reset(self.count(Axis::Rows))),
        ]
    }

    /// A `rows` message: its rows join those that came, in place in the
    /// sort and filter in force, and a row whose primary key came before
    /// takes that row's place. A cursor on the first page means the table
    /// is over the cap.
    fn take_rows(&mut self, message: RowsMessage) -> Vec<(Axis, ChangeNotice)> {
        if self.pages == 0 && message.cursor.is_some() {
            self.status.partial = true;
        }
        self.pages += 1;
        let mut added = Vec::new();
        let mut replaced = false;
        for row in message.rows {
            let (key, values) = self.parse(&row);
            match self.row_at.get(&key) {
                Some(&at) => {
                    let old = std::mem::replace(&mut self.rows[at], values);
                    self.forget_mismatches(&old);
                    replaced = true;
                }
                None => {
                    self.row_at.insert(key, self.rows.len());
                    self.row_keys.push(key);
                    self.rows.push(values);
                    added.push(key);
                }
            }
        }
        let base = self.grouped.source_mut();
        let notices = match replaced {
            true => base.replaced(&self.rows, Some(self.row_keys.clone())),
            false => base.appended(&self.rows, &added),
        };
        let grouped = Axis::ALL.map(|axis| self.grouped.grouped(axis));
        if !grouped.contains(&true) {
            return notices;
        }
        self.grouped.refresh(&self.rows);
        (Axis::ALL.into_iter())
            .filter(|&axis| grouped[axis.index()] || notices.iter().any(|(a, _)| *a == axis))
            .map(|axis| (axis, ChangeNotice::Reset(self.count(axis))))
            .collect()
    }

    /// A row's values left: its invalid ones no longer count.
    fn forget_mismatches(&mut self, values: &Values) {
        for (column, value) in self.columns.iter_mut().zip(values) {
            if matches!(value, Value::Invalid { .. }) {
                column.mismatches -= 1;
                self.mismatches -= 1;
            }
        }
    }

    /// `row`'s Key and its Typed Values, counting those that don't match
    /// their type.
    fn parse(&mut self, row: &Row) -> (Key, Values) {
        let null = Wire::Null;
        let tuple: Vec<Wire> = match self.primary.is_empty() {
            true => vec![Wire::Int(self.rows.len() as i128)],
            false => (self.primary.iter())
                .map(|name| row.get(name).unwrap_or(&null).clone())
                .collect(),
        };
        let key = self.row_ids.intern(&identity(&tuple));
        let values = (self.columns.iter_mut())
            .map(|column| {
                let value = typed(&column.field, row.get(&column.field.name).unwrap_or(&null));
                if matches!(value, Value::Invalid { .. }) {
                    column.mismatches += 1;
                    self.mismatches += 1;
                }
                value
            })
            .collect();
        (key, values)
    }

    /// Builds the In-Memory Table Source over the rows anew, with the View
    /// Spec last asked for.
    fn rebuild(&mut self) {
        let column_keys: Vec<Key> = self.columns.iter().map(|c| c.key).collect();
        let mut source = MemorySource::<Values>::new(self.rows.len(), self.columns.len())
            .with_keys(Axis::Rows, self.row_keys.clone())
            .with_keys(Axis::Cols, column_keys);
        for (c, column) in self.columns.iter().enumerate() {
            if !column.nested() {
                let compared = match column.expected {
                    ValueType::Int | ValueType::UInt | ValueType::Number => Some(Compared::Numbers),
                    ValueType::DateTime => Some(Compared::Dates),
                    _ => None,
                };
                source = source
                    .typed_column(column.key, compared, move |row: &Values| sort_key(&row[c]));
            }
            let zone = Rc::clone(&self.zone);
            let (unit, role) = (column.field.unit.clone(), column.role);
            let options = FormatOptions::default();
            source = source.text_column(column.key, move |row: &Values| {
                stable_text(&row[c], unit.as_deref(), role, &options, &*zone)
            });
        }
        self.grouped = GroupedMemory::new(source);
        if self.spec != ViewSpec::default() {
            let (spec, _) = self.tie_broken(&self.spec);
            self.grouped.view(&self.rows, &spec);
        }
    }

    /// `spec` with the primary key's columns, ascending, ending the rows'
    /// sort when it sorts or groups them, and the Keys of the terms added.
    fn tie_broken(&self, spec: &ViewSpec) -> (ViewSpec, Vec<Key>) {
        let rows = spec.axis(Axis::Rows);
        if rows.sort.is_empty() && spec.grouping.is_none() {
            return (spec.clone(), Vec::new());
        }
        let added: Vec<Key> = (self.primary.iter())
            .filter_map(|name| self.column_key(name))
            .filter(|&k| !self.column(k).is_some_and(Column::nested))
            .filter(|&k| rows.sort.find(k).is_none())
            .collect();
        let mut sort = rows.sort.columns().to_vec();
        sort.extend(added.iter().map(|&k| SortColumn::ascending(k)));
        let spec = spec.clone().with_sort(Axis::Rows, SortSpec::new(sort));
        (spec, added)
    }

    fn over(&mut self) -> impl PumpSource + '_ {
        self.grouped.over(&self.rows)
    }
}

/// A primary-key tuple's identity: its deterministic CBOR.
fn identity(tuple: &[Wire]) -> Vec<u8> {
    deterministic_cbor(&Wire::Array(tuple.to_vec()))
}

/// The kind of Typed Value `field`'s values are read as.
fn expected(field: &Field) -> ValueType {
    if field.categories.is_some() {
        return ValueType::Category;
    }
    match field.field_type {
        FieldType::String => ValueType::Text,
        FieldType::Integer => ValueType::Int,
        FieldType::Number => ValueType::Number,
        FieldType::Boolean => ValueType::Bool,
        FieldType::Datetime => ValueType::DateTime,
        FieldType::Object | FieldType::Array => ValueType::Nested,
        _ => ValueType::Text,
    }
}

/// `wire` read as `field` declares: a null (or a missing value) is `Null`,
/// a value that doesn't match the type is `Invalid`. A field of a type this
/// crate does not read takes text as text, numbers as numbers and trees as
/// Nested.
pub fn typed(field: &Field, wire: &Wire) -> Value {
    let invalid = |expected| Value::Invalid {
        raw: wire.raw_text(),
        expected,
    };
    if wire.is_null() {
        return Value::Null;
    }
    if let Some(categories) = &field.categories {
        let text = match wire {
            Wire::Text(s) => s.clone(),
            Wire::Int(_) | Wire::Float(_) | Wire::Bool(_) => wire.to_json().to_string(),
            _ => return invalid(ValueType::Category),
        };
        return match categories.iter().find(|c| c.value == text) {
            Some(c) => Value::Category {
                value: c.value.clone(),
                label: c.label.clone(),
            },
            None => invalid(ValueType::Category),
        };
    }
    match (&field.field_type, wire) {
        (FieldType::String, Wire::Text(s)) => Value::Text(s.clone()),
        (FieldType::String, _) => invalid(ValueType::Text),
        (FieldType::Integer, Wire::Int(i)) => match (i64::try_from(*i), u64::try_from(*i)) {
            (Ok(i), _) => Value::Int(i),
            (_, Ok(u)) => Value::UInt(u),
            _ => invalid(ValueType::Int),
        },
        (FieldType::Integer, _) => invalid(ValueType::Int),
        (FieldType::Number, Wire::Float(x)) => Value::Number(*x),
        (FieldType::Number, Wire::Int(i)) => Value::Number(*i as f64),
        (FieldType::Number, _) => invalid(ValueType::Number),
        (FieldType::Boolean, Wire::Bool(b)) => Value::Bool(*b),
        (FieldType::Boolean, _) => invalid(ValueType::Bool),
        (FieldType::Datetime, Wire::Text(s)) => match Timestamp::parse_rfc3339(s) {
            Some(at) => Value::DateTime(at),
            None => invalid(ValueType::DateTime),
        },
        (FieldType::Datetime, _) => invalid(ValueType::DateTime),
        (FieldType::Object, Wire::Map(_)) | (FieldType::Array, Wire::Array(_)) => {
            Value::Nested(wire.to_json())
        }
        (FieldType::Object | FieldType::Array, _) => invalid(ValueType::Nested),
        (_, Wire::Text(s)) => Value::Text(s.clone()),
        (_, Wire::Bool(b)) => Value::Bool(*b),
        (_, Wire::Int(i)) => match (i64::try_from(*i), u64::try_from(*i)) {
            (Ok(i), _) => Value::Int(i),
            (_, Ok(u)) => Value::UInt(u),
            _ => Value::Number(*i as f64),
        },
        (_, Wire::Float(x)) => Value::Number(*x),
        (_, Wire::Bytes(b)) => Value::Bytes(b.clone()),
        (_, other) => Value::Nested(other.to_json()),
    }
}

/// A Typed Value's sort key: `None` for a null.
fn sort_key(value: &Value) -> Option<TypedKey<'_>> {
    Some(match value {
        Value::Null => return None,
        Value::Invalid { raw, .. } => TypedKey::Invalid(Cow::Borrowed(raw)),
        Value::Bool(b) => TypedKey::I64(i64::from(*b)),
        Value::Int(i) => TypedKey::I64(*i),
        Value::UInt(u) => TypedKey::U64(*u),
        Value::Number(x) => TypedKey::F64(*x),
        Value::DateTime(at) => TypedKey::Instant(at.unix_nanos()),
        Value::Text(s) => TypedKey::Bytes(Cow::Borrowed(s.as_bytes())),
        Value::Category { value, .. } => TypedKey::Bytes(Cow::Borrowed(value.as_bytes())),
        Value::Bytes(b) => TypedKey::Bytes(Cow::Borrowed(b)),
        Value::Nested(json) => TypedKey::Bytes(Cow::Owned(json.to_string().into_bytes())),
    })
}

/// A Typed Value's Stable Text: `None` for a null, a blank.
fn stable_text(
    value: &Value,
    unit: Option<&str>,
    role: Role,
    options: &FormatOptions,
    zone: &dyn Zone,
) -> Option<Cow<'static, str>> {
    if matches!(value, Value::Null) {
        return None;
    }
    let ctx = FormatContext::new(Timestamp::UNIX_EPOCH, zone);
    Some(Cow::Owned(
        format_stable(value, unit, role, options, &ctx).text,
    ))
}

impl PumpSource for DescribedSource {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        self.spec = spec.clone();
        let (tie_broken, added) = self.tie_broken(spec);
        let mut viewed = self.over().view(&tie_broken);
        let rows = viewed.applied.axis_mut(Axis::Rows);
        let shown: Vec<SortColumn> = (rows.sort.columns().iter())
            .filter(|c| !added.contains(&c.key))
            .copied()
            .collect();
        rows.sort = SortSpec::new(shown);
        viewed
    }

    fn support(&mut self, axis: Axis, key: Key) -> Support {
        self.over().support(axis, key)
    }

    fn quick_filter(&mut self, axis: Axis) -> Option<TextSupport> {
        self.over().quick_filter(axis)
    }

    fn hide_support(&mut self, axis: Axis) -> bool {
        self.over().hide_support(axis)
    }

    fn distinct(&mut self, axis: Axis, key: Key, query: &DistinctQuery) -> Option<Distinct> {
        self.over().distinct(axis, key, query)
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.over().locate(axis, key)
    }

    fn find_support(&mut self) -> Option<TextSupport> {
        self.over().find_support()
    }

    fn find_next(
        &mut self,
        query: &FindQuery,
        from: Option<FindMatch>,
        direction: FindDirection,
    ) -> Option<FindStep> {
        self.over().find_next(query, from, direction)
    }

    fn ancestors(&mut self, axis: Axis, key: Key) -> Vec<Key> {
        self.over().ancestors(axis, key)
    }

    fn fits(&mut self, axis: Axis, key: Key) -> bool {
        self.over().fits(axis, key)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        self.over().movable(axis)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Result<Moved, String> {
        self.over().move_items(axis, keys, to)
    }
}
