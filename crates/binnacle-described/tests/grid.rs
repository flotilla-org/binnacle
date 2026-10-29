//! The Grid over a Described Table, through the scenario harness: sorting
//! by the total order, filters and find on Stable Text, Distinct Values and
//! grouping, on the made `crews` table.

mod support;

use binnacle_core::{
    Action, Axis, Blanks, DistinctQuery, DistinctValue, FilterSpec, FindQuery, Grouping, Key,
    SortColumn, SortSpec, TextQuery, ViewSpec,
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
