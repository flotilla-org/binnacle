//! Slicers in the terminal: the column menu's "Slice by this column", offered
//! only while the Table Source groups, hands the host the column, and the
//! column filter popup opened in slicer mode edits the grouping's slicers on
//! it instead of the rows' conditions; the conditions line lists the slicers
//! after `Sliced by`, and the levels and values of a grouping the source left
//! out as chips not applied.

use std::time::Duration;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Compared, Comparison, Condition, ConditionState, Effect,
    Grid, Grouping, Key, Level, Placement, Scalar, Snapshot, Support, TextQuery, TextSupport,
    Totals, ViewEdit, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{ColumnFilter, Content, HeaderMenu, Theme, render_conditions};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["region", "port", "weight"][col.0 as usize].to_string()
    }
}

const W: u16 = 72;
const H: u16 = 9;

const REGION: Key = Key(0);
const PORT: Key = Key(1);
const WEIGHT: Key = Key(2);

fn char(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Five rows over region, port and weight. With `groups`, the rows group by
/// the region; the port takes text conditions and the weight comparisons.
fn grid(groups: bool) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 12.0).header(4.0).gap(1.0),
    );
    let text = Support::default().with_text(Some(TextSupport::new(false)));
    grid.set_support(Axis::Rows, REGION, text.with_group(groups));
    grid.set_support(Axis::Rows, PORT, text);
    let compare = Support::default()
        .with_text(None)
        .with_compare(Some(Compared::Numbers));
    grid.set_support(Axis::Rows, WEIGHT, compare);
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(W.into(), H.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// The edits the Grid requested since the last call.
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

/// The menu and popup over a blank screen, as a terminal of `W` x `H` shows them.
fn screen(menu: &HeaderMenu, popup: &ColumnFilter, grid: &mut Grid) -> String {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    menu.render(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
    popup.render(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
    text(&buf)
}

fn line(grid: &mut Grid) -> String {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, 2 * W, 1));
    render_conditions(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
    text(&buf)
}

fn contains(key: Key, text: &str) -> Condition {
    Condition::text(key, TextQuery::new(text))
}

#[test]
fn the_column_menu_slices_by_its_column_and_the_popup_edits_the_slicer() {
    let mut grid = grid(true);
    snapshot(&mut grid);
    grid.set_focus(Key(0), PORT);
    let mut menu = HeaderMenu::new();
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    assert!(menu.open(&snapshot(&mut grid), Axis::Cols));
    let open = screen(&menu, &popup, &mut grid);
    assert!(open.contains("s Slice by this column"), "{open}");

    assert!(menu.key(&char('s'), &mut grid, |_, _| Vec::new()));
    assert!(!menu.is_open());
    let key = menu.take_slicer().expect("the menu hands over its column");
    assert_eq!(key, PORT);
    assert_eq!(menu.take_slicer(), None, "once");
    assert!(popup.open_slicer(&snapshot(&mut grid), key));
    assert!(popup.slicing());
    let open = screen(&menu, &popup, &mut grid);
    assert!(open.contains(" Slice by port "), "{open}");

    popup.key(&char('b'), &mut grid);
    popup.key(&char('e'), &mut grid);
    assert_eq!(
        edits(&mut grid),
        vec![
            ViewEdit::AddSlicer {
                condition: contains(PORT, "b")
            },
            ViewEdit::ReplaceSlicer {
                from: contains(PORT, "b"),
                to: contains(PORT, "be"),
            },
        ]
    );
    let snap = snapshot(&mut grid);
    assert_eq!(
        snap.conditions(Axis::Rows),
        vec![],
        "no condition on the rows"
    );
    assert_eq!(
        snap.slicers(),
        vec![(contains(PORT, "be"), ConditionState::Adding)]
    );

    // Esc puts back the slicers from before: none.
    popup.key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    assert!(!popup.slicing());
    assert_eq!(
        edits(&mut grid),
        vec![ViewEdit::RemoveSlicer {
            condition: contains(PORT, "be")
        }]
    );
}

#[test]
fn the_slicer_popup_opens_on_any_column_with_its_slicers_not_its_conditions() {
    let mut grid = grid(true);
    let heavy = Condition::compare(WEIGHT, Comparison::Greater(Scalar::Number(100.0)));
    let light = Condition::compare(WEIGHT, Comparison::AtMost(Scalar::Number(5.0)));
    grid.add_condition(Axis::Rows, light.clone());
    grid.add_slicer(heavy.clone());
    edits(&mut grid);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    // Nothing focuses the weight column: the host opens it by its Key.
    assert!(popup.open_slicer(&snapshot(&mut grid), WEIGHT));
    let open = screen(&HeaderMenu::new(), &popup, &mut grid);
    assert!(open.contains(" Slice by weight "), "{open}");
    assert!(
        open.contains("‹Greater than› 100"),
        "the slicer's comparison: {open}"
    );
    assert!(!open.contains("At most"), "not the condition's: {open}");
    popup.key(
        &KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
        &mut grid,
    );
    popup.key(
        &KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &mut grid,
    );
    let removed = edits(&mut grid);
    assert!(
        removed.contains(&ViewEdit::RemoveSlicer { condition: heavy }),
        "{removed:?}"
    );
    assert!(
        !removed
            .iter()
            .any(|e| matches!(e, ViewEdit::RemoveCondition { .. }))
    );
}

#[test]
fn a_source_that_groups_nothing_offers_no_grouping_or_slicing() {
    let mut grid = grid(false);
    snapshot(&mut grid);
    grid.set_focus(Key(0), PORT);
    let mut menu = HeaderMenu::new();
    assert!(menu.open(&snapshot(&mut grid), Axis::Cols));
    let open = screen(&menu, &ColumnFilter::new(), &mut grid);
    assert!(!open.contains("Slice"), "{open}");
    assert!(!open.contains("Group"), "{open}");
    menu.key(&char('s'), &mut grid, |_, _| Vec::new());
    assert_eq!(menu.take_slicer(), None);
}

#[test]
fn the_conditions_line_lists_slicers_and_the_parts_of_a_grouping_left_out() {
    let mut grid = grid(true);
    let mut out = vec![];
    let native = Condition::native(PORT, "port LIKE 'A%'");
    let heavy = Condition::compare(WEIGHT, Comparison::Greater(Scalar::Number(100.0)));
    let mut wanted = Grouping::rows_by(REGION);
    wanted.row_levels = vec![
        Level::field(REGION),
        Level::Field {
            field: PORT,
            subtotal: Placement::Before,
        },
    ];
    wanted.values = vec![
        AggregateValue::of(WEIGHT, "sum"),
        AggregateValue::of(WEIGHT, "median"),
    ];
    wanted.slicers = vec![heavy.clone(), native];
    grid.request_view(ViewSpec::default().with_grouping(Some(wanted)));
    edits(&mut grid);
    out.push(line(&mut grid));
    // The source groups by the region alone, sums and slices by comparisons.
    let mut applied = Grouping::rows_by(REGION);
    applied.values = vec![AggregateValue::of(WEIGHT, "sum")];
    applied.slicers = vec![heavy];
    grid.view_applied(
        ViewSpec::default().with_grouping(Some(applied)),
        Totals::unknown(),
    );
    out.push(line(&mut grid));
    let last = out.last().unwrap();
    assert!(last.contains("Sliced by"), "{last}");
    assert!(last.contains("port: ‹port LIKE 'A%'›!"), "{last}");
    assert!(
        last.contains("Grouped by port (Subtotal before)!"),
        "{last}"
    );
    assert!(last.contains("median(weight)!"), "{last}");
    insta::assert_snapshot!(out.join("\n"));
}
