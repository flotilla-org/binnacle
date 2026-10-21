//! Seam 1 scenarios for hiding: the Grid asks for hidden Keys as a third part
//! of each axis's View Spec, the Table Source leaves the items out and
//! answers with `Refiltered`, and focus, the range and picks follow the
//! `Refiltered` rules while Size Overrides stay.

use binnacle_core::{
    Action, Axis, AxisConfig, Condition, Edge, Effect, Grid, HIDE_CAP, HideCapped, Key, Modifiers,
    TextQuery, ViewEdit, ViewSpec, Viewport,
};
use binnacle_testkit::{ColumnMajorTable, RecordTable, Scenario};

const V: Viewport = Viewport {
    width: 33.0,
    height: 5.0,
};

/// Six records over four columns (column `c` of record `r` holds `10 * r +
/// c`), with a 1-high header band and a 4-wide row header band; rows of 1,
/// columns of 6 with a Gap of 1. Both axes hide.
fn records() -> Scenario<RecordTable> {
    let rows = (0..6)
        .map(|r| (0..4).map(|c| Some(10 * r + c)).collect())
        .collect();
    let mut grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(4, 6.0).header(4.0).gap(1.0),
    );
    for axis in Axis::ALL {
        grid.set_hide_support(axis, true);
    }
    Scenario::new(grid, RecordTable::new(rows, 4, 0..4))
}

fn requests(effects: &[Effect]) -> Vec<(ViewSpec, ViewEdit)> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::ViewRequested { spec, edit } => Some((spec.clone(), edit.clone())),
            _ => None,
        })
        .collect()
}

/// The runs of Positions the range covers on `axis`, as `(start, end)`.
fn runs(s: &mut Scenario<RecordTable>, axis: Axis) -> Option<Vec<(u64, u64)>> {
    let snap = s.grid.snapshot();
    let range = snap.selection_range()?;
    Some(range.span(axis).iter().map(|r| (r.start, r.end)).collect())
}

#[test]
fn hiding_the_focused_column_moves_focus_to_its_neighbour_and_keeps_its_width() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.set_size(Axis::Cols, Key(1), 4.0));
    s.click(14.0, 2.0, Modifiers::NONE);
    let mut out = s.step("column 1 sized 4; focus record 1, column 1", V);
    s.input(|g| g.hide(Axis::Cols, &[Key(1)]).unwrap());
    out += &s.step(
        "hide column 1: the source leaves it out, focus goes to column 2, the next one",
        V,
    );
    assert_eq!(s.grid.snapshot().hidden(Axis::Cols), &[Key(1)]);
    assert_eq!(s.grid.size_override(Axis::Cols, Key(1)), Some(4.0));
    s.input(|g| g.hide(Axis::Cols, &[Key(3), Key(1)]).unwrap());
    out += &s.step("hide column 3 too (column 1 is hidden already)", V);
    s.input(|g| g.show(Axis::Cols, &[Key(1)]));
    out += &s.step(
        "show column 1 again: it comes back in its place, 4 wide; focus stays on column 2",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn hiding_rows_drops_their_picks_and_a_range_whole_across_them_stays_whole() {
    let mut s = records();
    s.frame(V);
    s.click(8.0, 2.0, Modifiers::NONE);
    s.input(|g| g.action(Action::SelectColumn));
    let mut out = s.step("select column 0", V);
    s.click(1.0, 2.0, Modifiers::CTRL);
    s.click(1.0, 4.0, Modifiers::CTRL);
    out += &s.step("pick records 1 and 3", V);
    s.input(|g| g.hide(Axis::Rows, &[Key(1), Key(4)]).unwrap());
    out += &s.step(
        "hide records 1 and 4: the pick of 1 goes, 3 stays; column 0 stays selected whole",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn hiding_columns_shrinks_a_range_across_them_and_clears_it_when_none_remain() {
    let mut s = records();
    s.frame(V);
    s.click(8.0, 2.0, Modifiers::NONE);
    s.click(29.0, 2.0, Modifiers::SHIFT);
    s.input(|g| g.action(Action::SelectColumn));
    let mut out = s.step("select columns 0 to 3 whole", V);
    s.input(|g| g.hide(Axis::Cols, &[Key(0), Key(2)]).unwrap());
    out += &s.step(
        "hide columns 0 and 2: the range pulls in to columns 1 and 3, now Positions 0 and 1",
        V,
    );
    assert_eq!(runs(&mut s, Axis::Cols), Some(vec![(0, 2)]));
    s.click(8.0, 2.0, Modifiers::NONE);
    s.input(|g| g.action(Action::SelectColumn));
    out += &s.step("select column 1 whole", V);
    s.input(|g| g.hide(Axis::Cols, &[Key(0), Key(2), Key(1)]).unwrap());
    out += &s.step(
        "hide column 1 too: nothing of the range remains, so it clears",
        V,
    );
    assert_eq!(s.grid.snapshot().selection_range(), None);
    insta::assert_snapshot!(out);
}

#[test]
fn hiding_columns_shrinks_a_cell_range_across_them() {
    let mut s = records();
    s.frame(V);
    s.click(8.0, 2.0, Modifiers::NONE);
    s.click(29.0, 3.0, Modifiers::SHIFT);
    let mut out = s.step("select records 1 and 2 in columns 0 to 3", V);
    s.input(|g| g.hide(Axis::Cols, &[Key(0), Key(2)]).unwrap());
    out += &s.step(
        "hide columns 0 and 2: the rows are left alone and the range pulls in to columns 1 and 3",
        V,
    );
    assert_eq!(runs(&mut s, Axis::Rows), Some(vec![(1, 3)]));
    assert_eq!(runs(&mut s, Axis::Cols), Some(vec![(0, 2)]));
    insta::assert_snapshot!(out);
}

#[test]
fn clearing_the_filter_leaves_hidden_items_hidden() {
    let mut s = records();
    s.frame(V);
    s.input(|g| {
        g.hide(Axis::Rows, &[Key(0)]).unwrap();
        g.add_condition(Axis::Rows, Condition::text(Key(0), TextQuery::new("1")));
    });
    let mut out = s.step("hide record 0; keep values in column 0 holding a 1", V);
    s.input(|g| {
        let condition = Condition::text(Key(0), TextQuery::new("1"));
        g.remove_condition(Axis::Rows, &condition);
    });
    out += &s.step("remove the condition: every record comes back but 0", V);
    insta::assert_snapshot!(out);
}

#[test]
fn hidden_keys_the_data_lost_are_pruned_from_the_answer() {
    let mut s = records();
    s.frame(V);
    s.input(|g| g.hide(Axis::Rows, &[Key(5), Key(2)]).unwrap());
    s.frame(V);
    let notices = s.source.replace(vec![vec![Some(1); 4]; 3]);
    s.input(|g| notices.into_iter().for_each(|(a, n)| g.notify(a, n)));
    s.frame(V);
    s.input(|g| g.hide(Axis::Rows, &[Key(0)]).unwrap());
    let effects = s.effects();
    let (spec, edit) = requests(&effects).pop().expect("a request");
    assert_eq!(spec.rows.hidden, [Key(5), Key(2), Key(0)]);
    assert_eq!(
        edit,
        ViewEdit::Hide {
            axis: Axis::Rows,
            keys: vec![Key(0)]
        }
    );
    // The source has three rows now: record 5 is gone, so the answer drops it.
    let snap = s.frame(V);
    assert_eq!(snap.hidden(Axis::Rows), &[Key(2), Key(0)]);
    assert_eq!(snap.applied_view().rows.hidden, [Key(2), Key(0)]);
    assert_eq!(snap.count(Axis::Rows), 1);
}

#[test]
fn nothing_hides_on_an_axis_whose_source_cannot() {
    let mut s = records();
    s.input(|g| g.set_hide_support(Axis::Rows, false));
    let snap = s.frame(V);
    assert!(!snap.hide_support(Axis::Rows));
    assert!(snap.hide_support(Axis::Cols));
    s.input(|g| g.hide(Axis::Rows, &[Key(0)]).unwrap());
    assert_eq!(requests(&s.effects()), []);
    s.input(|g| g.hide(Axis::Cols, &[Key(0), Key(0)]).unwrap());
    let hidden = ViewSpec::default().with_hidden(Axis::Cols, vec![Key(0)]);
    let edit = ViewEdit::Hide {
        axis: Axis::Cols,
        keys: vec![Key(0)],
    };
    assert_eq!(requests(&s.effects()), [(hidden, edit)]);
    s.input(|g| {
        g.show(Axis::Cols, &[Key(3)]);
        g.hide(Axis::Cols, &[Key(0)]).unwrap();
    });
    assert_eq!(requests(&s.effects()), [], "nothing to show or hide");
}

#[test]
fn hidden_records_of_a_column_major_table_are_its_columns() {
    let records = (0..5).map(|r| vec![Some(r), Some(r * r)]).collect();
    let mut grid = Grid::new(
        AxisConfig::new(2, 1.0).header(1.0),
        AxisConfig::new(5, 4.0).header(4.0).gap(1.0),
    );
    grid.set_hide_support(Axis::Cols, true);
    let mut s = Scenario::new(grid, ColumnMajorTable::new(records, 2, [0, 1]));
    s.frame(V);
    s.input(|g| {
        g.hide(Axis::Cols, &[Key(1), Key(3)]).unwrap();
        g.action(Action::SelectAll);
    });
    let out = s.step("hide records 1 and 3, the table's columns 1 and 3", V);
    insta::assert_snapshot!(out);
}

#[test]
fn hiding_past_the_cap_hides_nothing_and_says_so() {
    let mut s = records();
    let snap = s.frame(V);
    assert_eq!(snap.hide_cap(Axis::Rows), HIDE_CAP);
    s.input(|g| g.set_hide_cap(Axis::Rows, 2));
    assert_eq!(s.frame(V).hide_cap(Axis::Rows), 2);
    s.input(|g| assert_eq!(g.hide(Axis::Rows, &[Key(0)]), Ok(())));
    s.frame(V);
    let refused = s.grid.hide(Axis::Rows, &[Key(1), Key(2)]);
    assert_eq!(refused, Err(HideCapped { cap: 2 }));
    assert_eq!(requests(&s.effects()), [], "nothing is asked for");
    assert_eq!(s.grid.snapshot().hidden(Axis::Rows), &[Key(0)]);
    s.input(|g| assert_eq!(g.hide(Axis::Rows, &[Key(0), Key(1)]), Ok(())));
    assert_eq!(s.frame(V).hidden(Axis::Rows), &[Key(0), Key(1)]);
    // A host can't raise the cap past the Grid's own.
    s.input(|g| g.set_hide_cap(Axis::Cols, HIDE_CAP + 5));
    assert_eq!(s.frame(V).hide_cap(Axis::Cols), HIDE_CAP);
}

#[test]
fn a_hidden_pinned_column_goes_dormant_and_returns_to_its_place_when_shown() {
    let mut s = records();
    s.input(|g| {
        g.pin(Axis::Cols, Edge::Leading, Key(2));
        g.pin(Axis::Cols, Edge::Leading, Key(0));
        g.pin(Axis::Cols, Edge::Trailing, Key(3));
    });
    let mut out = s.step("columns 2 then 0 pinned left, 3 right", V);
    s.input(|g| assert_eq!(g.hide(Axis::Cols, &[Key(2)]), Ok(())));
    out += &s.step(
        "hide column 2: its pin goes dormant, column 0 leads alone",
        V,
    );
    s.input(|g| assert_eq!(g.hide(Axis::Cols, &[Key(3)]), Ok(())));
    out += &s.step("hide column 3 too: the trailing Strip empties", V);
    s.input(|g| g.show(Axis::Cols, &[Key(2), Key(3)]));
    out += &s.step(
        "show both: 2 is back before 0, in pin order, and 3 pinned right",
        V,
    );
    insta::assert_snapshot!(out);
}
