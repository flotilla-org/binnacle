//! The count read: while a Described Table source sends `where` clauses,
//! it asks for a second, count-only read, whose `count` gives the rows'
//! Totals.

mod support;

use binnacle_core::{Axis, Comparison, Condition, Key, Scalar, Value as Listed};
use binnacle_described::{CountRequest, DescribedSource, HostRequest, Message, Scope};
use binnacle_ratatui::row_count;
use binnacle_testkit::Scenario;
use serde_json::json;
use support::{Table, VIEWPORT, feed_count, scenario};

/// Twelve convoys, every one Running, within a Scope that keeps
/// `retries >= 0`.
fn source() -> DescribedSource {
    let scope = Scope::new("flotilla").with_where("retries >= 0");
    let mut source = DescribedSource::new("convoys", scope);
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    let ordered = ["==", "!=", "<", "<=", ">", ">=", "in"];
    let table = json!({"type": "table", "schema": {"primaryKey": ["name"], "fields": [
        {"name": "name", "type": "string", "whereOperators": ordered},
        {"name": "phase", "type": "string", "whereOperators": ordered},
        {"name": "retries", "type": "integer", "whereOperators": ordered}
    ]}});
    let rows: Vec<_> = (0..12)
        .map(|i| json!({"name": format!("c{i:02}"), "phase": "Running", "retries": i}))
        .collect();
    let rows = json!({"type": "rows", "cursor": null, "rows": rows});
    for message in [table, rows, json!({"type": "synced"})] {
        source.receive(Message::from_json(&message).unwrap());
    }
    source
}

fn key(s: &Scenario<Table>, name: &str) -> Key {
    s.source.0.column_key(name).unwrap()
}

fn requests(s: &mut Scenario<Table>) -> Vec<HostRequest> {
    std::iter::from_fn(|| s.source.0.poll_request()).collect()
}

fn count(count: u64, estimate: bool) -> Message {
    Message::Count { count, estimate }
}

/// The status line's row count.
fn status(s: &mut Scenario<Table>) -> String {
    row_count(&s.frame(VIEWPORT))
}

fn running(s: &Scenario<Table>) -> Condition {
    Condition::one_of(key(s, "phase"), vec![Listed::Text("Running".into())])
}

#[test]
fn a_sent_clause_asks_for_the_count_read_once_and_its_count_is_the_rows_totals() {
    let mut s = scenario(source());
    assert_eq!(status(&mut s), "12 rows");
    let running = running(&s);
    s.input(|g| g.add_slicer(running.clone()));
    s.frame(VIEWPORT);
    let asked = requests(&mut s);
    let [HostRequest::OpenWatch(_), HostRequest::OpenCount(read)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    // The Scope's own clauses only, not the Slicer's.
    assert_eq!(
        read.target(),
        "/tables/convoys/rows?watch&count=exact&limit=1&namespace=flotilla&where=retries%20%3E%3D%200"
    );
    assert_eq!(s.source.0.totals().rows, None, "unknown until counted");
    assert_eq!(status(&mut s), "12 rows");

    feed_count(&mut s, count(340, false));
    assert_eq!(s.source.0.totals().rows, Some(340));
    assert_eq!(s.source.0.row_count(), 12);
    assert_eq!(status(&mut s), "12 of 340 rows");

    // A second Slicer opens the watch again, but not the count read.
    let few = Condition::compare(key(&s, "retries"), Comparison::Less(Scalar::Number(5.0)));
    s.input(|g| g.add_slicer(few.clone()));
    s.frame(VIEWPORT);
    let asked = requests(&mut s);
    assert!(
        matches!(asked.as_slice(), [HostRequest::OpenWatch(_)]),
        "{asked:?}"
    );
    assert_eq!(status(&mut s), "5 of 340 rows");

    // Live: a later count updates the Totals.
    feed_count(&mut s, count(341, false));
    assert_eq!(status(&mut s), "5 of 341 rows");

    s.input(|g| g.remove_slicer(&few));
    s.frame(VIEWPORT);
    assert!(
        matches!(requests(&mut s).as_slice(), [HostRequest::OpenWatch(_)]),
        "a clause is still sent"
    );
    s.input(|g| g.remove_slicer(&running));
    s.frame(VIEWPORT);
    let asked = requests(&mut s);
    assert!(
        matches!(
            asked.as_slice(),
            [HostRequest::OpenWatch(_), HostRequest::CloseCount]
        ),
        "{asked:?}"
    );
    assert_eq!(s.source.0.totals().rows, Some(12), "the held rows again");
    assert_eq!(status(&mut s), "12 rows");
    // A count of the read just closed changes nothing.
    feed_count(&mut s, count(340, false));
    assert_eq!(s.source.0.totals().rows, Some(12));
}

#[test]
fn an_estimate_is_the_totals_all_the_same() {
    let mut s = scenario(source());
    let running = running(&s);
    s.input(|g| g.add_slicer(running));
    s.frame(VIEWPORT);
    feed_count(&mut s, count(400, true));
    assert_eq!(s.source.0.totals().rows, Some(400));
    assert_eq!(status(&mut s), "12 of 400 rows");
}

#[test]
fn only_the_count_reads_count_is_taken_from_it() {
    let mut s = scenario(source());
    let running = running(&s);
    s.input(|g| g.add_slicer(running));
    s.frame(VIEWPORT);
    let _ = requests(&mut s);
    let row = json!({"type": "rows", "cursor": null, "rows": [{"name": "zz", "phase": "Done", "retries": 1}]});
    feed_count(&mut s, Message::from_json(&row).unwrap());
    feed_count(&mut s, Message::Synced { sequence: None });
    assert_eq!(s.source.0.row_count(), 12, "its rows are not held");
    assert_eq!(s.source.0.totals().rows, None);
}

#[test]
fn a_count_read_not_yet_opened_is_taken_back_and_one_that_ended_is_asked_again() {
    let mut s = scenario(source());
    let running = running(&s);
    s.input(|g| g.add_slicer(running.clone()));
    s.frame(VIEWPORT);
    s.input(|g| g.remove_slicer(&running));
    s.frame(VIEWPORT);
    let asked = requests(&mut s);
    assert!(
        matches!(asked.as_slice(), [HostRequest::OpenWatch(_)]),
        "{asked:?}"
    );
    s.source.0.count_ended();
    assert_eq!(requests(&mut s), [], "no clause, no count read");

    s.input(|g| g.add_slicer(running.clone()));
    s.frame(VIEWPORT);
    let _ = requests(&mut s);
    feed_count(&mut s, count(340, false));
    s.source.0.count_ended();
    s.source.0.count_ended();
    let asked = requests(&mut s);
    let [HostRequest::OpenCount(CountRequest { table, .. })] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert_eq!(table, "convoys");
    assert_eq!(s.source.0.totals().rows, Some(340), "until the next count");
    assert_eq!(s.source.0.count(Axis::Rows), 12);
}
