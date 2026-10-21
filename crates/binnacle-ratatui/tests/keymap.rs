//! The adapter's default keymap from crossterm keys to Actions, and rebinding.

use binnacle_core::{Action, Axis, FocusStep};
use binnacle_ratatui::{Keymap, Preset};
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
        (
            KeyCode::PageDown,
            KeyModifiers::ALT,
            focus(Axis::Cols, FocusStep::Page, true, false),
        ),
        (
            KeyCode::PageUp,
            KeyModifiers::ALT,
            focus(Axis::Cols, FocusStep::Page, false, false),
        ),
        (
            KeyCode::PageUp,
            KeyModifiers::ALT | KeyModifiers::SHIFT,
            focus(Axis::Cols, FocusStep::Page, false, true),
        ),
        (KeyCode::Char('x'), KeyModifiers::ALT, None),
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
fn the_default_keymap_selects_all_selects_columns_and_rows_and_cancels() {
    let keys = Keymap::default();
    let cases = [
        (KeyCode::Char('a'), KeyModifiers::CONTROL, Action::SelectAll),
        (
            KeyCode::Char(' '),
            KeyModifiers::CONTROL,
            Action::SelectColumn,
        ),
        (KeyCode::Char(' '), KeyModifiers::SHIFT, Action::SelectRow),
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

#[test]
fn enter_activates_f2_begins_an_edit_and_unbound_characters_type() {
    let keys = Keymap::default();
    let none = KeyModifiers::NONE;
    assert_eq!(
        keys.action(&press(KeyCode::Enter, none)),
        Some(Action::Activate)
    );
    assert_eq!(
        keys.action(&press(KeyCode::F(2), none)),
        Some(Action::BeginEdit)
    );
    assert_eq!(
        keys.action(&press(KeyCode::Char('q'), none)),
        Some(Action::TypeText("q".into()))
    );
    assert_eq!(
        keys.action(&press(KeyCode::Char('Q'), KeyModifiers::SHIFT)),
        Some(Action::TypeText("Q".into()))
    );
    // Control characters and bound characters do not type.
    assert_eq!(
        keys.action(&press(KeyCode::Char('b'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        keys.action(&press(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        Some(Action::SelectAll)
    );
    let mut rebound = Keymap::default();
    rebound.bind(KeyCode::Char('m'), none, Action::SelectAll);
    assert_eq!(
        rebound.action(&press(KeyCode::Char('m'), none)),
        Some(Action::SelectAll)
    );
    assert_eq!(
        Keymap::empty().action(&press(KeyCode::Char('m'), none)),
        None,
        "typing is part of the default keymap"
    );
}

const PRESETS: [Preset; 3] = [Preset::Chords, Preset::Modes, Preset::BareKeys];

/// The Actions every preset's keymap reaches, beside moving by item.
fn parity_actions() -> Vec<Action> {
    let page = |forward| focus(Axis::Cols, FocusStep::Page, forward, false).unwrap();
    vec![
        Action::SelectRow,
        Action::SelectColumn,
        Action::SelectAll,
        page(true),
        page(false),
    ]
}

#[test]
fn each_preset_keymap_selects_rows_and_columns_and_pages_the_columns() {
    for preset in PRESETS {
        let keys = Keymap::preset(preset);
        for action in parity_actions() {
            assert!(
                !keys.keys(&action).is_empty(),
                "{preset:?} reaches {action:?}"
            );
            for (code, modifiers) in keys.keys(&action) {
                let pressed = keys.action(&press(code, modifiers));
                assert_eq!(pressed, Some(action.clone()), "{preset:?} {code:?}");
            }
        }
    }
}

#[test]
fn home_and_end_stay_the_first_and_last_row_in_every_preset() {
    for preset in PRESETS {
        let keys = Keymap::preset(preset);
        let none = KeyModifiers::NONE;
        let home = keys.action(&press(KeyCode::Home, none));
        let end = keys.action(&press(KeyCode::End, none));
        assert_eq!(home, focus(Axis::Rows, FocusStep::Edge, false, false));
        assert_eq!(end, focus(Axis::Rows, FocusStep::Edge, true, false));
    }
}

#[test]
fn the_modes_and_bare_keys_presets_select_the_row_with_a_bare_key_too() {
    // Few terminals report Shift with Space, so a key without modifiers
    // reaches it where typing is off.
    for preset in [Preset::Modes, Preset::BareKeys] {
        let keys = Keymap::preset(preset);
        let row = press(KeyCode::Char('V'), KeyModifiers::SHIFT);
        assert_eq!(keys.action(&row), Some(Action::SelectRow), "{preset:?}");
        let typed = press(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(keys.action(&typed), None, "{preset:?} types");
    }
    let modes = Keymap::preset(Preset::Modes);
    let l = press(KeyCode::Char('l'), KeyModifiers::NONE);
    assert_eq!(
        modes.action(&l),
        focus(Axis::Cols, FocusStep::Item, true, false)
    );
    let chords = Keymap::preset(Preset::Chords);
    let typed = chords.action(&press(KeyCode::Char('q'), KeyModifiers::NONE));
    assert_eq!(typed, Some(Action::TypeText("q".into())));
}
