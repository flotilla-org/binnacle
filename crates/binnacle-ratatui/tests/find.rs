//! Find in the ratatui adapter: the chords preset's keys, the find bar's
//! editing, stepping and drawing, and the marks and the current match drawn
//! over the cells.

use std::time::Duration;

use binnacle_core::{
    Axis, AxisConfig, Effect, FindDirection, FindMatch, FindQuery, Grid, Key, Snapshot, TextQuery,
    TextSupport, ViewEdit, Viewport,
};
use binnacle_memory::{FindCount, IdentitySource};
use binnacle_ratatui::{
    Content, Control, Controls, FindBar, FindStatus, Preset, RenderContext, Theme, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c), KeyModifiers::NONE)
}

/// A Grid of 6 rows by 3 columns that finds (with regexes, if `regex`).
fn grid(regex: bool) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(3.0),
    );
    grid.set_find_support(Some(TextSupport::new(regex)));
    grid.set_quick_filter_support(Axis::Rows, Some(TextSupport::new(regex)));
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(24.0, 7.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// What the Grid asked of the host since the last call: the Find Queries
/// set, the steps, and the quick filters.
#[derive(Debug, PartialEq)]
enum Asked {
    Find(Option<String>),
    Step(FindDirection),
    Quick(Option<String>),
}

fn asked(grid: &mut Grid) -> Vec<Asked> {
    let mut out = vec![];
    while let Some(stamped) = grid.poll_effect() {
        match stamped.effect {
            Effect::FindRequested { query } => {
                out.push(Asked::Find(query.map(|q| describe(&q))));
            }
            Effect::FindStepRequested { direction, .. } => {
                out.push(Asked::Step(direction));
                grid.find_stepped(None);
            }
            Effect::ViewRequested {
                edit: ViewEdit::SetQuickFilter { query, .. },
                ..
            } => out.push(Asked::Quick(query.map(|q| q.text))),
            _ => {}
        }
    }
    out
}

/// A Find Query as `text [case] [regex] [by cols]`.
fn describe(q: &FindQuery) -> String {
    let mut out = q.query.text.clone();
    if q.query.case_sensitive {
        out += " case";
    }
    if q.query.regex {
        out += " regex";
    }
    if q.by == Axis::Cols {
        out += " by cols";
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

fn bar_line(bar: &FindBar, snapshot: &Snapshot, status: &FindStatus) -> String {
    let mut buf = Buffer::empty(Rect::new(0, 0, 90, 1));
    bar.render(snapshot, status, &Theme::classic(), buf.area, &mut buf);
    text(&buf)
}

#[test]
fn the_chords_preset_opens_find_with_ctrl_f_and_steps_with_f3() {
    let controls = Controls::preset(Preset::Chords);
    let at = |code, modifiers| controls.control(&press(code, modifiers));
    assert_eq!(
        at(KeyCode::Char('f'), KeyModifiers::CONTROL),
        Some(Control::Find)
    );
    assert_eq!(
        at(KeyCode::F(3), KeyModifiers::NONE),
        Some(Control::FindNext)
    );
    assert_eq!(
        at(KeyCode::F(3), KeyModifiers::SHIFT),
        Some(Control::FindPrevious)
    );
    assert_eq!(at(KeyCode::Char('f'), KeyModifiers::NONE), None);
}

#[test]
fn the_bar_opens_only_while_the_grid_finds() {
    let mut without = Grid::new(AxisConfig::new(3, 1.0), AxisConfig::new(2, 6.0));
    let mut bar = FindBar::new().pause(Duration::ZERO);
    assert!(!bar.open(&snapshot(&mut without)));
    assert!(!bar.is_open());
    assert!(!bar.key(&char('a'), &mut without));
    assert!(!bar.step(FindDirection::Next, &mut without));
    assert_eq!(asked(&mut without), vec![]);
    let mut with = grid(false);
    assert!(bar.open(&snapshot(&mut with)));
    assert!(bar.is_open() && bar.focused());
}

#[test]
fn typing_sets_the_find_query_and_enter_and_the_chords_step() {
    let mut grid = grid(true);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    bar.open(&snapshot(&mut grid));
    let alt = KeyModifiers::ALT;
    let ctrl = KeyModifiers::CONTROL;
    for key in [
        char('a'),
        press(KeyCode::Char('B'), KeyModifiers::SHIFT),
        press(KeyCode::Char('c'), alt),
        press(KeyCode::Char('r'), alt),
        press(KeyCode::Char('b'), alt),
        press(KeyCode::Backspace, KeyModifiers::NONE),
    ] {
        assert!(bar.key(&key, &mut grid));
    }
    assert_eq!(
        asked(&mut grid),
        [
            "a",
            "aB",
            "aB case",
            "aB case regex",
            "aB case regex by cols",
            "a case regex by cols",
        ]
        .map(|q| Asked::Find(Some(q.into())))
    );
    // Each step is answered before the next, which the Grid holds meanwhile.
    use FindDirection::{Next, Previous};
    for (key, direction) in [
        (press(KeyCode::Enter, KeyModifiers::NONE), Next),
        (press(KeyCode::Enter, KeyModifiers::SHIFT), Previous),
        (press(KeyCode::Char('n'), ctrl), Next),
        (press(KeyCode::Char('p'), ctrl), Previous),
    ] {
        bar.key(&key, &mut grid);
        assert_eq!(asked(&mut grid), vec![Asked::Step(direction)]);
    }
    // Ctrl+U empties the text, which closes the Find Query.
    bar.key(&press(KeyCode::Char('u'), ctrl), &mut grid);
    assert_eq!(asked(&mut grid), vec![Asked::Find(None)]);
}

#[test]
fn esc_leaves_the_bar_with_find_open_so_f3_steps_and_close_ends_find() {
    let mut grid = grid(false);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    bar.open(&snapshot(&mut grid));
    bar.key(&char('x'), &mut grid);
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    assert!(bar.is_open() && !bar.focused());
    assert!(
        !bar.key(&char('y'), &mut grid),
        "unfocused, it takes no keys"
    );
    assert_eq!(asked(&mut grid), vec![Asked::Find(Some("x".into()))]);
    assert!(bar.step(FindDirection::Next, &mut grid));
    assert_eq!(asked(&mut grid), vec![Asked::Step(FindDirection::Next)]);
    assert!(bar.step(FindDirection::Previous, &mut grid));
    assert_eq!(asked(&mut grid), vec![Asked::Step(FindDirection::Previous)]);
    bar.close(&mut grid);
    assert!(!bar.step(FindDirection::Next, &mut grid));
    assert_eq!(asked(&mut grid), vec![Asked::Find(None)]);
    // Reopening finds the text again.
    bar.open(&snapshot(&mut grid));
    bar.apply(&mut grid);
    assert_eq!(asked(&mut grid), vec![Asked::Find(Some("x".into()))]);
}

#[test]
fn alt_f_shows_only_matching_rows_and_again_shows_them_all() {
    let mut grid = grid(false);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    bar.open(&snapshot(&mut grid));
    bar.key(&char('b'), &mut grid);
    asked(&mut grid);
    let alt_f = press(KeyCode::Char('f'), KeyModifiers::ALT);
    bar.key(&alt_f, &mut grid);
    bar.key(&alt_f, &mut grid);
    assert_eq!(
        asked(&mut grid),
        vec![Asked::Quick(Some("b".into())), Asked::Quick(None)]
    );
}

#[test]
fn the_bar_shows_the_text_toggles_count_and_wrap() {
    let mut grid = grid(true);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    let count = |total, partial| Some(FindCount { total, partial });
    let mut out = vec![];
    bar.open(&snapshot(&mut grid));
    for c in "bergen".chars() {
        bar.key(&char(c), &mut grid);
    }
    asked(&mut grid);
    let mut status = FindStatus::default();
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    status.count = count(120, false);
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    status.count = count(120, true);
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    status.count = count(0, false);
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    // A step lands on the current match: its place among the matches.
    bar.key(&press(KeyCode::Enter, KeyModifiers::SHIFT), &mut grid);
    grid.find_stepped(Some(FindMatch::new(Key(2), Key(1))));
    status = FindStatus {
        count: count(1204, false),
        index: Some(1203),
        wrapped: true,
        ..FindStatus::default()
    };
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    bar.key(&press(KeyCode::Char('c'), KeyModifiers::ALT), &mut grid);
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    status = FindStatus {
        count: count(1, false),
        ..FindStatus::default()
    };
    out.push(bar_line(&bar, &snapshot(&mut grid), &status));
    bar.close(&mut grid);
    out.push(format!(
        "closed: [{}]",
        bar_line(&bar, &snapshot(&mut grid), &status)
    ));
    insta::assert_snapshot!(out.join("\n"));
}

/// Cells read `r<row>c<col>`; those in `marked` match the Find Query.
struct Marked(Vec<(u64, u64)>);

impl Content for Marked {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }

    fn marked(&self, row: Key, col: Key) -> bool {
        self.0.contains(&(row.0, col.0))
    }
}

#[test]
fn marked_cells_and_the_current_match_are_drawn_in_their_styles() {
    let mut grid = grid(false);
    grid.set_find(Some(FindQuery::new(TextQuery::new("1"))));
    grid.action(binnacle_core::Action::FindNext);
    asked(&mut grid);
    snapshot(&mut grid);
    grid.find_stepped(Some(FindMatch::new(Key(3), Key(1))));
    asked(&mut grid);
    let content = Marked(vec![(1, 0), (1, 1), (3, 1), (4, 2)]);
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme).scrollbars(false);
    let area = Rect::new(0, 0, 24, 7);
    let mut buf = Buffer::empty(area);
    render_with(&snapshot(&mut grid), &content, &ctx, area, &mut buf);
    insta::assert_snapshot!(text(&buf));
    // Row 1's first cell is marked; row 2's is not.
    assert_eq!(buf[(3, 2)].bg, theme.find_match.bg.unwrap());
    assert_ne!(buf[(3, 3)].bg, theme.find_match.bg.unwrap());
    // The current match, focused, ends in its marker.
    let marker = &buf[(14, 4)];
    assert_eq!(marker.symbol(), "◂");
    assert_eq!(marker.fg, theme.find_current.fg.unwrap());
}

#[test]
fn opened_backwards_enter_and_repeat_step_back_and_reversed_steps_on() {
    let mut grid = grid(false);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    assert!(bar.open_backward(&snapshot(&mut grid)));
    bar.key(&char('a'), &mut grid);
    // Each step is answered before the next, which would otherwise wait.
    let mut out = vec![];
    bar.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut grid);
    out.extend(asked(&mut grid));
    bar.key(&press(KeyCode::Enter, KeyModifiers::SHIFT), &mut grid);
    out.extend(asked(&mut grid));
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    assert!(bar.repeat(false, &mut grid));
    out.extend(asked(&mut grid));
    assert!(bar.repeat(true, &mut grid));
    out.extend(asked(&mut grid));
    use FindDirection::{Next, Previous};
    assert_eq!(
        out,
        vec![
            Asked::Find(Some("a".into())),
            Asked::Step(Previous),
            Asked::Step(Next),
            Asked::Step(Previous),
            Asked::Step(Next),
        ]
    );
    assert!(bar.open(&snapshot(&mut grid)));
    bar.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut grid);
    bar.close(&mut grid);
    assert!(!bar.repeat(false, &mut grid), "closed, it steps nowhere");
    assert_eq!(
        asked(&mut grid),
        vec![Asked::Step(Next), Asked::Find(None)],
        "opened forwards again, Enter steps on"
    );
}

#[test]
fn from_the_table_case_and_only_matching_act_as_alt_c_and_alt_f_do() {
    let mut grid = grid(false);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    bar.only_matching(&mut grid);
    assert_eq!(asked(&mut grid), vec![], "no find text, no filter");
    bar.open(&snapshot(&mut grid));
    bar.key(&char('b'), &mut grid);
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    bar.toggle_case(&mut grid);
    bar.only_matching(&mut grid);
    bar.only_matching(&mut grid);
    assert_eq!(
        asked(&mut grid),
        vec![
            Asked::Find(Some("b".into())),
            Asked::Find(Some("b case".into())),
            Asked::Quick(Some("b".into())),
            Asked::Quick(None),
        ]
    );
}

#[test]
fn opening_again_selects_the_text_so_typing_replaces_it_and_other_keys_keep_it() {
    let mut grid = grid(false);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    bar.open(&snapshot(&mut grid));
    bar.key(&char('a'), &mut grid);
    bar.key(&char('b'), &mut grid);
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    bar.open(&snapshot(&mut grid));
    bar.key(&char('c'), &mut grid);
    bar.key(&char('d'), &mut grid);
    bar.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    bar.open_backward(&snapshot(&mut grid));
    bar.key(&press(KeyCode::Char('c'), KeyModifiers::ALT), &mut grid);
    bar.key(&char('e'), &mut grid);
    let found = asked(&mut grid);
    assert_eq!(
        found.last(),
        Some(&Asked::Find(Some("cde case".into()))),
        "{found:?}"
    );
    assert!(found.contains(&Asked::Find(Some("c".into()))), "{found:?}");
    assert!(
        !found.contains(&Asked::Find(Some("abc".into()))),
        "{found:?}"
    );
}
