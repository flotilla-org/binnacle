//! The Described Table source against a naive model, through random
//! message streams: `table` messages that add, remove and retype fields,
//! `rows` pages (a first one over the cap among them), upserts, deletes,
//! resyncs, `synced` and stream ends followed by a fresh stream. The model
//! holds the table by primary key, applies a Text condition and sorts the
//! whole table by the total order with the invalid tier, ties on the
//! primary key; after every message the rows the source shows, its
//! partial flag and its mismatches must be the model's.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use binnacle_core::{
    Axis, Condition, ConditionTree, FilterSpec, Key, SortColumn, SortSpec, TextQuery, ViewSpec,
};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Row, RowsMessage, Schema, Scope,
    TableMessage, Wire,
};
use binnacle_memory::PumpSource;
use hegel::TestCase;
use hegel::generators as gs;

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

fn table(fields: &[(String, Kind)]) -> Message {
    let mut all = vec![Field::new("id", FieldType::Integer)];
    all.extend(
        fields
            .iter()
            .map(|(n, k)| Field::new(n.clone(), k.field_type())),
    );
    Message::Table(TableMessage {
        schema: Schema {
            fields: all,
            primary_key: vec!["id".into()],
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
}

#[hegel::composite]
fn step(tc: &TestCase) -> Step {
    let message = match tc.draw(gs::integers::<u8>().max_value(9)) {
        0 => table(&tc.draw(schema())),
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
            let i = id(tc);
            Message::Upsert {
                key: vec![Wire::Int(i.into())],
                row: tc.draw(row(i)),
            }
        }
        5 => Message::Delete {
            key: vec![Wire::Int(id(tc).into())],
        },
        6 => Message::Resync,
        7 => Message::Synced { sequence: Some(1) },
        8 => Message::Progress { sequence: Some(2) },
        _ => return Step::End,
    };
    Step::Message(message)
}

/// The table as the source should hold it.
#[derive(Default)]
struct Naive {
    fields: Vec<(String, Kind)>,
    /// Each row by its id, as it last came.
    rows: BTreeMap<i64, Row>,
    /// The ids in the order they came.
    order: Vec<i64>,
    /// While in a resync, the ids that came again.
    confirmed: Option<Vec<i64>>,
    pages: u64,
    partial: bool,
    synced: bool,
}

impl Naive {
    fn upsert(&mut self, row: Row) {
        let Wire::Int(id) = row["id"] else {
            unreachable!()
        };
        let id = id as i64;
        if self.rows.insert(id, row).is_none() {
            self.order.push(id);
        }
        if let Some(confirmed) = &mut self.confirmed {
            confirmed.push(id);
        }
    }

    fn delete(&mut self, id: i64) {
        self.rows.remove(&id);
        self.order.retain(|&i| i != id);
    }

    fn receive(&mut self, message: &Message) {
        match message {
            Message::Table(table) => {
                self.fields = (table.schema.fields[1..].iter())
                    .map(|f| {
                        let kind = KINDS.into_iter().find(|k| k.field_type() == f.field_type);
                        (f.name.clone(), kind.unwrap())
                    })
                    .collect();
                self.pages = 0;
            }
            Message::Rows(rows) => {
                if self.pages == 0 {
                    self.partial = rows.cursor.is_some();
                }
                self.pages += 1;
                for row in &rows.rows {
                    self.upsert(row.clone());
                }
            }
            Message::Upsert { row, .. } => self.upsert(row.clone()),
            Message::Delete { key } => {
                let Wire::Int(id) = key[0] else {
                    unreachable!()
                };
                self.delete(id as i64);
            }
            Message::Resync => {
                self.confirmed = Some(Vec::new());
                self.synced = false;
            }
            Message::Synced { .. } => {
                self.synced = true;
                if let Some(confirmed) = self.confirmed.take() {
                    let gone: Vec<i64> = (self.rows.keys())
                        .filter(|id| !confirmed.contains(id))
                        .copied()
                        .collect();
                    for id in gone {
                        self.delete(id);
                    }
                }
            }
            _ => {}
        }
    }

    fn kind(&self, name: &str) -> Option<Kind> {
        self.fields.iter().find(|(n, _)| n == name).map(|&(_, k)| k)
    }

    fn mismatches(&self) -> u64 {
        let invalid = |(name, kind): &(String, Kind)| {
            (self.rows.values())
                .filter(|r| matches!(read(*kind, &r[name]), Model::Invalid(_)))
                .count() as u64
        };
        self.fields.iter().map(invalid).sum()
    }

    /// The ids shown under `view`, in order.
    fn shown(&self, view: &View) -> Vec<i64> {
        let mut ids: Vec<i64> = (self.order.iter().copied())
            .filter(
                |id| match (&view.condition, self.kind(POOL[view.condition_on()])) {
                    (Some(c), Some(kind)) => {
                        let value = text(&read(kind, &self.rows[id][POOL[c.0]]));
                        let found =
                            value.is_some_and(|t| t.to_lowercase().contains(&c.1.to_lowercase()));
                        found != c.2
                    }
                    _ => true,
                },
            )
            .collect();
        if !view.sort.is_empty() {
            ids.sort_by(|a, b| {
                for &(c, descending) in &view.sort {
                    let Some(kind) = self.kind(POOL[c]) else {
                        continue;
                    };
                    let (x, y) = (&self.rows[a][POOL[c]], &self.rows[b][POOL[c]]);
                    let ordering = compare(&read(kind, x), &read(kind, y));
                    let ordering = if descending {
                        ordering.reverse()
                    } else {
                        ordering
                    };
                    if ordering.is_ne() {
                        return ordering;
                    }
                }
                a.cmp(b)
            });
        }
        ids
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
    fn condition_on(&self) -> usize {
        self.condition.as_ref().map_or(0, |c| c.0)
    }

    fn spec(&self, key: impl Fn(usize) -> Key) -> ViewSpec {
        let sort = (self.sort.iter())
            .map(|&(c, descending)| match descending {
                true => SortColumn::descending(key(c)),
                false => SortColumn::ascending(key(c)),
            })
            .collect();
        let conditions = (self.condition.iter())
            .map(|(c, text, not)| {
                let mut condition = Condition::text(key(*c), TextQuery::new(text.clone()));
                condition.not = *not;
                condition
            })
            .collect();
        ViewSpec::default()
            .with_sort(Axis::Rows, SortSpec::new(sort))
            .with_filter(
                Axis::Rows,
                FilterSpec::default().with_conditions(ConditionTree::All(conditions)),
            )
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

#[hegel::test(test_cases = 500)]
fn the_source_shows_the_naive_model_after_every_message(tc: TestCase) {
    let mut source = DescribedSource::new("t", Scope::new("n"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    let mut naive = Naive::default();
    // Every pool field comes first, so each has its Key.
    let all: Vec<(String, Kind)> = POOL.iter().map(|n| (n.to_string(), Kind::String)).collect();
    let first = table(&all);
    naive.receive(&first);
    source.receive(first);
    let keys: Vec<Key> = POOL.iter().map(|n| source.column_key(n).unwrap()).collect();
    let view = tc.draw(view());
    let spec = view.spec(|c| keys[c]);
    source.view(&spec);
    let steps = tc.draw(gs::vecs(step()).max_size(24));
    for step in steps {
        match step {
            Step::Message(message) => {
                naive.receive(&message);
                source.receive(message);
            }
            Step::End => {
                naive.confirmed = Some(Vec::new());
                naive.synced = false;
                source.ended();
                assert!(
                    matches!(source.poll_request(), Some(HostRequest::OpenWatch(_))),
                    "the watch is opened again"
                );
            }
        }
        if let Some((applied, _)) = source.poll_view() {
            let rows = applied.axis(Axis::Rows);
            let present = |k: Key| {
                let c = keys.iter().position(|&x| x == k).unwrap();
                naive.kind(POOL[c]).is_some()
            };
            assert!(
                rows.sort.columns().iter().all(|s| present(s.key)),
                "{applied:?}"
            );
            assert!(
                (rows.filter.conditions.conditions().iter()).all(|c| present(c.key)),
                "{applied:?}"
            );
        }
        let count = source.count(Axis::Rows);
        let shown = source.window(Axis::Rows, 0..count).keys;
        let want: Vec<Key> = (naive.shown(&view).into_iter())
            .map(|id| source.row_key(&[Wire::Int(id.into())]).unwrap())
            .collect();
        assert_eq!(shown, want, "{view:?}");
        assert_eq!(source.status().partial, naive.partial);
        assert_eq!(source.mismatches(), naive.mismatches());
        assert_eq!(source.row_count(), naive.rows.len());
        assert_eq!(source.interned_rows(), naive.rows.len());
        assert_eq!(source.status().synced, naive.synced);
    }
}
