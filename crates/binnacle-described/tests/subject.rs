//! A Described Table's subject: the address kind of what each row is
//! about, and the field holding each row's address.

use std::collections::BTreeMap;

use binnacle_described::{DescribedSource, HostRequest, Message, Received, Scope, Wire};
use hegel::TestCase;
use hegel::generators as gs;
use serde_json::{Value as Json, json};

/// A source told of a table with `id`, `address` and `count` fields, the
/// description's `subject` set to `subject` when it is not null.
fn described(subject: Json) -> DescribedSource {
    let mut source = DescribedSource::new("convoys", Scope::new("flotilla"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    let mut schema = json!({
        "fields": [
            {"name": "id", "type": "integer"},
            {"name": "address", "type": "string"},
            {"name": "count", "type": "integer"},
        ],
        "primaryKey": ["id"],
    });
    if !subject.is_null() {
        schema["subject"] = subject;
    }
    receive(&mut source, json!({"type": "table", "schema": schema}));
    source
}

fn receive(source: &mut DescribedSource, message: Json) {
    let message = Message::from_json(&message).unwrap();
    assert!(matches!(source.receive(message), Received::Applied(_)));
}

fn row_key(source: &DescribedSource, id: i64) -> binnacle_core::Key {
    source.row_key(&[Wire::Int(id.into())]).unwrap()
}

#[test]
fn a_subject_names_the_kind_and_each_rows_address() {
    let mut source = described(json!({"kind": "convoy", "field": "address"}));
    receive(
        &mut source,
        json!({"type": "rows", "rows": [
            {"id": 1, "address": "wheelhouse/c1", "count": 3},
            {"id": 2, "address": "wheelhouse/c2"},
        ]}),
    );
    let address = source.column_key("address").unwrap();
    assert_eq!(source.subject(), Some(("convoy", address)));
    assert_eq!(
        source.row_subject(row_key(&source, 1)),
        Some("wheelhouse/c1")
    );
    assert_eq!(
        source.row_subject(row_key(&source, 2)),
        Some("wheelhouse/c2")
    );
    assert_eq!(source.row_subject(binnacle_core::Key(u64::MAX)), None);
    // The subject column is an ordinary column, shown like the others.
    assert_eq!(source.columns().len(), 3);
    assert_eq!(source.columns()[1].key(), address);
}

#[test]
fn a_table_without_a_subject_or_naming_an_unknown_field_has_none() {
    for subject in [
        Json::Null,
        json!({"kind": "convoy", "field": "nowhere"}),
        json!({"kind": "convoy"}),
        json!("convoy"),
    ] {
        let mut source = described(subject.clone());
        receive(
            &mut source,
            json!({"type": "rows", "rows": [{"id": 1, "address": "wheelhouse/c1"}]}),
        );
        assert_eq!(source.subject(), None, "{subject}");
        assert_eq!(source.schema().unwrap().subject, None, "{subject}");
        assert_eq!(source.row_subject(row_key(&source, 1)), None, "{subject}");
        assert_eq!(source.row_count(), 1, "{subject}: the table still loads");
        assert_eq!(source.columns().len(), 3, "{subject}");
    }
}

/// A subject address as a row may carry it: text, null, another type, or
/// left out (`None`).
#[hegel::composite]
fn address(tc: &TestCase) -> Option<Wire> {
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 => None,
        1 => Some(Wire::Null),
        2 => Some(Wire::Int(tc.draw(gs::integers::<i64>()).into())),
        3 => Some(Wire::Bool(tc.draw(gs::booleans()))),
        4 => Some(Wire::Array(vec![Wire::Text("a".into())])),
        _ => Some(Wire::Text(tc.draw(gs::text().max_size(6)))),
    }
}

// Each row's subject is its subject field's value when that is text, and
// none otherwise, through rows, upserts and a later `table` message that
// changes the subject; a subject naming a field the table lacks is none.
#[hegel::test(test_cases = 200)]
fn each_rows_subject_is_its_fields_text(tc: TestCase) {
    let fields = ["address", "count", "nowhere"];
    let subject = |tc: &TestCase| {
        tc.draw(gs::optional(gs::sampled_from(fields.to_vec())))
            .map(|field| json!({"kind": "convoy", "field": field}))
            .unwrap_or(Json::Null)
    };
    let mut source = described(subject(&tc));
    let mut model: BTreeMap<i64, BTreeMap<&str, Wire>> = BTreeMap::new();
    let mut held = |tc: &TestCase, source: &mut DescribedSource, id: i64, upsert: bool| {
        let mut row = serde_json::Map::new();
        row.insert("id".into(), json!(id));
        let mut values = BTreeMap::new();
        for name in ["address", "count"] {
            if let Some(value) = tc.draw(address()) {
                row.insert(name.into(), value.to_json());
                values.insert(name, value);
            }
        }
        model.insert(id, values);
        let message = match upsert {
            true => json!({"type": "upsert", "key": [id], "row": row}),
            false => json!({"type": "rows", "rows": [row]}),
        };
        receive(source, message);
    };
    for id in 0..tc.draw(gs::integers::<i64>().max_value(4).min_value(0)) {
        held(&tc, &mut source, id, false);
    }
    for _ in 0..tc.draw(gs::integers::<usize>().max_value(3)) {
        let id = tc.draw(gs::integers::<i64>().min_value(0).max_value(5));
        held(&tc, &mut source, id, true);
    }
    let check = |source: &DescribedSource, subject: &Json| {
        let field = (subject.get("field").and_then(Json::as_str)).filter(|f| *f != "nowhere");
        let want = field.map(|f| ("convoy", source.column_key(f).unwrap()));
        assert_eq!(source.subject(), want, "{subject}");
        for (id, values) in &model {
            let want = field.and_then(|f| values.get(f)).and_then(Wire::as_str);
            assert_eq!(source.row_subject(row_key(source, *id)), want, "{subject}");
        }
    };
    let first = source.schema().unwrap().to_json();
    check(&source, first.get("subject").unwrap_or(&Json::Null));
    let subject = subject(&tc);
    let mut schema = first.clone();
    match subject.is_null() {
        true => schema.as_object_mut().unwrap().remove("subject"),
        false => schema
            .as_object_mut()
            .unwrap()
            .insert("subject".into(), subject.clone()),
    };
    receive(&mut source, json!({"type": "table", "schema": schema}));
    check(&source, &subject);
}
