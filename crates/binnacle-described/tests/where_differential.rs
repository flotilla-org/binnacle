//! The `where` clause a row condition is sent as keeps every row the
//! source's own filter keeps, and of the rows whose cell the server holds, no
//! other, over the recorded fixtures with rows added whose cells the server
//! reads as missing: a value of another type, a NaN, an integer past i64.
//! Each clause is evaluated with `rigging-condition`, as the fake table
//! server (and flotillad) evaluates it.

mod support;

use std::collections::BTreeSet;

use binnacle_core::{
    Axis, Comparison, Condition, ConditionKind, FilterSpec, Key, Scalar, Value as Listed, ViewSpec,
};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Row, Schema, Scope, Wire,
};
use binnacle_format::{Value, ValueType};
use binnacle_memory::PumpSource;
use binnacle_testkit::fake_tables::{ORDERED, Where};
use hegel::TestCase;
use hegel::generators as gs;
use serde_json::json;
use support::{messages, table};

/// The fixtures compared over: the made ones, and those recorded from
/// flotillad for binnacle#282.
const FIXTURES: [&str; 6] = [
    "crews.ndjson",
    "flags.ndjson",
    "rows-hosts.ndjson",
    "rows-repositories.ndjson",
    "rows-fulfilmentkinds.ndjson",
    "rows-convoys-cols.ndjson",
];

/// A fixture's table, every field taking the operators flotillad lists for
/// its type, and its rows.
struct Fixture {
    name: &'static str,
    messages: Vec<Message>,
    schema: Schema,
    rows: Vec<Row>,
}

fn fixture(name: &'static str) -> Fixture {
    let mut messages = messages(name);
    let mut schema = None;
    let mut rows = Vec::new();
    for message in &mut messages {
        match message {
            Message::Table(t) => {
                t.schema.fields = t.schema.fields.iter().map(operated).collect();
                schema = Some(t.schema.clone());
            }
            Message::Rows(r) => {
                if rows.is_empty() {
                    let schema = schema.as_ref().unwrap();
                    r.rows.extend(odd_rows(schema, &r.rows[0]));
                }
                rows.extend(r.rows.iter().cloned());
            }
            _ => {}
        }
    }
    Fixture {
        name,
        messages,
        schema: schema.unwrap(),
        rows,
    }
}

/// Rows like `like`, each with one cell the server reads as missing: a value
/// of another type, a NaN, an integer past i64, or text outside the field's
/// categories or enum. Each has a primary key of its own.
fn odd_rows(schema: &Schema, like: &Row) -> Vec<Row> {
    let mut rows = Vec::new();
    for field in &schema.fields {
        let mut odd = match field.field_type {
            FieldType::String => vec![Wire::Int(5), Wire::Bool(true), Wire::Float(1.5)],
            FieldType::Integer => vec![
                Wire::Text("5".into()),
                Wire::Text("x".into()),
                Wire::Float(2.5),
                Wire::Int(i128::from(i64::MAX) + 5),
            ],
            FieldType::Number => vec![
                Wire::Text("5".into()),
                Wire::Text("x".into()),
                Wire::Float(f64::NAN),
            ],
            FieldType::Boolean => vec![Wire::Text("true".into()), Wire::Int(1)],
            FieldType::Datetime => vec![Wire::Text("soon".into()), Wire::Int(5)],
            _ => Vec::new(),
        };
        let enumerated = field.categories.is_some()
            || (field.property("constraints")).is_some_and(|c| c.get("enum").is_some());
        if matches!(field.field_type, FieldType::String) && enumerated {
            odd.push(Wire::Text("outside".into()));
        }
        for value in odd {
            let n = rows.len();
            let mut row = like.clone();
            for name in &schema.primary_key {
                let pk = (schema.fields.iter()).find(|f| &f.name == name).unwrap();
                row.insert(
                    name.clone(),
                    match pk.field_type {
                        FieldType::Integer => Wire::Int(1_000_000 + n as i128),
                        _ => Wire::Text(format!("odd-{n}")),
                    },
                );
            }
            row.insert(field.name.clone(), value);
            rows.push(row);
        }
    }
    rows
}

/// `field` with the `whereOperators` flotillad lists for its type.
fn operated(field: &Field) -> Field {
    let ops: &[&str] = match field.field_type {
        FieldType::String | FieldType::Integer | FieldType::Number | FieldType::Datetime => {
            &ORDERED
        }
        FieldType::Boolean => &["==", "!=", "has"],
        _ => &[],
    };
    let mut properties = field.properties().clone();
    properties.insert("whereOperators".into(), json!(ops));
    Field::from_properties(properties).unwrap()
}

impl Fixture {
    fn source(&self) -> DescribedSource {
        let mut source = DescribedSource::new(table(self.name), Scope::new("flotilla"));
        assert!(matches!(
            source.poll_request(),
            Some(HostRequest::OpenWatch(_))
        ));
        for message in &self.messages {
            source.receive(message.clone());
        }
        source
    }

    fn key(&self, source: &DescribedSource, row: &Row) -> Key {
        let tuple: Vec<Wire> = (self.schema.primary_key.iter())
            .map(|name| row.get(name).cloned().unwrap_or(Wire::Null))
            .collect();
        source.row_key(&tuple).unwrap()
    }
}

/// Filters `fixture` by `condition` in the source and, if it sends a
/// clause, in the server. The server keeps every row the source shows, and
/// of the rows whose cell it holds, no other. Whether a clause was sent.
fn compare(fixture: &Fixture, condition: &Condition) -> bool {
    let mut source = fixture.source();
    let filter = FilterSpec::default().with_conditions(vec![condition.clone()].into());
    let viewed = source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    let applied = viewed.applied.rows.filter.conditions.conditions();
    if applied.is_empty() {
        assert_eq!(source.wheres(), [] as [String; 0], "{condition:?}");
        return false;
    }
    let [clause] = source.wheres() else {
        assert_eq!(source.wheres(), [] as [String; 0], "{condition:?}");
        return false;
    };
    let clause = clause.clone();
    let server = Where::parse(&clause, &fixture.schema)
        .unwrap_or_else(|e| panic!("{clause} on {}: {e:?}", fixture.name));
    let shown: BTreeSet<Key> = (source.window(Axis::Rows, 0..source.count(Axis::Rows)).keys)
        .into_iter()
        .collect();
    let kept: BTreeSet<Key> = (fixture.rows.iter())
        .filter(|r| server.keeps(r))
        .map(|r| fixture.key(&source, r))
        .collect();
    let what = format!("{condition:?} as `{clause}` on {}", fixture.name);
    assert!(
        shown.is_subset(&kept),
        "{what}: {:?} not sent",
        shown.difference(&kept)
    );
    // A cell the server holds: not a blank, an invalid one, a NaN or an
    // integer past i64.
    let held = |key: &Key| {
        !matches!(
            source.cell(*key, condition.key),
            None | Some(Value::Null | Value::Invalid { .. } | Value::UInt(_))
        ) && !matches!(source.cell(*key, condition.key), Some(Value::Number(x)) if x.is_nan())
    };
    let kept: BTreeSet<Key> = kept.into_iter().filter(held).collect();
    let shown: BTreeSet<Key> = shown.into_iter().filter(held).collect();
    assert_eq!(shown, kept, "{what}");
    true
}

/// Whether `condition` is a comparison a clause can express: on a number,
/// an integer within ±2^53 (and whole, for equality) or a date.
fn expressible(source: &DescribedSource, condition: &Condition) -> bool {
    let ConditionKind::Compare(comparison) = &condition.kind else {
        return false;
    };
    let Some(column) = source.columns().iter().find(|c| c.key() == condition.key) else {
        return false;
    };
    let scalars = match *comparison {
        Comparison::Between(a, b) => vec![a, b],
        Comparison::Equal(s)
        | Comparison::NotEqual(s)
        | Comparison::Less(s)
        | Comparison::AtMost(s)
        | Comparison::Greater(s)
        | Comparison::AtLeast(s) => vec![s],
    };
    let whole = !matches!(comparison, Comparison::Equal(_) | Comparison::NotEqual(_));
    let fits = |s: &Scalar| match (column.field().field_type.clone(), column.value_type(), *s) {
        (FieldType::Number, ValueType::Number, Scalar::Number(x)) => x.is_finite(),
        (FieldType::Integer, ValueType::Int, Scalar::Number(x)) => {
            x.abs() < 9_007_199_254_740_992.0 && (whole || x.fract() == 0.0)
        }
        (FieldType::Datetime, ValueType::DateTime, Scalar::Date(_)) => true,
        _ => false,
    };
    scalars.iter().all(fits)
}

/// The cases the drawn conditions rarely reach: a blank listed, an integer
/// compared with a fraction, and each comparison on a number and a date.
#[test]
fn pinned_conditions_are_sent_and_keep_the_rows_the_filter_keeps() {
    let crews = fixture("crews.ndjson");
    let source = crews.source();
    let column = |name: &str| source.column_key(name).unwrap();
    let (phase, cpus) = (column("phase"), column("cpus"));
    let running = vec![Listed::Blank, Listed::Text("Running".into())];
    let half = Scalar::Number(4.5);
    let mut cases = vec![
        Condition::one_of(phase, running.clone()),
        Condition::one_of(phase, running).not(true),
        Condition::compare(cpus, Comparison::Less(half)),
        Condition::compare(cpus, Comparison::AtMost(half)),
        Condition::compare(cpus, Comparison::Greater(Scalar::Number(2.0))),
    ];
    let at = binnacle_format::Timestamp::parse_rfc3339("2026-10-28T09:00:00Z").unwrap();
    let date = Scalar::Date((at.unix_nanos() / 1_000_000) as i64);
    let later = Scalar::Date((at.unix_nanos() / 1_000_000) as i64 + 86_400_000);
    for (key, s, t) in [
        (column("load"), Scalar::Number(1.25), Scalar::Number(3.5)),
        (column("started"), date, later),
    ] {
        for comparison in [
            Comparison::Equal(s),
            Comparison::NotEqual(s),
            Comparison::Less(s),
            Comparison::AtMost(s),
            Comparison::Greater(s),
            Comparison::AtLeast(s),
            Comparison::Between(s, t),
        ] {
            cases.push(Condition::compare(key, comparison));
            cases.push(Condition::compare(key, comparison).not(true));
        }
    }
    for condition in cases {
        assert!(compare(&crews, &condition), "{condition:?} is sent");
    }
    // A boolean, an integer, a category and an enum, beside cells that show
    // as the listed text but the server reads as missing.
    let flags = fixture("flags.ndjson");
    let source = flags.source();
    let column = |name: &str| source.column_key(name).unwrap();
    let texts = |values: &[&str]| values.iter().map(|v| Listed::Text((*v).into())).collect();
    for (name, values, sent) in [
        ("done", &["true"][..], true),
        ("count", &["5"], true),
        ("count", &["x"], true),
        ("kind", &["feat"], true),
        ("kind", &["Fix"], false),
        ("tier", &["gold"], true),
        ("tier", &["outside"], false),
    ] {
        let condition = Condition::one_of(column(name), texts(values));
        assert_eq!(compare(&flags, &condition), sent, "{condition:?}");
    }
    // 2^53 + 1 compares as 2^53 in the filter.
    let edge = Scalar::Number(9_007_199_254_740_992.0);
    for comparison in [Comparison::Equal(edge), Comparison::AtMost(edge)] {
        compare(&flags, &Condition::compare(column("count"), comparison));
    }
}

#[hegel::composite]
fn condition(tc: &TestCase, fixture: &Fixture, source: &DescribedSource) -> Condition {
    let columns: Vec<_> = (source.columns().iter()).filter(|c| !c.nested()).collect();
    let column = tc.draw(gs::sampled_from(columns));
    let key = column.key();
    // Values from the rows, with near misses.
    let shown: Vec<String> = (source.row_keys().iter())
        .filter_map(|&r| {
            let value = source.cell(r, key)?;
            let ctx = binnacle_format::FormatContext::new(
                binnacle_format::Timestamp::UNIX_EPOCH,
                &binnacle_format::FixedOffset(0),
            );
            let options = binnacle_format::FormatOptions::default();
            let text =
                binnacle_format::format_stable(value, column.unit(), column.role(), &options, &ctx);
            (!matches!(value, Value::Null)).then_some(text.text)
        })
        .chain(["", "x", "0", "1", "✓", "Running"].map(String::from))
        .collect();
    let scalars: Vec<f64> = (fixture.rows.iter())
        .filter_map(|r| match r.get(column.name()) {
            Some(Wire::Int(i)) => Some(*i as f64),
            Some(Wire::Float(x)) => Some(*x),
            Some(Wire::Text(t)) => binnacle_format::Timestamp::parse_rfc3339(t)
                .map(|at| (at.unix_nanos() / 1_000_000) as f64),
            _ => None,
        })
        .chain([0.0, -1.0, 0.5])
        .collect();
    let scalar = |tc: &TestCase| {
        let base = tc.draw(gs::sampled_from(scalars.clone()));
        let nudge = tc.draw(gs::sampled_from(vec![0.0, 1.0, -1.0, 0.5, -0.25, 1000.0]));
        match column.value_type() {
            binnacle_format::ValueType::DateTime => Scalar::Date((base + nudge).floor() as i64),
            _ => Scalar::Number(base + nudge),
        }
    };
    let not = tc.draw(gs::booleans());
    let condition = match tc.draw(gs::booleans()) {
        true => {
            let n = tc.draw(gs::integers::<usize>().max_value(3));
            let mut values: Vec<Listed> = (0..n)
                .map(|_| Listed::Text(tc.draw(gs::sampled_from(shown.clone()))))
                .collect();
            if tc.draw(gs::booleans()) {
                values.push(Listed::Blank);
            }
            Condition::one_of(key, values)
        }
        false => {
            let comparison = match tc.draw(gs::integers::<u8>().max_value(6)) {
                0 => Comparison::Equal(scalar(tc)),
                1 => Comparison::NotEqual(scalar(tc)),
                2 => Comparison::Less(scalar(tc)),
                3 => Comparison::AtMost(scalar(tc)),
                4 => Comparison::Greater(scalar(tc)),
                5 => Comparison::AtLeast(scalar(tc)),
                _ => Comparison::Between(scalar(tc), scalar(tc)),
            };
            Condition::compare(key, comparison)
        }
    };
    condition.not(not)
}

#[hegel::test(test_cases = 300)]
fn a_drawn_condition_sent_keeps_the_rows_the_filter_keeps(tc: TestCase) {
    let fixture = fixture(tc.draw(gs::sampled_from(FIXTURES.to_vec())));
    let source = fixture.source();
    let condition = tc.draw(condition(&fixture, &source));
    let sent = compare(&fixture, &condition);
    assert!(
        sent || !expressible(&source, &condition),
        "{condition:?} is sent"
    );
}
