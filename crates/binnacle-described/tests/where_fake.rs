//! End to end: a source behind the fake table server, over HTTP, sends its
//! slicer as a `where` clause and gets every matching row of a table bigger
//! than its cap.

mod support;

use std::time::{Duration, Instant};

use binnacle_core::{Condition, Value as Listed};
use binnacle_described::{DescribedSource, Scope, Wire};
use binnacle_described_http::{Blocking, Endpoint, Event, Options};
use binnacle_memory::PumpSource;
use binnacle_testkit::Scenario;
use binnacle_testkit::fake_tables::{FakeTable, FakeTables};
use support::{Table, scenario};

/// Feeds the scenario's source what `transport` brings, opening each watch
/// it asks for, until it is synced.
fn until_synced(s: &mut Scenario<Table>, transport: &Blocking) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(request) = s.source.0.poll_request() {
            transport.open(request);
        }
        let left = deadline.saturating_duration_since(Instant::now());
        match transport.recv_timeout(left).expect("an event in time") {
            Event::Message(message) => {
                support::feed(s, message);
            }
            Event::Ended => {
                let received = s.source.0.ended();
                support::apply(s, received);
            }
            other => panic!("{other:?}"),
        }
        if s.source.0.status().synced && !s.source.0.has_request() {
            return;
        }
    }
}

#[test]
fn a_slicer_on_a_table_past_the_cap_ends_with_every_matching_row() {
    const ROWS: usize = 10_050;
    let done: Vec<Wire> = (FakeTable::generated("big", ROWS).rows.into_iter())
        .filter(|r| r["phase"] == Wire::Text("Done".into()))
        .map(|r| r["id"].clone())
        .collect();
    let fake = FakeTables::new().generate("big", ROWS).start().unwrap();
    let transport = Blocking::new(Options::new(Endpoint::Tcp(fake.url().unwrap()))).unwrap();
    let mut s = scenario(DescribedSource::new("big", Scope::new("flotilla")));
    until_synced(&mut s, &transport);
    assert!(s.source.0.status().partial);
    assert_eq!(s.source.0.row_count(), 10_000);
    let snapshot = s.frame(support::VIEWPORT);
    assert_eq!(
        binnacle_ratatui::row_count(&snapshot),
        "10,000 of 10,050 rows"
    );
    let phase = s.source.0.column_key("phase").unwrap();
    let slicer = Condition::one_of(phase, vec![Listed::Text("Done".into())]);
    s.input(|g| g.add_slicer(slicer));
    s.frame(support::VIEWPORT);
    until_synced(&mut s, &transport);
    let last = fake.requests().pop().unwrap();
    assert!(
        last.target
            .ends_with("&where=phase%20in%20%5B%22Done%22%5D"),
        "{}",
        last.target
    );
    let status = s.source.0.status();
    assert!(!status.partial, "every matching row came");
    assert_eq!(s.source.0.row_count(), done.len(), "only they are held");
    let shown = s.source.0.count(binnacle_core::Axis::Rows) as usize;
    assert_eq!(shown, done.len());
    let snapshot = s.frame(support::VIEWPORT);
    let counted = binnacle_ratatui::row_count(&snapshot);
    assert!(counted.ends_with(" of 10,050 rows"), "{counted}");
    for id in &done {
        assert!(
            s.source.0.row_key(std::slice::from_ref(id)).is_some(),
            "{id:?}"
        );
    }
}

#[test]
fn a_slicer_counts_its_rows_of_the_tables_and_its_checklist_keeps_the_values_it_excludes() {
    const ROWS: usize = 60;
    let fake = FakeTables::new().generate("t", ROWS).start().unwrap();
    let transport = Blocking::new(Options::new(Endpoint::Tcp(fake.url().unwrap()))).unwrap();
    let mut s = scenario(DescribedSource::new("t", Scope::new("flotilla")));
    until_synced(&mut s, &transport);
    let phase = s.source.0.column_key("phase").unwrap();
    let running = Condition::one_of(phase, vec![Listed::Text("Running".into())]);
    s.input(|g| g.add_slicer(running));
    s.frame(support::VIEWPORT);
    until_synced(&mut s, &transport);
    let shown = s.source.0.count(binnacle_core::Axis::Rows);
    assert!(shown > 0 && shown < ROWS as u64);
    let snapshot = s.frame(support::VIEWPORT);
    assert_eq!(
        binnacle_ratatui::row_count(&snapshot),
        format!("{shown} of {ROWS} rows")
    );
    let query = binnacle_core::DistinctQuery::new(10);
    let listed = s
        .source
        .distinct(binnacle_core::Axis::Rows, phase, &query)
        .unwrap();
    let texts: Vec<&str> = listed.values.iter().map(|v| v.text.as_str()).collect();
    assert_eq!(texts, ["Done", "Pending", "Running", "Stalled"]);
}
