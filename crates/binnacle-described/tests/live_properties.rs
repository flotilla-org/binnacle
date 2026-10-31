//! The Described Table source against a naive model, through random
//! message streams: `table` messages that add, remove and retype fields
//! and change the primary key (one field, two, or none), `rows` pages (a
//! first one over the cap among them), upserts, deletes, resyncs, `synced`
//! and stream ends followed by a fresh stream. The model holds the table by
//! primary-key tuple (by place in its stream, for a table with none),
//! applies a Text condition and sorts the whole table by the total order
//! with the invalid tier, ties on the primary key, then the Key. A Grid
//! takes the source's Change Notices and View Specs as a host hands them.
//! After every message the rows and columns the source and the Grid show,
//! the Grid's focus, the View Spec handed over, the partial flag and the
//! mismatches must be the model's.

mod support;

use std::cmp::Ordering;
use std::collections::BTreeMap;

use binnacle_core::{
    Axis, Condition, ConditionTree, FilterSpec, Key, SortColumn, SortSpec, TextQuery, ViewSpec,
    Viewport,
};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Received, Row, RowsMessage, Schema,
    Scope, TableMessage, Wire,
};
use binnacle_testkit::Scenario;
use hegel::TestCase;
use hegel::generators as gs;
use support::Table;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    String,
    Integer,
    Number,
    Boolean,
}

const KINDS: [Kind; 4] = [Kind::String, Kind::Integer, Kind::Number, Kind::Boolean];

/// The fields a table may have, beside its primary key `id`.
const POOL: [&str; 4] = ["c0", "c1", "c2", "c3"];

impl Kind {
    fn field_type(self) -> FieldType {
        match self {
            Kind::String => FieldType::String,
            Kind::Integer => FieldType::Integer,
            Kind::Number => FieldType::Number,
            Kind::Boolean => FieldType::Boolean,
        }
    }
}

/// A value as the model reads it.
#[derive(Clone, Debug)]
enum Model {
    Null,
    Invalid(String),
    Text(String),
    Int(i128),
    Float(f64),
    Bool(bool),
}

/// What an invalid value shows: text as it is, anything else as JSON.
fn raw(wire: &Wire) -> String {
    match wire {
        Wire::Text(s) => s.clone(),
        Wire::Int(i) => i.to_string(),
        Wire::Bool(b) => b.to_string(),
        Wire::Float(x) => serde_json::Number::from_f64(*x).unwrap().to_string(),
        other => panic!("not in the pool: {other:?}"),
    }
}

/// `wire` read as a `kind` field reads it.
fn read(kind: Kind, wire: &Wire) -> Model {
    match (kind, wire) {
        (_, Wire::Null) => Model::Null,
        (Kind::String, Wire::Text(s)) => Model::Text(s.clone()),
        (Kind::Integer, Wire::Int(i)) => Model::Int(*i),
        (Kind::Number, Wire::Float(x)) => Model::Float(*x),
        (Kind::Number, Wire::Int(i)) => Model::Float(*i as f64),
        (Kind::Boolean, Wire::Bool(b)) => Model::Bool(*b),
        _ => Model::Invalid(raw(wire)),
    }
}

/// The text a Text condition reads: none for a null; a typed value that is
/// not text has no letter in this pool.
fn text(model: &Model) -> Option<String> {
    match model {
        Model::Null => None,
        Model::Invalid(s) | Model::Text(s) => Some(s.clone()),
        _ => Some("#".into()),
    }
}

/// The total order with the invalid tier.
fn compare(a: &Model, b: &Model) -> Ordering {
    let tier = |m: &Model| match m {
        Model::Null => 0,
        Model::Invalid(_) => 1,
        _ => 2,
    };
    match (a, b) {
        _ if tier(a) != tier(b) => tier(a).cmp(&tier(b)),
        (Model::Null, Model::Null) => Ordering::Equal,
        (Model::Invalid(x), Model::Invalid(y)) | (Model::Text(x), Model::Text(y)) => {
            x.as_bytes().cmp(y.as_bytes())
        }
        (Model::Int(x), Model::Int(y)) => x.cmp(y),
        (Model::Bool(x), Model::Bool(y)) => x.cmp(y),
        (Model::Float(x), Model::Float(y)) => x.partial_cmp(y).unwrap(),
        _ => unreachable!("a column holds one kind: {a:?} {b:?}"),
    }
}

#[hegel::composite]
fn wire(tc: &TestCase) -> Wire {
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 => Wire::Null,
        1 => Wire::Int(
            tc.draw(gs::integers::<i64>().min_value(-2).max_value(2))
                .into(),
        ),
        2 => Wire::Float(tc.draw(gs::sampled_from(vec![-1.5, 0.5, 2.0]))),
        3 => Wire::Bool(tc.draw(gs::booleans())),
        _ => Wire::Text(
            tc.draw(gs::sampled_from(vec!["", "a", "b", "ab", "B", "1"]))
                .to_owned(),
        ),
    }
}

/// A row with every field of the pool, keyed by `id`.
/// A row with every field of the pool and `id`.
#[hegel::composite]
fn row(tc: &TestCase, id: i64) -> Row {
    let mut row = BTreeMap::from([("id".to_owned(), Wire::Int(id.into()))]);
    for name in POOL {
        row.insert(name.to_owned(), tc.draw(wire()));
    }
    row
}

fn id(tc: &TestCase) -> i64 {
    tc.draw(gs::integers::<i64>().min_value(0).max_value(7))
}

/// The primary keys a table may have: none, one field, or two (one of
/// them a pool field the table may lack).
const KEYS: [&[&str]; 3] = [&[], &["id"], &["id", "c0"]];

#[hegel::composite]
fn primary_key(tc: &TestCase) -> Vec<String> {
    let key = tc.draw(gs::sampled_from(KEYS.to_vec()));
    key.iter().map(|n| n.to_string()).collect()
}

#[hegel::composite]
fn schema(tc: &TestCase) -> Vec<(String, Kind)> {
    let mut fields = Vec::new();
    for name in POOL {
        if tc.draw(gs::integers::<u8>().max_value(3)) > 0 {
            fields.push((name.to_owned(), tc.draw(gs::sampled_from(KINDS.to_vec()))));
        }
    }
    fields
}

fn table(fields: &[(String, Kind)], primary_key: &[String]) -> Message {
    let mut all = vec![Field::new("id", FieldType::Integer)];
    all.extend(
        fields
            .iter()
            .map(|(n, k)| Field::new(n.clone(), k.field_type())),
    );
    Message::Table(TableMessage {
        schema: Schema {
            fields: all,
            primary_key: primary_key.to_vec(),
            foreign_keys: Vec::new(),
        },
        incarnation: Some("durable".into()),
        table: None,
        sequence: None,
    })
}

/// One step of a stream.
#[derive(Debug)]
enum Step {
    Message(Message),
    /// The stream ends; a fresh one follows.
    End,
    /// The user focuses the cell of the shown row and column at these
    /// places, modulo the counts.
    Focus(usize, usize),
}

/// A place in a stream, as a table with no primary key names a row.
fn place(tc: &TestCase) -> Vec<Wire> {
    let n = tc.draw(gs::integers::<i64>().min_value(0).max_value(9));
    vec![Wire::Int(n.into())]
}

/// A primary-key tuple: half the time one the model holds, so deletes
/// and upserts hit.
fn tuple(tc: &TestCase, naive: &Naive, fresh: impl FnOnce() -> Vec<Wire>) -> Vec<Wire> {
    match naive.rows.is_empty() || tc.draw(gs::booleans()) {
        true => fresh(),
        false => {
            let at = tc.draw(gs::integers::<usize>().max_value(naive.rows.len() - 1));
            naive.rows[at].0.clone()
        }
    }
}

/// The next step, drawn against what the model holds.
fn step(tc: &TestCase, naive: &Naive) -> Step {
    let message = match tc.draw(gs::integers::<u8>().max_value(10)) {
        0 => {
            let primary = match tc.draw(gs::integers::<u8>().max_value(2)) {
                0 => tc.draw(primary_key()),
                _ => naive.primary.clone(),
            };
            table(&tc.draw(schema()), &primary)
        }
        1 | 2 => {
            let ids =
                tc.draw(gs::vecs(gs::integers::<i64>().min_value(0).max_value(7)).max_size(4));
            let rows = ids.into_iter().map(|i| tc.draw(row(i))).collect();
            let cursor = tc.draw(gs::booleans()).then(|| "next".to_owned());
            let partial = tc.draw(gs::booleans());
            Message::Rows(RowsMessage {
                rows,
                cursor,
                partial,
                sequence: None,
            })
        }
        3 | 4 => {
            match naive.primary.len() {
                // A row by its place in the stream.
                0 => {
                    let key = tuple(tc, naive, || place(tc));
                    Message::Upsert {
                        key,
                        row: tc.draw(row(id(tc))),
                    }
                }
                // The key and the row agree, as a server sends them.
                _ => {
                    let key = tuple(tc, naive, || vec![Wire::Int(id(tc).into())]);
                    let Wire::Int(i) = key[0] else { unreachable!() };
                    let mut row = tc.draw(row(i as i64));
                    if let Some(c0) = key.get(1) {
                        row.insert("c0".into(), c0.clone());
                    }
                    let key = naive.tuple_of(&row);
                    Message::Upsert { key, row }
                }
            }
        }
        5 => {
            let key = match naive.primary.len() {
                0 => tuple(tc, naive, || place(tc)),
                1 => tuple(tc, naive, || vec![Wire::Int(id(tc).into())]),
                _ => tuple(tc, naive, || {
                    vec![Wire::Int(id(tc).into()), tc.draw(wire())]
                }),
            };
            Message::Delete { key }
        }
        6 => Message::Resync,
        7 => Message::Synced { sequence: Some(1) },
        8 => Message::Progress { sequence: Some(2) },
        9 => return Step::End,
        _ => {
            let row = tc.draw(gs::integers::<usize>().max_value(15));
            return Step::Focus(row, tc.draw(gs::integers::<usize>().max_value(4)));
        }
    };
    Step::Message(message)
}

/// A table's pool fields and primary key.
type Described = (Vec<(String, Kind)>, Vec<String>);

/// The table as the source should hold it.
#[derive(Default)]
struct Naive {
    /// The pool fields present, with their kinds.
    fields: Vec<(String, Kind)>,
    /// The columns in the source's order: the first table's, then each
    /// added at the end.
    columns: Vec<String>,
    primary: Vec<String>,
    /// The last table's fields and primary key, to tell a changed one.
    schema: Option<Described>,
    /// Each row by its primary-key tuple, as it last came, in the order
    /// they came.
    rows: Vec<(Vec<Wire>, Row)>,
    /// The next place a row of a table with no primary key is keyed by.
    ordinal: u64,
    /// While in a resync, the tuples that came again.
    confirmed: Option<Vec<Vec<Wire>>>,
    pages: u64,
    partial: bool,
    synced: bool,
}

/// What a message did to the model.
#[derive(Default)]
struct Changed {
    /// A `table` changed the schema, so the source hands over its View Spec.
    schema: bool,
    /// The primary key changed: every row's Key may have changed.
    rekeyed: bool,
}

impl Naive {
    /// The primary-key tuple of `row`.
    fn tuple_of(&self, row: &Row) -> Vec<Wire> {
        (self.primary.iter())
            .map(|n| row.get(n).cloned().unwrap_or(Wire::Null))
            .collect()
    }

    /// The tuple a row that comes in a stream is keyed by: by its
    /// primary key, or for a table with none, by the next place.
    fn key_of(&mut self, row: &Row) -> Vec<Wire> {
        if self.primary.is_empty() {
            self.ordinal += 1;
            return vec![Wire::Int((self.ordinal - 1).into())];
        }
        self.tuple_of(row)
    }

    fn put(&mut self, tuple: Vec<Wire>, row: Row) {
        match self.rows.iter_mut().find(|(t, _)| *t == tuple) {
            Some(held) => held.1 = row,
            None => self.rows.push((tuple.clone(), row)),
        }
        if let Some(confirmed) = &mut self.confirmed {
            confirmed.push(tuple);
        }
    }

    fn delete(&mut self, tuple: &[Wire]) {
        self.rows.retain(|(t, _)| t != tuple);
    }

    fn resync(&mut self) {
        self.confirmed = Some(Vec::new());
        self.synced = false;
        self.ordinal = 0;
    }

    /// The primary key changed: every row is keyed again, in order, the
    /// later of two with one tuple taking the earlier's place.
    fn rekey(&mut self) {
        self.ordinal = 0;
        let confirmed = self.confirmed.take();
        let mut again = Vec::new();
        for (old, row) in std::mem::take(&mut self.rows) {
            let tuple = self.key_of(&row);
            if confirmed.as_ref().is_some_and(|c| c.contains(&old)) {
                again.push(tuple.clone());
            }
            match self.rows.iter_mut().find(|(t, _)| *t == tuple) {
                Some(held) => held.1 = row,
                None => self.rows.push((tuple, row)),
            }
        }
        self.confirmed = confirmed.map(|_| again);
    }

    fn receive(&mut self, message: &Message) -> Changed {
        let mut changed = Changed::default();
        match message {
            Message::Table(table) => {
                self.pages = 0;
                let names: Vec<String> = (table.schema.fields.iter())
                    .map(|f| f.name.clone())
                    .collect();
                self.fields = (table.schema.fields[1..].iter())
                    .map(|f| {
                        let kind = KINDS.into_iter().find(|k| k.field_type() == f.field_type);
                        (f.name.clone(), kind.unwrap())
                    })
                    .collect();
                let schema = (self.fields.clone(), table.schema.primary_key.clone());
                match self.schema.replace(schema.clone()) {
                    None => {
                        self.columns = names;
                        self.primary = schema.1;
                    }
                    Some(old) if old == schema => {}
                    Some(_) => {
                        changed.schema = true;
                        self.columns.retain(|c| names.contains(c));
                        for name in names {
                            if !self.columns.contains(&name) {
                                self.columns.push(name);
                            }
                        }
                        if self.primary != schema.1 {
                            self.primary = schema.1;
                            changed.rekeyed = true;
                            self.rekey();
                        }
                    }
                }
            }
            Message::Rows(rows) if self.schema.is_some() => {
                if self.pages == 0 {
                    self.partial = rows.cursor.is_some();
                }
                self.pages += 1;
                for row in &rows.rows {
                    let tuple = self.key_of(row);
                    self.put(tuple, row.clone());
                }
            }
            Message::Upsert { key, row } if self.schema.is_some() => {
                if let ([Wire::Int(n)], true) = (&key[..], self.primary.is_empty()) {
                    self.ordinal = self.ordinal.max(*n as u64 + 1);
                }
                self.put(key.clone(), row.clone());
            }
            Message::Delete { key } => self.delete(key),
            Message::Resync => self.resync(),
            Message::Synced { .. } => {
                self.synced = true;
                if let Some(confirmed) = self.confirmed.take() {
                    self.rows.retain(|(t, _)| confirmed.contains(t));
                }
            }
            _ => {}
        }
        changed
    }

    /// The kind of field `name`, if the table has it.
    fn kind(&self, name: &str) -> Option<Kind> {
        match name {
            "id" => Some(Kind::Integer),
            _ => self.fields.iter().find(|(n, _)| n == name).map(|&(_, k)| k),
        }
    }

    fn mismatches(&self) -> u64 {
        let invalid = |(name, kind): &(String, Kind)| {
            (self.rows.iter())
                .filter(|(_, r)| matches!(read(*kind, &r[name]), Model::Invalid(_)))
                .count() as u64
        };
        self.fields.iter().map(invalid).sum()
    }

    /// The tuples shown under `view`, in order; `key` gives a tuple's Key,
    /// the last tie-break.
    fn shown(&self, view: &View, key: impl Fn(&[Wire]) -> Key) -> Vec<Vec<Wire>> {
        let mut rows: Vec<&(Vec<Wire>, Row)> = (self.rows.iter())
            .filter(|(_, row)| match &view.condition {
                Some(c) => match self.kind(POOL[c.0]) {
                    Some(kind) => {
                        let value = text(&read(kind, &row[POOL[c.0]]));
                        let found =
                            value.is_some_and(|t| t.to_lowercase().contains(&c.1.to_lowercase()));
                        found != c.2
                    }
                    None => true,
                },
                None => true,
            })
            .collect();
        // The terms on fields the table has, then its primary key's fields,
        // ascending, that the sort leaves out.
        let mut terms: Vec<(&str, bool)> = (view.sort.iter())
            .filter(|&&(c, _)| self.kind(POOL[c]).is_some())
            .map(|&(c, descending)| (POOL[c], descending))
            .collect();
        if !view.sort.is_empty() {
            terms.extend(
                (self.primary.iter())
                    .filter(|n| self.kind(n).is_some())
                    .filter(|n| view.sort.iter().all(|&(c, _)| POOL[c] != n.as_str()))
                    .map(|n| (n.as_str(), false)),
            );
        }
        if !terms.is_empty() {
            rows.sort_by(|(a, x), (b, y)| {
                for &(name, descending) in &terms {
                    let kind = self.kind(name).unwrap();
                    let ordering = compare(&read(kind, &x[name]), &read(kind, &y[name]));
                    let ordering = if descending {
                        ordering.reverse()
                    } else {
                        ordering
                    };
                    if ordering.is_ne() {
                        return ordering;
                    }
                }
                key(a).cmp(&key(b))
            });
        }
        rows.into_iter().map(|(t, _)| t.clone()).collect()
    }
}

/// A View Spec: sort terms on pool fields, and a Text condition on one
/// (field, text, not).
#[derive(Debug)]
struct View {
    sort: Vec<(usize, bool)>,
    condition: Option<(usize, String, bool)>,
}

impl View {
    /// The View Spec, with only the terms on fields `present` holds.
    fn spec(&self, key: impl Fn(usize) -> Key, present: impl Fn(usize) -> bool) -> ViewSpec {
        let sort: Vec<SortColumn> = (self.sort.iter())
            .filter(|&&(c, _)| present(c))
            .map(|&(c, descending)| match descending {
                true => SortColumn::descending(key(c)),
                false => SortColumn::ascending(key(c)),
            })
            .collect();
        let conditions: Vec<Condition> = (self.condition.iter())
            .filter(|c| present(c.0))
            .map(|(c, text, not)| {
                let mut condition = Condition::text(key(*c), TextQuery::new(text.clone()));
                condition.not = *not;
                condition
            })
            .collect();
        let mut spec = ViewSpec::default();
        if !sort.is_empty() {
            spec = spec.with_sort(Axis::Rows, SortSpec::new(sort));
        }
        if !conditions.is_empty() {
            spec = spec.with_filter(
                Axis::Rows,
                FilterSpec::default().with_conditions(ConditionTree::All(conditions)),
            );
        }
        spec
    }
}

#[hegel::composite]
fn view(tc: &TestCase) -> View {
    let mut sort: Vec<(usize, bool)> = Vec::new();
    for (c, descending) in tc.draw(
        gs::vecs(hegel::tuples!(
            gs::integers::<usize>().max_value(POOL.len() - 1),
            gs::booleans()
        ))
        .max_size(2),
    ) {
        if sort.iter().all(|&(s, _)| s != c) {
            sort.push((c, descending));
        }
    }
    let condition = tc.draw(gs::booleans()).then(|| {
        (
            tc.draw(gs::integers::<usize>().max_value(POOL.len() - 1)),
            tc.draw(gs::sampled_from(vec!["a", "b", "A"])).to_owned(),
            tc.draw(gs::booleans()),
        )
    });
    View { sort, condition }
}

/// Tall enough to draw every row the model can hold.
const TALL: Viewport = Viewport {
    width: 12.0 * 21.0,
    height: 401.0,
};

/// Hands the Grid what `received` changed, as a host does: the columns'
/// support, the View Spec the source applies now, then the Change Notices.
/// Returns the View Spec handed over.
fn hand_over(s: &mut Scenario<Table>, received: Received) -> Option<ViewSpec> {
    let Received::Applied(notices) = received else {
        panic!("{received:?}")
    };
    let Scenario { grid, source, .. } = s;
    source.0.declare(grid);
    let viewed = source.0.poll_view();
    if let Some((spec, totals)) = &viewed {
        grid.view_changed(spec.clone(), *totals);
    }
    for (axis, notice) in notices {
        grid.notify(axis, notice);
    }
    s.frame(TALL);
    viewed.map(|(spec, _)| spec)
}

#[hegel::test(test_cases = 500)]
fn the_source_and_its_grid_show_the_naive_model_after_every_message(tc: TestCase) {
    let mut source = DescribedSource::new("t", Scope::new("n"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    let mut naive = Naive::default();
    // Every pool field comes first, so each has its Key.
    let all: Vec<(String, Kind)> = POOL.iter().map(|n| (n.to_string(), Kind::String)).collect();
    let first = table(&all, &tc.draw(primary_key()));
    naive.receive(&first);
    source.receive(first);
    let keys: Vec<Key> = POOL.iter().map(|n| source.column_key(n).unwrap()).collect();
    let mut s = support::scenario(source);
    let view = tc.draw(view());
    let asked = view.spec(|c| keys[c], |_| true);
    let viewing = asked != ViewSpec::default();
    if viewing {
        s.input(|g| g.request_view(asked));
    }
    s.frame(TALL);
    let mut focus: Option<(Vec<Wire>, String)> = None;
    let steps = tc.draw(gs::integers::<usize>().max_value(24));
    for _ in 0..steps {
        let step = step(&tc, &naive);
        let mut changed = Changed::default();
        let mut handed = None;
        match step {
            Step::Message(message) => {
                changed = naive.receive(&message);
                let received = s.source.0.receive(message);
                handed = hand_over(&mut s, received);
            }
            Step::End => {
                naive.resync();
                let received = s.source.0.ended();
                assert!(
                    matches!(s.source.0.poll_request(), Some(HostRequest::OpenWatch(_))),
                    "the watch is opened again"
                );
                handed = hand_over(&mut s, received);
            }
            Step::Focus(row, col) => {
                let source = &s.source.0;
                let shown = naive.shown(&view, |t| source.row_key(t).unwrap());
                if !shown.is_empty() {
                    let tuple = shown[row % shown.len()].clone();
                    let name = naive.columns[col % naive.columns.len()].clone();
                    let (row, col) = (source.row_key(&tuple).unwrap(), source.column_key(&name));
                    s.input(|g| g.set_focus(row, col.unwrap()));
                    focus = Some((tuple, name));
                }
            }
        }
        let source = &mut s.source.0;
        let key = |t: &[Wire]| source.row_key(t).unwrap();
        let want: Vec<Key> = (naive.shown(&view, key).iter()).map(|t| key(t)).collect();
        let count = source.count(Axis::Rows);
        assert_eq!(source.window(Axis::Rows, 0..count).keys, want, "{view:?}");
        let columns: Vec<Key> = (naive.columns.iter())
            .map(|n| source.column_key(n).unwrap())
            .collect();
        let count = source.count(Axis::Cols);
        assert_eq!(source.window(Axis::Cols, 0..count).keys, columns);
        assert_eq!(source.status().partial, naive.partial);
        assert_eq!(source.mismatches(), naive.mismatches());
        assert_eq!(source.row_count(), naive.rows.len());
        assert_eq!(source.interned_rows(), naive.rows.len());
        assert_eq!(source.status().synced, naive.synced);
        // A changed table hands over the View Spec the source applies: the
        // asked one, less the terms on fields the table lacks.
        let present = |c: usize| naive.kind(POOL[c]).is_some();
        let applied = view.spec(|c| keys[c], present);
        match changed.schema && viewing {
            true => {
                let handed = handed.expect("a View Spec handed over");
                assert_eq!(handed.axis(Axis::Rows).sort, applied.axis(Axis::Rows).sort);
                assert_eq!(
                    handed.axis(Axis::Rows).filter.conditions,
                    applied.axis(Axis::Rows).filter.conditions
                );
            }
            false => assert_eq!(handed, None),
        }
        // The Grid draws the source's rows and columns, and its focus stays
        // on its row while the row is shown and keeps its Key, and on its
        // column while the column is there.
        let snapshot = s.frame(TALL);
        let rows: Vec<Key> = snapshot.drawn(Axis::Rows).filter_map(|i| i.key).collect();
        let cols: Vec<Key> = snapshot.drawn(Axis::Cols).filter_map(|i| i.key).collect();
        let focused = snapshot.focus(Axis::Rows).and_then(|f| f.key);
        let focused_col = snapshot.focus(Axis::Cols).and_then(|f| f.key);
        assert_eq!(rows, want);
        assert_eq!(cols, columns);
        let source = &s.source.0;
        let held = |t: &Vec<Wire>| want.contains(&source.row_key(t).unwrap_or(Key(u64::MAX)));
        if let Some((tuple, _)) = focus.as_ref().filter(|(t, _)| !changed.rekeyed && held(t)) {
            assert_eq!(focused, source.row_key(tuple), "focus follows its Key");
        }
        if let Some((_, name)) = focus.as_ref().filter(|_| focused.is_some())
            && naive.columns.contains(name)
        {
            assert_eq!(focused_col, source.column_key(name), "on its column");
        }
        assert!(
            focused.is_none_or(|k| want.contains(&k)),
            "focus on a row shown"
        );
        assert!(
            focused_col.is_none_or(|k| columns.contains(&k)),
            "focus on a column shown"
        );
        focus = focused.zip(focused_col).map(|(k, c)| {
            let at = want.iter().position(|&w| w == k).unwrap();
            let tuple = naive.shown(&view, |t| source.row_key(t).unwrap())[at].clone();
            (tuple, source.column(c).unwrap().name().to_owned())
        });
    }
}
