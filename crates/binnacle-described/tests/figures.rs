//! What a watch sends besides its rows: a `count` gives the rows' Totals,
//! and a `values` a column's Distinct Values.

mod support;

use binnacle_core::{
    Axis, Blanks, Condition, DistinctQuery, DistinctValue, FilterSpec, Value as Listed, ViewSpec,
};
use binnacle_described::{DescribedSource, Message, Scope, Wire};
use binnacle_memory::PumpSource;
use binnacle_testkit::Scenario;
use serde_json::json;
use support::{Table, VIEWPORT, feed, scenario};

/// A convoys stream of `n` Running rows, its phase filterable.
fn convoys(n: usize) -> Vec<Message> {
    let table = json!({"type": "table", "schema": {"primaryKey": ["name"], "fields": [
        {"name": "name", "type": "string", "whereOperators": ["==", "in"]},
        {"name": "phase", "type": "string", "whereOperators": ["==", "in", "has"]}
    ]}});
    let rows: Vec<_> = (0..n)
        .map(|i| json!({"name": format!("c{i:02}"), "phase": "Running"}))
        .collect();
    let rows = json!({"type": "rows", "cursor": null, "rows": rows});
    [table, rows, json!({"type": "synced"})]
        .iter()
        .map(|m| Message::from_json(m).unwrap())
        .collect()
}

/// Feeds `messages`, taking each request between them as a host does.
fn stream(s: &mut Scenario<Table>, messages: Vec<Message>) {
    for message in messages {
        feed(s, message);
        s.source.0.poll_request();
    }
}

/// A scenario over the convoys stream of `n` rows.
fn fed(n: usize) -> Scenario<Table> {
    let mut s = scenario(DescribedSource::new("convoys", Scope::new("flotilla")));
    s.source.0.poll_request();
    stream(&mut s, convoys(n));
    assert_eq!(s.source.0.row_count(), n);
    s
}

fn count(count: u64, total: Option<u64>) -> Message {
    Message::Count {
        count,
        total,
        estimate: false,
    }
}

fn values(entries: &[(Option<&str>, u64)], more: bool) -> Message {
    Message::Values {
        field: "phase".into(),
        values: (entries.iter())
            .map(|(v, n)| (v.map_or(Wire::Null, |v| Wire::Text(v.into())), *n))
            .collect(),
        more,
        sequence: None,
    }
}

#[test]
fn the_rows_totals_are_the_total_a_count_gives() {
    let mut s = fed(12);
    feed(&mut s, count(12, Some(340)));
    assert_eq!(s.source.0.totals().rows, Some(340));
    let snapshot = s.frame(VIEWPORT);
    assert_eq!(snapshot.total(Axis::Rows), Some(340));
    assert_eq!(binnacle_ratatui::row_count(&snapshot), "12 of 340 rows");
}

#[test]
fn a_count_from_an_older_server_without_a_total_leaves_the_totals_as_the_rows_held() {
    let mut s = fed(12);
    feed(&mut s, count(12, Some(340)));
    feed(&mut s, count(5, None));
    assert_eq!(s.source.0.totals().rows, Some(12));
    let snapshot = s.frame(VIEWPORT);
    assert_eq!(binnacle_ratatui::row_count(&snapshot), "12 rows");
}

#[test]
fn a_filtered_columns_distinct_values_are_the_servers_with_its_counts() {
    let mut s = fed(3);
    let phase = s.source.0.column_key("phase").unwrap();
    let running = Condition::one_of(phase, vec![Listed::Text("Running".into())]);
    let filter = FilterSpec::default().with_conditions(vec![running].into());
    s.source
        .0
        .view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    s.source.0.poll_request();
    let mut again = convoys(3);
    again.insert(
        1,
        values(
            &[(Some("Running"), 7), (Some("Landing"), 2), (None, 1)],
            false,
        ),
    );
    stream(&mut s, again);
    let value = |text: &str, count| DistinctValue {
        text: text.into(),
        count: Some(count),
    };
    let query = DistinctQuery::new(10);
    let listed = s.source.distinct(Axis::Rows, phase, &query).unwrap();
    assert_eq!(listed.values, [value("Landing", 2), value("Running", 7)]);
    assert_eq!(listed.blanks, Some(Blanks::Counted(1)));
    assert!(!listed.partial);
    let searched = DistinctQuery::new(10).search("land");
    let listed = s.source.distinct(Axis::Rows, phase, &searched).unwrap();
    assert_eq!(listed.values, [value("Landing", 2)]);
    assert_eq!(listed.blanks, None, "a search lists no blanks");
    let listed = s
        .source
        .distinct(Axis::Rows, phase, &DistinctQuery::new(1))
        .unwrap();
    assert_eq!(listed.values, [value("Landing", 2)]);
    assert!(listed.partial, "the cap");
    feed(&mut s, values(&[(Some("Landing"), 0)], true));
    let listed = s.source.distinct(Axis::Rows, phase, &query).unwrap();
    assert_eq!(listed.values, [value("Running", 7)]);
    assert!(listed.partial, "there were more");
}
