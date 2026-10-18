//! Seam 1 scenarios for the quick filter: setting it is a View Edit, a host
//! can request a whole View Spec, showing or hiding columns asks again when
//! the quick filter searches the shown ones, and the Table Source's
//! unfiltered totals reach the Snapshot.

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Effect, FilterSpec, Grid, Key, Searched, SortColumn, SortSpec,
    TextQuery, TextSupport, Totals, ViewEdit, ViewSpec, Viewport,
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

fn quick(query: TextQuery) -> ViewSpec {
    ViewSpec::default().with_filter(Axis::Rows, FilterSpec::default().with_quick(Some(query)))
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
fn the_quick_filter_is_a_view_edit_answered_with_the_rows_kept_and_the_totals() {
    let mut s = records();
    let mut out = s.step("eight records", V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3"))));
    out += &s.step(
        "quick filter \"3\": the rows with a 3 in any shown column are kept (13, 31, 23, 3 and the 30 in column 1), 4 of a total of 8",
        V,
    );
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("^3").regex(true))));
    out += &s.step("regex \"^3\": 31, 3 and the 30", V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new(""))));
    out += &s.step("no text clears the quick filter", V);
    insta::assert_snapshot!(out);
}

#[test]
fn setting_the_quick_filter_the_latest_request_has_asks_for_nothing() {
    let mut s = records();
    s.frame(V);
    let ber = TextQuery::new("ber");
    s.input(|g| {
        g.set_quick_filter(Axis::Rows, Some(ber.clone()));
        g.set_quick_filter(Axis::Rows, Some(ber.clone()));
        g.set_quick_filter(Axis::Rows, Some(ber.clone().case_sensitive(true)));
        g.set_quick_filter(Axis::Rows, None);
        g.set_quick_filter(Axis::Rows, Some(TextQuery::new("")));
    });
    let edit = |query| ViewEdit::SetQuickFilter {
        axis: Axis::Rows,
        query,
    };
    assert_eq!(
        requests(&mut s),
        vec![
            (quick(ber.clone()), edit(Some(ber.clone()))),
            (
                quick(ber.clone().case_sensitive(true)),
                edit(Some(ber.case_sensitive(true)))
            ),
            (ViewSpec::default(), edit(None)),
        ]
    );
}

#[test]
fn the_quick_filter_keeps_the_sort_and_a_sort_keeps_the_quick_filter() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("1"))));
    s.click(5.0, 0.5, binnacle_core::Modifiers::NONE);
    let sorted = SortSpec::new(vec![SortColumn::ascending(Key(0))]);
    let both = quick(TextQuery::new("1")).with_sort(Axis::Rows, sorted);
    let specs: Vec<ViewSpec> = requests(&mut s).into_iter().map(|(spec, _)| spec).collect();
    assert_eq!(specs.last(), Some(&both));
    s.input(|g| g.set_quick_filter(Axis::Rows, None));
    let specs: Vec<ViewSpec> = requests(&mut s).into_iter().map(|(spec, _)| spec).collect();
    assert_eq!(
        specs,
        vec![both.with_filter(Axis::Rows, FilterSpec::default())]
    );
}

#[test]
fn a_host_requests_a_whole_view_spec_as_a_replace() {
    let mut s = records();
    s.frame(V);
    let saved = quick(TextQuery::new("2").searching(Searched::Keys(vec![Key(0)]))).with_sort(
        Axis::Rows,
        SortSpec::new(vec![SortColumn::descending(Key(1))]),
    );
    s.input(|g| g.request_view(saved.clone()));
    let mut out = s.step(
        "restore a saved view: column 0 containing 2 (23 only), sorted by column 1 descending",
        V,
    );
    s.input(|g| g.request_view(ViewSpec::default()));
    out += &s.step("restore the base view", V);
    insta::assert_snapshot!(out);
}

#[test]
fn columns_shown_or_hidden_ask_again_when_the_quick_filter_searches_the_shown_ones() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3"))));
    s.frame(V);
    let shown_changed =
        |spec: &ViewSpec| vec![(spec.clone(), ViewEdit::ShownChanged { axis: Axis::Cols })];
    let spec = quick(TextQuery::new("3"));
    let notices = [
        ChangeNotice::Removed { at: 1, n: 1 },
        ChangeNotice::Inserted { at: 1, n: 1 },
        ChangeNotice::Refiltered(3),
        ChangeNotice::Reset(3),
    ];
    for notice in notices {
        s.input(|g| g.notify(Axis::Cols, notice.clone()));
        assert_eq!(requests(&mut s), shown_changed(&spec), "after {notice:?}");
    }
    // Columns moved or changed in place show and hide nothing, nor do rows.
    for (axis, notice) in [
        (Axis::Cols, ChangeNotice::Reordered),
        (Axis::Cols, ChangeNotice::ContentChanged(0..1)),
        (Axis::Rows, ChangeNotice::Reset(8)),
    ] {
        s.input(|g| g.notify(axis, notice.clone()));
        assert_eq!(requests(&mut s), vec![], "after {axis:?} {notice:?}");
    }
    // A quick filter naming its columns does not follow the shown ones.
    let named = TextQuery::new("3").searching(Searched::Keys(vec![Key(0)]));
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(named)));
    s.frame(V);
    s.input(|g| g.notify(Axis::Cols, ChangeNotice::Removed { at: 1, n: 1 }));
    assert_eq!(requests(&mut s), vec![]);
}

#[test]
fn the_snapshot_shows_the_quick_filter_support_and_the_totals() {
    let mut s = records();
    assert_eq!(s.frame(V).quick_filter(Axis::Rows), None);
    s.input(|g| {
        g.set_quick_filter_support(Axis::Rows, Some(TextSupport::new(true)));
        g.view_applied(
            ViewSpec::default(),
            Totals::unknown().with(Axis::Rows, Some(40)),
        );
    });
    let snap = s.frame(V);
    assert_eq!(snap.quick_filter(Axis::Rows), Some(TextSupport::new(true)));
    assert_eq!(snap.quick_filter(Axis::Cols), None);
    assert_eq!(
        (snap.total(Axis::Rows), snap.total(Axis::Cols)),
        (Some(40), None)
    );
    s.input(|g| g.view_applied(ViewSpec::default(), Totals::unknown()));
    assert_eq!(s.frame(V).total(Axis::Rows), None);
}

#[test]
fn an_edited_row_the_filter_drops_stays_until_the_view_spec_is_applied_again() {
    let mut s = records();
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("3"))));
    let mut out = s.step("quick filter \"3\": 13, 31, 23, 3 and the 30", V);
    // The user edits 13 (base row 1) to 12: it no longer matches, and stays.
    let notice = s.source.edit(1, 0, Some(12)).unwrap();
    s.input(|g| g.notify(Axis::Rows, notice));
    out += &s.step("13 edited to 12 stays, unmatched", V);
    s.input(|g| g.reapply_view());
    out += &s.step("re-apply: the same View Spec, and 12 goes", V);
    insta::assert_snapshot!(out);
}
