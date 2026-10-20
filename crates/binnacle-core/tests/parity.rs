//! Seam 1 scenarios and properties for selection on both axes: picks on
//! columns, whole-column selection units, selecting rows by key, paging the
//! columns and Size Overrides through `Reset` and `Removed`.

use std::collections::{BTreeMap, BTreeSet};

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, FocusStep, Grid, HeaderClick, Key, Modifiers,
    SelectionUnit, Viewport,
};
use binnacle_testkit::{ColumnMajorTable, KeyedTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

mod support;

use support::{V, table};

#[test]
fn ctrl_click_on_a_column_header_picks_the_column_whatever_the_header_click() {
    let mut s = table();
    s.frame(V);
    s.click(12.0, 2.0, Modifiers::NONE);
    let mut out = s.step("focus row 1, column 1", V);
    s.click(5.0, 0.0, Modifiers::CTRL);
    out += &s.step(
        "ctrl-click column header 0 (Sort): picks the column, focus moves to it",
        V,
    );
    s.input(|g| g.set_header_click(Axis::Cols, HeaderClick::Select));
    s.click(19.0, 0.0, Modifiers::META);
    out += &s.step("cmd-click column header 2 (Select): picked too", V);
    s.click(1.0, 3.0, Modifiers::CTRL);
    out += &s.step("ctrl-click row header 2: a row pick beside them", V);
    s.click(5.0, 0.0, Modifiers::CTRL);
    out += &s.step("ctrl-click column header 0 again: unpicked", V);
    let snap = s.frame(V);
    assert_eq!(snap.picked(Axis::Cols), [Key(2)]);
    assert_eq!(snap.picked(Axis::Rows), [Key(2)]);
    let selected: Vec<_> = (snap.headers(Axis::Cols))
        .filter(|h| h.selected)
        .map(|h| h.key)
        .collect();
    assert_eq!(selected, [Some(Key(2))]);
    s.input(|g| g.set_header_click(Axis::Cols, HeaderClick::None));
    s.click(12.0, 0.0, Modifiers::CTRL);
    out += &s.step(
        "header click None: ctrl-click column header 1 does nothing",
        V,
    );
    s.click(12.0, 2.0, Modifiers::NONE);
    out += &s.step("plain click: every pick goes", V);
    insta::assert_snapshot!(out);
}

#[test]
fn the_cols_unit_selects_the_focused_column_and_ctrl_click_picks_columns() {
    let mut s = table();
    s.input(|g| g.set_selection_unit(SelectionUnit::Cols));
    s.frame(V);
    s.click(12.0, 2.0, Modifiers::NONE);
    let mut out = s.step("click row 1, column 1: its whole column", V);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        })
    });
    out += &s.step("arrow right: the next column", V);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Item,
            forward: true,
            extend: true,
        })
    });
    out += &s.step("shift-arrow down: still the whole column", V);
    s.click(5.0, 3.0, Modifiers::CTRL);
    out += &s.step("ctrl-click a cell in column 0: picks the column", V);
    insta::assert_snapshot!(out);
}

#[test]
fn select_row_selects_the_rows_of_the_range_or_the_focused_row() {
    let mut s = table();
    s.frame(V);
    s.input(|g| g.action(Action::SelectRow));
    let mut out = s.step("select row with nothing focused: nothing", V);
    s.click(12.0, 2.0, Modifiers::NONE);
    s.input(|g| g.action(Action::SelectRow));
    out += &s.step("focus row 1, column 1 and select row: row 1", V);
    s.click(19.0, 4.0, Modifiers::SHIFT);
    s.input(|g| g.action(Action::SelectRow));
    out += &s.step("shift-click row 3, column 2 and select row: rows 1 to 3", V);
    s.input(|g| g.action(Action::SelectColumn));
    out += &s.step(
        "select column: the range spans every column, so the focused one",
        V,
    );
    insta::assert_snapshot!(out);
}

#[test]
fn a_page_along_the_columns_moves_focus_a_viewport_across() {
    let mut s = Scenario::identity(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(30, 4.0).header(2.0),
    );
    let page = |forward| Action::MoveFocus {
        axis: Axis::Cols,
        by: FocusStep::Page,
        forward,
        extend: false,
    };
    let v = Viewport::new(22.0, 6.0);
    s.frame(v);
    s.click(3.0, 2.0, Modifiers::NONE);
    s.input(|g| g.action(page(true)));
    let snap = s.frame(v);
    let col = snap.focus(Axis::Cols).map(|f| f.position);
    assert_eq!(col, Some(5), "five columns fit the body");
    assert_eq!(snap.scroll_anchor(Axis::Cols).position, 1);
    s.input(|g| g.action(page(true)));
    s.input(|g| g.action(page(false)));
    let snap = s.frame(v);
    assert_eq!(snap.focus(Axis::Cols).map(|f| f.position), Some(5));
    assert_eq!(snap.focus(Axis::Rows).map(|f| f.position), Some(1));
}

/// What the source and the host do to the sizes of one axis.
#[derive(Clone, Debug)]
enum SizeOp {
    Set(u64, f32),
    Clear(u64),
    /// Some Keys stay, in a new order, and some arrive: a `Reset`.
    Reset(Vec<bool>, u64),
    /// The items kept by the mask: a `Refiltered`.
    Refilter(Vec<bool>),
    /// `n` items from a fraction of the count: a `Removed`.
    Remove(f32, u64),
    /// `n` items before a fraction of the count: an `Inserted`.
    Insert(f32, u64),
    /// A frame, which fetches the Window.
    Frame,
}

#[hegel::composite]
fn size_op(tc: &TestCase) -> SizeOp {
    let key = || gs::integers::<u64>().max_value(30);
    let fraction = || gs::floats::<f32>().min_value(0.0).max_value(0.999);
    let n = || gs::integers::<u64>().min_value(1).max_value(4);
    let mask = || gs::vecs(gs::booleans()).max_size(30);
    tc.draw(hegel::one_of!(
        hegel::tuples!(key(), gs::sampled_from(vec![1.0f32, 2.0, 3.5, 7.0]))
            .map(|(k, size)| SizeOp::Set(k, size)),
        key().map(SizeOp::Clear),
        hegel::tuples!(mask(), gs::integers::<u64>().max_value(3))
            .map(|(m, fresh)| SizeOp::Reset(m, fresh)),
        mask().map(SizeOp::Refilter),
        hegel::tuples!(fraction(), n()).map(|(at, n)| SizeOp::Remove(at, n)),
        hegel::tuples!(fraction(), n()).map(|(at, n)| SizeOp::Insert(at, n)),
        gs::just(SizeOp::Frame),
    ))
}

/// The Keys a mask keeps, in order.
fn kept(keys: &[Key], mask: &[bool]) -> Vec<Key> {
    let keep = |i: usize| mask.get(i).copied().unwrap_or(true);
    (keys.iter().enumerate())
        .filter(|&(i, _)| keep(i))
        .map(|(_, &k)| k)
        .collect()
}

/// Runs `op` on `axis` of a square table; returns the Keys it removed.
fn run(s: &mut Scenario<KeyedTable>, axis: Axis, op: &SizeOp, next: &mut u64) -> Vec<Key> {
    let i = axis.index();
    let v = Viewport::new(8.0, 8.0);
    let keys = s.source.axes[i].clone();
    let count = keys.len() as u64;
    let at = |f: f32| (f64::from(f) * (count + 1) as f64) as u64;
    match op {
        &SizeOp::Set(k, size) => s.input(|g| g.set_size(axis, Key(k), size)),
        &SizeOp::Clear(k) => s.input(|g| g.clear_size(axis, Key(k))),
        SizeOp::Reset(mask, fresh) => {
            let mut keys = kept(&keys, mask);
            keys.reverse();
            keys.extend((*next..*next + fresh).map(Key));
            *next += fresh;
            let n = keys.len() as u64;
            s.source.axes[i] = keys;
            s.input(|g| g.notify(axis, ChangeNotice::Reset(n)));
        }
        SizeOp::Refilter(mask) => {
            let keys = kept(&keys, mask);
            let n = keys.len() as u64;
            s.source.axes[i] = keys;
            s.input(|g| g.notify(axis, ChangeNotice::Refiltered(n)));
        }
        &SizeOp::Remove(f, n) => {
            let at = at(f).min(count);
            let n = n.min(count - at);
            let gone = keys[at as usize..(at + n) as usize].to_vec();
            let notice = s.source.remove(axis, at, n);
            s.input(|g| g.notify(axis, notice));
            return gone;
        }
        &SizeOp::Insert(f, n) => {
            let notice = s.source.insert(axis, at(f), n);
            s.input(|g| g.notify(axis, notice));
        }
        SizeOp::Frame => {
            s.frame(v);
        }
    }
    Vec::new()
}

/// Size Overrides are kept the same way on both axes: the same inputs on the
/// rows of one square table and on the columns of another leave the same
/// overrides. A `Reset` or a `Refiltered` keeps every one; a `Removed` drops
/// only those of removed Keys.
#[hegel::test(test_cases = 1000)]
fn size_overrides_behave_the_same_on_both_axes_through_reset_and_removed(tc: TestCase) {
    let n = tc.draw(gs::integers::<u64>().max_value(12));
    let ops = tc.draw(gs::vecs(size_op()).max_size(16));
    let square = || {
        let config = AxisConfig::new(n, 1.0).header(1.0);
        Scenario::new(Grid::new(config, config), KeyedTable::new(n, n))
    };
    let mut tables = [square(), square()];
    // Fresh Keys for `Reset`, past any `KeyedTable::insert` hands out.
    let mut next = [1000, 1000];
    let mut model: BTreeMap<Key, f32> = BTreeMap::new();
    let mut removed: BTreeSet<Key> = BTreeSet::new();
    for op in &ops {
        for (s, axis) in tables.iter_mut().zip(Axis::ALL) {
            let gone = run(s, axis, op, &mut next[axis.index()]);
            removed.extend(gone);
        }
        match *op {
            SizeOp::Set(k, size) => {
                model.insert(Key(k), size);
                removed.remove(&Key(k));
            }
            SizeOp::Clear(k) => {
                model.remove(&Key(k));
            }
            _ => {}
        }
        for k in (0..1010).map(Key) {
            let [rows, cols] = &tables;
            let on_rows = rows.grid.size_override(Axis::Rows, k);
            assert_eq!(on_rows, cols.grid.size_override(Axis::Cols, k), "{k:?}");
            assert_eq!(rows.grid.size_override(Axis::Cols, k), None, "{k:?}");
            if !removed.contains(&k) {
                assert_eq!(on_rows, model.get(&k).copied(), "{k:?} after {op:?}");
            }
        }
    }
}

/// A selection input on a square table, which turns into the same input on
/// the other axis.
#[derive(Clone, Debug)]
enum SelectOp {
    Click(f32, f32, Modifiers),
    MoveFocus(Axis, FocusStep, bool, bool),
    /// `SelectRow` on the rows, `SelectColumn` on the columns.
    SelectWhole(Axis),
    SelectAll,
    Cancel,
    /// The unit selecting the whole item across `axis` (`Rows` for
    /// [`SelectionUnit::Rows`]).
    Unit(Axis),
    HeaderClick(Axis, HeaderClick),
    /// The source keeps the items of `axis` the mask keeps.
    Refilter(Axis, Vec<bool>),
    Frame,
}

impl SelectOp {
    fn turned(&self) -> SelectOp {
        match self.clone() {
            SelectOp::Click(x, y, m) => SelectOp::Click(y, x, m),
            SelectOp::MoveFocus(a, by, fw, ex) => SelectOp::MoveFocus(a.other(), by, fw, ex),
            SelectOp::SelectWhole(a) => SelectOp::SelectWhole(a.other()),
            SelectOp::Unit(a) => SelectOp::Unit(a.other()),
            SelectOp::HeaderClick(a, c) => SelectOp::HeaderClick(a.other(), c),
            SelectOp::Refilter(a, mask) => SelectOp::Refilter(a.other(), mask),
            op => op,
        }
    }

    fn run(&self, s: &mut Scenario<KeyedTable>) {
        let action = |s: &mut Scenario<KeyedTable>, action| s.input(|g| g.action(action));
        match self.clone() {
            SelectOp::Click(x, y, m) => s.click(x, y, m),
            SelectOp::MoveFocus(axis, by, forward, extend) => action(
                s,
                Action::MoveFocus {
                    axis,
                    by,
                    forward,
                    extend,
                },
            ),
            SelectOp::SelectWhole(Axis::Rows) => action(s, Action::SelectRow),
            SelectOp::SelectWhole(Axis::Cols) => action(s, Action::SelectColumn),
            SelectOp::SelectAll => action(s, Action::SelectAll),
            SelectOp::Cancel => action(s, Action::Cancel),
            SelectOp::Unit(axis) => {
                let unit = match axis {
                    Axis::Rows => SelectionUnit::Rows,
                    Axis::Cols => SelectionUnit::Cols,
                };
                s.input(|g| g.set_selection_unit(unit));
            }
            SelectOp::HeaderClick(axis, click) => s.input(|g| g.set_header_click(axis, click)),
            SelectOp::Refilter(axis, mask) => {
                let keys = kept(&s.source.axes[axis.index()], &mask);
                let n = keys.len() as u64;
                s.source.axes[axis.index()] = keys;
                s.input(|g| g.notify(axis, ChangeNotice::Refiltered(n)));
            }
            SelectOp::Frame => {
                s.frame(SQUARE);
            }
        }
    }
}

const SQUARE: Viewport = Viewport {
    width: 8.0,
    height: 8.0,
};

#[hegel::composite]
fn select_op(tc: &TestCase) -> SelectOp {
    let axis = || gs::sampled_from(vec![Axis::Rows, Axis::Cols]);
    let at = || gs::floats::<f32>().min_value(0.0).max_value(8.5);
    let modifiers = gs::sampled_from(vec![
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::META,
    ]);
    let by = gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]);
    // Rows never sort yet, so a plain header click only selects or does
    // nothing.
    let click = gs::sampled_from(vec![HeaderClick::Select, HeaderClick::None]);
    tc.draw(hegel::one_of!(
        hegel::tuples!(at(), at(), modifiers).map(|(x, y, m)| SelectOp::Click(x, y, m)),
        hegel::tuples!(axis(), by, gs::booleans(), gs::weighted_booleans(0.3))
            .map(|(a, by, fw, ex)| SelectOp::MoveFocus(a, by, fw, ex)),
        axis().map(SelectOp::SelectWhole),
        gs::just(SelectOp::SelectAll),
        gs::just(SelectOp::Cancel),
        axis().map(SelectOp::Unit),
        hegel::tuples!(axis(), click).map(|(a, c)| SelectOp::HeaderClick(a, c)),
        hegel::tuples!(axis(), gs::vecs(gs::weighted_booleans(0.8)).max_size(12))
            .map(|(a, mask)| SelectOp::Refilter(a, mask)),
        gs::just(SelectOp::Frame),
    ))
}

/// Selection is the same on both axes: any selection input on a square
/// table, and the same input turned onto the other axis on another, leave
/// the same focus, range and picks turned. The unit is always a whole item,
/// since under [`SelectionUnit::Cells`] a Ctrl-click on a cell picks its
/// row, as a list does.
#[hegel::test(test_cases = 1000)]
fn selection_on_one_axis_is_selection_on_the_other_turned(tc: TestCase) {
    let n = tc.draw(gs::integers::<u64>().max_value(12));
    let ops = tc.draw(gs::vecs(select_op()).max_size(20));
    let square = || {
        let config = AxisConfig::new(n, 1.0).header(1.0);
        let mut grid = Grid::new(config, config);
        grid.set_header_click(Axis::Cols, HeaderClick::Select);
        Scenario::new(grid, KeyedTable::new(n, n))
    };
    let (mut upright, mut turned) = (square(), square());
    let unit = SelectOp::Unit(tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols])));
    for op in std::iter::once(&unit).chain(&ops) {
        op.run(&mut upright);
        op.turned().run(&mut turned);
        let a = upright.frame(SQUARE);
        let focus = Axis::ALL.map(|axis| a.focus(axis));
        let range = a.selection_range().map(|r| (r.rows, r.cols));
        let picks = Axis::ALL.map(|axis| a.picked(axis).to_vec());
        let b = turned.frame(SQUARE);
        assert_eq!(
            focus,
            [b.focus(Axis::Cols), b.focus(Axis::Rows)],
            "focus after {op:?}"
        );
        let turned_range = b.selection_range().map(|r| (r.cols, r.rows));
        assert_eq!(range, turned_range, "range after {op:?}");
        let turned_picks = [b.picked(Axis::Cols).to_vec(), b.picked(Axis::Rows).to_vec()];
        assert_eq!(picks, turned_picks, "picks after {op:?}");
        if picks.iter().any(|p| !p.is_empty()) {
            tc.event("picked");
        }
    }
}

/// Six records as columns over three fields as rows, as in `column_major.rs`.
fn records() -> Scenario<ColumnMajorTable> {
    let field0 = [5, 13, 8, 31, 9, 23];
    let records = (0..6)
        .map(|i| vec![Some(field0[i]), Some(i as i64 * 10), None])
        .collect();
    let grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(6, 4.0).header(4.0).gap(1.0),
    );
    Scenario::new(grid, ColumnMajorTable::new(records, 3, [0, 1]))
}

#[test]
fn picked_records_on_their_side_are_dropped_when_a_filter_takes_them_out() {
    let mut s = records();
    let v = Viewport::new(40.0, 5.0);
    s.frame(v);
    s.click(6.0, 0.0, Modifiers::CTRL);
    s.click(21.0, 0.0, Modifiers::CTRL);
    let mut out = s.step("ctrl-click the headers of records 0 and 3: both picked", v);
    s.input(|g| g.set_selection_unit(SelectionUnit::Cols));
    s.click(31.0, 2.5, Modifiers::CTRL);
    out += &s.step(
        "in the Cols unit, ctrl-click a cell of record 5: picked, and focused",
        v,
    );
    let notice = s.source.filter(|r| r[1].is_some_and(|v| v >= 20));
    s.input(|g| g.notify(Axis::Cols, notice));
    out += &s.step(
        "the host keeps records with field 1 at least 20: record 0 leaves the picks, 3 and 5 stay",
        v,
    );
    assert_eq!(s.frame(v).picked(Axis::Cols), [Key(3), Key(5)]);
    insta::assert_snapshot!(out);
}
