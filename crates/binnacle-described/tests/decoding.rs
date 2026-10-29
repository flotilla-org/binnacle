//! Decoding: the two encodings of every recorded fixture decode to equal
//! messages, as does any generated message, whole or fed in pieces.

mod support;

use binnacle_described::{Decoder, Encoding, Message, Schema, decode_cbor_item, decode_json_line};
use hegel::generators as gs;
use hegel::{Generator, TestCase};
use serde_json::{Value as Json, json};
use support::{decode, fixtures, messages, path};

#[test]
fn each_described_schema_decodes_the_same_from_json_and_cbor() {
    let names = fixtures("desc-", &[".json"]);
    assert_eq!(names.len(), 5);
    for name in names {
        let json = std::fs::read(path(&name)).unwrap();
        let cbor = std::fs::read(path(&name.replace(".json", ".cbor"))).unwrap();
        let schema = Schema::from_json_bytes(&json).unwrap();
        assert_eq!(schema, Schema::from_cbor_bytes(&cbor).unwrap(), "{name}");
        assert_eq!(schema.primary_key, ["metadata.name"]);
    }
}

#[test]
fn each_rows_fixture_decodes_the_same_from_ndjson_and_cbor_seq() {
    let names = fixtures("rows-", &[".cborseq"]);
    assert_eq!(names.len(), 4);
    for name in names {
        let cbor = messages(&name);
        let json = messages(&name.replace(".cborseq", ".ndjson"));
        assert!(cbor.len() >= 3, "{name}: table, rows, synced");
        assert_eq!(json, cbor, "{name}");
    }
}

#[test]
fn a_decoder_fed_a_byte_at_a_time_gives_the_same_messages() {
    for name in ["rows-hosts.ndjson", "rows-hosts.cborseq"] {
        let bytes = std::fs::read(path(name)).unwrap();
        let mut decoder = Decoder::new(support::encoding(name));
        let mut fed = Vec::new();
        for b in &bytes {
            decoder.feed(std::slice::from_ref(b));
            while let Some(message) = decoder.next() {
                fed.push(message.unwrap());
            }
        }
        assert!(decoder.finish().is_none());
        assert_eq!(fed, messages(name), "{name}");
    }
}

#[test]
fn an_ndjson_stream_may_end_without_a_line_break() {
    let messages = decode(
        Encoding::Ndjson,
        b"{\"type\":\"resync\"}\n\n{\"type\":\"synced\"}",
    );
    assert_eq!(
        messages,
        [Message::Resync, Message::Synced { sequence: None }]
    );
}

#[test]
fn a_bad_message_is_an_error_and_the_stream_goes_on() {
    let mut decoder = Decoder::new(Encoding::Ndjson);
    decoder.feed(b"{\"type\":\"rows\"}\nnot json\n{\"type\":\"resync\"}\n");
    assert!(decoder.next().unwrap().is_err(), "rows without rows");
    assert!(decoder.next().unwrap().is_err());
    assert_eq!(decoder.next().unwrap(), Ok(Message::Resync));
}

/// A JSON value for a row's cell: any JSON, a few levels deep.
#[hegel::composite]
fn json_value(tc: &TestCase, depth: u32) -> Json {
    let scalar = hegel::one_of!(
        gs::just(Json::Null),
        gs::booleans().map(Json::Bool),
        gs::integers::<i64>().map(Json::from),
        gs::integers::<u64>().map(Json::from),
        gs::floats::<f64>()
            .allow_nan(false)
            .allow_infinity(false)
            .map(Json::from),
        gs::text().max_size(6).map(Json::String),
        gs::sampled_from(vec!["2026-07-27T22:00:39.399401197Z", "", "é", "\u{10000}"])
            .map(|s| Json::String(s.into())),
    );
    if depth == 0 || !tc.draw(gs::booleans()) {
        return tc.draw(scalar);
    }
    if tc.draw(gs::booleans()) {
        Json::Array(tc.draw(gs::vecs(json_value(depth - 1)).max_size(3)))
    } else {
        let keys = tc.draw(gs::vecs(gs::text().max_size(4)).max_size(3));
        let map = (keys.into_iter())
            .map(|k| (k, tc.draw(json_value(depth - 1))))
            .collect();
        Json::Object(map)
    }
}

#[hegel::composite]
fn row(tc: &TestCase) -> Json {
    let names = tc.draw(gs::vecs(gs::sampled_from(vec!["a", "b", "c.d", ""])).max_size(4));
    let map = (names.into_iter())
        .map(|n| (n.to_string(), tc.draw(json_value(2))))
        .collect();
    Json::Object(map)
}

#[hegel::composite]
fn message(tc: &TestCase) -> Json {
    let sequence = tc.draw(gs::integers::<u64>());
    let key = || gs::vecs(json_value(0)).min_size(1).max_size(2);
    match tc.draw(gs::integers::<u8>().max_value(7)) {
        0 => {
            let types = vec![
                "string", "integer", "number", "datetime", "object", "geopoint",
            ];
            let fields: Vec<Json> = (0..tc.draw(gs::integers::<usize>().max_value(3)))
                .map(|i| {
                    json!({
                        "name": format!("f{i}"),
                        "type": tc.draw(gs::sampled_from(types.clone())),
                        "title": tc.draw(gs::optional(gs::text().max_size(4))),
                        "roles": tc.draw(gs::vecs(gs::sampled_from(vec!["key", "time", "state"])).max_size(2)),
                        "constraints": {"required": tc.draw(gs::booleans())},
                        "extra": tc.draw(json_value(1)),
                    })
                })
                .collect();
            json!({"type": "table", "sequence": sequence, "incarnation": "durable",
                "schema": {"fields": fields, "primaryKey": ["f0"], "foreignKeys": []}})
        }
        1 => json!({"type": "rows", "sequence": sequence,
            "rows": tc.draw(gs::vecs(row()).max_size(3)),
            "cursor": tc.draw(gs::optional(gs::text().max_size(8))),
            "partial": tc.draw(gs::booleans())}),
        2 => json!({"type": "synced", "sequence": sequence}),
        3 => {
            json!({"type": "upsert", "sequence": sequence, "key": tc.draw(key()), "row": tc.draw(row())})
        }
        4 => json!({"type": "delete", "sequence": sequence, "key": tc.draw(key())}),
        5 => json!({"type": "progress", "sequence": sequence}),
        6 => json!({"type": "count", "count": sequence, "estimate": tc.draw(gs::booleans())}),
        _ => {
            json!({"type": tc.draw(gs::sampled_from(vec!["resync", "later"])), "x": tc.draw(json_value(1))})
        }
    }
}

#[hegel::test(test_cases = 500)]
fn any_message_decodes_the_same_from_json_and_cbor(tc: TestCase) {
    let message = tc.draw(message());
    let json = serde_json::to_vec(&message).unwrap();
    let mut cbor = Vec::new();
    ciborium::into_writer(&message, &mut cbor).unwrap();
    let from_json = decode_json_line(&json).unwrap();
    assert_eq!(from_json, decode_cbor_item(&cbor).unwrap());
    assert_eq!(from_json, Message::from_json(&message).unwrap());
    let mut decoder = Decoder::new(Encoding::CborSeq);
    let (first, rest) = cbor.split_at(cbor.len() / 2);
    decoder.feed(first);
    if !rest.is_empty() {
        assert!(decoder.next().is_none(), "half an item is not a message");
    }
    decoder.feed(rest);
    assert_eq!(decoder.next().unwrap().unwrap(), from_json);
}

#[test]
fn a_schema_naming_a_field_twice_is_an_error() {
    let schema =
        json!({"fields": [{"name": "a", "type": "string"}, {"name": "a", "type": "integer"}]});
    assert!(Schema::from_json(&schema).is_err());
}
