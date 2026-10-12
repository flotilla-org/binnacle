//! The adapter's default keymap from crossterm keys to Actions, and rebinding.

use binnacle_core::{Action, Axis, FocusStep};
use binnacle_ratatui::Keymap;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn focus(axis: Axis, by: FocusStep, forward: bool, extend: bool) -> Option<Action> {
    Some(Action::MoveFocus {
        axis,
        by,
        forward,
        extend,
    })
}

#[test]
fn the_default_keymap_moves_focus_with_arrows_pages_and_edges() {
    let keys = Keymap::default();
    let none = KeyModifiers::NONE;
    let cases = [
        (
            KeyCode::Down,
            none,
            focus(Axis::Rows, FocusStep::Item, true, false),
        ),
        (
            KeyCode::Up,
            none,
            focus(Axis::Rows, FocusStep::Item, false, false),
        ),
        (
            KeyCode::Right,
            none,
            focus(Axis::Cols, FocusStep::Item, true, false),
        ),
        (
            KeyCode::Left,
            none,
            focus(Axis::Cols, FocusStep::Item, false, false),
        ),
        (
            KeyCode::PageDown,
            none,
            focus(Axis::Rows, FocusStep::Page, true, false),
        ),
        (
            KeyCode::PageUp,
            none,
            focus(Axis::Rows, FocusStep::Page, false, false),
        ),
        (
            KeyCode::Home,
            none,
            focus(Axis::Rows, FocusStep::Edge, false, false),
        ),
        (
            KeyCode::End,
            none,
            focus(Axis::Rows, FocusStep::Edge, true, false),
        ),
        (
            KeyCode::Right,
            KeyModifiers::CONTROL,
            focus(Axis::Cols, FocusStep::Edge, true, false),
        ),
        (
            KeyCode::Down,
            KeyModifiers::SHIFT,
            focus(Axis::Rows, FocusStep::Item, true, true),
        ),
        (
            KeyCode::Left,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            focus(Axis::Cols, FocusStep::Edge, false, true),
        ),
        (KeyCode::Char('x'), none, None),
    ];
    for (code, modifiers, expected) in cases {
        assert_eq!(
            keys.action(&press(code, modifiers)),
            expected,
            "{modifiers:?} {code:?}"
        );
    }
}

#[test]
fn key_releases_map_to_nothing() {
    let mut release = press(KeyCode::Down, KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert_eq!(Keymap::default().action(&release), None);
}

#[test]
fn hosts_rebind_and_unbind_keys() {
    let mut keys = Keymap::default();
    let down = focus(Axis::Rows, FocusStep::Item, true, false).unwrap();
    keys.bind(KeyCode::Char('j'), KeyModifiers::NONE, down.clone());
    assert_eq!(
        keys.unbind(KeyCode::Down, KeyModifiers::NONE),
        Some(down.clone())
    );
    assert_eq!(
        keys.action(&press(KeyCode::Char('j'), KeyModifiers::NONE)),
        Some(down)
    );
    assert_eq!(keys.action(&press(KeyCode::Down, KeyModifiers::NONE)), None);
    assert_eq!(
        Keymap::empty().action(&press(KeyCode::Up, KeyModifiers::NONE)),
        None
    );
}

#[test]
fn the_default_keymap_selects_all_selects_columns_and_cancels() {
    let keys = Keymap::default();
    let cases = [
        (KeyCode::Char('a'), KeyModifiers::CONTROL, Action::SelectAll),
        (
            KeyCode::Char(' '),
            KeyModifiers::CONTROL,
            Action::SelectColumn,
        ),
        (KeyCode::Esc, KeyModifiers::NONE, Action::Cancel),
    ];
    for (code, modifiers, expected) in cases {
        assert_eq!(
            keys.action(&press(code, modifiers)),
            Some(expected),
            "{code:?}"
        );
    }
}
