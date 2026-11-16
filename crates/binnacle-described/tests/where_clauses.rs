//! The `where` clauses a Described Table source sends for its filter: their
//! shapes, that the server never gets one it would refuse, and when the
//! source asks for its watch again.

mod support;

use binnacle_core::{
    AggregateValue, Axis, Comparison, Condition, FilterSpec, Grouping, Key, Scalar, TextQuery,
    Value as Listed, ViewSpec,
};
use binnacle_described::{DescribedSource, HostRequest, Message, Scope, WatchRequest};
use binnacle_memory::PumpSource;
use serde_json::json;
use support::{Table, scenario};

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

/// The rows' names shown, in order.
fn names(source: &mut DescribedSource) -> Vec<String> {
    let name = key(source, "name");
    let keys = source.window(Axis::Rows, 0..source.count(Axis::Rows)).keys;
    (keys.iter())
        .map(|&k| support::text(source, k, name))
        .collect()
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
fn native_text_goes_in_its_canonical_spelling_and_text_the_server_would_refuse_is_not_applied() {
    let mut source = source();
    let phase = key(&source, "phase");
    let natives = |texts: &[&str]| {
        let conditions: Vec<_> = texts.iter().map(|t| Condition::native(phase, *t)).collect();
        ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(conditions.into()),
        )
    };
    // Text that doesn't parse, a field that isn't there, one filterable by
    // nothing, an operator `state` lacks, and a value of the wrong type.
    let refused = [
        "phase ==",
        "nope == 1",
        r#"note == "x""#,
        r#"state < "x""#,
        r#"retries == "x""#,
    ];
    let viewed = source.view(&natives(&[&["phase=='Done'"], &refused[..]].concat()));
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
    let [HostRequest::OpenWatch(request), HostRequest::OpenCount(_)] = asked.as_slice() else {
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

#[test]
fn text_conditions_and_the_quick_filter_are_never_sent() {
    let mut source = source();
    let mut requests = Vec::new();
    let text = Condition::text(key(&source, "phase"), TextQuery::new("Run"));
    let filter = FilterSpec::default()
        .with_quick(Some(TextQuery::new("a")))
        .with_conditions(vec![text].into());
    let viewed = source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert_eq!(viewed.applied.rows.filter.conditions.conditions().len(), 1);
    assert_eq!(names(&mut source), ["a"]);
    requests.extend(source.poll_request());
    assert_eq!(requests, []);
    assert_eq!(source.wheres(), [] as [String; 0]);
    // Nor after the stream ends.
    source.ended();
    let Some(HostRequest::OpenWatch(WatchRequest { wheres, .. })) = source.poll_request() else {
        panic!("the watch again")
    };
    assert_eq!(wheres, [] as [String; 0]);
}

#[test]
fn adding_or_removing_a_slicer_asks_for_the_watch_once_and_a_sort_move_or_quick_filter_never() {
    let mut s = scenario(source());
    assert_eq!(s.source.0.poll_request(), None);
    let phase = key(&s.source.0, "phase");
    let running = Condition::one_of(phase, texts(&["Running", "Landing"]));
    let requests = |s: &mut binnacle_testkit::Scenario<Table>| {
        std::iter::from_fn(|| s.source.0.poll_request()).collect::<Vec<_>>()
    };
    s.input(|g| g.add_slicer(running.clone()));
    s.frame(support::VIEWPORT);
    let asked = requests(&mut s);
    let [HostRequest::OpenWatch(request), HostRequest::OpenCount(_)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert_eq!(request.wheres, [r#"phase in ["Running", "Landing"]"#]);
    assert_eq!(support::column_down(&mut s, "name"), ["a", "b"]);
    // A sort, a move and a quick filter send the same clauses.
    s.input(|g| g.cycle_sort(Axis::Rows, phase, false));
    s.frame(support::VIEWPORT);
    let name = key(&s.source.0, "name");
    s.source.0.move_items(Axis::Cols, &[name], 3).unwrap();
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("a"))));
    s.frame(support::VIEWPORT);
    assert_eq!(requests(&mut s), []);
    s.input(|g| g.remove_slicer(&running));
    s.frame(support::VIEWPORT);
    let asked = requests(&mut s);
    let [HostRequest::OpenWatch(request), HostRequest::CloseCount] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert_eq!(request.wheres, [] as [String; 0]);
}

#[test]
fn a_spec_asked_for_before_the_table_is_sent_once_the_table_comes_and_follows_later_tables() {
    let mut source = DescribedSource::new("convoys", Scope::new("flotilla"));
    assert!(source.poll_request().is_some());
    let messages = convoys();
    // The Key of a column is its field name's, so a saved spec names it
    // before the table comes.
    let probe = self::source();
    let (phase, retries) = (key(&probe, "phase"), key(&probe, "retries"));
    let running = Condition::one_of(phase, texts(&["Running"]));
    let counted = Condition::compare(retries, Comparison::Less(Scalar::Number(5.0)));
    let mut grouping = Grouping::default();
    grouping.slicers.push(counted);
    let spec = ViewSpec::default()
        .with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(vec![running].into()),
        )
        .with_grouping(Some(grouping));
    source.view(&spec);
    assert_eq!(source.poll_request(), None, "no columns to send by yet");
    let asked = |source: &mut DescribedSource, table: &serde_json::Value| {
        source.receive(Message::from_json(table).unwrap());
        let watch = std::iter::from_fn(|| source.poll_request()).find_map(|r| match r {
            HostRequest::OpenWatch(request) => Some(request),
            _ => None,
        });
        watch.expect("the watch again").wheres
    };
    let table = messages[0].to_wire().to_json();
    let both = [r#"phase in ["Running"]"#, "retries < 5"];
    assert_eq!(asked(&mut source, &table), both);
    // A table whose phase is no longer filterable takes its clause back.
    let mut unfiltered = table.clone();
    unfiltered["schema"]["fields"][1]["whereOperators"] = json!([]);
    assert_eq!(asked(&mut source, &unfiltered), ["retries < 5"]);
    assert_eq!(asked(&mut source, &table), both);
    // So does one without the Slicer's column.
    let mut removed = table.clone();
    removed["schema"]["fields"]
        .as_array_mut()
        .unwrap()
        .retain(|f| f["name"] != "retries");
    assert_eq!(asked(&mut source, &removed), [r#"phase in ["Running"]"#]);
}
