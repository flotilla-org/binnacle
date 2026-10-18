//! Seam 1 scenarios for slow and remote sources: View Specs answered late,
//! superseded, as subsets, unprompted and as failures, and find steps
//! pressed while a slow source finds. The rows stay as they are until the
//! answer; what the source leaves out stays requested, shown as not applied.

use binnacle_core::{
    Action, Axis, AxisConfig, Condition, ConditionState, Effect, FilterSpec, FindMatch, FindQuery,
    Grid, HeaderFilter, Key, TextQuery, Totals, ViewSpec, Viewport,
};
use binnacle_testkit::{PumpSource, RecordTable, Scenario};

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

fn filtered(quick: Option<&str>, list: Vec<Condition>) -> ViewSpec {
    let filter = FilterSpec::default()
        .with_quick(quick.map(TextQuery::new))
        .with_conditions(list.into());
    ViewSpec::default().with_filter(Axis::Rows, filter)
}

/// The View Specs requested since the last look, left unanswered.
fn requested(s: &mut Scenario<RecordTable>) -> Vec<ViewSpec> {
    std::iter::from_fn(|| s.grid.poll_effect())
        .filter_map(|e| match e.effect {
            Effect::ViewRequested { spec, .. } => Some(spec),
            _ => None,
        })
        .collect()
}

/// The source applies `spec` (or what it can of it) and answers.
fn answer(s: &mut Scenario<RecordTable>, spec: &ViewSpec) {
    let viewed = s.source.view(spec);
    s.input(|g| {
        g.view_applied(viewed.applied, viewed.totals);
        for (axis, notice) in viewed.notices {
            g.notify(axis, notice);
        }
    });
}

/// Renders the Grid as it stands, without pumping, so unanswered requests
/// stay unanswered.
fn unanswered(s: &mut Scenario<RecordTable>, label: &str) -> String {
    s.grid.begin_frame(V);
    format!("--- {label}\n{}", s.render())
}

fn rows(s: &mut Scenario<RecordTable>) -> Vec<Option<Key>> {
    s.frame(V).rows().iter().map(|i| i.key).collect()
}

#[test]
fn rows_stay_until_the_latest_answer_and_a_superseded_one_paints_nothing() {
    let mut s = records();
    let mut out = s.step("eight records", V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3"))));
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("31"))));
    let specs = requested(&mut s);
    assert_eq!(
        specs,
        vec![filtered(Some("3"), vec![]), filtered(Some("31"), vec![])]
    );
    out += &unanswered(
        &mut s,
        "\"3\" then \"31\" typed: both requested, the rows stay",
    );
    // The host aborts "3", answering with the spec in force.
    s.input(|g| g.view_applied(ViewSpec::default(), Totals::unknown()));
    out += &unanswered(
        &mut s,
        "\"3\" aborted, answered with the base view: still pending",
    );
    assert!(s.grid.snapshot().view_pending());
    answer(&mut s, &specs[1]);
    out += &s.step("\"31\" answered: 31 alone, nothing pending", V);
    assert!(!s.grid.snapshot().view_pending());
    assert_eq!(s.grid.snapshot().requested_view(), None);
    insta::assert_snapshot!(out);
}

#[test]
fn a_subset_answer_leaves_the_dropped_condition_not_applied_until_removed() {
    let mut s = records();
    s.frame(V);
    let (good, bad) = (contains(0, "3"), contains(1, "1"));
    s.input(|g| {
        g.add_condition(Axis::Rows, good.clone());
        g.add_condition(Axis::Rows, bad.clone());
    });
    let specs = requested(&mut s);
    let snap = s.frame(V);
    assert_eq!(
        snap.header_filter(Key(1)),
        HeaderFilter {
            filtered: false,
            pending: true,
            not_applied: false
        }
    );
    // Both answers apply column 0's condition alone.
    for _ in &specs {
        answer(&mut s, &filtered(None, vec![good.clone()]));
    }
    let snap = s.frame(V);
    assert!(!snap.view_pending());
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![
            (good.clone(), ConditionState::Applied),
            (bad.clone(), ConditionState::NotApplied),
        ]
    );
    assert_eq!(
        snap.header_filter(Key(1)),
        HeaderFilter {
            filtered: false,
            pending: false,
            not_applied: true
        },
        "column 1's header shows its condition is not applied"
    );
    assert!(!snap.header_filter(Key(0)).not_applied);
    assert_eq!(
        rows(&mut s),
        [1, 3, 5, 7].map(|k| Some(Key(k))),
        "13, 31, 23 and 3"
    );

    // Another edit asks for the not-applied condition again.
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("2"))));
    assert_eq!(
        requested(&mut s),
        vec![filtered(Some("2"), vec![good.clone(), bad.clone()])]
    );
    s.input(|g| g.view_applied(filtered(Some("2"), vec![good.clone()]), Totals::unknown()));
    // Removing it asks for what is applied, and nothing is left out.
    s.input(|g| g.remove_condition(Axis::Rows, &bad));
    let spec = requested(&mut s).pop().unwrap();
    assert_eq!(spec, filtered(Some("2"), vec![good.clone()]));
    s.input(|g| g.view_applied(spec, Totals::unknown()));
    let snap = s.frame(V);
    assert_eq!(snap.requested_view(), None);
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![(good, ConditionState::Applied)]
    );
}

#[test]
fn a_failed_request_keeps_the_text_asked_for_and_a_failed_removal_puts_the_condition_back() {
    let mut s = records();
    s.frame(V);
    let c = contains(0, "3");
    s.input(|g| g.add_condition(Axis::Rows, c.clone()));
    answer(&mut s, &filtered(None, vec![c.clone()]));
    s.frame(V);
    // An invalid regex: the source keeps what it applied.
    let bad = TextQuery::new("(").regex(true);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(bad.clone())));
    let spec = requested(&mut s).pop().unwrap();
    answer(&mut s, &spec);
    let snap = s.frame(V);
    assert!(!snap.view_pending());
    assert_eq!(snap.applied_view(), &filtered(None, vec![c.clone()]));
    let wanted = snap.requested_view().map(|r| r.rows.filter.quick.clone());
    assert_eq!(
        wanted,
        Some(Some(bad)),
        "the quick filter asked for stays, not applied"
    );

    // The quick filter cleared, and the condition's removal failed.
    s.input(|g| {
        g.set_quick_filter(Axis::Rows, None);
        g.remove_condition(Axis::Rows, &c);
    });
    let specs = requested(&mut s);
    assert_eq!(specs.len(), 2);
    s.input(|g| {
        g.view_applied(ViewSpec::default(), Totals::unknown());
        g.view_applied(filtered(None, vec![c.clone()]), Totals::unknown());
    });
    let snap = s.frame(V);
    assert_eq!(snap.requested_view(), None, "nothing is left out");
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![(c, ConditionState::Applied)]
    );
}

#[test]
fn unprompted_news_leaves_answers_due_and_keeps_a_dropped_condition_visible() {
    let mut s = records();
    s.frame(V);
    let c = contains(0, "3");
    s.input(|g| g.add_condition(Axis::Rows, c.clone()));
    answer(&mut s, &filtered(None, vec![c.clone()]));
    s.frame(V);

    // The source learns the total later.
    s.input(|g| {
        g.view_changed(
            filtered(None, vec![c.clone()]),
            Totals::unknown().with(Axis::Rows, Some(8)),
        )
    });
    let snap = s.frame(V);
    assert_eq!(snap.total(Axis::Rows), Some(8));
    assert_eq!(snap.requested_view(), None);

    // A request in flight stays pending through news.
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("1"))));
    let spec = requested(&mut s).pop().unwrap();
    s.input(|g| g.view_changed(filtered(None, vec![c.clone()]), Totals::unknown()));
    assert!(s.frame(V).view_pending(), "the answer is still due");
    answer(&mut s, &spec);
    assert!(!s.frame(V).view_pending());

    // The source stops supporting text on column 0 and says so: the
    // condition stays listed, not applied.
    s.input(|g| g.view_changed(filtered(Some("1"), vec![]), Totals::unknown()));
    let snap = s.frame(V);
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![(c.clone(), ConditionState::NotApplied)]
    );
    assert!(snap.header_filter(Key(0)).not_applied);
}

fn find(text: &str) -> Option<FindQuery> {
    Some(FindQuery::new(TextQuery::new(text)))
}

/// The find steps asked for since the last look, left unanswered.
fn steps(s: &mut Scenario<RecordTable>) -> usize {
    std::iter::from_fn(|| s.grid.poll_effect())
        .filter(|e| matches!(e.effect, Effect::FindStepRequested { .. }))
        .count()
}

#[test]
fn presses_before_the_first_find_answer_move_once() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_find(find("3"));
        for _ in 0..4 {
            g.action(Action::FindNext);
        }
    });
    assert_eq!(steps(&mut s), 1, "one step asked for, the rest dropped");
    s.input(|g| g.find_stepped(Some(FindMatch::new(Key(1), Key(0)))));
    assert_eq!(steps(&mut s), 0, "nothing queued behind it");
    assert_eq!(
        s.frame(V).find_match(),
        Some(FindMatch::new(Key(1), Key(0)))
    );

    // Once the query has an answer, at most one step waits behind the next.
    s.input(|g| {
        g.action(Action::FindNext);
        g.action(Action::FindNext);
        g.action(Action::FindPrevious);
        g.action(Action::FindNext);
    });
    assert_eq!(steps(&mut s), 1);
    s.input(|g| g.find_stepped(Some(FindMatch::new(Key(3), Key(0)))));
    assert_eq!(steps(&mut s), 1, "the one held, sent once the first lands");
    s.input(|g| g.find_stepped(Some(FindMatch::new(Key(3), Key(1)))));
    assert_eq!(steps(&mut s), 0);
}

#[test]
fn the_current_match_stays_until_the_first_step_for_a_new_query() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.set_find(find("3"));
        g.action(Action::FindNext);
    });
    s.frame(V);
    let first = FindMatch::new(Key(1), Key(0));
    assert_eq!(s.grid.snapshot().find_match(), Some(first));
    s.input(|g| {
        g.set_find(find("31"));
        g.action(Action::FindNext);
    });
    let from: Vec<Option<FindMatch>> = std::iter::from_fn(|| s.grid.poll_effect())
        .filter_map(|e| match e.effect {
            Effect::FindStepRequested { from, .. } => Some(from),
            _ => None,
        })
        .collect();
    assert_eq!(from, vec![Some(first)], "the step starts at the old match");
    s.grid.begin_frame(V);
    assert_eq!(
        s.grid.snapshot().find_match(),
        Some(first),
        "kept meanwhile"
    );
    s.input(|g| g.find_stepped(Some(FindMatch::new(Key(3), Key(0)))));
    assert_eq!(
        s.frame(V).find_match(),
        Some(FindMatch::new(Key(3), Key(0)))
    );
    s.input(|g| g.set_find(None));
    assert_eq!(s.frame(V).find_match(), None, "closing find drops it");
}
