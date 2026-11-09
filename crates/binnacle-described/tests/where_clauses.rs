//! The `where` clauses a Described Table source sends for its filter: their
//! shapes, what is never sent, and when the source asks for its watch again.

mod support;

use binnacle_core::{
    Axis, Comparison, Condition, FilterSpec, Grouping, Key, Scalar, TextQuery, Value as Listed,
    ViewSpec,
};
use binnacle_described::{DescribedSource, HostRequest, Message, Scope, WatchRequest};
use binnacle_memory::PumpSource;
use serde_json::json;
use support::{Table, scenario};

const ORDERED: [&str; 8] = ["==", "!=", "<", "<=", ">", ">=", "in", "has"];

/// A convoys table: most fields filterable as flotillad's are, `note` not
/// at all and `state` by `==` alone.
fn convoys() -> Vec<Message> {
    let table = json!({"type": "table", "schema": {"primaryKey": ["name"], "fields": [
        {"name": "name", "type": "string", "whereOperators": ORDERED},
        {"name": "phase", "type": "string", "whereOperators": ORDERED},
        {"name": "retries", "type": "integer", "whereOperators": ORDERED},
        {"name": "started", "type": "datetime", "whereOperators": ORDERED},
        {"name": "note", "type": "string", "whereOperators": []},
        {"name": "state", "type": "string", "whereOperators": ["=="]},
        {"name": "kind", "type": "string", "whereOperators": ORDERED,
            "constraints": {"enum": ["Fix", "Feature"]}},
        {"name": "reference", "type": "string", "whereOperators": ORDERED},
        {"name": "reference.home", "type": "string", "whereOperators": ORDERED},
        {"name": "status.phase", "type": "string", "whereOperators": ORDERED},
        {"name": "spec", "type": "object", "whereOperators": []}
    ]}});
    let row = |name: &str, phase: Option<&str>, retries: Option<i64>, note: &str| {
        json!({"name": name, "phase": phase, "retries": retries, "note": note,
            "started": "2026-11-01T09:00:00Z", "state": "open", "kind": "Fix", "spec": {}})
    };
    let rows = json!({"type": "rows", "cursor": null, "rows": [
        row("a", Some("Running"), Some(0), "tab\there"),
        row("b", Some("Landing"), Some(7), "tab here"),
        row("c", Some("Done"), None, ""),
        row("d", None, Some(5), "x"),
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
fn each_condition_is_one_clause_after_the_scopes_in_its_order() {
    let mut source = DescribedSource::new(
        "convoys",
        Scope::new("flotilla").with_where(r#"kind == "Fix""#),
    );
    source.poll_request();
    for message in convoys() {
        source.receive(message);
    }
    let started = Comparison::AtLeast(Scalar::date(2026, 11, 1));
    let conditions = vec![
        Condition::one_of(key(&source, "phase"), vec![Listed::Blank]),
        Condition::compare(key(&source, "started"), started),
        // The same clause twice is sent once.
        Condition::one_of(key(&source, "phase"), vec![Listed::Blank]),
    ];
    let filter = FilterSpec::default().with_conditions(conditions.into());
    source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    let Some(HostRequest::OpenWatch(request)) = source.poll_request() else {
        panic!("the watch again")
    };
    assert_eq!(
        request.wheres,
        [
            "!has(phase)",
            r#"started >= timestamp("2026-11-01T00:00:00Z")"#
        ]
    );
    assert_eq!(
        request.query(),
        "watch&limit=10000&namespace=flotilla\
         &where=kind%20%3D%3D%20%22Fix%22\
         &where=%21has%28phase%29\
         &where=started%20%3E%3D%20timestamp%28%222026-11-01T00%3A00%3A00Z%22%29"
    );
}

#[test]
fn fields_are_named_as_flotillad_names_them() {
    let one =
        |name: &'static str| sent(move |s| vec![Condition::one_of(key(s, name), texts(&["x"]))]);
    assert_eq!(one("status.phase"), [r#"status.phase in ["x"]"#]);
    // `reference` is filterable too, so the dotted name is one segment.
    assert_eq!(one("reference.home"), [r#"`reference.home` in ["x"]"#]);
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
fn a_condition_whose_field_lacks_its_operator_is_filtered_client_side_only() {
    let mut source = source();
    let note = Condition::one_of(key(&source, "note"), texts(&["x"]));
    let few = Condition::compare(
        key(&source, "retries"),
        Comparison::Less(Scalar::Number(1.0)),
    );
    let open = Condition::one_of(key(&source, "state"), texts(&["open", "shut"]));
    let filter = FilterSpec::default().with_conditions(vec![note.clone()].into());
    source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert_eq!(names(&mut source), ["d"]);
    assert_eq!(source.poll_request(), None, "nothing to send");
    // `state` takes `==` but not `in`: its OneOf is `==` joined by `||`.
    let filter = FilterSpec::default().with_conditions(vec![note, few, open].into());
    source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert_eq!(
        source.wheres(),
        ["retries < 1", r#"state == "open" || state == "shut""#]
    );
}

#[test]
fn what_no_clause_can_list_stays_client_side() {
    // A tab shows as a space, so text with a space can't be listed; nor
    // can an integer with a fraction be equal. A value outside the enum
    // is in no row, and is left out.
    let space = sent(|s| vec![Condition::one_of(key(s, "name"), texts(&["tab here"]))]);
    assert_eq!(space, [] as [String; 0]);
    let fraction = Comparison::Equal(Scalar::Number(2.5));
    assert_eq!(
        sent(|s| vec![Condition::compare(key(s, "retries"), fraction)]),
        [] as [String; 0]
    );
    let kinds = sent(|s| vec![Condition::one_of(key(s, "kind"), texts(&["Fix", "Chore"]))]);
    assert_eq!(kinds, [r#"kind in ["Fix"]"#]);
    let none = sent(|s| vec![Condition::one_of(key(s, "kind"), texts(&["Chore"]))]);
    assert_eq!(none, ["false"], "keeps nothing");
}

#[test]
fn a_native_condition_goes_as_it_is_and_the_source_keeps_every_row_for_it() {
    let mut source = source();
    let phase = key(&source, "phase");
    assert!(source.support(Axis::Rows, phase).native);
    let native = Condition::native(phase, r#"phase == "Done" && retries > 1"#);
    let filter = FilterSpec::default().with_conditions(vec![native.clone()].into());
    let viewed = source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert_eq!(
        viewed.applied.rows.filter.conditions.conditions(),
        std::slice::from_ref(&native)
    );
    assert_eq!(source.wheres(), [r#"phase == "Done" && retries > 1"#]);
    assert_eq!(
        names(&mut source),
        ["a", "b", "c", "d"],
        "the server narrows"
    );
    // With `not`, canonical text is negated; other text can't be.
    let negated = native.clone().not(true);
    let filter = FilterSpec::default().with_conditions(vec![negated.clone()].into());
    source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert_eq!(source.wheres(), [r#"!(phase == "Done" && retries > 1)"#]);
    let loose = Condition::native(phase, "phase=='Done'").not(true);
    let filter = FilterSpec::default().with_conditions(vec![loose].into());
    let viewed = source.view(&ViewSpec::default().with_filter(Axis::Rows, filter));
    assert!(
        viewed.applied.rows.filter.conditions.is_empty(),
        "not applied"
    );
    assert_eq!(source.wheres(), [] as [String; 0]);
}

#[test]
fn a_comparison_a_grouped_axis_reads_after_aggregating_is_not_sent() {
    let mut source = source();
    let (phase, retries) = (key(&source, "phase"), key(&source, "retries"));
    let mut grouping = Grouping::rows_by(phase);
    grouping
        .values
        .push(binnacle_core::AggregateValue::new(Some(retries), "sum"));
    let having = Condition::compare(retries, Comparison::Greater(Scalar::Number(1.0)));
    let slicer = Condition::one_of(phase, vec![Listed::Blank]).not(true);
    grouping.slicers.push(slicer);
    let spec = ViewSpec::default()
        .with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(vec![having].into()),
        )
        .with_grouping(Some(grouping));
    source.view(&spec);
    assert_eq!(source.wheres(), ["has(phase)"]);
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
    let [HostRequest::OpenWatch(request)] = asked.as_slice() else {
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
    let [HostRequest::OpenWatch(request)] = asked.as_slice() else {
        panic!("{asked:?}")
    };
    assert_eq!(request.wheres, [] as [String; 0]);
}

#[test]
fn the_new_watch_replaces_the_rows_in_place_and_the_old_streams_messages_are_dropped() {
    let mut s = scenario(source());
    let phase = key(&s.source.0, "phase");
    let keys = s.source.0.row_keys().to_vec();
    s.input(|g| g.add_slicer(Condition::one_of(phase, texts(&["Running", "Landing"]))));
    s.frame(support::VIEWPORT);
    assert!(!s.source.0.status().synced, "a resync began");
    // The old stream goes on until the host takes the request.
    let late = json!({"type": "upsert", "key": ["e"], "row": {"name": "e", "phase": "Running"}});
    support::feed(&mut s, Message::from_json(&late).unwrap());
    assert_eq!(s.source.0.row_count(), 4, "dropped");
    assert!(matches!(
        s.source.0.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
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
    assert_eq!(support::column_down(&mut s, "name"), ["a", "b"]);
}
