//! Slow and failing sources in the terminal: the typing pause on the fields
//! that write a Text Query, and the pending and error states the controls
//! show (a dimmed field, `~` chips and headers, "filtering…" and
//! "finding…"; a red field, chip, header or count with the source's
//! message).

use std::time::{Duration, Instant};

use binnacle_core::{
    Axis, AxisConfig, Condition, Effect, FilterSpec, FindQuery, Grid, Key, Snapshot, Support,
    TextQuery, TextSupport, Totals, ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{
    ColumnFilter, Content, FindBar, FindStatus, QuickFilter, RenderContext, TYPING_PAUSE, Theme,
    render_conditions, render_with, row_count,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};

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

fn enter() -> KeyEvent {
    press(KeyCode::Enter, KeyModifiers::NONE)
}

const W: u16 = 64;
const H: u16 = 12;

/// Five rows over name, port and crew, 12 wide with a one-cell Gap. The rows
/// take a quick filter with regexes, port Text conditions with regexes, and
/// the Grid finds.
fn grid() -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(5, 1.0).header(1.0),
        AxisConfig::new(3, 12.0).header(4.0).gap(1.0),
    );
    let regex = Some(TextSupport::new(true));
    grid.set_quick_filter_support(Axis::Rows, regex);
    grid.set_support(Axis::Rows, Key(1), Support::default().with_text(regex));
    grid.set_find_support(regex);
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(W.into(), 6.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// The View Specs the Grid requested since the last call, unanswered.
fn requested(grid: &mut Grid) -> Vec<ViewSpec> {
    let mut out = vec![];
    while let Some(stamped) = grid.poll_effect() {
        if let Effect::ViewRequested { spec, .. } = stamped.effect {
            out.push(spec);
        }
    }
    out
}

/// The Find Queries the Grid set since the last call, and how many steps
/// it asked for.
fn finds(grid: &mut Grid) -> (Vec<Option<FindQuery>>, usize) {
    let (mut queries, mut steps) = (vec![], 0);
    while let Some(stamped) = grid.poll_effect() {
        match stamped.effect {
            Effect::FindRequested { query } => queries.push(query),
            Effect::FindStepRequested { .. } => steps += 1,
            _ => {}
        }
    }
    (queries, steps)
}

fn quick(text: &str) -> Option<TextQuery> {
    Some(TextQuery::new(text))
}

#[test]
fn the_quick_filter_waits_for_a_pause_in_typing_and_enter_and_toggles_send_at_once() {
    let mut grid = grid();
    let mut field = QuickFilter::new();
    field.focus(&snapshot(&mut grid));
    let typed = Instant::now();
    for c in "ber".chars() {
        field.key(&char(c), &mut grid);
    }
    assert_eq!(requested(&mut grid), vec![], "nothing while typing");
    let deadline = field.deadline().expect("text waits");
    assert!(deadline >= typed + TYPING_PAUSE);
    field.tick(&mut grid, deadline - Duration::from_millis(1));
    assert_eq!(requested(&mut grid), vec![]);
    field.tick(&mut grid, deadline);
    let spec = |q| ViewSpec::default().with_filter(Axis::Rows, FilterSpec::default().with_quick(q));
    assert_eq!(
        requested(&mut grid),
        vec![spec(quick("ber"))],
        "once, after the pause"
    );
    assert_eq!(field.deadline(), None);

    // A toggle sends what was typed with it, at once.
    field.key(&char('g'), &mut grid);
    field.key(&alt('c'), &mut grid);
    let ci = Some(TextQuery::new("berg").case_sensitive(true));
    assert_eq!(requested(&mut grid), vec![spec(ci.clone())]);
    // Enter sends at once and leaves the field.
    field.key(&char('e'), &mut grid);
    field.key(&enter(), &mut grid);
    let cie = Some(TextQuery::new("berge").case_sensitive(true));
    assert_eq!(requested(&mut grid), vec![spec(cie)]);
    assert!(!field.focused());
    field.tick(&mut grid, Instant::now() + Duration::from_secs(1));
    assert_eq!(requested(&mut grid), vec![], "nothing left waiting");

    // Esc drops what waits and puts back the filter from before.
    field.focus(&snapshot(&mut grid));
    field.key(&char('n'), &mut grid);
    field.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    field.tick(&mut grid, Instant::now() + Duration::from_secs(1));
    assert_eq!(
        requested(&mut grid),
        vec![],
        "\"n\" never sent, and the filter from before is in force"
    );
}

#[test]
fn the_find_bar_waits_for_a_pause_and_a_step_sends_the_query_first() {
    let mut grid = grid();
    let mut bar = FindBar::new();
    bar.open(&snapshot(&mut grid));
    bar.apply(&mut grid);
    finds(&mut grid);
    for c in "ab".chars() {
        bar.key(&char(c), &mut grid);
    }
    assert_eq!(finds(&mut grid), (vec![], 0), "nothing while typing");
    // Enter steps: the query goes first, then one step.
    bar.key(&enter(), &mut grid);
    bar.key(&enter(), &mut grid);
    bar.key(&enter(), &mut grid);
    let ab = FindQuery::new(TextQuery::new("ab"));
    assert_eq!(
        finds(&mut grid),
        (vec![Some(ab)], 1),
        "pressed three times before the first answer: one step"
    );
}

#[test]
fn the_column_filter_waits_for_a_pause_and_not_sends_at_once() {
    let mut grid = grid();
    snapshot(&mut grid);
    grid.set_focus(Key(0), Key(1));
    let mut popup = ColumnFilter::new();
    assert!(popup.open(&snapshot(&mut grid)));
    for c in "ber".chars() {
        popup.key(&char(c), &mut grid);
    }
    assert_eq!(requested(&mut grid), vec![]);
    popup.key(&alt('n'), &mut grid);
    let not_ber = Condition::text(Key(1), TextQuery::new("ber")).not(true);
    let spec = |c: Vec<Condition>| {
        ViewSpec::default().with_filter(Axis::Rows, FilterSpec::default().with_conditions(c.into()))
    };
    assert_eq!(requested(&mut grid), vec![spec(vec![not_ber.clone()])]);
    popup.key(&char('g'), &mut grid);
    let deadline = popup.deadline().expect("text waits");
    popup.tick(&mut grid, deadline);
    let not_berg = Condition::text(Key(1), TextQuery::new("berg")).not(true);
    assert_eq!(requested(&mut grid), vec![spec(vec![not_berg])]);
}

/// Each cell of `buf` as a style letter: `R` in the error colour, `d` dimmed,
/// `.` otherwise.
fn styles(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| {
                    let cell = &buf[(x, y)];
                    if cell.fg == Color::LightRed {
                        'R'
                    } else if cell.modifier.contains(Modifier::DIM) && cell.symbol() != " " {
                        'd'
                    } else {
                        '.'
                    }
                })
                .collect();
            line.trim_end_matches('.').to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// A terminal of `W` x `H`: the quick filter, the chips, the grid with the
/// popup over it, the find bar and the row count; its text, then its styles.
fn screen(
    grid: &mut Grid,
    field: &QuickFilter,
    popup: &ColumnFilter,
    bar: &FindBar,
    status: &FindStatus,
) -> String {
    let snapshot = snapshot(grid);
    let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
    let theme = Theme::classic();
    let line = |y| Rect::new(0, y, W, 1);
    field.render(&snapshot, &theme, line(0), &mut buf);
    render_conditions(&snapshot, &Labels, &theme, line(1), &mut buf);
    let body = Rect::new(0, 2, W, 8);
    render_with(
        &snapshot,
        &Labels,
        &RenderContext::new(&theme),
        body,
        &mut buf,
    );
    popup.render(&snapshot, &Labels, &theme, body, &mut buf);
    bar.render(&snapshot, status, &theme, line(10), &mut buf);
    let count = row_count(&snapshot);
    for (i, ch) in count.chars().enumerate() {
        buf[(i as u16, 11)].set_char(ch);
    }
    format!("{}\n{}", text(&buf), styles(&buf))
}

#[test]
fn pending_requests_show_on_the_controls_and_the_rows_stay() {
    let mut grid = grid();
    let mut field = QuickFilter::new().pause(Duration::ZERO);
    let popup = ColumnFilter::new();
    let mut bar = FindBar::new();
    let port = Condition::text(Key(1), TextQuery::new("ber"));
    grid.add_condition(Axis::Rows, port.clone());
    grid.view_applied(
        ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(vec![port].into()),
        ),
        Totals::unknown().with(Axis::Rows, Some(9)),
    );
    // A quick filter and a condition on name, both unanswered; find typed,
    // waiting out its pause.
    field.focus(&snapshot(&mut grid));
    field.key(&char('a'), &mut grid);
    field.key(&enter(), &mut grid);
    grid.add_condition(Axis::Rows, Condition::text(Key(0), TextQuery::new("x")));
    bar.open(&snapshot(&mut grid));
    bar.key(&char('q'), &mut grid);
    let status = FindStatus::default();
    insta::assert_snapshot!(screen(&mut grid, &field, &popup, &bar, &status));
}

#[test]
fn what_the_source_did_not_apply_shows_red_with_its_message() {
    let mut grid = grid();
    let mut field = QuickFilter::new().pause(Duration::ZERO);
    let mut popup = ColumnFilter::new().pause(Duration::ZERO);
    let mut bar = FindBar::new().pause(Duration::ZERO);
    // A regex quick filter and a regex condition on port, both invalid: the
    // source answers each with the spec in force.
    field.focus(&snapshot(&mut grid));
    field.key(&alt('r'), &mut grid);
    field.key(&char('('), &mut grid);
    field.key(&enter(), &mut grid);
    field.set_error(Some("unclosed group".into()));
    snapshot(&mut grid);
    grid.set_focus(Key(0), Key(1));
    popup.open(&snapshot(&mut grid));
    popup.key(&alt('r'), &mut grid);
    popup.key(&char('['), &mut grid);
    popup.set_error(Some("unclosed character class".into()));
    for _ in requested(&mut grid) {
        grid.view_applied(ViewSpec::default(), Totals::unknown());
    }
    // Find failed too.
    bar.open(&snapshot(&mut grid));
    bar.key(&alt('r'), &mut grid);
    bar.key(&char('('), &mut grid);
    let status = FindStatus {
        error: Some("unclosed group".into()),
        ..FindStatus::default()
    };
    insta::assert_snapshot!(screen(&mut grid, &field, &popup, &bar, &status));
}
