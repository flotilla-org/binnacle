//! The fake table server: what it serves is shaped like flotillad's
//! recorded answers, and a watch plays its script to a Described Table
//! source.

use std::path::PathBuf;
use std::time::Duration;

use binnacle_described::{
    Decoder, DescribedSource, Encoding, Field, FieldType, HostRequest, Message, Received, Scope,
    Wire,
};
use binnacle_testkit::fake_tables::{FakeTable, FakeTables, Running, Step, Stream, fetch, open};
use serde_json::{Value as Json, json};

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../binnacle-described/tests/data")
        .join(name)
}

fn crews() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../binnacle-described/tests/fixtures/crews.ndjson")
}

const NDJSON: &str = "application/x-ndjson";
const CBOR_SEQ: &str = "application/cbor-seq";

/// Each raw item of a rows stream, as maps.
fn items(encoding: Encoding, bytes: &[u8]) -> Vec<Wire> {
    match encoding {
        Encoding::Ndjson => (bytes.split(|&b| b == b'\n'))
            .filter(|l| !l.is_empty())
            .map(|l| Wire::from_json(&serde_json::from_slice::<Json>(l).unwrap()))
            .collect(),
        Encoding::CborSeq => {
            let mut reader = bytes;
            let mut items = Vec::new();
            while !reader.is_empty() {
                items.push(ciborium::from_reader::<Wire, _>(&mut reader).unwrap());
            }
            items
        }
    }
}

fn keys(wire: &Wire) -> Vec<String> {
    wire.as_map().unwrap().keys().cloned().collect()
}

/// Each item's type, keys, its schema's and fields' keys, and its rows'.
fn shape(items: &[Wire]) -> Vec<String> {
    let mut shape = Vec::new();
    for item in items {
        let map = item.as_map().unwrap();
        shape.push(format!(
            "{}: {:?}",
            map["type"].as_str().unwrap(),
            keys(item)
        ));
        if let Some(schema) = map.get("schema") {
            shape.push(format!("  schema {:?}", keys(schema)));
            let Some(Wire::Array(fields)) = schema.as_map().unwrap().get("fields") else {
                panic!("fields")
            };
            for field in fields {
                shape.push(format!("  field {:?}", keys(field)));
            }
        }
        if let Some(Wire::Array(rows)) = map.get("rows") {
            for row in rows {
                shape.push(format!("  row {:?}", keys(row)));
            }
        }
    }
    shape
}

#[test]
fn each_fixture_table_is_served_as_flotillad_recorded_it_in_both_encodings() {
    let mut names: Vec<String> = std::fs::read_dir(data(""))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("rows-") && (n.ends_with(".ndjson") || n.ends_with(".cborseq")))
        // The fake gives no `count` messages.
        .filter(|n| !n.contains("-count-"))
        .collect();
    names.sort();
    assert_eq!(names.len(), 11);
    for name in names {
        let table = FakeTable::from_fixture(data(&name)).unwrap();
        let fixture_encoding = match name.ends_with(".cborseq") {
            true => Encoding::CborSeq,
            false => Encoding::Ndjson,
        };
        let want = shape(&items(
            fixture_encoding,
            &std::fs::read(data(&name)).unwrap(),
        ));
        let fake = FakeTables::new().table(table.clone()).start().unwrap();
        for encoding in [Encoding::Ndjson, Encoding::CborSeq] {
            let target = format!("/tables/{}/rows?namespace=flotilla", table.name);
            let response =
                fetch(fake.addr(), &target, &[("accept", encoding.content_type())]).unwrap();
            assert_eq!(response.status, 200);
            assert_eq!(
                response.header("content-type"),
                Some(encoding.content_type())
            );
            assert_eq!(response.header("transfer-encoding"), Some("chunked"));
            assert_eq!(
                shape(&items(encoding, &response.body)),
                want,
                "{name} as {encoding:?}"
            );
        }
    }
}

#[test]
fn a_description_and_the_listing_are_shaped_as_recorded() {
    let fake = FakeTables::new()
        .fixture(data("rows-hosts.ndjson"))
        .unwrap()
        .start()
        .unwrap();
    let json = fetch(
        fake.addr(),
        "/tables/hosts",
        &[("accept", "application/json")],
    )
    .unwrap();
    let recorded: Json =
        serde_json::from_slice(&std::fs::read(data("desc-hosts.json")).unwrap()).unwrap();
    assert_eq!(
        serde_json::from_slice::<Json>(&json.body).unwrap(),
        recorded
    );
    let cbor = fetch(
        fake.addr(),
        "/tables/hosts",
        &[("accept", "application/cbor")],
    )
    .unwrap();
    assert_eq!(cbor.header("content-type"), Some("application/cbor"));
    let schema = ciborium::from_reader::<Wire, _>(&cbor.body[..]).unwrap();
    assert_eq!(schema.to_json(), recorded);
    let listing = fetch(fake.addr(), "/tables", &[]).unwrap();
    let listing: Json = serde_json::from_slice(&listing.body).unwrap();
    assert_eq!(
        listing,
        json!({"kinds": [{"kind": "Host", "root": "hosts", "tables": ["hosts"]}]})
    );
}

#[test]
fn a_request_without_a_namespace_reads_default_an_empty_table() {
    let fake = FakeTables::new()
        .fixture(data("rows-hosts.ndjson"))
        .unwrap()
        .start()
        .unwrap();
    let response = fetch(fake.addr(), "/tables/hosts/rows", &[("accept", NDJSON)]).unwrap();
    assert_eq!(response.status, 200);
    let items = items(Encoding::Ndjson, &response.body);
    let rows = &items[1].as_map().unwrap()["rows"];
    assert_eq!(*rows, Wire::Array(Vec::new()));
}

#[test]
fn an_unknown_table_is_a_404_and_a_bad_where_a_400_shaped_as_recorded() {
    let fake = FakeTables::new()
        .fixture(data("rows-hosts.ndjson"))
        .unwrap()
        .start()
        .unwrap();
    let body_keys = |bytes: &[u8]| -> Vec<String> {
        let json: Json = serde_json::from_slice(bytes).unwrap();
        json.as_object().unwrap().keys().cloned().collect()
    };
    let missing = fetch(
        fake.addr(),
        "/tables/nosuchtable",
        &[("accept", "application/json")],
    )
    .unwrap();
    assert_eq!(missing.status, 404);
    let recorded = std::fs::read(data("error-unknown-table.json")).unwrap();
    assert_eq!(missing.body, recorded, "the same body");
    let rows = fetch(
        fake.addr(),
        "/tables/nosuchtable/rows?namespace=flotilla",
        &[],
    )
    .unwrap();
    assert_eq!(rows.status, 404);
    let bad = fetch(
        fake.addr(),
        "/tables/hosts/rows?namespace=flotilla&where=garbage",
        &[],
    )
    .unwrap();
    assert_eq!(bad.status, 400);
    let recorded = std::fs::read(data("error-bad-where.json")).unwrap();
    assert_eq!(body_keys(&bad.body), body_keys(&recorded));
    assert_eq!(bad.body, recorded, "flotillad's very message");
    let good = fetch(
        fake.addr(),
        "/tables/hosts/rows?namespace=flotilla&where=.metadata.name%20%3D%3D%20nobody",
        &[],
    )
    .unwrap();
    assert_eq!(good.status, 200);
}

#[test]
fn a_where_clause_reads_a_field_or_into_an_object() {
    let fake = FakeTables::new().generate("big", 30).start().unwrap();
    let count = |clause: &str| {
        let target = format!(
            "/tables/big/rows?namespace=flotilla&where={}",
            clause.replace(' ', "%20").replace('=', "%3D")
        );
        let response = fetch(fake.addr(), &target, &[]).unwrap();
        assert_eq!(response.status, 200, "{clause}");
        let items = items(Encoding::Ndjson, &response.body);
        let Wire::Array(rows) = &items[1].as_map().unwrap()["rows"] else {
            panic!()
        };
        rows.len()
    };
    assert_eq!(count(".phase == Done"), 6);
    assert_eq!(count(".spec.image == zig"), 10);
    assert_eq!(count(".spec.replicas == 3"), 7);
    assert_eq!(count(".id == 4"), 1);
}

#[test]
fn a_read_pages_with_an_opaque_cursor() {
    let fake = FakeTables::new().generate("big", 450).start().unwrap();
    let mut target = "/tables/big/rows?namespace=flotilla".to_owned();
    let mut seen = 0;
    let mut pages = 0;
    loop {
        let response = fetch(fake.addr(), &target, &[]).unwrap();
        let items = items(Encoding::Ndjson, &response.body);
        let types: Vec<&str> = items
            .iter()
            .map(|i| i.as_map().unwrap()["type"].as_str().unwrap())
            .collect();
        assert_eq!(types, ["table", "rows", "synced"]);
        let rows = items[1].as_map().unwrap();
        let Wire::Array(page) = &rows["rows"] else {
            panic!()
        };
        seen += page.len();
        pages += 1;
        match &rows["cursor"] {
            Wire::Text(cursor) => {
                assert_eq!(rows["partial"], Wire::Bool(true));
                target = format!("/tables/big/rows?namespace=flotilla&cursor={cursor}");
            }
            _ => {
                assert_eq!(rows["partial"], Wire::Bool(true), "a cursor was given");
                break;
            }
        }
    }
    assert_eq!((seen, pages), (450, 3));
}

#[test]
fn a_generated_table_has_nulls_empty_text_invalid_values_datetimes_and_objects() {
    let table = FakeTable::generated("big", 40);
    let mut source = DescribedSource::new("big", Scope::new("flotilla"));
    source.receive(Message::Table(binnacle_described::TableMessage {
        schema: table.schema.clone(),
        incarnation: None,
        table: None,
        sequence: None,
    }));
    source.receive(Message::Rows(binnacle_described::RowsMessage {
        rows: table.rows.clone(),
        cursor: None,
        partial: false,
        sequence: None,
    }));
    assert_eq!(source.row_count(), 40);
    assert!(source.mismatches() > 0, "invalid values");
    let types: Vec<&FieldType> = source.columns().iter().map(|c| c.field_type()).collect();
    assert!(types.contains(&&FieldType::Datetime) && types.contains(&&FieldType::Object));
    let values = || table.rows.iter().flat_map(|r| r.values());
    assert!(values().any(Wire::is_null));
    assert!(values().any(|v| *v == Wire::Text(String::new())));
}

/// Feeds each message `stream` sends to `source` until `until` holds of
/// it or the stream ends; returns how many messages failed to decode, and
/// whether the stream ended.
fn feed(
    stream: &mut Stream,
    decoder: &mut Decoder,
    source: &mut DescribedSource,
    until: impl Fn(&DescribedSource) -> bool,
) -> (usize, bool) {
    let mut undecodable = 0;
    loop {
        while let Some(message) = decoder.next() {
            match message {
                Ok(message) => {
                    assert!(matches!(source.receive(message), Received::Applied(_)));
                }
                Err(_) => undecodable += 1,
            }
            if until(source) {
                return (undecodable, false);
            }
        }
        match stream.next_chunk().unwrap() {
            Some(chunk) => decoder.feed(&chunk),
            None => return (undecodable, true),
        }
    }
}

fn watch(fake: &Running, source: &mut DescribedSource, accept: &str) -> (Stream, Decoder) {
    let Some(HostRequest::OpenWatch(request)) = source.poll_request() else {
        panic!("a watch first")
    };
    let stream = open(fake.addr(), &request.target(), &[("accept", accept)]).unwrap();
    assert_eq!(stream.status, 200);
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let encoding = Encoding::from_content_type(stream.header("content-type").unwrap()).unwrap();
    (stream, Decoder::new(encoding))
}

#[test]
fn a_watch_plays_the_script_to_a_described_source() {
    for accept in [NDJSON, CBOR_SEQ] {
        let fake = FakeTables::new()
            .fixture(crews())
            .unwrap()
            .heartbeat(Duration::from_millis(30))
            .start()
            .unwrap();
        let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
        let (mut stream, mut decoder) = watch(&fake, &mut source, accept);
        feed(&mut stream, &mut decoder, &mut source, |s| {
            s.status().synced
        });
        assert_eq!(source.row_count(), 6);
        let text = |s: &str| Wire::Text(s.into());
        let mut alpha = fake.table("crews").unwrap().rows[0].clone();
        alpha.insert("load".into(), Wire::Float(9.5));
        let golf = [
            ("name".to_owned(), text("golf")),
            ("cpus".to_owned(), Wire::Int(1)),
        ]
        .into();
        let zone = Field::new("zone", FieldType::String);
        for step in [
            Step::Upsert {
                table: "crews".into(),
                row: alpha,
            },
            Step::Delete {
                table: "crews".into(),
                key: vec![text("bravo")],
            },
            Step::Upsert {
                table: String::new(),
                row: golf,
            },
            Step::Garbage {
                table: "crews".into(),
            },
            Step::Resync {
                table: "crews".into(),
            },
            Step::AddField {
                table: "crews".into(),
                field: zone,
                value: text("eu"),
            },
            Step::RemoveField {
                table: "crews".into(),
                name: "note".into(),
            },
            Step::Sleep(Duration::from_millis(80)),
            Step::End,
        ] {
            fake.push(step);
        }
        let (undecodable, ended) = feed(&mut stream, &mut decoder, &mut source, |_| false);
        assert!(ended);
        assert_eq!(undecodable, 1, "the garbage, and the stream went on");
        assert!(fake.settle(Duration::from_secs(5)));
        let want = fake.table("crews").unwrap();
        let names: Vec<String> = (want.rows.iter())
            .map(|r| r["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            names,
            ["alpha", "charlie", "delta", "echo", "foxtrot", "golf"]
        );
        assert_eq!(source.row_count(), 6);
        for row in &want.rows {
            let key = source.row_key(&[row["name"].clone()]).unwrap();
            for column in source.columns() {
                let wire = row.get(column.name()).cloned().unwrap_or(Wire::Null);
                assert_eq!(
                    source.cell(key, column.key()),
                    Some(&binnacle_described::typed(column.field(), &wire)),
                    "{accept}: {} {}",
                    row["name"].as_str().unwrap(),
                    column.name()
                );
            }
        }
        assert!(source.column_key("note").is_none());
        assert!(source.column_key("zone").is_some());
        source.ended();
        assert!(matches!(
            source.poll_request(),
            Some(HostRequest::OpenWatch(_))
        ));
    }
}

#[test]
fn a_quiet_watch_gets_heartbeats_and_the_fake_logs_each_request() {
    let fake = FakeTables::new()
        .fixture(crews())
        .unwrap()
        .heartbeat(Duration::from_millis(20))
        .start()
        .unwrap();
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    let (mut stream, mut decoder) = watch(&fake, &mut source, NDJSON);
    feed(&mut stream, &mut decoder, &mut source, |s| {
        s.status().synced
    });
    let synced = source.status().sequence;
    // A heartbeat brings the sequence again.
    let mut beats = 0;
    while beats < 2 {
        decoder.feed(&stream.next_chunk().unwrap().unwrap());
        while let Some(Ok(message)) = decoder.next() {
            assert!(matches!(message, Message::Progress { .. }), "{message:?}");
            source.receive(message);
            beats += 1;
        }
    }
    assert_eq!(source.status().sequence, synced);
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path(), "/tables/crews/rows");
    assert_eq!(requests[0].header("Accept"), Some(NDJSON));
    assert_eq!(fake.watches(), 1);
}

#[test]
fn a_failed_step_answers_the_next_rows_request_with_its_status() {
    let fake = FakeTables::new().fixture(crews()).unwrap().start().unwrap();
    // The script plays once a watch has synced.
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    let (mut stream, mut decoder) = watch(&fake, &mut source, NDJSON);
    feed(&mut stream, &mut decoder, &mut source, |s| {
        s.status().synced
    });
    fake.push(Step::Fail { status: 500 });
    assert!(fake.settle(Duration::from_secs(5)));
    let target = "/tables/crews/rows?namespace=flotilla";
    assert_eq!(fetch(fake.addr(), target, &[]).unwrap().status, 500);
    assert_eq!(fetch(fake.addr(), target, &[]).unwrap().status, 200, "once");
}

#[cfg(unix)]
#[test]
fn a_fake_listens_on_a_unix_socket() {
    let dir = std::env::temp_dir().join(format!("fake-tables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tables.sock");
    let fake = FakeTables::new()
        .fixture(crews())
        .unwrap()
        .unix(&path)
        .start()
        .unwrap();
    let response = fetch(fake.addr(), "/tables/crews/rows?namespace=flotilla", &[]).unwrap();
    assert_eq!(response.status, 200);
    drop(fake);
    assert!(!path.exists(), "removed when it stops");
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn a_script_reads_from_json_lines() {
    let steps = binnacle_testkit::fake_tables::parse_script(
        r#"
        # comments and blank lines are skipped
        {"step": "upsert", "table": "hosts", "row": {"metadata.name": "a"}}
        {"step": "delete", "key": ["a"]}
        {"step": "resync"}
        {"step": "add-field", "field": {"name": "zone", "type": "string"}, "value": "eu"}
        {"step": "remove-field", "field": "zone"}
        {"step": "sleep", "ms": 5}
        {"step": "fail", "status": 503}
        {"step": "garbage"}
        {"step": "end"}
        "#,
    )
    .unwrap();
    assert_eq!(steps.len(), 9);
    assert_eq!(
        steps[1],
        Step::Delete {
            table: String::new(),
            key: vec![Wire::Text("a".into())]
        }
    );
    assert!(binnacle_testkit::fake_tables::parse_script(r#"{"step": "dance"}"#).is_err());
}

/// Feeds `source` what `stream` sends up to the first item that doesn't
/// decode.
fn until_garbage(stream: &mut Stream, decoder: &mut Decoder, source: &mut DescribedSource) {
    loop {
        while let Some(message) = decoder.next() {
            match message {
                Ok(message) => {
                    source.receive(message);
                }
                Err(_) => return,
            }
        }
        let chunk = stream
            .next_chunk()
            .unwrap()
            .expect("the garbage before the end");
        decoder.feed(&chunk);
    }
}

/// `source`'s rows are `rows`, each value as its field reads it. In any
/// order: a resync matches rows by place, so it can leave the source's
/// rows in another order than the server's.
fn holds(source: &DescribedSource, rows: &[binnacle_described::Row], what: &str) {
    let read = |row: &binnacle_described::Row| -> String {
        let values: Vec<_> = (source.columns().iter())
            .map(|c| {
                let wire = row.get(c.name()).cloned().unwrap_or(Wire::Null);
                binnacle_described::typed(c.field(), &wire)
            })
            .collect();
        format!("{values:?}")
    };
    let mut want: Vec<String> = rows.iter().map(read).collect();
    let mut held: Vec<String> = (source.row_keys().iter())
        .map(|&k| {
            let values: Vec<_> = (source.columns().iter())
                .map(|c| source.cell(k, c.key()).cloned())
                .collect();
            format!("{:?}", values.into_iter().flatten().collect::<Vec<_>>())
        })
        .collect();
    want.sort();
    held.sort();
    assert_eq!(held, want, "{what}");
}

// A table with no primary key has rows by place, as the source keys them:
// an upsert adds a row and tells each watch the next place of its stream,
// and a delete names a place in the table and tells each watch the place
// its stream gave that row. So a watch, with a `where` clause or none,
// holds the fake's rows through upserts, deletes, a resync and a fresh
// watch.
#[test]
fn a_table_with_no_primary_key_takes_upserts_and_deletes_by_place() {
    let row = |body: &str, tag: &str| -> binnacle_described::Row {
        [
            ("body".to_owned(), Wire::Text(body.into())),
            ("tag".to_owned(), Wire::Text(tag.into())),
        ]
        .into()
    };
    let schema = binnacle_described::Schema {
        fields: vec![
            Field::new("body", FieldType::String),
            Field::new("tag", FieldType::String),
        ],
        primary_key: Vec::new(),
        foreign_keys: Vec::new(),
    };
    let notes = FakeTable::new("notes", schema).with_rows(vec![
        row("a", "x"),
        row("b", "y"),
        row("c", "x"),
    ]);
    let fake = FakeTables::new().table(notes).start().unwrap();
    let scopes = [
        Scope::new("flotilla"),
        Scope::new("flotilla").with_where(".tag == x"),
    ];
    let mut sources = scopes.map(|scope| DescribedSource::new("notes", scope));
    let check = |sources: &mut [DescribedSource; 2], steps: Vec<Step>, what: &str, after| {
        let mut watches: Vec<(Stream, Decoder)> = (sources.iter_mut())
            .map(|s| watch(&fake, s, NDJSON))
            .collect();
        for ((stream, decoder), source) in watches.iter_mut().zip(sources.iter_mut()) {
            feed(stream, decoder, source, |s| s.status().synced);
        }
        for step in steps {
            let what = format!("{what}: {step:?}");
            // Every watch gets the garbage after the step's messages.
            fake.push(step);
            fake.push(Step::Garbage {
                table: "notes".into(),
            });
            for ((stream, decoder), source) in watches.iter_mut().zip(sources.iter_mut()) {
                until_garbage(stream, decoder, source);
            }
            let rows = fake.table("notes").unwrap().rows;
            holds(&sources[0], &rows, &what);
            let x: Vec<_> = (rows.into_iter())
                .filter(|r| r["tag"] == Wire::Text("x".into()))
                .collect();
            holds(&sources[1], &x, &what);
        }
        fake.push(Step::End);
        for ((stream, decoder), source) in watches.iter_mut().zip(sources.iter_mut()) {
            assert_eq!(feed(stream, decoder, source, |_| false), (0, true));
            source.ended();
        }
        let bodies: Vec<Wire> = (fake.table("notes").unwrap().rows.iter())
            .map(|r| r["body"].clone())
            .collect();
        let after: Vec<Wire> = (after as Vec<&str>)
            .into_iter()
            .map(|b| Wire::Text(b.into()))
            .collect();
        assert_eq!(bodies, after, "{what}");
    };
    let upsert = |row| Step::Upsert {
        table: "notes".into(),
        row,
    };
    let delete = |n: i128| Step::Delete {
        table: "notes".into(),
        key: vec![Wire::Int(n)],
    };
    let steps = vec![
        delete(1),
        upsert(row("d", "x")),
        delete(1),
        upsert(row("e", "y")),
        delete(9),
        upsert(row("f", "x")),
        delete(2),
    ];
    check(&mut sources, steps, "live", vec!["a", "d", "f"]);
    let resync = Step::Resync {
        table: "notes".into(),
    };
    let steps = vec![delete(0), resync, upsert(row("g", "x")), delete(1)];
    check(
        &mut sources,
        steps,
        "a fresh watch and a resync",
        vec!["d", "g"],
    );
}
