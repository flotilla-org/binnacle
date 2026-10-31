//! The Described Table source: a Table Source over the rows a described
//! table's stream carries, each parsed once against its field's type.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
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
    /// Decoded but of a type this source does not know. The source is as
    /// it was.
    Unhandled(Unhandled),
}

/// What [`Received::Unhandled`] holds.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Unhandled {
    /// A message of a type this crate does not know ([`Message::Other`]).
    Message(Message),
}

/// How far the rows have come.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Status {
    /// The `table` message has come.
    pub described: bool,
    /// A `synced` has come and no resync began since: the rows are the
    /// table as of then.
    pub synced: bool,
    /// The first `rows` message after the last `table` had a cursor: the
    /// table has more rows than [`ROW_CAP`], and only those that came are
    /// shown ("10,000 of more"). Only a `table` starts it afresh, so a
    /// resync whose server sends none keeps it.
    pub partial: bool,
    /// The table's row count, if a `count` message gave it, and whether it
    /// is an estimate.
    pub count: Option<(u64, bool)>,
    /// Which life of the table the last `table` message named: `durable`
    /// for one that outlives the daemon.
    pub incarnation: Option<String>,
    /// The sequence the last `synced` or `progress` gave.
    pub sequence: Option<u64>,
}

/// More changed rows than this in one `rows` message are applied at once,
/// as the rows replaced, rather than value by value: each changed value in
/// a sorted column sorts the rows again, so a resync of a large sorted
/// table would be quadratic. Below it, moves and unmatched rows survive.
const BULK: usize = 256;

/// The Described Table source. Built from a [`Scope`], it asks its host to
/// open a watch ([`DescribedSource::poll_request`]), takes the messages
/// the host feeds back ([`DescribedSource::receive`]) and answers the Grid
/// as an In-Memory Table Source over the rows (sort, filter, find,
/// Distinct Values, hiding and grouping), through [`PumpSource`].
///
/// Each row is parsed once, as it comes, into Typed Values in column order
/// (see [`DescribedSource::cell`]). A row's Key is interned from the
/// deterministic CBOR of its primary-key tuple and a column's from its
/// field's name, so a row or column that leaves and comes back keeps its
/// Key, and as a Key is a hash of its identity, every source gives the same
/// row the same Key, barring a hash collision. The interner lets go of a
/// row's identity when the row goes, unless a collision gave it another
/// Key. A table with no primary key keys each row by where it came in its
/// stream instead (a resync's rows replace the held ones by place, not by
/// identity), so its Keys hold only within one source. Such a table has no
/// key tuples for live changes: an `upsert` or `delete` naming `[n]` is
/// taken to name the row at place `n` of the stream.
///
/// Live changes apply in place: an `upsert` changes or adds one row, a
/// `delete` removes one, and the sort, filters, moves and the Grid's state
/// all stay. A `resync`, or the stream's end ([`DescribedSource::ended`]),
/// starts a resync: the `table`, `rows` and `synced` that follow replace
/// the rows in place, and `synced` removes those that did not come again.
/// A `table` message once the columns are known is diffed by field name
/// (see [`DescribedSource::poll_view`]).
/// The host maps the row of an `Activated` Effect back to its primary key
/// with [`DescribedSource::row_identity`].
///
/// Sorting is the Sort Order with an invalid tier between nulls and typed
/// values. Rows that tie on every term of a sort, or within a group, break
/// the tie on the primary key, ascending, as a final term the applied View
/// Spec does not show; the Key is the last resort. Filters, find and
/// Distinct Values read each value's Stable Text, never its drawn text; a
/// null is a blank and an empty text is not.
/// It is `!Send` (it holds an `Rc<dyn Zone>` and boxed closures): use it
/// from one thread.
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
    /// Each row as it came, to read again when its field's type changes.
    raw: Vec<Row>,
    row_keys: Vec<Key>,
    row_at: HashMap<Key, usize>,
    row_ids: StableKeys,
    column_ids: StableKeys,
    /// The next ordinal a row of a table with no primary key is keyed by.
    ordinal: u64,
    grouped: GroupedMemory<Values>,
    /// The View Spec last asked for, applied again when the rows change.
    spec: ViewSpec,
    /// The View Spec applied since a `table` message changed the columns,
    /// for the host to take.
    viewed: Option<(ViewSpec, Totals)>,
    /// While in a resync, the rows that have not come again since it began.
    unconfirmed: Option<HashSet<Key>>,
    status: Status,
    /// How many `rows` messages came since the `table`.
    pages: u64,
    mismatches: u64,
}

impl DescribedSource {
    /// A source for the rows of `table` within `scope`, its dates' Stable
    /// Text in UTC until [`DescribedSource::zone`] sets another.
    pub fn new(table: impl Into<String>, scope: Scope) -> DescribedSource {
        let mut source = DescribedSource {
            table: table.into(),
            scope,
            requests: VecDeque::new(),
            zone: Rc::new(FixedOffset(0)),
            schema: None,
            columns: Vec::new(),
            column_at: HashMap::new(),
            primary: Vec::new(),
            rows: Vec::new(),
            raw: Vec::new(),
            row_keys: Vec::new(),
            row_at: HashMap::new(),
            row_ids: StableKeys::default(),
            column_ids: StableKeys::default(),
            ordinal: 0,
            grouped: GroupedMemory::new(MemorySource::new(0, 0)),
            spec: ViewSpec::default(),
            viewed: None,
            unconfirmed: None,
            status: Status::default(),
            pages: 0,
            mismatches: 0,
        };
        source.open_watch();
        source
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
    /// watch, and again each time the stream ends.
    pub fn poll_request(&mut self) -> Option<HostRequest> {
        self.requests.pop_front()
    }

    /// Asks to open the watch, unless that is asked already.
    fn open_watch(&mut self) {
        if (self.requests.iter()).any(|r| matches!(r, HostRequest::OpenWatch(_))) {
            return;
        }
        self.requests
            .push_back(HostRequest::OpenWatch(WatchRequest {
                table: self.table.clone(),
                scope: self.scope.clone(),
                limit: ROW_CAP,
            }));
    }

    /// Takes one message from the stream. Messages about rows before the
    /// first `table` are dropped: there are no columns to read them by.
    pub fn receive(&mut self, message: Message) -> Received {
        let notices = match message {
            Message::Table(table) => {
                self.status.incarnation = table.incarnation;
                self.describe(table.schema)
            }
            Message::Rows(rows) => self.take_rows(rows),
            Message::Synced { sequence } => {
                self.status.synced = true;
                self.status.sequence = sequence;
                self.confirm()
            }
            Message::Progress { sequence } => {
                if sequence.is_some() {
                    self.status.sequence = sequence;
                }
                Vec::new()
            }
            Message::Count { count, estimate } => {
                self.status.count = Some((count, estimate));
                Vec::new()
            }
            Message::Upsert { key, row } => self.upsert(&key, row),
            Message::Delete { key } => self.delete(&key),
            Message::Resync => {
                self.resync();
                Vec::new()
            }
            other => return Received::Unhandled(Unhandled::Message(other)),
        };
        Received::Applied(notices)
    }

    /// The stream ended: the source asks its host to open the watch again
    /// (once, however often it is called before the host polls)
    /// ([`DescribedSource::poll_request`]) and starts a resync, so the next
    /// stream's `table`, `rows` and `synced` replace the rows in place. The
    /// rows stay shown meanwhile, so it needs no Change Notices.
    pub fn ended(&mut self) -> Received {
        self.open_watch();
        self.resync();
        Received::Applied(Vec::new())
    }

    /// After a `table` message changed the columns, the View Spec the
    /// source applies now and each axis's unfiltered count, once, for the
    /// host to hand the Grid ([`Grid::view_changed`]) with the Change
    /// Notices. A removed column's sort terms and conditions are left out,
    /// so the Grid shows them not applied; the source keeps the View Spec
    /// last asked for, and they apply again if the field comes back.
    pub fn poll_view(&mut self) -> Option<(ViewSpec, Totals)> {
        self.viewed.take()
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// The columns: the first `table` message's fields in its order, then
    /// those a later one added.
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

    /// How many rows the source holds, filtered or not.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The base rows' Keys, in the order they came.
    pub fn row_keys(&self) -> &[Key] {
        &self.row_keys
    }

    /// How many row identities the Key Interner holds: the rows held, and
    /// any that left after a hash collision gave them another Key.
    pub fn interned_rows(&self) -> usize {
        self.row_ids.len()
    }

    /// How many values did not match their field's type, in every column.
    pub fn mismatches(&self) -> u64 {
        self.mismatches
    }

    /// The Key a primary-key tuple names, if a row with it is held (or left
    /// after a collision).
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

    /// Starts a resync: every row is unconfirmed until it comes again, and
    /// the rows are not synced.
    fn resync(&mut self) {
        // A table with no primary key keys the rows of each stream by where
        // they come in it, so a resync's rows match the held ones by place.
        self.ordinal = 0;
        self.unconfirmed = Some(self.row_keys.iter().copied().collect());
        self.status.synced = false;
    }

    /// A `synced`: a resync in progress ends, removing the rows that did not
    /// come again, in one batch.
    fn confirm(&mut self) -> Vec<(Axis, ChangeNotice)> {
        let Some(unconfirmed) = self.unconfirmed.take() else {
            return Vec::new();
        };
        let gone: Vec<usize> = (0..self.rows.len())
            .filter(|&r| unconfirmed.contains(&self.row_keys[r]))
            .collect();
        let notices = self.remove(&gone);
        self.regroup(notices)
    }

    /// A `table` message. The first sets the columns; a later one is diffed
    /// by field name: an unchanged field keeps its column and values, a
    /// removed field's column goes, an added field's comes at the end, and
    /// a field whose type or categories changed is read again for every
    /// row. A changed primary key keys every row again.
    fn describe(&mut self, schema: Schema) -> Vec<(Axis, ChangeNotice)> {
        self.pages = 0;
        self.status.described = true;
        let Some(old) = self.schema.take() else {
            self.columns = self.columns_for(&schema, schema.fields.iter().collect());
            self.column_at = (self.columns.iter().enumerate())
                .map(|(i, c)| (c.key, i))
                .collect();
            self.primary = schema.primary_key.clone();
            self.schema = Some(schema);
            self.rebuild();
            self.viewed = self.applied();
            return vec![
                (Axis::Cols, ChangeNotice::Reset(self.count(Axis::Cols))),
                (Axis::Rows, ChangeNotice::Reset(self.count(Axis::Rows))),
            ];
        };
        // Even an unchanged table starts the pages again, so the first
        // `rows` after it sets `partial`.
        if old == schema {
            self.schema = Some(old);
            return Vec::new();
        }
        let pivoted = self.grouped.grouped(Axis::Cols);
        let shown_before = self.window(Axis::Cols, 0..self.count(Axis::Cols)).keys;
        let field = |name: &str| schema.fields.iter().find(|f| f.name == name);
        let removed: Vec<Key> = (self.columns.iter())
            .filter(|c| field(c.name()).is_none())
            .map(|c| c.key)
            .collect();
        let mut gone: Vec<Position> = (removed.iter())
            .filter_map(|&k| match self.grouped.locate(&self.rows, Axis::Cols, k) {
                Located::Found(p) => Some(p),
                Located::Gone { .. } => None,
            })
            .collect();
        gone.sort_unstable();
        let mut order: Vec<&Field> = (self.columns.iter())
            .filter_map(|c| field(c.name()))
            .collect();
        let added: Vec<&Field> = (schema.fields.iter())
            .filter(|f| !self.columns.iter().any(|c| c.name() == f.name))
            .collect();
        order.extend(&added);
        let columns = self.columns_for(&schema, order);
        let added: Vec<String> = added.iter().map(|f| f.name.clone()).collect();
        let changed = (columns.iter())
            .filter(|c| !added.iter().any(|n| n == c.name()))
            .any(|c| Some(&c.field) != self.column(c.key).map(|o| &o.field));
        let reread: Vec<bool> = (columns.iter())
            .map(|c| {
                let old = self.column(c.key);
                old.is_none_or(|o| o.field.field_type != c.field.field_type)
                    || old.is_none_or(|o| o.field.categories != c.field.categories)
            })
            .collect();
        let from: Vec<Option<usize>> = (columns.iter())
            .map(|c| self.column_at.get(&c.key).copied())
            .collect();
        for (values, raw) in self.rows.iter_mut().zip(&self.raw) {
            let null = Wire::Null;
            *values = (columns.iter().enumerate())
                .map(|(i, c)| match (reread[i], from[i]) {
                    (false, Some(f)) => std::mem::replace(&mut values[f], Value::Null),
                    _ => typed(&c.field, raw.get(&c.field.name).unwrap_or(&null)),
                })
                .collect();
        }
        self.columns = columns;
        self.column_at = (self.columns.iter().enumerate())
            .map(|(i, c)| (c.key, i))
            .collect();
        self.recount();
        let rekeyed = self.primary != schema.primary_key;
        self.primary = schema.primary_key.clone();
        self.schema = Some(schema);
        if rekeyed {
            self.rekey();
        }
        self.rebuild();
        self.viewed = self.applied();
        let rows = (Axis::Rows, ChangeNotice::Reset(self.count(Axis::Rows)));
        let count = self.count(Axis::Cols);
        if rekeyed || pivoted {
            return vec![(Axis::Cols, ChangeNotice::Reset(count)), rows];
        }
        let shown = self.window(Axis::Cols, 0..count).keys;
        let kept: Vec<Key> = (shown_before.into_iter())
            .filter(|k| !removed.contains(k))
            .collect();
        let (before, after) = shown.split_at(kept.len().min(shown.len()));
        let appended = (after.iter()).all(|k| added.iter().any(|n| self.column_key(n) == Some(*k)));
        if before != kept || !appended {
            return vec![(Axis::Cols, ChangeNotice::Reset(count)), rows];
        }
        let mut notices = Vec::new();
        for &p in gone.iter().rev() {
            match notices.last_mut() {
                Some((_, ChangeNotice::Removed { at, n })) if *at == p + 1 => {
                    *at = p;
                    *n += 1;
                }
                _ => notices.push((Axis::Cols, ChangeNotice::Removed { at: p, n: 1 })),
            }
        }
        if !after.is_empty() {
            let at = before.len() as u64;
            let n = after.len() as u64;
            notices.push((Axis::Cols, ChangeNotice::Inserted { at, n }));
        }
        if changed && count > 0 {
            notices.push((Axis::Cols, ChangeNotice::ContentChanged(0..count)));
        }
        notices.push(rows);
        notices
    }

    /// The columns for `fields` of `schema`, in that order.
    fn columns_for(&mut self, schema: &Schema, fields: Vec<&Field>) -> Vec<Column> {
        let reference = |name: &str| {
            schema.foreign_keys.iter().find_map(|fk| {
                let at = fk.fields.iter().position(|f| f == name)?;
                Some(Reference {
                    resource: fk.resource.clone(),
                    field: fk.reference_fields.get(at)?.clone(),
                })
            })
        };
        (fields.into_iter())
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
            .collect()
    }

    /// Counts the invalid values again, in every column.
    fn recount(&mut self) {
        for column in &mut self.columns {
            column.mismatches = 0;
        }
        self.mismatches = 0;
        for values in &self.rows {
            for (column, value) in self.columns.iter_mut().zip(values) {
                if matches!(value, Value::Invalid { .. }) {
                    column.mismatches += 1;
                    self.mismatches += 1;
                }
            }
        }
    }

    /// The primary key changed: every row is keyed again from its own
    /// values, the later of two rows with one tuple taking its place.
    fn rekey(&mut self) {
        self.ordinal = 0;
        let unconfirmed = self.unconfirmed.take();
        let old_keys = std::mem::take(&mut self.row_keys);
        for &key in &old_keys {
            self.forget_row(key);
        }
        let (rows, raw) = (
            std::mem::take(&mut self.rows),
            std::mem::take(&mut self.raw),
        );
        self.row_at.clear();
        let mut confirmed = HashSet::new();
        for ((values, raw), old) in rows.into_iter().zip(raw).zip(old_keys) {
            let key = self.key_of(&raw);
            if unconfirmed.as_ref().is_some_and(|u| !u.contains(&old)) {
                confirmed.insert(key);
            }
            match self.row_at.get(&key) {
                Some(&at) => {
                    self.rows[at] = values;
                    self.raw[at] = raw;
                }
                None => {
                    self.row_at.insert(key, self.rows.len());
                    self.row_keys.push(key);
                    self.rows.push(values);
                    self.raw.push(raw);
                }
            }
        }
        self.recount();
        self.unconfirmed = unconfirmed.map(|_| {
            (self.row_keys.iter())
                .filter(|k| !confirmed.contains(k))
                .copied()
                .collect()
        });
    }

    /// The View Spec the source applies and the totals, as the Grid should
    /// hold them, if any was asked for.
    fn applied(&mut self) -> Option<(ViewSpec, Totals)> {
        if self.spec == ViewSpec::default() {
            return None;
        }
        let spec = self.spec.clone();
        let viewed = self.view(&spec);
        Some((viewed.applied, viewed.totals))
    }

    /// A `rows` message: each row is upserted, in place in the sort and
    /// filter in force; a row whose primary key is held takes that row's
    /// place. A cursor on the first page after a `table` means the table
    /// is over the cap.
    fn take_rows(&mut self, message: RowsMessage) -> Vec<(Axis, ChangeNotice)> {
        if self.schema.is_none() {
            return Vec::new();
        }
        if self.pages == 0 {
            self.status.partial = message.cursor.is_some();
        }
        self.pages += 1;
        let mut known = Vec::new();
        let mut known_at: HashMap<usize, usize> = HashMap::new();
        let mut fresh: Vec<(Key, Row)> = Vec::new();
        let mut fresh_at: HashMap<Key, usize> = HashMap::new();
        for row in message.rows {
            let key = self.key_of(&row);
            if let Some(unconfirmed) = &mut self.unconfirmed {
                unconfirmed.remove(&key);
            }
            if let Some(&at) = self.row_at.get(&key) {
                // A row twice in one page: the later takes its place.
                match known_at.get(&at) {
                    Some(&i) => known[i] = (at, row),
                    None => {
                        known_at.insert(at, known.len());
                        known.push((at, row));
                    }
                }
            } else if let Some(&i) = fresh_at.get(&key) {
                fresh[i].1 = row;
            } else {
                fresh_at.insert(key, fresh.len());
                fresh.push((key, row));
            }
        }
        let mut notices = self.update(known);
        notices.extend(self.append(fresh));
        self.regroup(notices)
    }

    /// An `upsert`: the row its key tuple names changes in place, or joins
    /// the rows.
    fn upsert(&mut self, key: &[Wire], row: Row) -> Vec<(Axis, ChangeNotice)> {
        if self.schema.is_none() {
            return Vec::new();
        }
        if let ([Wire::Int(n)], true) = (key, self.primary.is_empty()) {
            // A place the stream has not reached: the rows that come next
            // start after it. A negative place moves nothing, and no place
            // goes past the most rows a source holds, so the count never
            // overflows.
            let next = u64::try_from(*n).map_or(0, |n| n.saturating_add(1));
            let next = next.min(u64::from(u32::MAX));
            self.ordinal = self.ordinal.max(next);
        }
        let key = self.row_ids.intern(&identity(key));
        if let Some(unconfirmed) = &mut self.unconfirmed {
            unconfirmed.remove(&key);
        }
        let notices = match self.row_at.get(&key) {
            Some(&at) => self.update(vec![(at, row)]),
            None => self.append(vec![(key, row)]),
        };
        self.regroup(notices)
    }

    /// A `delete`: the row its key tuple names goes. A row the source does
    /// not hold is no matter.
    fn delete(&mut self, key: &[Wire]) -> Vec<(Axis, ChangeNotice)> {
        let at = (self.row_ids.key(&identity(key))).and_then(|k| self.row_at.get(&k).copied());
        let Some(at) = at else {
            return Vec::new();
        };
        let notices = self.remove(&[at]);
        self.regroup(notices)
    }

    /// Rows `at` (base indices) take new values: each value that changed is
    /// told to the In-Memory Table Source, or past [`BULK`] changed rows,
    /// the rows are replaced at once.
    fn update(&mut self, rows: Vec<(usize, Row)>) -> Vec<(Axis, ChangeNotice)> {
        let mut changes: Vec<(usize, Vec<Key>)> = Vec::new();
        for (at, row) in rows {
            let values = self.parse(&row);
            let old = std::mem::replace(&mut self.rows[at], values);
            self.raw[at] = row;
            self.forget_mismatches(&old);
            let changed: Vec<Key> = (self.columns.iter().zip(old.iter().zip(&self.rows[at])))
                .filter(|(_, (a, b))| a != b)
                .map(|(c, _)| c.key)
                .collect();
            if !changed.is_empty() {
                changes.push((at, changed));
            }
        }
        let base = self.grouped.source_mut();
        if changes.len() > BULK {
            return base.replaced(&self.rows, Some(self.row_keys.clone()));
        }
        let mut notices = Vec::new();
        for (at, columns) in changes {
            for column in columns {
                notices.extend(base.changed(&self.rows, at, column));
            }
        }
        notices
    }

    /// New rows join the rows, in place in the sort and filter in force.
    fn append(&mut self, rows: Vec<(Key, Row)>) -> Vec<(Axis, ChangeNotice)> {
        if rows.is_empty() {
            return Vec::new();
        }
        let mut added = Vec::with_capacity(rows.len());
        for (key, row) in rows {
            let values = self.parse(&row);
            self.row_at.insert(key, self.rows.len());
            self.row_keys.push(key);
            self.rows.push(values);
            self.raw.push(row);
            added.push(key);
        }
        self.grouped.source_mut().appended(&self.rows, &added)
    }

    /// Rows `gone` (base indices, each once) leave, and the interner lets
    /// go of them.
    fn remove(&mut self, gone: &[usize]) -> Vec<(Axis, ChangeNotice)> {
        debug_assert!(
            gone.iter().collect::<HashSet<_>>().len() == gone.len(),
            "each row once"
        );
        if gone.is_empty() {
            return Vec::new();
        }
        let mut flags = vec![false; self.rows.len()];
        for &at in gone {
            flags[at] = true;
        }
        for &at in gone {
            let values = std::mem::take(&mut self.rows[at]);
            self.forget_mismatches(&values);
            let key = self.row_keys[at];
            self.forget_row(key);
            if let Some(unconfirmed) = &mut self.unconfirmed {
                unconfirmed.remove(&key);
            }
        }
        let keep = |i: &mut usize| {
            *i += 1;
            !flags[*i - 1]
        };
        let mut i = 0;
        self.rows.retain(|_| keep(&mut i));
        let mut i = 0;
        self.raw.retain(|_| keep(&mut i));
        let mut i = 0;
        self.row_keys.retain(|_| keep(&mut i));
        self.row_at = (self.row_keys.iter().enumerate())
            .map(|(i, &k)| (k, i))
            .collect();
        self.grouped.source_mut().removed(&self.rows, gone)
    }

    /// The interner lets go of row `key`'s identity.
    fn forget_row(&mut self, key: Key) {
        if let Some(identity) = self.row_ids.identity(key).map(<[u8]>::to_vec) {
            self.row_ids.forget(&identity);
        }
    }

    /// The notices for changes to the base rows: while an axis is grouped,
    /// the groups are worked out again and it, and any other axis the
    /// changes touched, is `Reset`.
    fn regroup(&mut self, notices: Vec<(Axis, ChangeNotice)>) -> Vec<(Axis, ChangeNotice)> {
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

    /// The Key of `row`, by its primary-key tuple, or for a table with no
    /// primary key, by the next ordinal.
    fn key_of(&mut self, row: &Row) -> Key {
        let null = Wire::Null;
        let tuple: Vec<Wire> = match self.primary.is_empty() {
            true => {
                self.ordinal += 1;
                vec![Wire::Int(i128::from(self.ordinal - 1))]
            }
            false => (self.primary.iter())
                .map(|name| row.get(name).unwrap_or(&null).clone())
                .collect(),
        };
        self.row_ids.intern(&identity(&tuple))
    }

    /// `row`'s Typed Values, counting those that don't match their type.
    fn parse(&mut self, row: &Row) -> Values {
        let null = Wire::Null;
        (self.columns.iter_mut())
            .map(|column| {
                let value = typed(&column.field, row.get(&column.field.name).unwrap_or(&null));
                if matches!(value, Value::Invalid { .. }) {
                    column.mismatches += 1;
                    self.mismatches += 1;
                }
                value
            })
            .collect()
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
            let spec = self.widened(&spec);
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
        let tie_broken = self.widened(&tie_broken);
        let mut viewed = self.over().view(&tie_broken);
        self.unwidened(spec, &mut viewed.applied);
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
        let query = self.widened_find(query, &self.spec.axis(Axis::Cols).hidden);
        self.over().find_next(&query, from, direction)
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
