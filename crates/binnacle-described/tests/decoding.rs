//! Decoding: the two encodings of every recorded fixture decode to equal
//! messages, as does any generated message, whole or fed in pieces.

mod support;

use binnacle_described::{
    DecodeError, Decoder, Encoding, Message, Schema, decode_cbor_item, decode_json_line,
    encode_cbor_item, encode_json_line,
};
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

/// A CBOR head of `major` and argument `n`, in any width that holds `n`.
fn head(tc: &TestCase, major: u8, n: u64, out: &mut Vec<u8>) {
    let widths: Vec<usize> = ([0, 1, 2, 4, 8].into_iter())
        .filter(|&w| match w {
            0 => n < 24,
            w => u128::from(n) < 1 << (8 * w),
        })
        .collect();
    match tc.draw(gs::sampled_from(widths)) {
        0 => out.push(major << 5 | n as u8),
        w => {
            out.push(major << 5 | (24 + w.trailing_zeros() as u8));
            out.extend(&n.to_be_bytes()[8 - w..]);
        }
    }
}

/// `json` as CBOR in any valid form: heads wider than they need be, text
/// in chunks, arrays and maps of indefinite length, and tags.
/// Sets `tagged` if it writes a tag, which [`binnacle_described::Wire`]
/// does not read.
fn any_cbor(tc: &TestCase, json: &Json, out: &mut Vec<u8>, tagged: &mut bool) {
    if tc.draw(gs::integers::<u8>().max_value(7)) == 0 {
        let tag = tc.draw(gs::sampled_from(vec![32, 1000, 55799, 1 << 33]));
        head(tc, 6, tag, out);
        *tagged = true;
    }
    let indefinite = tc.draw(gs::booleans());
    match json {
        Json::Null => out.push(0xf6),
        Json::Bool(b) => out.push(0xf4 | u8::from(*b)),
        Json::Number(n) => match (n.as_u64(), n.as_i64()) {
            (Some(u), _) => head(tc, 0, u, out),
            (_, Some(i)) => head(tc, 1, (-1 - i) as u64, out),
            _ => {
                out.push(0xfb);
                out.extend(n.as_f64().unwrap().to_bits().to_be_bytes());
            }
        },
        Json::String(s) if indefinite => {
            out.push(0x7f);
            let mut rest = s.as_str();
            while !rest.is_empty() {
                let mut at = tc.draw(gs::integers::<usize>().max_value(rest.len()));
                while !rest.is_char_boundary(at) {
                    at += 1;
                }
                let (chunk, after) = rest.split_at(at);
                head(tc, 3, chunk.len() as u64, out);
                out.extend(chunk.as_bytes());
                rest = after;
            }
            out.push(0xff);
        }
        Json::String(s) => {
            head(tc, 3, s.len() as u64, out);
            out.extend(s.as_bytes());
        }
        Json::Array(items) => {
            match indefinite {
                true => out.push(0x9f),
                false => head(tc, 4, items.len() as u64, out),
            }
            for item in items {
                any_cbor(tc, item, out, tagged);
            }
            if indefinite {
                out.push(0xff);
            }
        }
        Json::Object(map) => {
            match indefinite {
                true => out.push(0xbf),
                false => head(tc, 5, map.len() as u64, out),
            }
            for (key, value) in map {
                any_cbor(tc, &Json::String(key.clone()), out, tagged);
                any_cbor(tc, value, out, tagged);
            }
            if indefinite {
                out.push(0xff);
            }
        }
    }
}

/// Every message the decoder gives for `pieces` fed one after another,
/// errors included, then what it gives at the end.
fn feed_all<'a>(
    mut decoder: Decoder,
    pieces: impl IntoIterator<Item = &'a [u8]>,
) -> Vec<Result<Message, DecodeError>> {
    let mut messages = Vec::new();
    for piece in pieces {
        decoder.feed(piece);
        messages.extend(std::iter::from_fn(|| decoder.next()));
    }
    messages.extend(decoder.finish());
    messages
}

#[hegel::test(test_cases = 500)]
fn any_stream_cut_anywhere_decodes_as_it_does_whole(tc: TestCase) {
    let jsons = tc.draw(gs::vecs(message()).min_size(1).max_size(4));
    let encoding = tc.draw(gs::sampled_from(vec![Encoding::Ndjson, Encoding::CborSeq]));
    let mut bytes = Vec::new();
    let mut tagged = false;
    for (i, json) in jsons.iter().enumerate() {
        match encoding {
            Encoding::Ndjson => {
                bytes.extend(serde_json::to_vec(json).unwrap());
                if i + 1 < jsons.len() || tc.draw(gs::booleans()) {
                    bytes.push(b'\n');
                }
                if i + 1 < jsons.len() && tc.draw(gs::booleans()) {
                    bytes.extend(b" \n");
                }
            }
            Encoding::CborSeq => any_cbor(&tc, json, &mut bytes, &mut tagged),
        }
    }
    let mut cuts = tc.draw(gs::vecs(gs::integers::<usize>().max_value(bytes.len())).max_size(8));
    cuts.extend([0, bytes.len()]);
    cuts.sort();
    let pieces = cuts.windows(2).map(|w| &bytes[w[0]..w[1]]);
    let whole = feed_all(Decoder::new(encoding), [bytes.as_slice()]);
    let expected: Vec<_> = jsons.iter().map(Message::from_json).collect();
    if !tagged {
        assert_eq!(whole, expected);
    }
    assert_eq!(whole.len(), expected.len());
    assert_eq!(feed_all(Decoder::new(encoding), pieces), whole);
}

/// A `later` message whose encoding is `len` bytes long, its line break
/// left off.
fn message_of_len(encoding: Encoding, len: usize) -> (Message, Vec<u8>) {
    let encode = |pad: usize| {
        let json = json!({"type": "later", "pad": "x".repeat(pad)});
        let message = Message::from_json(&json).unwrap();
        let bytes = match encoding {
            Encoding::Ndjson => serde_json::to_vec(&json).unwrap(),
            Encoding::CborSeq => encode_cbor_item(&message),
        };
        (message, bytes)
    };
    let (_, bare) = encode(0);
    let mut pad = len - bare.len();
    loop {
        let (message, bytes) = encode(pad);
        if bytes.len() == len {
            return (message, bytes);
        }
        pad -= bytes.len() - len;
    }
}

#[test]
fn an_item_as_long_as_the_maximum_decodes_and_one_longer_is_an_error() {
    for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
        for len in [30, 200, 70_000] {
            let (message, mut bytes) = message_of_len(encoding, len);
            if encoding == Encoding::Ndjson {
                bytes.push(b'\n');
            }
            let decoded = feed_all(Decoder::new(encoding).with_max_item(len), [&bytes[..]]);
            assert_eq!(decoded, [Ok(message)], "{encoding:?} {len}");
            let decoded = feed_all(Decoder::new(encoding).with_max_item(len - 1), [&bytes[..]]);
            assert!(matches!(decoded[..], [Err(_)]), "{encoding:?} {len}");
        }
    }
}

#[test]
fn an_item_past_the_maximum_arriving_slowly_is_an_error_as_it_passes_it() {
    for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
        let (first, first_bytes) = message_of_len(encoding, 40);
        let (_, long) = message_of_len(encoding, 500);
        let (_, after) = message_of_len(encoding, 40);
        let mut decoder = Decoder::new(encoding).with_max_item(100);
        let mut stream = Vec::new();
        for bytes in [first_bytes, long, after] {
            stream.extend(bytes);
            if encoding == Encoding::Ndjson {
                stream.push(b'\n');
            }
        }
        let mut decoded = Vec::new();
        let mut failed_at = None;
        for (i, b) in stream.iter().enumerate() {
            decoder.feed(std::slice::from_ref(b));
            while let Some(message) = decoder.next() {
                if message.is_err() {
                    failed_at.get_or_insert(i);
                }
                decoded.push(message);
            }
        }
        decoded.extend(decoder.finish());
        assert_eq!(decoded.len(), 2, "{encoding:?}: nothing after the error");
        assert_eq!(decoded[0], Ok(first), "{encoding:?}");
        assert!(decoded[1].is_err(), "{encoding:?}");
        // The pending item fails on the byte that takes it past 100, not
        // when it ends: for CBOR its text's head declares it too long.
        let start = 40 + usize::from(encoding == Encoding::Ndjson);
        let failed_at = failed_at.unwrap() - start;
        match encoding {
            Encoding::Ndjson => assert_eq!(failed_at, 100, "{encoding:?}"),
            Encoding::CborSeq => assert!(failed_at < 20, "{encoding:?}: {failed_at}"),
        }
    }
}

#[test]
fn a_cbor_head_declaring_a_length_past_the_maximum_is_an_error_at_once() {
    let heads: [&[u8]; 4] = [
        // Text of 2^40 bytes.
        &[0x7b, 0, 0, 0x01, 0, 0, 0, 0, 0],
        // Bytes that with their head make 16 MiB and one.
        &[0x5a, 0x00, 0xff, 0xff, 0xfc],
        // An array of 2^32 items, each at least a byte.
        &[0x9b, 0, 0, 0, 0x01, 0, 0, 0, 0],
        // A map of 2^63 pairs, within an indefinite array.
        &[0x9f, 0xbb, 0x80, 0, 0, 0, 0, 0, 0, 0],
    ];
    for head in heads {
        let mut decoder = Decoder::new(Encoding::CborSeq);
        decoder.feed(head);
        assert!(matches!(decoder.next(), Some(Err(_))), "{head:x?}");
        decoder.feed(&encode_cbor_item(&Message::Resync));
        assert_eq!(decoder.next(), None, "{head:x?}: nothing after it");
        assert_eq!(decoder.finish(), None, "{head:x?}");
    }
    let mut decoder = Decoder::new(Encoding::CborSeq);
    decoder.feed(&[0x5a, 0x00, 0xff, 0xff, 0xfb]);
    assert_eq!(decoder.next(), None, "16 MiB in all may come");
}

#[test]
fn a_large_cbor_item_fed_a_byte_at_a_time_is_read_in_linear_work() {
    let rows: Vec<Json> = (0..5_000)
        .map(|i| json!({"id": i, "name": format!("row-{i}"), "load": i as f64 / 3.0, "spec": {"tags": [i, "x"]}}))
        .collect();
    let json = json!({"type": "rows", "rows": rows, "cursor": null});
    for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
        let bytes = match encoding {
            Encoding::Ndjson => encode_json_line(&Message::from_json(&json).unwrap()),
            Encoding::CborSeq => encode_cbor_item(&Message::from_json(&json).unwrap()),
        };
        assert!(bytes.len() > 100_000);
        let mut decoder = Decoder::new(encoding);
        let mut decoded = 0;
        for b in &bytes {
            decoder.feed(std::slice::from_ref(b));
            while let Some(message) = decoder.next() {
                message.unwrap();
                decoded += 1;
            }
        }
        assert_eq!(decoded, 1, "{encoding:?}");
        let read = decoder.bytes_read();
        assert!(
            read <= 3 * bytes.len() as u64,
            "{encoding:?}: {read} for {}",
            bytes.len()
        );
    }
}
