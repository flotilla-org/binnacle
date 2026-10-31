//! Fixture replay: every recorded rows read and watch replays through the
//! source, and the Grid shows the table it describes.

mod support;

use binnacle_described::{Message, Received, Unhandled};
use support::{fixtures, messages, picture, replay, scenario};

#[test]
fn every_rows_and_watch_fixture_replays_into_the_grid() {
    let names = fixtures("rows-", &[".ndjson", ".cborseq"])
        .into_iter()
        .chain(fixtures("watch-", &[".ndjson"]));
    let mut count = 0;
    for name in names {
        let mut scenario = scenario(replay(&name));
        insta::assert_snapshot!(name.clone(), picture(&mut scenario));
        count += 1;
    }
    assert_eq!(count, 15);
}

#[test]
fn a_first_page_with_a_cursor_is_over_the_cap_and_one_without_is_not() {
    let page1 = replay("rows-vessels-page1.ndjson");
    assert!(page1.status().partial, "page 1 has a cursor");
    assert_eq!(page1.row_count(), 5, "the rows that came are kept");
    // Page 2 says `partial: true` too, with a null cursor: it is the last.
    let page2 = replay("rows-vessels-page2.ndjson");
    assert!(!page2.status().partial, "a null cursor ends the paging");
}

#[test]
fn the_watch_fixtures_upserts_change_the_row_in_place() {
    let mut source = replay("rows-hosts.ndjson");
    let before: Vec<_> = source.row_keys().to_vec();
    let upserts: Vec<Message> = (messages("watch-hosts.ndjson").into_iter())
        .filter(|m| matches!(m, Message::Upsert { .. }))
        .collect();
    assert_eq!(upserts.len(), 3);
    let version = source.column_key("metadata.resourceVersion").unwrap();
    for upsert in upserts {
        let Message::Upsert { row, .. } = &upsert else {
            unreachable!()
        };
        let want = row["metadata.resourceVersion"].as_str().unwrap().to_owned();
        assert!(matches!(source.receive(upsert), Received::Applied(_)));
        assert_eq!(source.row_keys(), before, "the same row, the same Key");
        let cell = source.cell(before[0], version).unwrap();
        assert_eq!(*cell, binnacle_format::Value::Text(want));
    }
    let other = Message::Other {
        kind: "news".into(),
        body: binnacle_described::Wire::Map(Default::default()),
    };
    assert_eq!(
        source.receive(other.clone()),
        Received::Unhandled(Unhandled::Message(other)),
        "only a type this crate does not know comes back unhandled"
    );
}
