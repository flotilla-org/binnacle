//! Seam 1 scenarios for column conditions: adding, replacing and removing
//! one is a View Edit, conditions AND with each other and with the quick
//! filter, and the headers and the Snapshot's conditions show which are
//! applied and which are pending.

use binnacle_core::{
    Axis, AxisConfig, Condition, ConditionState, Effect, FilterSpec, Grid, HeaderFilter, Key,
    Support, TextQuery, TextSupport, Totals, Value, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{RecordTable, Scenario};

const V: Viewport = Viewport {
    width: 26.0,
    height: 6.0,
};

/// Eight records over three columns: column 0 holds `[5, 13, 8, 31, 9, 23, 7,
/// 3]`, column 1 the row's index times 10, column 2 nothing. A 1-high header
/// band and 4-wide row header band; rows of 1, columns of 6 with a Gap of 1.
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

fn contains(key: u64, text: &str) -> Condition {
    Condition::text(Key(key), TextQuery::new(text))
}

fn conditions(list: Vec<Condition>) -> ViewSpec {
    ViewSpec::default().with_filter(
        Axis::Rows,
        FilterSpec::default().with_conditions(list.into()),
    )
}

/// The View Specs and edits requested since the last pump.
fn requests(s: &mut Scenario<RecordTable>) -> Vec<(ViewSpec, ViewEdit)> {
    s.effects()
        .into_iter()
        .filter_map(|e| match e {
            Effect::ViewRequested { spec, edit } => Some((spec, edit)),
            _ => None,
        })
        .collect()
}

#[test]
fn column_conditions_are_view_edits_anded_with_each_other_and_the_quick_filter() {
    let mut s = records();
    let mut out = s.step("eight records", V);
    s.input(|g| g.add_condition(Axis::Rows, contains(0, "3")));
    out += &s.step(
        "column 0 contains 3: 13, 31, 23 and 3 are kept (not row 3's 30 in column 1)",
        V,
    );
    s.input(|g| g.add_condition(Axis::Rows, contains(0, "1").not(true)));
    out += &s.step("and column 0 does not contain 1: 23 and 3", V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("5"))));
    out += &s.step("and the quick filter 5: only 23, whose column 1 is 50", V);
    s.input(|g| g.remove_condition(Axis::Rows, &contains(0, "1").not(true)));
    out += &s.step("without the not: still 23 (13 and 31 have no 5)", V);
    s.input(|g| {
        g.replace_condition(
            Axis::Rows,
            &contains(0, "3"),
            Condition::text(Key(1), TextQuery::new("^[1-5]").regex(true)),
        )
    });
    out += &s.step(
        "column 1 matches ^[1-5] instead, with the quick filter 5: 23 alone (its 50)",
        V,
    );
    s.input(|g| g.set_quick_filter(Axis::Rows, None));
    out += &s.step("no quick filter: 13, 8, 31, 9 and 23 (10 to 50)", V);
    insta::assert_snapshot!(out);
}

#[test]
fn each_condition_edit_is_named_and_one_that_changes_nothing_asks_for_nothing() {
    let mut s = records();
    s.frame(V);
    let (ber, not_ber) = (contains(0, "ber"), contains(0, "ber").not(true));
    let other = Condition::text(Key(1), TextQuery::new("x").case_sensitive(true));
    s.input(|g| {
        g.add_condition(Axis::Rows, ber.clone());
        g.add_condition(Axis::Rows, contains(0, ""));
        g.add_condition(Axis::Rows, not_ber.clone());
        g.remove_condition(Axis::Rows, &other);
        g.replace_condition(Axis::Rows, &ber, other.clone());
        g.replace_condition(Axis::Rows, &other, contains(1, ""));
        g.remove_condition(Axis::Rows, &not_ber);
    });
    let add = |c: &Condition| ViewEdit::AddCondition {
        axis: Axis::Rows,
        condition: c.clone(),
    };
    let remove = |c: &Condition| ViewEdit::RemoveCondition {
        axis: Axis::Rows,
        condition: c.clone(),
    };
    assert_eq!(
        requests(&mut s),
        vec![
            (conditions(vec![ber.clone()]), add(&ber)),
            (
                conditions(vec![ber.clone(), not_ber.clone()]),
                add(&not_ber)
            ),
            (
                conditions(vec![other.clone(), not_ber.clone()]),
                ViewEdit::ReplaceCondition {
                    axis: Axis::Rows,
                    from: ber.clone(),
                    to: other.clone(),
                }
            ),
            (conditions(vec![not_ber.clone()]), remove(&other)),
            (ViewSpec::default(), remove(&not_ber)),
        ]
    );
}

#[test]
fn a_column_may_hold_the_same_condition_twice_and_removing_takes_one() {
    let mut s = records();
    s.frame(V);
    let c = contains(0, "3");
    s.input(|g| {
        g.add_condition(Axis::Rows, c.clone());
        g.add_condition(Axis::Rows, c.clone());
        g.remove_condition(Axis::Rows, &c);
    });
    let specs: Vec<ViewSpec> = requests(&mut s).into_iter().map(|(spec, _)| spec).collect();
    assert_eq!(
        specs,
        vec![
            conditions(vec![c.clone()]),
            conditions(vec![c.clone(), c.clone()]),
            conditions(vec![c.clone()]),
        ]
    );
}

#[test]
fn a_condition_query_searches_only_its_own_column() {
    let c = Condition::text(Key(4), TextQuery::new("a"));
    let kind = c.kind.clone();
    let binnacle_core::ConditionKind::Text(query) = kind else {
        panic!("a Text condition");
    };
    assert_eq!(
        query.searches,
        binnacle_core::Searched::Keys(vec![Key(4)]),
        "the condition's column, whatever the query searched"
    );
}

#[test]
fn headers_and_conditions_show_what_is_applied_and_what_is_pending() {
    let mut s = records();
    s.frame(V);
    let (a, b) = (contains(0, "3"), contains(1, "1"));
    s.input(|g| g.view_applied(conditions(vec![a.clone()]), Totals::unknown()));
    let snap = s.frame(V);
    assert_eq!(
        snap.header_filter(Key(0)),
        HeaderFilter {
            filtered: true,
            pending: false,
            not_applied: false
        }
    );
    assert_eq!(snap.header_filter(Key(1)), HeaderFilter::default());
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![(a.clone(), ConditionState::Applied)]
    );

    // Swap the condition on column 0 for one on column 1, unanswered.
    s.input(|g| g.replace_condition(Axis::Rows, &a, b.clone()));
    let _ = std::iter::from_fn(|| s.grid.poll_effect()).count();
    s.grid.begin_frame(V);
    let snap = s.grid.snapshot();
    assert_eq!(
        snap.header_filter(Key(0)),
        HeaderFilter {
            filtered: true,
            pending: true,
            not_applied: false
        },
        "applied until the answer, pending removal"
    );
    assert_eq!(
        snap.header_filter(Key(1)),
        HeaderFilter {
            filtered: false,
            pending: true,
            not_applied: false
        }
    );
    assert_eq!(snap.header_filter(Key(2)), HeaderFilter::default());
    let placed: Vec<HeaderFilter> = snap.headers(Axis::Cols).map(|h| h.filter).collect();
    assert_eq!(
        placed,
        [0, 1, 2].map(|k| snap.header_filter(Key(k))).to_vec(),
        "the header placements carry the same"
    );
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![
            (b.clone(), ConditionState::Adding),
            (a.clone(), ConditionState::Removing),
        ]
    );
    assert_eq!(snap.conditions(Axis::Cols), vec![]);

    // The source answers with a subset: the swap without the new condition,
    // which stays listed, not applied.
    s.input(|g| g.view_applied(ViewSpec::default(), Totals::unknown()));
    let snap = s.frame(V);
    assert_eq!(snap.header_filter(Key(0)), HeaderFilter::default());
    assert!(snap.header_filter(Key(1)).not_applied);
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![(b.clone(), ConditionState::NotApplied)]
    );
}

#[test]
fn support_declares_text_with_contains_by_default_and_the_snapshot_shows_it() {
    let mut s = records();
    let snap = s.frame(V);
    assert_eq!(
        snap.support(Axis::Rows, Key(0)).text,
        Some(TextSupport::new(false)),
        "contains, no regex, by default"
    );
    s.input(|g| {
        g.set_support(Axis::Rows, Key(0), Support::default().with_text(None));
        g.set_support(
            Axis::Rows,
            Key(1),
            Support::default()
                .with_sort(false)
                .with_text(Some(TextSupport::new(true))),
        );
    });
    let snap = s.frame(V);
    assert_eq!(snap.support(Axis::Rows, Key(0)).text, None);
    assert!(snap.support(Axis::Rows, Key(0)).sort);
    assert_eq!(
        snap.support(Axis::Rows, Key(1)),
        Support::default()
            .with_sort(false)
            .with_text(Some(TextSupport::new(true)))
    );
    assert_eq!(snap.support(Axis::Cols, Key(1)), Support::default());
}

fn one_of(key: u64, values: &[Option<&str>]) -> Condition {
    Condition::one_of(Key(key), values.iter().map(|v| Value::of(*v)).collect())
}

#[test]
fn one_of_conditions_keep_exactly_the_values_listed_and_blanks_only_when_listed() {
    let mut s = records();
    let mut out = s.step("eight records", V);
    s.input(|g| g.add_condition(Axis::Rows, one_of(0, &[Some("13"), Some("3"), Some("1")])));
    out += &s.step(
        "column 0 is one of 13, 3 and 1: 13 and 3 exactly (not 31 or 23, nor 1, absent)",
        V,
    );
    s.input(|g| {
        g.replace_condition(
            Axis::Rows,
            &one_of(0, &[Some("13"), Some("3"), Some("1")]),
            one_of(0, &[Some("13"), Some("3")]).not(true),
        )
    });
    out += &s.step("column 0 is not 13 or 3: the other six", V);
    s.input(|g| g.add_condition(Axis::Rows, one_of(2, &[Some("")])));
    out += &s.step(
        "and column 2 is empty text: none, as column 2 is blank, not empty",
        V,
    );
    s.input(|g| g.replace_condition(Axis::Rows, &one_of(2, &[Some("")]), one_of(2, &[None])));
    out += &s.step("column 2 is blank instead: the six again", V);
    s.input(|g| g.replace_condition(Axis::Rows, &one_of(2, &[None]), one_of(2, &[]).not(true)));
    out += &s.step("column 2 is not one of nothing: the six", V);
    s.input(|g| g.replace_condition(Axis::Rows, &one_of(2, &[]).not(true), one_of(2, &[])));
    out += &s.step("column 2 is one of nothing: none", V);
    insta::assert_snapshot!(out);
}

#[test]
fn one_of_is_unsupported_by_default_and_declared_with_distinct_values() {
    let mut s = records();
    let snap = s.frame(V);
    assert!(
        !Support::default().one_of,
        "only a source listing values offers it"
    );
    assert!(!snap.support(Axis::Rows, Key(0)).one_of);
    s.input(|g| g.set_support(Axis::Rows, Key(0), Support::default().with_one_of(true)));
    let snap = s.frame(V);
    assert!(snap.support(Axis::Rows, Key(0)).one_of);
    assert!(!snap.support(Axis::Rows, Key(1)).one_of);
}
