//! The bare keys preset's filter drawer: a column filter shown at the
//! table's right that applies as keys arrive, gives focus back to the table
//! with Enter or Esc, and then follows the focused column.

use std::time::Duration;

use binnacle_core::{
    Axis, AxisConfig, Blanks, Condition, Distinct, DistinctQuery, DistinctValue, Effect, Grid, Key,
    Snapshot, Support, TextQuery, TextSupport, ViewEdit, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{ColumnFilter, Content, RenderContext, Theme, render_with};
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
        ["id", "name", "port"][col.0 as usize].to_string()
    }
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c))
}

const W: u16 = 80;
const H: u16 = 14;

/// Five rows over id, name and port. The id takes no conditions; name Text
/// conditions; port Text conditions with regexes and OneOf.
fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 10.0).header(3.0).gap(1.0),
    );
    grid.set_support(Axis::Rows, Key(0), Support::default().with_text(None));
    let port = Support::default()
        .with_text(Some(TextSupport::new(true)))
        .with_one_of(true);
    grid.set_support(Axis::Rows, Key(2), port);
    grid
}

/// Lays the grid out in what the drawer leaves of the screen.
fn snapshot<'a>(grid: &'a mut Grid, drawer: &ColumnFilter) -> Snapshot<'a> {
    let width = W - drawer.width();
    for (axis, range) in grid.begin_frame(Viewport::new(width.into(), H.into())) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

fn focus(grid: &mut Grid, drawer: &ColumnFilter, col: u64) {
    snapshot(grid, drawer);
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

fn ports(_: &DistinctQuery) -> Distinct {
    let values = [("Bergen", 2), ("Hull", 1)]
        .into_iter()
        .map(|(p, n)| DistinctValue {
            text: p.to_string(),
            count: Some(n),
        })
        .collect();
    Distinct {
        values,
        blanks: Some(Blanks::Counted(2)),
        partial: false,
        narrowed: false,
    }
}

/// Fetches the values (following focus first), as a host does each frame.
fn fetch(drawer: &mut ColumnFilter, grid: &mut Grid) {
    let snapshot = snapshot(grid, drawer);
    drawer.fetch(&snapshot, |_, _, q| Some(ports(q)));
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

/// The grid at the left and the drawer in the columns it takes at the right.
fn screen(drawer: &mut ColumnFilter, grid: &mut Grid) -> Buffer {
    fetch(drawer, grid);
    let snapshot = snapshot(grid, drawer);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    let side = drawer.width();
    let table = Rect::new(0, 0, W - side, H);
    render_with(&snapshot, &Labels, &ctx, table, &mut buf);
    let area = Rect::new(W - side, 0, side, H);
    drawer.render(&snapshot, &Labels, &theme, area, &mut buf);
    buf
}

#[test]
fn the_drawer_takes_no_room_and_no_keys_until_opened() {
    let mut grid = grid();
    let mut drawer = ColumnFilter::new().drawer().pause(Duration::ZERO);
    assert_eq!(drawer.width(), 0);
    assert!(!drawer.key(&char('a'), &mut grid));
    assert!(!drawer.close());
    focus(&mut grid, &drawer, 2);
    assert!(drawer.open(&snapshot(&mut grid, &drawer)));
    assert!(drawer.focused());
    assert!(drawer.width() > 0);
    assert!(drawer.key(&char('a'), &mut grid));
    assert!(drawer.close());
    assert_eq!(drawer.width(), 0);
    assert_eq!(drawer.column(), None);
    let added = |c: Condition| ViewEdit::AddCondition {
        axis: Axis::Rows,
        condition: c,
    };
    assert_eq!(
        edits(&mut grid),
        vec![added(Condition::text(Key(2), TextQuery::new("a")))],
        "closing keeps what it asked for"
    );
}

#[test]
fn esc_gives_the_table_focus_keeping_the_conditions_and_the_drawer_follows_it() {
    let mut grid = grid();
    let mut drawer = ColumnFilter::new().drawer().pause(Duration::from_secs(60));
    focus(&mut grid, &drawer, 2);
    drawer.open(&snapshot(&mut grid, &drawer));
    drawer.key(&char('b'), &mut grid);
    assert_eq!(edits(&mut grid), vec![], "typing waits");
    drawer.key(&press(KeyCode::Esc), &mut grid);
    assert!(!drawer.focused());
    let bergen = Condition::text(Key(2), TextQuery::new("b"));
    assert_eq!(
        edits(&mut grid),
        vec![ViewEdit::AddCondition {
            axis: Axis::Rows,
            condition: bergen,
        }],
        "Esc sends what waits, and puts nothing back"
    );
    assert!(!drawer.key(&char('x'), &mut grid), "keys go to the table");
    focus(&mut grid, &drawer, 1);
    fetch(&mut drawer, &mut grid);
    assert_eq!(drawer.column(), Some(Key(1)), "follows the focused column");
    focus(&mut grid, &drawer, 0);
    fetch(&mut drawer, &mut grid);
    assert_eq!(drawer.column(), None, "the id takes no conditions");
    assert!(drawer.width() > 0, "but the drawer stays");
    assert!(
        !drawer.open(&snapshot(&mut grid, &drawer)),
        "nothing to focus"
    );
    focus(&mut grid, &drawer, 2);
    assert!(drawer.open(&snapshot(&mut grid, &drawer)));
    drawer.key(&press(KeyCode::Enter), &mut grid);
    assert!(!drawer.focused(), "Enter goes back to the table too");
    assert_eq!(drawer.column(), Some(Key(2)));
}

#[test]
fn the_drawer_draws_at_the_right_following_focus_and_lit_only_while_focused() {
    let mut grid = grid();
    let mut drawer = ColumnFilter::new().drawer().pause(Duration::ZERO);
    focus(&mut grid, &drawer, 2);
    drawer.open(&snapshot(&mut grid, &drawer));
    fetch(&mut drawer, &mut grid);
    drawer.key(&press(KeyCode::Tab), &mut grid);
    drawer.key(&press(KeyCode::Tab), &mut grid);
    drawer.key(&press(KeyCode::Down), &mut grid);
    drawer.key(&char(' '), &mut grid);
    let focused = screen(&mut drawer, &mut grid);
    let mut out = vec![text(&focused)];
    let cursor = (0..H).any(|y| {
        (0..W).any(|x| focused[(x, y)].modifier.contains(Modifier::REVERSED))
            && text(&focused)
                .lines()
                .nth(y as usize)
                .is_some_and(|l| l.contains("(Blanks)"))
    });
    assert!(cursor, "the list's cursor is lit");
    drawer.key(&press(KeyCode::Esc), &mut grid);
    let away = screen(&mut drawer, &mut grid);
    out.push(text(&away));
    let side = W - drawer.width();
    let lit = (0..H).any(|y| (side..W).any(|x| away[(x, y)].modifier.contains(Modifier::REVERSED)));
    assert!(!lit, "no cursor once the table has focus");
    focus(&mut grid, &drawer, 1);
    out.push(text(&screen(&mut drawer, &mut grid)));
    focus(&mut grid, &drawer, 0);
    out.push(text(&screen(&mut drawer, &mut grid)));
    insta::assert_snapshot!(out.join("\n\n"));
}
