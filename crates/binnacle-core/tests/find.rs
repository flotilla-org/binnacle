//! Seam 1 scenarios for find: setting the Find Query emits `FindRequested`,
//! Find Next and Find Previous emit `FindStepRequested` from the current
//! match or the focused cell, and the answer moves focus to the match and
//! scrolls it into view without touching the selection. The current match
//! goes when its row leaves or find closes.

use binnacle_core::{
    Action, Axis, AxisConfig, Effect, FindDirection, FindMatch, FindQuery, FocusStep, Grid, Key,
    TextQuery, Viewport,
};
use binnacle_testkit::{RecordTable, Scenario};

const V: Viewport = Viewport {
    width: 26.0,
    height: 6.0,
};

/// Eight records over three columns: column 0 holds `[5, 13, 8, 31, 9, 23, 7,
/// 3]`, column 1 the row's index times 10, column 2 nothing. A 1-high header
/// band and 4-wide row header band; rows of 1, columns of 6 with a Gap of 1.
/// Five rows fit.
fn records() -> Scenario<RecordTable> {
    let col0 = [5, 13, 8, 31, 9, 23, 7, 3];
    let rows = (0..8)
        .map(|i| vec![Some(col0[i]), Some(i as i64 * 10), None])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(8, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, RecordTable::new(rows, 3, [0, 1]))
}

fn find(text: &str) -> Option<FindQuery> {
    Some(FindQuery::new(TextQuery::new(text)))
}

fn cell(row: u64, col: u64) -> FindMatch {
    FindMatch::new(Key(row), Key(col))
}

#[test]
fn find_next_steps_through_the_matches_by_rows_and_wraps() {
    let mut s = records();
    let mut out = s.step("eight records", V);
    s.input(|g| g.set_find(find("3")));
    out += &s.step(
        "find \"3\": 13, 31 and 30, 23, 3 match; nothing is focused yet",
        V,
    );
    for label in [
        "next: 13, the first match from the start",
        "next: 31 in row 3",
        "next: 30, in the same row, column 1",
        "next: 23",
        "next: 3 in row 7, scrolled into view",
        "next wraps to 13",
    ] {
        s.input(|g| g.action(Action::FindNext));
        out += &s.step(label, V);
    }
    s.input(|g| g.action(Action::FindPrevious));
    out += &s.step("previous: back to 3 in row 7", V);
    insta::assert_snapshot!(out);
}

#[test]
fn by_columns_steps_down_each_column_in_turn() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_find(Some(FindQuery::new(TextQuery::new("3")).by(Axis::Cols))));
    let mut stepped = Vec::new();
    for _ in 0..6 {
        s.input(|g| g.action(Action::FindNext));
        s.frame(V);
        stepped.push(s.grid.snapshot().find_match());
    }
    let order = [
        cell(1, 0),
        cell(3, 0),
        cell(5, 0),
        cell(7, 0),
        cell(3, 1),
        cell(1, 0),
    ];
    assert_eq!(stepped, order.map(Some));
}

#[test]
fn a_step_starts_from_the_focused_cell_and_leaves_the_selection() {
    let mut s = records();
    s.frame(V);
    // Focus row 2, column 1, then select down to row 4.
    s.input(|g| g.set_focus(Key(2), Key(1)));
    s.input(|g| {
        for _ in 0..2 {
            g.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: true,
            });
        }
    });
    let mut out = s.step("rows 2 to 4 of column 1 selected, focus on row 4", V);
    s.input(|g| g.set_find(find("3")));
    s.input(|g| g.action(Action::FindPrevious));
    let effects = s.effects();
    assert!(effects.contains(&Effect::FindStepRequested {
        query: find("3").unwrap(),
        from: Some(cell(4, 1)),
        direction: FindDirection::Previous,
    }));
    out += &s.step(
        "previous from the focused cell: 30 in row 3, the range stays",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn the_current_match_goes_when_a_filter_takes_its_row() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_find(find("3"));
        g.action(Action::FindNext);
    });
    s.frame(V);
    s.input(|g| g.action(Action::FindNext));
    let mut out = s.step("find \"3\", stepped twice: 31 in row 3", V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("2"))));
    out += &s.step(
        "a quick filter on \"2\" keeps 20 and 23: row 3 leaves, and the match with it",
        V,
    );
    s.input(|g| g.action(Action::FindNext));
    out += &s.step(
        "next steps from the focused cell, where focus fell: 23 in row 5",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn closing_find_drops_the_match_and_an_answer_for_an_old_query_is_ignored() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_find(find("3"));
        g.action(Action::FindNext);
    });
    s.frame(V);
    assert_eq!(s.grid.snapshot().find_match(), Some(cell(1, 0)));
    s.input(|g| g.set_find(None));
    assert_eq!(
        s.effects(),
        vec![Effect::FindRequested { query: None }],
        "closing find tells the host, to clear the marks"
    );
    s.frame(V);
    assert_eq!(s.grid.snapshot().find_match(), None);
    // A step asked for "3", answered after the query became "9".
    s.input(|g| {
        g.set_find(find("3"));
        g.action(Action::FindNext);
        g.set_find(find("9"));
    });
    s.frame(V);
    let snap = s.grid.snapshot();
    assert_eq!(snap.find_match(), None, "the answer for \"3\" is ignored");
    assert_eq!(snap.find().map(|f| f.query.text.as_str()), Some("9"));
    // An empty text closes find; setting the query in force asks nothing.
    s.input(|g| {
        g.set_find(find("9"));
        g.set_find(find(""));
    });
    assert_eq!(s.effects(), vec![Effect::FindRequested { query: None }]);
    s.input(|g| g.action(Action::FindNext));
    assert_eq!(s.effects(), vec![], "no step while find is closed");
}

#[test]
fn a_step_with_no_match_answers_none_and_focus_stays() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_focus(Key(2), Key(0)));
    s.input(|g| {
        g.set_find(find("xyz"));
        g.action(Action::FindNext);
    });
    s.frame(V);
    let snap = s.grid.snapshot();
    assert_eq!(snap.find_match(), None);
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(Key(2)));
}

#[test]
fn the_current_match_stays_when_a_filter_keeps_its_row() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_find(find("3"));
        g.action(Action::FindNext);
    });
    s.frame(V);
    s.input(|g| g.action(Action::FindNext));
    s.frame(V);
    assert_eq!(s.grid.snapshot().find_match(), Some(cell(3, 0)));
    // "3" keeps 13, 31, 30, 23 and 3: row 3 stays.
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3"))));
    s.frame(V);
    s.frame(V);
    assert_eq!(s.grid.snapshot().find_match(), Some(cell(3, 0)));
}
