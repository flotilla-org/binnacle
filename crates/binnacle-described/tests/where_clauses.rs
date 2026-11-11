//! The `where` clauses a Described Table source sends for its filter: their
//! shapes, that the server never gets one it would refuse, and when the
//! source asks for its watch again.

mod support;

use binnacle_core::{
    AggregateValue, Axis, Comparison, Condition, FilterSpec, Grouping, Key, Scalar,
    Value as Listed, ViewSpec,
};
use binnacle_described::{DescribedSource, HostRequest, Message, Scope};
use binnacle_memory::PumpSource;
use serde_json::json;
use support::scenario;

const ORDERED: [&str; 8] = ["==", "!=", "<", "<=", ">", ">=", "in", "has"];

/// A convoys table: `note` filterable by nothing, `state` by `==` alone,
/// and `reference.home` beside a filterable `reference`.
fn convoys() -> Vec<Message> {
    let table = json!({"type": "table", "schema": {"primaryKey": ["name"], "fields": [
        {"name": "name", "type": "string", "whereOperators": ORDERED},
        {"name": "phase", "type": "string", "whereOperators": ORDERED},
        {"name": "retries", "type": "integer", "whereOperators": ORDERED},
        {"name": "note", "type": "string", "whereOperators": []},
        {"name": "state", "type": "string", "whereOperators": ["=="]},
        {"name": "reference", "type": "string", "whereOperators": ORDERED},
        {"name": "reference.home", "type": "string", "whereOperators": ORDERED}
    ]}});
    let row = |name: &str, phase: Option<&str>, retries: Option<i64>| json!({"name": name, "phase": phase, "retries": retries, "note": "", "state": "open"});
    let rows = json!({"type": "rows", "cursor": null, "rows": [
        row("a", Some("Running"), Some(0)),
        row("b", Some("Landing"), Some(7)),
        row("c", Some("Done"), None),
        row("d", None, Some(5)),
    ]});
    [table, rows, json!({"type": "synced"})]
        .iter()
        .map(|m| Message::from_json(m).unwrap())
        .collect()
}

/// A source fed the convoys, its first request taken.
fn source() -> DescribedSource {
    let mut source = DescribedSource::new("convoys", Scope::new("flotilla"));
    assert!(source.poll_request().is_some());
    for message in convoys() {
        source.receive(message);
    }
    source
}

fn key(source: &DescribedSource, name: &str) -> Key {
    source.column_key(name).unwrap()
}

fn texts(values: &[&str]) -> Vec<Listed> {
    values.iter().map(|v| Listed::Text((*v).into())).collect()
}

/// The clauses `conditions` on the rows send.
fn sent(conditions: impl FnOnce(&DescribedSource) -> Vec<Condition>) -> Vec<String> {
    let mut source = source();
    let conditions = conditions(&source);
    let filter = FilterSpec::default().with_conditions(conditions.into());
    source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    source.wheres().to_vec()
}

#[test]
fn the_owner_approved_shapes() {
    let one_of = sent(|s| {
        vec![Condition::one_of(
            key(s, "phase"),
            texts(&["Running", "Landing"]),
        )]
    });
    assert_eq!(one_of, [r#"phase in ["Running", "Landing"]"#]);
    let less = Comparison::Less(Scalar::Number(5.0));
    let not = sent(|s| vec![Condition::compare(key(s, "retries"), less).not(true)]);
    assert_eq!(not, ["!has(retries) || !(retries < 5)"]);
}

#[test]
fn a_condition_whose_field_lacks_its_operator_is_not_sent() {
    // The server would refuse it: `note` takes no operator, `state` no `<`.
    let note = sent(|s| vec![Condition::one_of(key(s, "note"), texts(&["x"]))]);
    assert_eq!(note, [] as [String; 0]);
    let less = Comparison::Less(Scalar::Number(1.0));
    let state = sent(|s| vec![Condition::compare(key(s, "state"), less)]);
    assert_eq!(state, [] as [String; 0]);
}

#[test]
fn a_dotted_name_is_one_segment_where_its_prefix_is_a_field_too() {
    let home = sent(|s| vec![Condition::one_of(key(s, "reference.home"), texts(&["x"]))]);
    assert_eq!(home, [r#"`reference.home` in ["x"]"#]);
}

#[test]
fn a_comparison_a_grouped_axis_reads_after_aggregating_is_not_sent() {
    let mut source = source();
    let (phase, retries) = (key(&source, "phase"), key(&source, "retries"));
    let mut grouping = Grouping::rows_by(phase);
    (grouping.values).push(AggregateValue::new(Some(retries), "sum"));
    let having = Condition::compare(retries, Comparison::Greater(Scalar::Number(1.0)));
    let filter = FilterSpec::default().with_conditions(vec![having].into());
    let spec = (ViewSpec::default().with_filter(Axis::Rows, filter)).with_grouping(Some(grouping));
    source.view(&spec);
    assert_eq!(source.wheres(), [] as [String; 0]);
}

#[test]
fn native_text_goes_in_its_canonical_spelling_and_text_that_does_not_parse_is_not_applied() {
    let mut source = source();
    let phase = key(&source, "phase");
    let natives = |texts: &[&str]| {
        let conditions: Vec<_> = texts.iter().map(|t| Condition::native(phase, *t)).collect();
        ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(conditions.into()),
        )
    };
    let viewed = source.view(&natives(&["phase=='Done'", "phase =="]));
    assert_eq!(source.wheres(), [r#"phase == "Done""#]);
    let applied = viewed.applied.rows.filter.conditions;
    assert_eq!(
        applied.conditions(),
        [Condition::native(phase, "phase=='Done'")]
    );
}

#[test]
fn a_slicer_change_asks_for_the_watch_once_and_the_old_streams_messages_are_dropped() {
    let mut s = scenario(source());
    assert_eq!(s.source.0.poll_request(), None);
    let phase = key(&s.source.0, "phase");
    let keys = s.source.0.row_keys().to_vec();
    s.input(|g| g.add_slicer(Condition::one_of(phase, texts(&["Running", "Landing"]))));
    s.frame(support::VIEWPORT);
    // The old stream goes on until the host takes the request.
    let late = json!({"type": "upsert", "key": ["e"], "row": {"name": "e", "phase": "Running"}});
    support::feed(&mut s, Message::from_json(&late).unwrap());
    assert_eq!(s.source.0.row_count(), 4, "dropped");
    let asked: Vec<_> = std::iter::from_fn(|| s.source.0.poll_request()).collect();
    let [HostRequest::OpenWatch(request)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert_eq!(request.wheres, [r#"phase in ["Running", "Landing"]"#]);
    let mut messages = convoys();
    if let Message::Rows(rows) = &mut messages[1] {
        rows.rows
            .retain(|r| r["name"].as_str().is_some_and(|n| n == "a" || n == "b"));
    }
    for message in messages {
        support::feed(&mut s, message);
    }
    assert!(s.source.0.status().synced);
    assert_eq!(s.source.0.row_keys(), &keys[..2], "the same Keys");
}
