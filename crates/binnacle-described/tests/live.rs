//! Live changes through the scenario harness and in small examples:
//! upserts, deletes, resyncs, schema changes and stream ends keep the
//! Grid's focus, picks and pins, and the source's Keys, interner and
//! status stay as the brief says.

mod support;

use binnacle_core::{
    Axis, ChangeNotice, Condition, ConditionState, ConditionTree, Edge, FilterSpec, Key,
    SortColumn, SortSpec, TextQuery, ViewSpec,
};
use binnacle_described::{
    DescribedSource, Field, FieldType, HostRequest, Message, Received, RowsMessage, Scope,
    TableMessage, Wire,
};
use binnacle_memory::PumpSource;
use binnacle_testkit::Scenario;
use serde_json::json;
use support::{Table, VIEWPORT, column_down, feed, messages, replay, scenario};

fn table_message() -> TableMessage {
    match messages("crews.ndjson").remove(0) {
        Message::Table(table) => table,
        other => panic!("{other:?}"),
    }
}

/// The crews rows, as the fixture has them, `change` applied to each.
fn crews_rows(change: impl Fn(&mut serde_json::Value)) -> Vec<binnacle_described::Row> {
    let Message::Rows(rows) = messages("crews.ndjson").remove(1) else {
        panic!("rows second")
    };
    (rows.rows.into_iter())
        .map(|row| {
            let mut json = Wire::Map(row).to_json();
            change(&mut json);
            match Wire::from_json(&json) {
                Wire::Map(row) => row,
                _ => unreachable!(),
            }
        })
        .collect()
}

fn rows(rows: Vec<binnacle_described::Row>) -> Message {
    Message::Rows(RowsMessage {
        rows,
        cursor: None,
        partial: false,
        sequence: None,
    })
}

fn upsert(row: serde_json::Value) -> Message {
    let key = vec![Wire::from_json(&row["name"])];
    let Wire::Map(row) = Wire::from_json(&row) else {
        unreachable!()
    };
    Message::Upsert { key, row }
}

fn delete(name: &str) -> Message {
    Message::Delete {
        key: vec![Wire::Text(name.into())],
    }
}

fn row(s: &Scenario<Table>, name: &str) -> Key {
    s.source.0.row_key(&[Wire::Text(name.into())]).unwrap()
}

fn col(s: &Scenario<Table>, name: &str) -> Key {
    s.source.0.column_key(name).unwrap()
}

/// What the brief asks to survive: focus, picks and pins on both axes.
#[derive(Debug, PartialEq)]
struct Held {
    focus: (Option<Key>, Option<Key>),
    picks: Vec<Key>,
    pins: [Vec<Key>; 2],
}

fn held(s: &mut Scenario<Table>) -> Held {
    let snapshot = s.frame(VIEWPORT);
    Held {
        focus: (
            snapshot.focus(Axis::Rows).and_then(|f| f.key),
            snapshot.focus(Axis::Cols).and_then(|f| f.key),
        ),
        picks: snapshot.picked(Axis::Rows).to_vec(),
        pins: Axis::ALL.map(|a| snapshot.pins(a, Edge::Leading).collect()),
    }
}

/// The crews table with focus on charlie's phase, bravo and delta picked,
/// echo's row and the name column pinned.
fn held_crews() -> Scenario<Table> {
    let mut s = scenario(replay("crews.ndjson"));
    let (charlie, phase) = (row(&s, "charlie"), col(&s, "phase"));
    let (bravo, delta, echo) = (row(&s, "bravo"), row(&s, "delta"), row(&s, "echo"));
    let name = col(&s, "name");
    s.input(|g| {
        g.set_focus(charlie, phase);
        g.pick(Axis::Rows, bravo);
        g.pick(Axis::Rows, delta);
        g.pin(Axis::Rows, Edge::Leading, echo);
        g.pin(Axis::Cols, Edge::Leading, name);
    });
    s.frame(VIEWPORT);
    s
}

#[test]
fn a_resync_that_drops_a_row_and_changes_a_value_keeps_focus_picks_and_pins() {
    let mut s = held_crews();
    let before = held(&mut s);
    assert_eq!(before.picks.len(), 2);
    let keys: Vec<Key> = s.source.0.row_keys().to_vec();
    feed(&mut s, Message::Resync);
    assert!(!s.source.0.status().synced, "not synced during a resync");
    let notices = feed(&mut s, Message::Table(table_message()));
    assert!(notices.is_empty(), "an unchanged table needs nothing");
    assert_eq!(s.source.0.row_count(), 6, "the table never shows empty");
    let mut again = crews_rows(|r| {
        if r["name"] == "alpha" {
            r["load"] = json!(0.75);
        }
    });
    again.retain(|r| r["name"] != Wire::Text("foxtrot".into()));
    feed(&mut s, rows(again));
    assert_eq!(s.source.0.row_count(), 6, "foxtrot stays until synced");
    let notices = feed(&mut s, Message::Synced { sequence: Some(9) });
    assert_eq!(
        notices,
        [(Axis::Rows, ChangeNotice::Removed { at: 5, n: 1 })]
    );
    assert!(s.source.0.status().synced);
    assert_eq!(s.source.0.status().sequence, Some(9));
    assert_eq!(held(&mut s), before);
    assert_eq!(
        s.source.0.row_keys(),
        &keys[..5],
        "surviving rows keep their Keys"
    );
    assert_eq!(
        column_down(&mut s, "name")[..2],
        ["echo", "alpha"],
        "echo pinned"
    );
    assert_eq!(column_down(&mut s, "load")[1], "0.75%");
}

#[test]
fn a_schema_change_keeps_focus_picks_and_pins() {
    let mut s = held_crews();
    let before = held(&mut s);
    let columns: Vec<Key> = s.source.0.columns().iter().map(|c| c.key()).collect();
    let mut table = table_message();
    table.schema.fields.retain(|f| f.name != "note");
    let mut zone = Field::new("zone", FieldType::String).properties().clone();
    zone.insert("title".into(), json!("Zone"));
    table
        .schema
        .fields
        .insert(1, Field::from_properties(zone).unwrap());
    let notices = feed(&mut s, Message::Table(table));
    assert_eq!(
        notices,
        [
            (Axis::Cols, ChangeNotice::Removed { at: 5, n: 1 }),
            (Axis::Cols, ChangeNotice::Inserted { at: 7, n: 1 }),
            (Axis::Rows, ChangeNotice::Reset(6)),
        ]
    );
    assert_eq!(held(&mut s), before);
    let now: Vec<Key> = s.source.0.columns().iter().map(|c| c.key()).collect();
    let note = s.source.0.column_key("note");
    assert_eq!(note, None);
    assert_eq!(now[..5], columns[..5], "unchanged fields keep their Keys");
    assert_eq!(now[5..7], columns[6..]);
    assert_eq!(s.source.0.columns()[7].header(), "Zone", "added at the end");
}

#[test]
fn a_retyped_field_is_read_again_for_every_row() {
    let mut source = replay("crews.ndjson");
    assert_eq!(source.mismatches(), 3);
    let mut table = table_message();
    let cpus = table
        .schema
        .fields
        .iter_mut()
        .find(|f| f.name == "cpus")
        .unwrap();
    let mut properties = cpus.properties().clone();
    properties.insert("type".into(), json!("string"));
    *cpus = Field::from_properties(properties).unwrap();
    source.receive(Message::Table(table));
    let charlie = source.row_key(&[Wire::Text("charlie".into())]).unwrap();
    let cpus = source.column_key("cpus").unwrap();
    assert_eq!(
        source.cell(charlie, cpus),
        Some(&binnacle_format::Value::Text("many".into()))
    );
    // charlie's "many" is text now, and five integers are not.
    assert_eq!(source.mismatches(), 3 - 1 + 4);
}

#[test]
fn a_changed_primary_key_keys_every_row_again() {
    let mut source = replay("crews.ndjson");
    let mut table = table_message();
    table.schema.primary_key = vec!["name".into(), "host".into()];
    let Received::Applied(notices) = source.receive(Message::Table(table)) else {
        panic!()
    };
    assert_eq!(
        notices,
        [
            (Axis::Cols, ChangeNotice::Reset(8)),
            (Axis::Rows, ChangeNotice::Reset(6)),
        ]
    );
    let tuple = [Wire::Text("alpha".into()), Wire::Text("feta".into())];
    assert!(source.row_key(&tuple).is_some());
    assert_eq!(source.row_key(&[Wire::Text("alpha".into())]), None);
    assert_eq!(source.interned_rows(), 6);
}

#[test]
fn an_upsert_into_a_sorted_view_moves_the_row_and_a_delete_removes_it() {
    let mut s = scenario(replay("crews.ndjson"));
    let load = col(&s, "load");
    let spec =
        ViewSpec::default().with_sort(Axis::Rows, SortSpec::new(vec![SortColumn::ascending(load)]));
    s.input(|g| g.request_view(spec));
    s.frame(VIEWPORT);
    assert_eq!(
        column_down(&mut s, "name"),
        ["charlie", "foxtrot", "alpha", "bravo", "echo", "delta"]
    );
    let notices = feed(
        &mut s,
        upsert(json!({"name": "bravo", "cpus": 2, "load": 99, "host": "feta", "spec": {}})),
    );
    assert_eq!(notices, [(Axis::Rows, ChangeNotice::Reordered)]);
    assert_eq!(
        column_down(&mut s, "name"),
        ["charlie", "foxtrot", "alpha", "echo", "delta", "bravo"]
    );
    let (echo, phase) = (row(&s, "echo"), col(&s, "phase"));
    s.input(|g| g.set_focus(echo, phase));
    s.frame(VIEWPORT);
    let notices = feed(&mut s, delete("echo"));
    assert_eq!(
        notices,
        [(Axis::Rows, ChangeNotice::Removed { at: 3, n: 1 })]
    );
    assert_eq!(
        column_down(&mut s, "name"),
        ["charlie", "foxtrot", "alpha", "delta", "bravo"]
    );
    let delta = row(&s, "delta");
    assert_eq!(
        held(&mut s).focus,
        (Some(delta), Some(phase)),
        "focus falls to the neighbour"
    );
}

#[test]
fn an_upsert_keeps_the_order_the_user_moved_rows_into() {
    let mut source = replay("crews.ndjson");
    let keys = source.row_keys().to_vec();
    source.move_items(Axis::Rows, &[keys[4]], 0).unwrap();
    let before = source.window(Axis::Rows, 0..6).keys;
    assert_eq!(before[0], keys[4]);
    source.receive(upsert(json!({"name": "alpha", "load": 7})));
    assert_eq!(source.window(Axis::Rows, 0..6).keys, before);
}

#[test]
fn a_delete_of_a_row_the_source_does_not_hold_is_no_matter() {
    let mut source = replay("crews.ndjson");
    assert_eq!(
        source.receive(delete("zulu")),
        Received::Applied(Vec::new())
    );
    assert_eq!(source.row_count(), 6);
}

#[test]
fn a_condition_on_a_removed_field_is_not_applied_until_the_field_returns() {
    let mut s = scenario(replay("crews.ndjson"));
    let host = col(&s, "host");
    let condition = Condition::text(host, TextQuery::new("brie"));
    let filter = FilterSpec::default().with_conditions(ConditionTree::All(vec![condition.clone()]));
    let spec = ViewSpec::default()
        .with_filter(Axis::Rows, filter)
        .with_sort(
            Axis::Rows,
            SortSpec::new(vec![SortColumn::descending(host)]),
        );
    s.input(|g| g.request_view(spec));
    s.frame(VIEWPORT);
    assert_eq!(column_down(&mut s, "name"), ["charlie", "delta"]);
    let mut without = table_message();
    without.schema.fields.retain(|f| f.name != "host");
    feed(&mut s, Message::Table(without));
    let snapshot = s.frame(VIEWPORT);
    assert_eq!(
        snapshot.conditions(Axis::Rows),
        [(condition.clone(), ConditionState::NotApplied)]
    );
    assert!(snapshot.applied_view().rows.sort.is_empty());
    assert_eq!(column_down(&mut s, "name").len(), 6, "the filter is off");
    feed(&mut s, Message::Table(table_message()));
    let snapshot = s.frame(VIEWPORT);
    assert_eq!(
        snapshot.conditions(Axis::Rows),
        [(condition, ConditionState::Applied)]
    );
    // Its values came back from the rows as they came.
    assert_eq!(column_down(&mut s, "name"), ["charlie", "delta"]);
}

#[test]
fn the_interner_holds_no_more_than_the_rows_through_a_thousand_rounds() {
    let mut source = replay("crews.ndjson");
    for i in 0..1000 {
        let name = format!("row-{i}");
        source.receive(upsert(json!({"name": name})));
        if i % 3 != 0 {
            source.receive(delete(&name));
        }
        assert!(source.interned_rows() <= source.row_count());
    }
    assert_eq!(source.row_count(), 6 + 334);
}

#[test]
fn a_row_deleted_and_upserted_again_gets_its_old_key() {
    let mut source = replay("crews.ndjson");
    let alpha = source.row_key(&[Wire::Text("alpha".into())]).unwrap();
    source.receive(delete("alpha"));
    assert_eq!(source.row_key(&[Wire::Text("alpha".into())]), None);
    source.receive(upsert(json!({"name": "alpha"})));
    assert_eq!(source.row_key(&[Wire::Text("alpha".into())]), Some(alpha));
    assert_eq!(*source.row_keys().last().unwrap(), alpha);
}

#[test]
fn the_end_of_a_stream_asks_for_the_watch_again_and_resyncs() {
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    let first = source.poll_request().unwrap();
    for message in messages("crews.ndjson") {
        source.receive(message);
    }
    assert_eq!(source.poll_request(), None);
    assert_eq!(source.ended(), Received::Applied(Vec::new()));
    assert!(matches!(first, HostRequest::OpenWatch(_)));
    assert_eq!(source.poll_request(), Some(first), "the same request again");
    assert!(!source.status().synced);
    assert_eq!(source.row_count(), 6, "the rows stay shown");
    // The fresh stream brings five rows back.
    let mut all = messages("crews.ndjson");
    let Message::Rows(rows) = &mut all[1] else {
        panic!()
    };
    rows.rows.pop();
    for message in all {
        source.receive(message);
    }
    assert_eq!(source.row_count(), 5);
    assert!(source.status().synced);
}

#[test]
fn a_cursor_on_the_first_page_sets_partial_and_no_page_is_asked_for() {
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.poll_request();
    source.receive(Message::Table(table_message()));
    let page = |cursor: Option<&str>, partial| {
        Message::Rows(RowsMessage {
            rows: Vec::new(),
            cursor: cursor.map(String::from),
            partial,
            sequence: None,
        })
    };
    source.receive(page(Some("more"), true));
    assert!(source.status().partial);
    source.receive(page(None, true));
    assert!(source.status().partial, "a later page leaves it");
    assert_eq!(source.poll_request(), None, "never a second page");
    // A fresh table: the first page with `partial` but no cursor is whole.
    source.receive(Message::Table(table_message()));
    source.receive(page(None, true));
    assert!(!source.status().partial);
    assert_eq!(source.poll_request(), None);
}

#[test]
fn a_table_with_no_primary_key_keys_rows_by_arrival_through_deletes() {
    let mut table = table_message();
    table.schema.primary_key.clear();
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table));
    source.receive(rows(crews_rows(|_| {})));
    let keys = source.row_keys().to_vec();
    source.receive(Message::Delete {
        key: vec![Wire::Int(1)],
    });
    source.receive(rows(crews_rows(|_| {})[..1].to_vec()));
    assert_eq!(source.row_count(), 6);
    assert!(
        !keys.contains(source.row_keys().last().unwrap()),
        "a fresh Key"
    );
}

#[test]
fn a_resync_of_a_table_with_no_primary_key_replaces_its_rows_by_place() {
    let mut table = table_message();
    table.schema.primary_key.clear();
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table.clone()));
    source.receive(rows(crews_rows(|_| {})));
    let keys = source.row_keys().to_vec();
    source.ended();
    source.receive(Message::Table(table));
    source.receive(rows(crews_rows(|_| {})[..5].to_vec()));
    source.receive(Message::Synced { sequence: None });
    assert_eq!(source.row_keys(), &keys[..5], "the same Keys, in place");
    assert_eq!(source.interned_rows(), 5);
}

#[test]
fn ending_twice_before_the_host_polls_asks_for_one_watch() {
    let mut source = replay("crews.ndjson");
    source.ended();
    source.ended();
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    assert_eq!(source.poll_request(), None);
}

#[test]
fn a_table_with_no_primary_key_takes_an_upserted_place_as_reached() {
    let mut table = table_message();
    table.schema.primary_key.clear();
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table.clone()));
    source.receive(rows(crews_rows(|_| {})[..2].to_vec()));
    let row = crews_rows(|_| {})[5].clone();
    source.receive(Message::Upsert {
        key: vec![Wire::Int(5)],
        row,
    });
    source.receive(rows(crews_rows(|_| {})[2..].to_vec()));
    assert_eq!(source.row_count(), 7, "the rows after it are new rows");
    // Keyed and back to none: the rows are keyed again from the start.
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table.clone()));
    source.receive(rows(crews_rows(|_| {})));
    let keys = source.row_keys().to_vec();
    source.receive(Message::Table(table_message()));
    assert!(source.row_keys().iter().all(|k| !keys.contains(k)));
    source.receive(Message::Table(table));
    assert_eq!(source.row_keys(), keys);
}

#[test]
fn a_row_twice_in_one_page_counts_once_the_later_winning() {
    let mut source = replay("crews.ndjson");
    let alpha = |load: f64| {
        let mut row = crews_rows(|_| {})[0].clone();
        row.insert("load".into(), Wire::Float(load));
        row
    };
    let Received::Applied(notices) = source.receive(rows(vec![alpha(7.0), alpha(8.0)])) else {
        panic!()
    };
    assert_eq!(notices.len(), 1, "{notices:?}");
    let key = source.row_key(&[Wire::Text("alpha".into())]).unwrap();
    let load = source.column_key("load").unwrap();
    assert_eq!(
        source.cell(key, load),
        Some(&binnacle_format::Value::Number(8.0))
    );
}

#[test]
fn an_upserted_place_past_any_count_neither_panics_nor_reuses_keys() {
    let mut table = table_message();
    table.schema.primary_key.clear();
    let mut source = DescribedSource::new("crews", Scope::new("flotilla"));
    source.receive(Message::Table(table));
    let row = crews_rows(|_| {})[0].clone();
    for place in [u64::MAX.into(), -1i128] {
        source.receive(Message::Upsert {
            key: vec![Wire::Int(place)],
            row: row.clone(),
        });
    }
    source.receive(rows(crews_rows(|_| {})));
    assert_eq!(source.row_count(), 8, "every row its own Key");
}
