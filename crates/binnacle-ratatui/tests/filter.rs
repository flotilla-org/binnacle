//! The find and filter controls: the chords preset's keys, the quick filter
//! field's editing and drawing, and the status line's row count.

use binnacle_core::{
    Axis, AxisConfig, Effect, FilterSpec, Grid, Snapshot, TextQuery, TextSupport, Totals, ViewEdit,
    ViewSpec, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Control, Controls, Preset, QuickFilter, Theme, row_count};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Modifier;

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c), KeyModifiers::NONE)
}

/// A Grid of `rows` rows whose rows take a quick filter (`regex` or not),
/// if `support`.
fn grid(rows: u64, support: Option<TextSupport>) -> Grid {
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(2, 8.0).header(5.0),
    );
    grid.set_quick_filter_support(Axis::Rows, support);
    grid
}

fn snapshot(grid: &mut Grid) -> Snapshot<'_> {
    for (axis, range) in grid.begin_frame(Viewport::new(30.0, 5.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    grid.snapshot()
}

/// The quick filters the Grid requested since the last call.
fn requested(grid: &mut Grid) -> Vec<Option<TextQuery>> {
    let mut out = vec![];
    while let Some(stamped) = grid.poll_effect() {
        if let Effect::ViewRequested {
            edit: ViewEdit::SetQuickFilter { query, .. },
            ..
        } = stamped.effect
        {
            out.push(query);
        }
    }
    out
}

fn line(field: &QuickFilter, snapshot: &Snapshot, width: u16) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, 1));
    field.render(snapshot, &Theme::classic(), buf.area, &mut buf);
    buf
}

fn text(buf: &Buffer) -> String {
    let a = buf.area;
    (a.left()..a.right())
        .map(|x| buf[(x, a.y)].symbol())
        .collect::<String>()
        .trim_end()
        .to_string()
}

#[test]
fn the_chords_preset_reaches_the_quick_filter_with_ctrl_l() {
    let mut controls = Controls::preset(Preset::Chords);
    let ctrl = KeyModifiers::CONTROL;
    let ctrl_l = press(KeyCode::Char('l'), ctrl);
    assert_eq!(controls.control(&ctrl_l), Some(Control::QuickFilter));
    assert_eq!(controls.control(&char('l')), None);
    let mut released = ctrl_l;
    released.kind = KeyEventKind::Release;
    assert_eq!(controls.control(&released), None);
    controls.bind(KeyCode::Char('/'), KeyModifiers::NONE, Control::QuickFilter);
    assert_eq!(controls.control(&char('/')), Some(Control::QuickFilter));
    assert_eq!(
        controls.unbind(KeyCode::Char('l'), ctrl),
        Some(Control::QuickFilter)
    );
    assert_eq!(controls.control(&ctrl_l), None);
}

#[test]
fn the_field_takes_focus_only_while_the_rows_have_a_quick_filter() {
    let mut field = QuickFilter::new();
    let mut without = grid(5, None);
    assert!(!field.focus(&snapshot(&mut without)));
    assert!(!field.key(&char('a'), &mut without));
    assert_eq!(requested(&mut without), vec![]);
    let mut with = grid(5, Some(TextSupport::new(false)));
    assert!(field.focus(&snapshot(&mut with)));
    assert!(field.key(&char('a'), &mut with));
    assert_eq!(requested(&mut with), vec![Some(TextQuery::new("a"))]);
}

#[test]
fn each_change_sets_the_quick_filter_and_esc_puts_back_the_one_before_focus() {
    let mut grid = grid(5, Some(TextSupport::new(true)));
    let mut field = QuickFilter::new();
    let alt = KeyModifiers::ALT;
    field.focus(&snapshot(&mut grid));
    for key in [char('a'), press(KeyCode::Char('B'), KeyModifiers::SHIFT)] {
        field.key(&key, &mut grid);
    }
    field.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut grid);
    assert!(!field.focused());
    assert_eq!(field.text(), "aB");
    let ab = TextQuery::new("aB");
    assert_eq!(
        requested(&mut grid),
        vec![Some(TextQuery::new("a")), Some(ab.clone())]
    );
    field.focus(&snapshot(&mut grid));
    field.key(&press(KeyCode::Char('c'), alt), &mut grid);
    field.key(&press(KeyCode::Char('r'), alt), &mut grid);
    field.key(&press(KeyCode::Backspace, KeyModifiers::NONE), &mut grid);
    field.key(&press(KeyCode::Char('u'), KeyModifiers::CONTROL), &mut grid);
    // Ctrl and Alt chords type nothing.
    field.key(&press(KeyCode::Char('x'), KeyModifiers::CONTROL), &mut grid);
    let options = |text: &str| TextQuery::new(text).case_sensitive(true).regex(true);
    assert_eq!(
        requested(&mut grid),
        vec![
            Some(ab.clone().case_sensitive(true)),
            Some(options("aB")),
            Some(options("a")),
            None,
        ]
    );
    field.key(&press(KeyCode::Esc, KeyModifiers::NONE), &mut grid);
    assert!(!field.focused());
    assert_eq!(field.text(), "aB");
    assert_eq!(requested(&mut grid), vec![Some(ab)]);
}

#[test]
fn alt_r_toggles_regex_only_when_the_rows_take_regexes() {
    let mut grid = grid(5, Some(TextSupport::new(false)));
    let mut field = QuickFilter::new();
    field.focus(&snapshot(&mut grid));
    field.key(&char('a'), &mut grid);
    field.key(&press(KeyCode::Char('r'), KeyModifiers::ALT), &mut grid);
    assert_eq!(requested(&mut grid), vec![Some(TextQuery::new("a"))]);
    assert_eq!(
        text(&line(&field, &snapshot(&mut grid), 20)),
        " Filter  a▏      Aa"
    );
}

#[test]
fn the_field_is_hidden_while_the_rows_have_no_quick_filter() {
    let mut grid = grid(5, None);
    let buf = line(&QuickFilter::new(), &snapshot(&mut grid), 30);
    assert_eq!(text(&buf), "");
}

#[test]
fn the_field_shows_its_placeholder_text_caret_and_lit_toggles() {
    let mut grid = grid(5, Some(TextSupport::new(true)));
    let mut field = QuickFilter::new().placeholder("Ctrl+L");
    let mut out = vec![text(&line(&field, &snapshot(&mut grid), 30))];
    field.focus(&snapshot(&mut grid));
    for c in "ber".chars() {
        field.key(&char(c), &mut grid);
    }
    out.push(text(&line(&field, &snapshot(&mut grid), 30)));
    field.key(&press(KeyCode::Char('r'), KeyModifiers::ALT), &mut grid);
    let buf = line(&field, &snapshot(&mut grid), 30);
    out.push(text(&buf));
    let lit = buf[(27, 0)].modifier;
    assert!(lit.contains(Modifier::REVERSED), ".* lit: {lit:?}");
    assert!(
        !buf[(24, 0)].modifier.contains(Modifier::REVERSED),
        "Aa off"
    );
    for c in "abcdefghijklmnopqrstuvwxyz".chars() {
        field.key(&char(c), &mut grid);
    }
    out.push(text(&line(&field, &snapshot(&mut grid), 30)));
    field.key(&press(KeyCode::Enter, KeyModifiers::NONE), &mut grid);
    out.push(text(&line(&field, &snapshot(&mut grid), 30)));
    insta::assert_snapshot!(out.join("\n"));
}

#[test]
fn unfocused_the_field_shows_a_quick_filter_set_elsewhere_and_focus_adopts_it() {
    let mut grid = grid(5, Some(TextSupport::new(true)));
    let mut field = QuickFilter::new();
    grid.set_quick_filter(Axis::Rows, Some(TextQuery::new("hal").case_sensitive(true)));
    requested(&mut grid);
    assert_eq!(
        text(&line(&field, &snapshot(&mut grid), 30)),
        " Filter  hal            Aa .*"
    );
    field.focus(&snapshot(&mut grid));
    field.key(&char('i'), &mut grid);
    assert_eq!(
        requested(&mut grid),
        vec![Some(TextQuery::new("hali").case_sensitive(true))]
    );
}

#[test]
fn the_status_line_counts_the_rows_shown_of_the_sources_total() {
    let mut grid = grid(1204, Some(TextSupport::new(true)));
    let mut counts = vec![row_count(&snapshot(&mut grid))];
    let spec = |text: &str| {
        ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_quick(Some(TextQuery::new(text))),
        )
    };
    let total = |n| Totals::unknown().with(Axis::Rows, Some(n));
    grid.view_applied(spec("a"), total(5001));
    counts.push(row_count(&snapshot(&mut grid)));
    grid.view_applied(ViewSpec::default(), total(1204));
    counts.push(row_count(&snapshot(&mut grid)));
    let mut one = self::grid(1, None);
    counts.push(row_count(&snapshot(&mut one)));
    insta::assert_snapshot!(counts.join("\n"));
}
