//! Comparison conditions in the terminal: the column filter popup's
//! comparison row, where Left and Right step through "Any value", "Equal
//! to" to "At least" and "Between", and the value (or both ends) is typed
//! as a number or a `YYYY-MM-DD` date; and its chip in the conditions line.

use std::time::Duration;

use binnacle_core::{
    Axis, AxisConfig, Compared, Comparison, Condition, Effect, FilterSpec, Grid, Key, Scalar,
    Snapshot, Support, Totals, ViewEdit, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    ColumnFilter, Content, RenderContext, Theme, render_conditions, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["port", "weight", "shipped"][col.0 as usize].to_string()
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn typed(popup: &mut ColumnFilter, grid: &mut Grid, text: &str) {
    for c in text.chars() {
        popup.key(&press(KeyCode::Char(c)), grid);
    }
}

const W: u16 = 64;
const H: u16 = 10;

/// Five rows over three columns 14 wide: port takes Text conditions,
/// weight compares numbers and shipped dates, neither taking text.
fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 14.0).header(4.0).gap(1.0),
    );
    let compares = |c| Support::default().with_text(None).with_compare(Some(c));
    grid.set_support(Axis::Rows, Key(1), compares(Compared::Numbers));
    grid.set_support(Axis::Rows, Key(2), compares(Compared::Dates));
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(W.into(), H.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

fn focus(grid: &mut Grid, col: u64) {
    snapshot(grid);
    grid.set_focus(Key(0), Key(col));
}

fn edits(grid: &mut Grid) -> Vec<ViewEdit> {
    let mut out = vec![];
    while let Some(stamped) = grid.poll_effect() {
        if let Effect::ViewRequested { edit, .. } = stamped.effect {
            out.push(edit);
        }
    }
    out
}

fn text(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn screen(popup: &ColumnFilter, grid: &mut Grid) -> String {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    let theme = Theme::classic();
    render_with(
        &snapshot,
        &Labels,
        &RenderContext::new(&theme),
        buf.area,
        &mut buf,
    );
    popup.render(&snapshot, &Labels, &theme, buf.area, &mut buf);
    text(&buf)
}

fn compare(key: u64, comparison: Comparison) -> Condition {
    Condition::compare(Key(key), comparison)
}

fn add(condition: Condition) -> ViewEdit {
    ViewEdit::AddCondition {
        axis: Axis::Rows,
        condition,
    }
}

fn replace(from: &Condition, to: &Condition) -> ViewEdit {
    ViewEdit::ReplaceCondition {
        axis: Axis::Rows,
        from: from.clone(),
        to: to.clone(),
    }
}

fn remove(condition: &Condition) -> ViewEdit {
    ViewEdit::RemoveCondition {
        axis: Axis::Rows,
        condition: condition.clone(),
    }
}

fn conditions(list: Vec<Condition>) -> ViewSpec {
    ViewSpec::default().with_filter(
        Axis::Rows,
        FilterSpec::default().with_conditions(list.into()),
    )
}

#[test]
fn a_column_that_only_compares_opens_at_its_comparison_and_right_steps_through_them() {
    let mut grid = grid();
    focus(&mut grid, 1);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    assert!(popup.open(&snapshot(&mut grid)), "weight takes comparisons");
    popup.key(&press(KeyCode::Right), &mut grid);
    assert_eq!(
        edits(&mut grid),
        vec![],
        "Equal to nothing yet asks nothing"
    );
    typed(&mut popup, &mut grid, "1e");
    let one = |c: fn(Scalar) -> Comparison| compare(1, c(Scalar::Number(1.0)));
    assert_eq!(
        edits(&mut grid),
        vec![add(one(Comparison::Equal)), remove(&one(Comparison::Equal))],
        "1e is no number, so the comparison goes until the field holds one"
    );
    popup.key(&press(KeyCode::Backspace), &mut grid);
    assert_eq!(edits(&mut grid), vec![add(one(Comparison::Equal))]);
    for _ in 0..5 {
        popup.key(&press(KeyCode::Right), &mut grid);
    }
    assert_eq!(
        edits(&mut grid),
        vec![
            replace(&one(Comparison::Equal), &one(Comparison::NotEqual)),
            replace(&one(Comparison::NotEqual), &one(Comparison::Less)),
            replace(&one(Comparison::Less), &one(Comparison::AtMost)),
            replace(&one(Comparison::AtMost), &one(Comparison::Greater)),
            replace(&one(Comparison::Greater), &one(Comparison::AtLeast)),
        ]
    );
    popup.key(&press(KeyCode::Right), &mut grid);
    popup.key(&press(KeyCode::Tab), &mut grid);
    typed(&mut popup, &mut grid, "2.5");
    let between = compare(
        1,
        Comparison::Between(Scalar::Number(1.0), Scalar::Number(2.5)),
    );
    let edited = edits(&mut grid);
    assert_eq!(edited.first(), Some(&remove(&one(Comparison::AtLeast))));
    let last = match edited.last() {
        Some(ViewEdit::AddCondition { condition, .. }) => condition,
        Some(ViewEdit::ReplaceCondition { to, .. }) => to,
        other => panic!("asks for the between, not {other:?}"),
    };
    assert_eq!(last, &between);
    popup.key(&press(KeyCode::Right), &mut grid);
    assert_eq!(edits(&mut grid), vec![remove(&between)], "Any value");
    popup.key(&press(KeyCode::Left), &mut grid);
    assert_eq!(edits(&mut grid), vec![add(between.clone())]);
    popup.key(&press(KeyCode::Esc), &mut grid);
    assert_eq!(popup.column(), None);
    assert_eq!(
        edits(&mut grid),
        vec![remove(&between)],
        "Esc puts back none"
    );
}

#[test]
fn a_date_is_asked_for_once_it_is_a_whole_day_and_reopens_as_written() {
    let mut grid = grid();
    focus(&mut grid, 2);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    popup.open(&snapshot(&mut grid));
    for _ in 0..6 {
        popup.key(&press(KeyCode::Right), &mut grid);
    }
    typed(&mut popup, &mut grid, "2024-02-3");
    assert_eq!(edits(&mut grid), vec![], "2024-02-3 is no day");
    typed(&mut popup, &mut grid, "0");
    assert_eq!(edits(&mut grid), vec![], "nor is the 30th of February");
    popup.key(&press(KeyCode::Backspace), &mut grid);
    popup.key(&press(KeyCode::Backspace), &mut grid);
    typed(&mut popup, &mut grid, "29");
    let leap = compare(2, Comparison::AtLeast(Scalar::date(2024, 2, 29)));
    assert_eq!(edits(&mut grid), vec![add(leap.clone())]);
    popup.key(&press(KeyCode::Enter), &mut grid);
    grid.view_applied(conditions(vec![leap.clone()]), Totals::unknown());
    focus(&mut grid, 2);
    popup.open(&snapshot(&mut grid));
    popup.key(&press(KeyCode::Left), &mut grid);
    let greater = compare(2, Comparison::Greater(Scalar::date(2024, 2, 29)));
    assert_eq!(edits(&mut grid), vec![replace(&leap, &greater)]);
}

#[test]
fn the_popup_shows_the_comparison_row_and_the_chips_write_each_comparison() {
    let mut grid = grid();
    focus(&mut grid, 1);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    popup.open(&snapshot(&mut grid));
    let mut out = vec![screen(&popup, &mut grid)];
    for _ in 0..5 {
        popup.key(&press(KeyCode::Right), &mut grid);
    }
    typed(&mut popup, &mut grid, "100");
    out.push(screen(&popup, &mut grid));
    popup.key(&press(KeyCode::Enter), &mut grid);
    focus(&mut grid, 2);
    popup.open(&snapshot(&mut grid));
    popup.key(&press(KeyCode::Left), &mut grid);
    out.push(screen(&popup, &mut grid));
    typed(&mut popup, &mut grid, "2024-04-01");
    popup.key(&press(KeyCode::Tab), &mut grid);
    out.push(screen(&popup, &mut grid));
    typed(&mut popup, &mut grid, "2024-06-30");
    popup.key(&press(KeyCode::Enter), &mut grid);
    let snapshot = snapshot(&mut grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, 2 * W, 1));
    render_conditions(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
    out.push(text(&buf));
    insta::assert_snapshot!(out.join("\n\n"));
}

#[test]
fn a_column_that_compares_nothing_and_takes_no_text_opens_no_popup() {
    let mut grid = grid();
    grid.set_support(Axis::Rows, Key(1), Support::default().with_text(None));
    focus(&mut grid, 1);
    assert!(!ColumnFilter::new().open(&snapshot(&mut grid)));
}
