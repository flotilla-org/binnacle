//! The Grid over a Described Table, through the scenario harness: sorting
//! by the total order, filters and find on Stable Text, Distinct Values and
//! grouping, on the made `crews` table.

mod support;

use binnacle_core::{
    Action, AggregateValue, Axis, Blanks, DistinctQuery, DistinctValue, FilterSpec, FindDirection,
    FindQuery, Grouping, Key, SortColumn, SortSpec, TextQuery, ViewSpec,
};
use binnacle_memory::PumpSource;
use binnacle_testkit::Scenario;
use support::{Table, column_down, picture, replay, scenario};

fn crews() -> Scenario<Table> {
    scenario(replay("crews.ndjson"))
}

fn key(s: &Scenario<Table>, name: &str) -> Key {
    s.source.0.column_key(name).unwrap()
}

fn sorted(s: &mut Scenario<Table>, column: SortColumn) {
    let spec = ViewSpec::default().with_sort(Axis::Rows, SortSpec::new(vec![column]));
    s.input(|g| g.request_view(spec));
}

#[test]
fn the_crews_table_as_it_comes() {
    let mut s = crews();
    insta::assert_snapshot!(picture(&mut s));
}

#[test]
fn sorting_puts_nulls_lowest_then_invalid_values_then_typed_ones_both_ways() {
    let mut s = crews();
    let started = key(&s, "started");
    sorted(&mut s, SortColumn::ascending(started));
    assert_eq!(
        column_down(&mut s, "started"),
        [
            "∅",
            "soon",
            "2026-10-27 00:00",
            "2026-10-28 09:00",
            // Echo, one nanosecond before alpha.
            "2026-10-29 10:00",
            "2026-10-29 10:00",
        ]
    );
    assert_eq!(column_down(&mut s, "name")[4..], ["echo", "alpha"]);
    sorted(&mut s, SortColumn::descending(started));
    assert_eq!(
        column_down(&mut s, "name"),
        ["alpha", "echo", "delta", "foxtrot", "charlie", "bravo"],
        "descending reverses all of it, nulls last"
    );
    let cpus = key(&s, "cpus");
    sorted(&mut s, SortColumn::ascending(cpus));
    assert_eq!(
        column_down(&mut s, "cpus"),
        ["∅", "many", "2", "4", "8", "18446744073709551615"],
        "u64::MAX sorts above every i64"
    );
    let load = key(&s, "load");
    sorted(&mut s, SortColumn::descending(load));
    assert_eq!(
        column_down(&mut s, "load"),
        ["3.5%", "2%", "1.25%", "0.5%", "high", "∅"]
    );
}

#[test]
fn a_quick_filter_matches_stable_text_never_drawn_text() {
    let mut s = crews();
    // The Started column has the `time` role: it draws relative ("3m
    // ago") but filters on its absolute Stable Text.
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("2026-10-29"))));
    assert_eq!(column_down(&mut s, "name"), ["alpha", "echo"]);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("ago"))));
    assert_eq!(column_down(&mut s, "name"), Vec::<String>::new());
    // Units are part of the text, as drawn.
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3.5%"))));
    assert_eq!(column_down(&mut s, "name"), ["delta"]);
}

#[test]
fn a_column_text_condition_filters_on_formatted_text() {
    let mut s = crews();
    let cpus = key(&s, "cpus");
    let condition = binnacle_core::Condition::text(cpus, TextQuery::new("1844"));
    let filter = FilterSpec::default().with_conditions(vec![condition].into());
    s.input(|g| g.request_view(ViewSpec::default().with_filter(Axis::Rows, filter)));
    assert_eq!(column_down(&mut s, "name"), ["echo"]);
}

#[test]
fn find_steps_through_cells_whose_stable_text_matches() {
    let mut s = crews();
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new("brie")))));
    s.frame(support::VIEWPORT);
    let mut found = Vec::new();
    for _ in 0..3 {
        s.input(|g| g.action(Action::FindNext));
        let at = s.frame(support::VIEWPORT).find_match().unwrap();
        let source = &s.source.0;
        let name = source.column_key("name").unwrap();
        found.push(support::text(source, at.row, name));
    }
    assert_eq!(found, ["charlie", "delta", "charlie"], "and wraps");
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new("2026-10-27 00:00")))));
    s.input(|g| g.action(Action::FindNext));
    let at = s.frame(support::VIEWPORT).find_match().unwrap();
    assert_eq!(
        support::text(&s.source.0, at.row, key(&s, "name")),
        "foxtrot"
    );
}

#[test]
fn distinct_values_list_nulls_and_empty_text_apart() {
    let mut s = crews();
    let note = key(&s, "note");
    let distinct = s.source.distinct(Axis::Rows, note, &DistinctQuery::new(10));
    let distinct = distinct.unwrap();
    let value = |text: &str, count| DistinctValue {
        text: text.into(),
        count: Some(count),
    };
    assert_eq!(
        distinct.values,
        [value("", 2), value("check disk", 1), value("late", 1)],
        "the empty text is a value"
    );
    assert_eq!(
        distinct.blanks,
        Some(Blanks::Counted(2)),
        "the nulls are blanks"
    );
    let started = key(&s, "started");
    let distinct = s
        .source
        .distinct(Axis::Rows, started, &DistinctQuery::new(10));
    let texts: Vec<String> = distinct
        .unwrap()
        .values
        .into_iter()
        .map(|v| v.text)
        .collect();
    assert_eq!(
        texts,
        [
            "soon",
            "2026-10-27 00:00",
            "2026-10-28 09:00",
            "2026-10-29 10:00"
        ],
        "in sort order, as Stable Text"
    );
}

#[test]
fn the_rows_group_by_a_column() {
    let mut s = crews();
    let phase = key(&s, "phase");
    assert!(s.source.support(Axis::Rows, phase).group);
    let spec = ViewSpec::default().with_grouping(Some(Grouping::rows_by(phase)));
    s.input(|g| g.request_view(spec));
    insta::assert_snapshot!(picture(&mut s));
}

#[test]
fn aggregates_take_numbers_from_integers_and_numbers_and_skip_invalid_values() {
    let mut s = crews();
    let (phase, cpus, load) = (key(&s, "phase"), key(&s, "cpus"), key(&s, "load"));
    let names = |k| -> Vec<String> {
        (s.source.0.aggregates(k).into_iter())
            .map(|a| a.name)
            .collect()
    };
    assert!(names(load).contains(&"sum".to_string()));
    assert!(names(cpus).contains(&"sum".to_string()));
    let mut grouping = Grouping::rows_by(phase);
    grouping.values = vec![
        AggregateValue::of(cpus, "sum"),
        AggregateValue::of(load, "sum"),
    ];
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    insta::assert_snapshot!(picture(&mut s));
}

#[test]
fn rows_that_tie_break_on_the_primary_key_ascending_both_ways() {
    let mut s = crews();
    let note = key(&s, "note");
    sorted(&mut s, SortColumn::ascending(note));
    assert_eq!(
        column_down(&mut s, "name"),
        ["bravo", "foxtrot", "alpha", "delta", "charlie", "echo"],
        "two nulls, then two empty texts, each pair by name"
    );
    sorted(&mut s, SortColumn::descending(note));
    assert_eq!(
        column_down(&mut s, "name"),
        ["echo", "charlie", "alpha", "delta", "bravo", "foxtrot"],
        "ties still by name ascending"
    );
    let applied = s.frame(support::VIEWPORT).applied_view().clone();
    assert_eq!(
        applied.axis(Axis::Rows).sort,
        SortSpec::new(vec![SortColumn::descending(note)]),
        "the primary key's term stays hidden"
    );
}

#[test]
fn find_counts_and_marks_a_match_in_a_nested_column_only_while_find_reaches_it() {
    let mut source = replay("crews.ndjson");
    let (name, spec) = (key_of(&source, "name"), key_of(&source, "spec"));
    let query = FindQuery::new(TextQuery::new("rust"));
    let alpha = (source.row_keys().iter().copied())
        .find(|&r| support::text(&source, r, name) == "alpha")
        .unwrap();
    assert_eq!(source.find_count(&query).total, 1);
    assert_eq!(
        source.find_marks(&[alpha], &[name, spec], &query),
        [false, true]
    );
    let step = source.find_next(&query, None, FindDirection::Next).unwrap();
    assert_eq!((step.at.row, step.at.col), (alpha, spec));
    // Hidden, the In-Memory Table Source searches only the columns shown,
    // so find can't step to it, and neither marks nor counts it.
    source.view(&ViewSpec::default().with_hidden(Axis::Cols, vec![spec]));
    assert_eq!(source.find_next(&query, None, FindDirection::Next), None);
    assert_eq!(source.find_count(&query).total, 0);
    assert_eq!(
        source.find_marks(&[alpha], &[name, spec], &query),
        [false, false]
    );
}

#[test]
fn find_marks_every_cell_find_steps_through_before_it_wraps() {
    let mut source = replay("crews.ndjson");
    let spec_key = source.column_key("spec").unwrap();
    source.view(&ViewSpec::default().with_hidden(Axis::Cols, vec![spec_key]));
    for text in ["e", "r", "2026", "check"] {
        let query = FindQuery::new(TextQuery::new(text));
        let mut stepped = std::collections::BTreeSet::new();
        let mut from = None;
        while let Some(step) = source.find_next(&query, from, FindDirection::Next) {
            if step.wrapped || !stepped.insert((step.at.row, step.at.col)) {
                break;
            }
            from = Some(step.at);
        }
        let rows = source.window(Axis::Rows, 0..source.count(Axis::Rows)).keys;
        let cols: Vec<Key> = source.columns().iter().map(|c| c.key()).collect();
        let marks = source.find_marks(&rows, &cols, &query);
        let marked: std::collections::BTreeSet<(Key, Key)> = (rows.iter())
            .flat_map(|&r| cols.iter().map(move |&c| (r, c)))
            .zip(marks)
            .filter_map(|(cell, marked)| marked.then_some(cell))
            .collect();
        assert!(!stepped.is_empty(), "{text}");
        assert_eq!(marked, stepped, "{text}");
        assert_eq!(
            source.find_count(&query).total,
            stepped.len() as u64,
            "{text}"
        );
    }
}

fn key_of(source: &binnacle_described::DescribedSource, name: &str) -> Key {
    source.column_key(name).unwrap()
}
