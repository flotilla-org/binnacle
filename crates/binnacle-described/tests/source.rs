//! Small examples of the Described Table source: its watch request, its
//! columns, typed values and Keys.

mod support;

use binnacle_core::{Axis, ChangeNotice, SortColumn, SortSpec, ViewSpec};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Received, Reference, Scope,
    WatchRequest, Wire, deterministic_cbor, typed,
};
use binnacle_format::{Timestamp, Value, ValueType};
use binnacle_memory::PumpSource;
use serde_json::json;
use support::{messages, replay};

#[test]
fn a_source_first_asks_its_host_to_open_a_watch_on_its_scope() {
    let scope = Scope::new("flotilla").with_where(".status.phase == Active");
    let mut source = DescribedSource::new("vessels", scope.clone());
    let want = WatchRequest {
        table: "vessels".into(),
        scope,
        limit: 10_000,
    };
    assert_eq!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(want.clone()))
    );
    assert_eq!(source.poll_request(), None, "once");
    assert_eq!(
        want.target(),
        "/tables/vessels/rows?watch&limit=10000&namespace=flotilla&where=.status.phase%20%3D%3D%20Active"
    );
}

#[test]
#[should_panic(expected = "a Scope needs a namespace")]
fn a_scope_needs_a_namespace() {
    Scope::new("");
}

#[test]
fn headers_use_the_title_else_the_name() {
    let source = replay("crews.ndjson");
    let headers: Vec<&str> = source.columns().iter().map(|c| c.header()).collect();
    assert_eq!(
        headers,
        [
            "Name", "Phase", "Started", "CPUs", "Load", "note", "Host", "Spec"
        ]
    );
    let hosts = replay("rows-hosts.ndjson");
    let version = hosts.column_key("metadata.resourceVersion").unwrap();
    assert_eq!(
        hosts.column(version).unwrap().header(),
        "metadata.resourceVersion"
    );
}

#[test]
fn each_column_reports_what_its_field_describes() {
    let source = replay("crews.ndjson");
    let column = |name| source.column(source.column_key(name).unwrap()).unwrap();
    let load = column("load");
    assert_eq!(load.field_type(), &FieldType::Number);
    assert_eq!(load.value_type(), ValueType::Number);
    assert_eq!(load.unit(), Some("%"));
    assert!(load.nullable());
    let name = column("name");
    assert!(!name.nullable(), "constraints.required");
    assert_eq!(name.roles(), ["key"]);
    assert_eq!(column("started").role(), binnacle_format::Role::Time);
    assert!(column("spec").nested());
    assert_eq!(
        column("host").reference(),
        Some(&Reference {
            resource: Some("hosts".into()),
            field: "metadata.name".into(),
        })
    );
    let host = source.column_key("host").unwrap();
    assert_eq!(
        source.property(host, "x-flotilla"),
        Some(&json!({"link": "hosts"})),
        "any other property, by name"
    );
    let hosts = replay("rows-hosts.ndjson");
    let labels = hosts.column_key("metadata.labels").unwrap();
    assert_eq!(
        hosts.property(labels, "additionalProperties").unwrap()["type"],
        "string"
    );
}

#[test]
fn a_value_that_does_not_match_its_type_is_invalid_and_counted() {
    let source = replay("crews.ndjson");
    let row = source.row_key(&[Wire::Text("charlie".into())]).unwrap();
    let cell = |name| source.cell(row, source.column_key(name).unwrap()).unwrap();
    assert_eq!(
        cell("started"),
        &Value::Invalid {
            raw: "soon".into(),
            expected: ValueType::DateTime,
        }
    );
    assert_eq!(
        cell("cpus"),
        &Value::Invalid {
            raw: "many".into(),
            expected: ValueType::Int,
        }
    );
    assert_eq!(source.mismatches(), 3, "and foxtrot's load");
    let mismatches: Vec<u64> = source.columns().iter().map(|c| c.mismatches()).collect();
    assert_eq!(mismatches, [0, 0, 1, 1, 1, 0, 0, 0]);
}

#[test]
fn values_are_read_once_against_their_declared_type() {
    let field = |t: FieldType| Field::new("f", t);
    let int = field(FieldType::Integer);
    assert_eq!(typed(&int, &Wire::Int(-3)), Value::Int(-3));
    assert_eq!(
        typed(&int, &Wire::Int(u64::MAX.into())),
        Value::UInt(u64::MAX)
    );
    assert!(matches!(
        typed(&int, &Wire::Int(i128::from(u64::MAX) + 1)),
        Value::Invalid { .. }
    ));
    assert!(matches!(
        typed(&int, &Wire::Float(1.5)),
        Value::Invalid { .. }
    ));
    let number = field(FieldType::Number);
    assert_eq!(typed(&number, &Wire::Int(2)), Value::Number(2.0));
    assert!(matches!(typed(&number, &Wire::Float(f64::NAN)), Value::Number(x) if x.is_nan()));
    let datetime = field(FieldType::Datetime);
    assert_eq!(
        typed(
            &datetime,
            &Wire::Text("2026-07-27T22:00:39.399401197Z".into())
        ),
        Value::DateTime(Timestamp::from_unix_nanos(1_785_189_639_399_401_197))
    );
    assert_eq!(typed(&datetime, &Wire::Null), Value::Null);
    let object = field(FieldType::Object);
    let tree = Wire::from_json(&json!({"a": [1, 2]}));
    assert_eq!(typed(&object, &tree), Value::Nested(json!({"a": [1, 2]})));
    assert_eq!(
        typed(&object, &Wire::Text("x".into())),
        Value::Invalid {
            raw: "x".into(),
            expected: ValueType::Nested,
        }
    );
    let array = field(FieldType::Array);
    assert_eq!(
        typed(&array, &Wire::Array(vec![])),
        Value::Nested(json!([]))
    );
    let boolean = field(FieldType::Boolean);
    assert_eq!(typed(&boolean, &Wire::Bool(true)), Value::Bool(true));
    let mut properties = Field::new("phase", FieldType::String).properties().clone();
    properties.insert(
        "categories".into(),
        json!(["Ready", {"value": "Gone", "label": "Retired"}]),
    );
    let phase = Field::from_properties(properties).unwrap();
    assert_eq!(
        typed(&phase, &Wire::Text("Gone".into())),
        Value::Category {
            value: "Gone".into(),
            label: Some("Retired".into()),
        }
    );
    assert!(matches!(
        typed(&phase, &Wire::Text("Lost".into())),
        Value::Invalid {
            expected: ValueType::Category,
            ..
        }
    ));
}

/// `messages` with its rows reversed and every row's fields given in
/// reverse, as another server might send them.
fn reversed(messages: Vec<Message>) -> Vec<Message> {
    (messages.into_iter())
        .map(|m| match m {
            Message::Rows(mut rows) => {
                rows.rows.reverse();
                Message::Rows(rows)
            }
            other => other,
        })
        .collect()
}

#[test]
fn a_primary_key_gives_the_same_row_key_in_every_source() {
    let first = replay("crews.ndjson");
    let mut second = DescribedSource::new("crews", Scope::new("flotilla"));
    for message in reversed(messages("crews.ndjson")) {
        second.receive(message);
    }
    for &key in first.row_keys() {
        let identity = first.row_identity(key).unwrap();
        assert_eq!(second.row_identity(key), Some(identity));
    }
    // A Key is interned from the deterministic CBOR of the primary-key
    // tuple: a one-item array (0x81) of a five-byte text (0x65).
    let alpha = first.row_key(&[Wire::Text("alpha".into())]).unwrap();
    assert_eq!(first.row_identity(alpha).unwrap(), b"\x81\x65alpha");
    // CBOR and JSON fixtures of the same table give the same Keys.
    let json = replay("rows-repositories.ndjson");
    let cbor = replay("rows-repositories.cborseq");
    assert_eq!(json.row_keys(), cbor.row_keys());
    insta::assert_debug_snapshot!(&first.row_keys()[..2]);
}

#[test]
fn a_row_or_column_that_leaves_and_comes_back_keeps_its_key() {
    let mut source = replay("crews.ndjson");
    let rows = source.row_keys().to_vec();
    let columns: Vec<_> = source.columns().iter().map(|c| c.key()).collect();
    let all = messages("crews.ndjson");
    let Message::Table(mut table) = all[0].clone() else {
        panic!("a table first");
    };
    let spec = table.schema.fields.pop().unwrap();
    let notices = source.receive(Message::Table(table.clone()));
    assert_eq!(source.count(Axis::Cols), 7);
    assert_eq!(source.row_count(), 6, "a table message is diffed");
    assert!(
        matches!(&notices, Received::Applied(n) if n.len() == 2),
        "{notices:?}"
    );
    // A resync that brings no rows back empties the table.
    source.receive(Message::Resync);
    source.receive(Message::Table(table.clone()));
    source.receive(Message::Synced { sequence: None });
    assert_eq!(source.row_count(), 0);
    table.schema.fields.push(spec);
    source.receive(Message::Resync);
    let notices = source.receive(Message::Table(table));
    assert!(
        matches!(&notices, Received::Applied(n) if n.len() == 2),
        "{notices:?}"
    );
    source.receive(all[1].clone());
    source.receive(Message::Synced { sequence: None });
    assert_eq!(source.row_keys(), rows);
    let again: Vec<_> = source.columns().iter().map(|c| c.key()).collect();
    assert_eq!(again, columns);
}

#[test]
fn deterministic_cbor_is_the_shortest_form_with_sorted_map_keys() {
    let map = Wire::from_json(&json!({"bb": 1, "a": -1, "c": [1.5, 1e300, 0.1, true, null]}));
    let bytes = deterministic_cbor(&map);
    let want: &[u8] = &[
        0xa3, // a map of 3, keys by their encoding: "a", "c", "bb"
        0x61, b'a', 0x20, // -1
        0x61, b'c', 0x85, // an array of 5
        0xf9, 0x3e, 0x00, // 1.5 as a half
        0xfb, 0x7e, 0x37, 0xe4, 0x3c, 0x88, 0x00, 0x75, 0x9c, // 1e300 as a double
        0xfb, 0x3f, 0xb9, 0x99, 0x99, 0x99, 0x99, 0x99, 0x9a, // 0.1 as a double
        0xf5, 0xf6, // true, null
        0x62, b'b', b'b', 0x01, // 1
    ];
    assert_eq!(bytes, want);
    assert_eq!(deterministic_cbor(&Wire::Int(500)), [0x19, 0x01, 0xf4]);
    assert_eq!(
        deterministic_cbor(&Wire::Float(f64::NAN)),
        [0xf9, 0x7e, 0x00]
    );
    assert_eq!(
        deterministic_cbor(&Wire::Float(100000.0)),
        [0xfa, 0x47, 0xc3, 0x50, 0x00]
    );
}

/// The crews table's `table` message, and a `rows` message of `rows`.
fn crews_with(rows: Vec<serde_json::Value>) -> (Message, Message) {
    let table = messages("crews.ndjson").remove(0);
    let rows = Message::from_json(&json!({"type": "rows", "rows": rows, "cursor": null})).unwrap();
    (table, rows)
}

#[test]
fn a_row_whose_primary_key_came_before_is_replaced_with_its_mismatches() {
    let (table, first) = crews_with(vec![
        json!({"name": "alpha", "cpus": "many", "load": "high"}),
    ]);
    let (_, second) = crews_with(vec![json!({"name": "alpha", "cpus": 3, "load": "high"})]);
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(table);
    source.receive(first);
    let key = source.row_keys()[0];
    assert_eq!(source.mismatches(), 2);
    source.receive(second);
    assert_eq!(source.row_keys(), [key], "one row, the same Key");
    assert_eq!(source.count(Axis::Rows), 1);
    let cpus = source.column_key("cpus").unwrap();
    assert_eq!(source.cell(key, cpus), Some(&Value::Int(3)));
    assert_eq!(source.mismatches(), 1, "only the load left invalid");
    let counts: Vec<u64> = source.columns().iter().map(|c| c.mismatches()).collect();
    assert_eq!(counts, [0, 0, 0, 0, 1, 0, 0, 0]);
}

#[test]
fn a_primary_key_field_the_schema_lacks_reads_as_null_in_the_tuple() {
    let Message::Table(mut table) = messages("crews.ndjson").remove(0) else {
        panic!("a table first");
    };
    table.schema.primary_key = vec!["name".into(), "ghost".into()];
    let (_, rows) = crews_with(vec![json!({"name": "alpha"}), json!({"name": "bravo"})]);
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table));
    source.receive(rows);
    assert_eq!(source.row_count(), 2);
    let alpha = source.row_keys()[0];
    assert_eq!(source.row_identity(alpha).unwrap(), b"\x82\x65alpha\xf6");
}

#[test]
fn the_status_keeps_the_incarnation_and_the_sequence_synced() {
    let source = replay("crews.ndjson");
    let status = source.status();
    assert_eq!(status.incarnation.as_deref(), Some("durable"));
    assert_eq!(status.sequence, Some(7));
}

#[test]
fn a_later_page_joins_the_rows_in_place_as_inserted_rows() {
    let all = messages("crews.ndjson");
    let Message::Rows(rows) = &all[1] else {
        panic!("rows second");
    };
    let page = |range: std::ops::Range<usize>| {
        let mut page = rows.clone();
        page.rows = page.rows[range].to_vec();
        Message::Rows(page)
    };
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(all[0].clone());
    source.receive(page(0..3));
    let name = source.column_key("name").unwrap();
    let spec = ViewSpec::default().with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::descending(name)]),
    );
    source.view(&spec);
    let notices = source.receive(page(3..6));
    assert!(
        matches!(&notices, Received::Applied(n) if n.iter().all(|(_, n)| matches!(n, ChangeNotice::Inserted { .. }))),
        "{notices:?}"
    );
    let count = source.count(Axis::Rows);
    let names: Vec<String> = (source.window(Axis::Rows, 0..count).keys.into_iter())
        .map(|k| match source.cell(k, name) {
            Some(Value::Text(t)) => t.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        names,
        ["foxtrot", "echo", "delta", "charlie", "bravo", "alpha"]
    );
}
