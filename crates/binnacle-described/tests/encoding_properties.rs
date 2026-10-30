//! Encoding round trips: any message written as an NDJSON line or a CBOR
//! item reads back as itself, fields' properties the crate does not model
//! included.

use std::collections::BTreeMap;

use binnacle_described::{
    Field, ForeignKey, Message, RowsMessage, Schema, TableMessage, Wire, decode_cbor_item,
    decode_json_line, encode_cbor_item, encode_json_line,
};
use hegel::generators as gs;
use hegel::{Generator, TestCase};
use serde_json::{Map, Value as Json, json};

/// A scalar either encoding carries exactly; with `cbor`, byte strings,
/// infinities and integers past `u64` and `i64` too.
#[hegel::composite]
fn scalar(tc: &TestCase, cbor: bool) -> Wire {
    match tc.draw(gs::integers::<u8>().max_value(if cbor { 7 } else { 5 })) {
        0 => Wire::Null,
        1 => Wire::Bool(tc.draw(gs::booleans())),
        2 => Wire::Int(tc.draw(hegel::one_of!(
            gs::integers::<i64>().map(i128::from),
            gs::integers::<u64>().map(i128::from),
        ))),
        3 => Wire::Float(tc.draw(gs::floats::<f64>().allow_nan(false).allow_infinity(cbor))),
        4 => Wire::Text(tc.draw(gs::text().max_size(6))),
        5 => Wire::Text(
            tc.draw(gs::sampled_from(vec!["", "a\nb", "\"", "é"]))
                .to_owned(),
        ),
        6 => Wire::Bytes(tc.draw(gs::vecs(gs::integers::<u8>()).max_size(4))),
        _ => Wire::Int(tc.draw(gs::sampled_from(vec![
            i128::from(u64::MAX) + 1,
            i128::from(i64::MIN) - 1,
        ]))),
    }
}

#[hegel::composite]
fn value(tc: &TestCase, cbor: bool) -> Wire {
    match tc.draw(gs::integers::<u8>().max_value(5)) {
        0 => Wire::Array(tc.draw(gs::vecs(scalar(cbor)).max_size(3))),
        1 => Wire::Map(tc.draw(map(cbor))),
        _ => tc.draw(scalar(cbor)),
    }
}

#[hegel::composite]
fn map(tc: &TestCase, cbor: bool) -> BTreeMap<String, Wire> {
    let names = tc.draw(gs::vecs(gs::sampled_from(vec!["a", "b", "c.d", "type"])).max_size(3));
    (names.into_iter())
        .map(|n| (n.to_owned(), tc.draw(scalar(cbor))))
        .collect()
}

#[hegel::composite]
fn field(tc: &TestCase, name: String) -> Field {
    let mut properties = Map::new();
    properties.insert("name".into(), Json::String(name));
    let kind = tc.draw(gs::sampled_from(vec![
        "string", "integer", "number", "boolean", "datetime", "object", "array", "duration",
    ]));
    properties.insert("type".into(), json!(kind));
    if tc.draw(gs::booleans()) {
        properties.insert("title".into(), json!(tc.draw(gs::text().max_size(4))));
    }
    if tc.draw(gs::booleans()) {
        properties.insert("roles".into(), json!(["key", "time"]));
    }
    if tc.draw(gs::booleans()) {
        properties.insert(
            "constraints".into(),
            json!({"required": tc.draw(gs::booleans())}),
        );
    }
    if tc.draw(gs::booleans()) {
        properties.insert(
            "categories".into(),
            json!(["a", {"value": "b", "label": "B"}]),
        );
    }
    if tc.draw(gs::booleans()) {
        let extra = tc.draw(gs::integers::<i64>());
        properties.insert("x-extra".into(), json!({"n": extra, "items": [1.5, null]}));
    }
    Field::from_properties(properties).unwrap()
}

#[hegel::composite]
fn message(tc: &TestCase, cbor: bool) -> Message {
    let key = || gs::vecs(scalar(cbor)).min_size(1).max_size(2);
    let sequence = || gs::optional(gs::integers::<u64>());
    match tc.draw(gs::integers::<u8>().max_value(8)) {
        0 => {
            let n = tc.draw(gs::integers::<usize>().max_value(3));
            let fields: Vec<Field> = (0..n).map(|i| tc.draw(field(format!("f{i}")))).collect();
            let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            let foreign_keys = match names.first() {
                Some(first) if tc.draw(gs::booleans()) => vec![ForeignKey {
                    fields: vec![first.clone()],
                    resource: tc.draw(gs::optional(gs::sampled_from(vec!["hosts".to_owned()]))),
                    reference_fields: vec!["metadata.name".into()],
                }],
                _ => Vec::new(),
            };
            Message::Table(TableMessage {
                schema: Schema {
                    fields,
                    primary_key: names.into_iter().take(1).collect(),
                    foreign_keys,
                },
                incarnation: tc.draw(gs::optional(gs::sampled_from(vec!["durable".to_owned()]))),
                table: tc.draw(gs::optional(gs::text().max_size(4))),
                sequence: tc.draw(sequence()),
            })
        }
        1 => Message::Rows(RowsMessage {
            rows: tc.draw(gs::vecs(map(cbor)).max_size(3)),
            cursor: tc.draw(gs::optional(gs::text().max_size(4))),
            partial: tc.draw(gs::booleans()),
            sequence: tc.draw(sequence()),
        }),
        2 => Message::Synced {
            sequence: tc.draw(sequence()),
        },
        3 => Message::Upsert {
            key: tc.draw(key()),
            row: tc.draw(map(cbor)),
        },
        4 => Message::Delete {
            key: tc.draw(key()),
        },
        5 => Message::Progress {
            sequence: tc.draw(sequence()),
        },
        6 => Message::Resync,
        7 => Message::Count {
            count: tc.draw(gs::integers::<u64>()),
            estimate: tc.draw(gs::booleans()),
        },
        _ => {
            // Kept whole, its `type` included.
            let kind = tc.draw(gs::sampled_from(vec![
                "news".to_owned(),
                "hello".to_owned(),
            ]));
            let mut body = tc.draw(map(cbor));
            body.insert("type".into(), Wire::Text(kind.clone()));
            body.insert("nested".into(), tc.draw(value(cbor)));
            Message::Other {
                kind,
                body: Wire::Map(body),
            }
        }
    }
}

#[hegel::test(test_cases = 1000)]
fn a_message_written_as_a_json_line_reads_back_as_itself(tc: TestCase) {
    let message = tc.draw(message(false));
    let line = encode_json_line(&message);
    assert_eq!(line.last(), Some(&b'\n'));
    assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1, "one line");
    assert_eq!(decode_json_line(&line), Ok(message));
}

#[hegel::test(test_cases = 1000)]
fn a_message_written_as_a_cbor_item_reads_back_as_itself(tc: TestCase) {
    let message = tc.draw(message(true));
    assert_eq!(decode_cbor_item(&encode_cbor_item(&message)), Ok(message));
}

#[test]
fn messages_are_written_under_the_wires_own_names() {
    let line = |m: &Message| {
        let json: Json = serde_json::from_slice(&encode_json_line(m)).unwrap();
        let mut keys: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys.join(",")
    };
    let rows = Message::Rows(RowsMessage {
        rows: Vec::new(),
        cursor: None,
        partial: false,
        sequence: None,
    });
    assert_eq!(line(&rows), "cursor,partial,rows,type");
    let count = Message::Count {
        count: 3,
        estimate: true,
    };
    assert_eq!(line(&count), "count,estimate,type");
    let table = Message::Table(TableMessage {
        schema: Schema {
            fields: Vec::new(),
            primary_key: vec!["id".into()],
            foreign_keys: Vec::new(),
        },
        incarnation: Some("durable".into()),
        table: None,
        sequence: None,
    });
    assert_eq!(line(&table), "incarnation,schema,type");
    let Message::Table(table) = table else {
        unreachable!()
    };
    assert_eq!(
        table.schema.to_json(),
        json!({"fields": [], "primaryKey": ["id"], "foreignKeys": []})
    );
}
