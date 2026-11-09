//! The `where` clause a row condition is sent as keeps exactly the rows the
//! source's own filter keeps, over the recorded fixtures: each clause is
//! evaluated with `rigging-condition`, as the fake table server (and
//! flotillad) evaluates it. A value the server can't hold (one invalid for
//! its field's type, or an integer past i64) is left out of the comparison:
//! a typed store has no such cell.

mod support;

use std::collections::BTreeSet;

use binnacle_core::{
    Axis, Comparison, Condition, FilterSpec, Key, Scalar, Value as Listed, ViewSpec,
};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Row, Schema, Scope, Wire,
};
use binnacle_format::Value;
use binnacle_memory::PumpSource;
use binnacle_testkit::fake_tables::{ORDERED, Where};
use hegel::TestCase;
use hegel::generators as gs;
use serde_json::json;
use support::{messages, table};

/// The fixtures compared over: the made one, and those recorded from
/// flotillad for binnacle#282.
const FIXTURES: [&str; 5] = [
    "crews.ndjson",
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
            Message::Rows(r) => rows.extend(r.rows.iter().cloned()),
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

/// What the comparison found: whether the condition was sent.
#[derive(Debug, PartialEq)]
enum Sent {
    Yes,
    No,
}

/// Filters `fixture` by `condition` in the source and, if it sends a
/// clause, in the server; the two keep the same rows, among those the
/// server can hold.
fn compare(fixture: &Fixture, condition: &Condition) -> Sent {
    let mut source = fixture.source();
    let filter = FilterSpec::default().with_conditions(vec![condition.clone()].into());
    let viewed = source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    let applied = viewed.applied.rows.filter.conditions.conditions();
    if applied.is_empty() {
        assert_eq!(source.wheres(), [] as [String; 0], "{condition:?}");
        return Sent::No;
    }
    let [clause] = source.wheres() else {
        assert_eq!(source.wheres(), [] as [String; 0], "{condition:?}");
        return Sent::No;
    };
    let clause = clause.clone();
    let server = Where::parse(&clause, &fixture.schema)
        .unwrap_or_else(|e| panic!("{clause} on {}: {e:?}", fixture.name));
    let shown = source.window(Axis::Rows, 0..source.count(Axis::Rows)).keys;
    let held = |key: Key| {
        !matches!(
            source.cell(key, condition.key),
            Some(Value::Invalid { .. } | Value::UInt(_))
        )
    };
    let shown: BTreeSet<Key> = (shown.into_iter()).filter(|&k| held(k)).collect();
    let kept: BTreeSet<Key> = (fixture.rows.iter())
        .filter(|r| server.keeps(r))
        .map(|r| fixture.key(&source, r))
        .filter(|&k| held(k))
        .collect();
    assert_eq!(
        shown, kept,
        "{condition:?} as `{clause}` on {}",
        fixture.name
    );
    Sent::Yes
}

fn texts(values: &[&str]) -> Vec<Listed> {
    (values.iter())
        .map(|v| match *v {
            "<blank>" => Listed::Blank,
            text => Listed::Text(text.into()),
        })
        .collect()
}

fn date(iso: &str) -> Scalar {
    let at = binnacle_format::Timestamp::parse_rfc3339(iso).unwrap();
    Scalar::Date((at.unix_nanos() / 1_000_000) as i64)
}

/// Each comparison kind over `scalars`, Between over each pair.
fn comparisons(scalars: &[Scalar]) -> Vec<Comparison> {
    let mut all = Vec::new();
    for &s in scalars {
        all.extend([
            Comparison::Equal(s),
            Comparison::NotEqual(s),
            Comparison::Less(s),
            Comparison::AtMost(s),
            Comparison::Greater(s),
            Comparison::AtLeast(s),
        ]);
        for &t in scalars {
            all.push(Comparison::Between(s, t));
        }
    }
    all
}

#[test]
fn every_condition_kind_on_the_crews_is_sent_and_keeps_the_rows_the_filter_keeps() {
    let crews = fixture("crews.ndjson");
    let source = crews.source();
    let column = |name: &str| source.column_key(name).unwrap();
    // Each condition, and whether it is sent with `not` too.
    let mut cases: Vec<(Condition, bool)> = Vec::new();
    // Nothing listed (no value shows as these) keeps nothing, so `not`
    // keeps every row and sends nothing.
    let one_ofs: [(&str, &[&str], bool); 12] = [
        ("phase", &["Running"], true),
        ("phase", &["Running", "Done"], true),
        ("phase", &["<blank>"], true),
        ("phase", &["<blank>", "Stalled"], true),
        ("phase", &[], false),
        ("phase", &["Stalled", "Nope"], true),
        ("note", &["", "late"], true),
        ("note", &["<blank>", ""], true),
        ("cpus", &["4"], true),
        ("cpus", &["<blank>", "2", "8"], true),
        ("cpus", &["many", "04", "+4"], false),
        ("host", &["brie"], true),
    ];
    for (name, values, negated) in one_ofs {
        cases.push((Condition::one_of(column(name), texts(values)), negated));
    }
    let numbers = [4.0, -1.0, 8.0, 2.0].map(Scalar::Number);
    for comparison in comparisons(&numbers) {
        cases.push((Condition::compare(column("cpus"), comparison), true));
    }
    let loads = [0.5, 1.25, 2.0, 3.5, 1.3].map(Scalar::Number);
    for comparison in comparisons(&loads) {
        cases.push((Condition::compare(column("load"), comparison), true));
    }
    let dates = [
        "2026-10-29T10:00:00Z",
        "2026-10-28T09:00:00Z",
        "2026-10-27T00:00:00Z",
        "2026-10-28T23:59:59.999Z",
    ]
    .map(date);
    for comparison in comparisons(&dates) {
        cases.push((Condition::compare(column("started"), comparison), true));
    }
    // Rounded to the integers they keep, but for equality, which no
    // integer meets.
    let fraction = Scalar::Number(4.5);
    for comparison in comparisons(&[fraction, Scalar::Number(2.0)]) {
        let whole =
            !matches!(comparison, Comparison::Equal(s) | Comparison::NotEqual(s) if s == fraction);
        cases.push((Condition::compare(column("cpus"), comparison), whole));
    }
    for (condition, negated) in cases {
        for not in [false, true] {
            let condition = condition.clone().not(not);
            let fraction = matches!(
                condition.kind,
                binnacle_core::ConditionKind::Compare(
                    Comparison::Equal(Scalar::Number(x)) | Comparison::NotEqual(Scalar::Number(x))
                ) if x.fract() != 0.0 && condition.key == column("cpus")
            );
            let want = match (not && !negated) || fraction {
                true => Sent::No,
                false => Sent::Yes,
            };
            assert_eq!(compare(&crews, &condition), want, "{condition:?}");
        }
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
    compare(&fixture, &condition);
}
