//! Column conditions in the terminal: Alt+Down's column filter popup, which
//! edits the focused column's Text condition as keys arrive; the conditions
//! line of chips; and the filter marker in a filtered column's header.

use std::time::Duration;

use binnacle_core::{
    Axis, AxisConfig, Condition, Effect, FilterSpec, Grid, Key, Snapshot, Support, TextQuery,
    TextSupport, Totals, ViewEdit, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    ColumnFilter, Content, Control, Controls, Preset, RenderContext, Theme, render_conditions,
    render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["name", "port", "crew"][col.0 as usize].to_string()
    }
}

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c), KeyModifiers::NONE)
}

fn alt(c: char) -> KeyEvent {
    press(KeyCode::Char(c), KeyModifiers::ALT)
}

const W: u16 = 64;
const H: u16 = 9;

/// Five rows over three columns 12 wide with a one-cell Gap: name, port and
/// crew. Port takes regexes, crew no conditions, name the default (contains).
fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 12.0).header(4.0).gap(1.0),
    );
    grid.set_support(
        Axis::Rows,
        Key(1),
        Support::default().with_text(Some(TextSupport::new(true))),
    );
    grid.set_support(Axis::Rows, Key(2), Support::default().with_text(None));
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(W.into(), H.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// Focuses column `col` (and the first row).
fn focus(grid: &mut Grid, col: u64) {
    snapshot(grid);
    grid.set_focus(Key(0), Key(col));
}

/// The condition edits the Grid requested since the last call.
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

/// The grid with the popup over it, as a terminal of `W` x `H` shows them.
fn screen(popup: &ColumnFilter, grid: &mut Grid) -> Buffer {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    render_with(&snapshot, &Labels, &ctx, buf.area, &mut buf);
    popup.render(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
    buf
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

#[test]
fn the_chords_preset_reaches_the_column_filter_with_alt_down() {
    let controls = Controls::preset(Preset::Chords);
    let alt_down = press(KeyCode::Down, KeyModifiers::ALT);
    assert_eq!(controls.control(&alt_down), Some(Control::ColumnFilter));
    assert_eq!(
        controls.control(&press(KeyCode::Down, KeyModifiers::NONE)),
        None
    );
}

#[test]
fn the_popup_opens_only_on_a_focused_column_that_takes_text_conditions() {
    let mut grid = grid();
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    assert!(!popup.open(&snapshot(&mut grid)), "no focus");
    focus(&mut grid, 2);
    assert!(!popup.open(&snapshot(&mut grid)), "crew takes none");
    assert!(
        !popup.key(&char('a'), &mut grid),
        "closed, it takes no keys"
    );
    focus(&mut grid, 1);
    assert!(popup.open(&snapshot(&mut grid)));
    assert_eq!(popup.column(), Some(Key(1)));
    assert!(popup.key(&char('a'), &mut grid));
    assert_eq!(
        edits(&mut grid),
        vec![ViewEdit::AddCondition {
            axis: Axis::Rows,
            condition: contains(1, "a"),
        }]
    );
}

#[test]
fn each_key_edits_the_columns_condition_and_esc_puts_back_the_one_from_before() {
    let mut grid = grid();
    grid.view_applied(conditions(vec![contains(1, "b")]), Totals::unknown());
    focus(&mut grid, 1);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    popup.open(&snapshot(&mut grid));
    edits(&mut grid);
    for key in [char('e'), alt('n'), alt('c'), alt('r')] {
        popup.key(&key, &mut grid);
    }
    let be = contains(1, "be");
    let not = be.clone().not(true);
    let case = Condition::text(Key(1), TextQuery::new("be").case_sensitive(true)).not(true);
    let regex = Condition::text(
        Key(1),
        TextQuery::new("be").case_sensitive(true).regex(true),
    )
    .not(true);
    let replace = |from: &Condition, to: &Condition| ViewEdit::ReplaceCondition {
        axis: Axis::Rows,
        from: from.clone(),
        to: to.clone(),
    };
    assert_eq!(
        edits(&mut grid),
        vec![
            replace(&contains(1, "b"), &be),
            replace(&be, &not),
            replace(&not, &case),
            replace(&case, &regex),
        ]
    );
    popup.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    assert_eq!(popup.column(), None);
    assert_eq!(edits(&mut grid), vec![replace(&regex, &contains(1, "b"))]);
}

#[test]
fn ctrl_u_removes_the_condition_enter_keeps_it_and_alt_r_needs_regex_support() {
    let mut grid = grid();
    focus(&mut grid, 0);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    popup.open(&snapshot(&mut grid));
    for key in [char('x'), alt('r')] {
        popup.key(&key, &mut grid);
    }
    popup.key(&press(KeyCode::Char('u'), KeyModifiers::CONTROL), &mut grid);
    popup.key(&char('y'), &mut grid);
    popup.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut grid);
    assert_eq!(popup.column(), None);
    let add = |c: Condition| ViewEdit::AddCondition {
        axis: Axis::Rows,
        condition: c,
    };
    assert_eq!(
        edits(&mut grid),
        vec![
            add(contains(0, "x")),
            ViewEdit::RemoveCondition {
                axis: Axis::Rows,
                condition: contains(0, "x"),
            },
            add(contains(0, "y")),
        ],
        "name takes no regexes, so Alt+R asks nothing"
    );
}

#[test]
fn the_popup_drops_under_its_column_header_with_the_condition_and_its_toggles() {
    let mut grid = grid();
    focus(&mut grid, 1);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    popup.open(&snapshot(&mut grid));
    let mut out = vec![text(&screen(&popup, &mut grid))];
    for key in [char('b'), char('e'), char('r'), alt('n'), alt('r')] {
        popup.key(&key, &mut grid);
    }
    let buf = screen(&popup, &mut grid);
    out.push(text(&buf));
    let lit = (0..W).find(|&x| buf[(x, 2)].symbol() == "." && buf[(x + 1, 2)].symbol() == "*");
    let lit = lit.expect("the regex toggle");
    assert!(buf[(lit, 2)].modifier.contains(Modifier::REVERSED), "lit");
    focus(&mut grid, 0);
    let mut name = ColumnFilter::new().pause(Duration::ZERO);
    name.open(&snapshot(&mut grid));
    out.push(text(&screen(&name, &mut grid)));
    insta::assert_snapshot!(out.join("\n\n"));
}

#[test]
fn the_conditions_line_shows_a_chip_for_each_dimmed_while_pending() {
    let mut grid = grid();
    let mut out = vec![];
    let line = |grid: &mut Grid| {
        let snapshot = snapshot(grid);
        let mut buf = Buffer::empty(Rect::new(0, 0, W, 1));
        render_conditions(&snapshot, &Labels, &Theme::classic(), buf.area, &mut buf);
        buf
    };
    out.push(text(&line(&mut grid)));
    let port = contains(1, "bergen").not(true);
    let regex = Condition::text(
        Key(0),
        TextQuery::new("^an").regex(true).case_sensitive(true),
    );
    grid.view_applied(conditions(vec![port.clone()]), Totals::unknown());
    out.push(text(&line(&mut grid)));
    grid.replace_condition(Axis::Rows, &port, regex.clone());
    let buf = line(&mut grid);
    out.push(text(&buf));
    let dim = (0..W).all(|x| {
        buf[(x, 0)].symbol() == " "
            || x < 9
            || buf[(x, 0)].modifier.contains(Modifier::DIM)
            || buf[(x, 0)].modifier.contains(Modifier::CROSSED_OUT)
    });
    assert!(dim, "both chips pending: {buf:?}");
    grid.view_applied(conditions(vec![regex]), Totals::unknown());
    out.push(text(&line(&mut grid)));
    insta::assert_snapshot!(out.join("\n"));
}

#[test]
fn a_filtered_columns_header_carries_the_filter_marker_with_a_tilde_while_pending() {
    let mut grid = grid();
    let header = |grid: &mut Grid| {
        let snapshot = snapshot(grid);
        let mut buf = Buffer::empty(Rect::new(0, 0, W, 1));
        let theme = Theme::classic();
        let ctx = RenderContext::new(&theme);
        render_with(&snapshot, &Labels, &ctx, buf.area, &mut buf);
        text(&buf)
    };
    let mut out = vec![header(&mut grid)];
    grid.view_applied(conditions(vec![contains(1, "b")]), Totals::unknown());
    out.push(header(&mut grid));
    grid.add_condition(Axis::Rows, contains(0, "a"));
    grid.remove_condition(Axis::Rows, &contains(1, "b"));
    out.push(header(&mut grid));
    insta::assert_snapshot!(out.join("\n"));
}
