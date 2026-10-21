//! The keymap presets for find and filter: chords, modes and bare keys, each
//! reaching every Control, none only through a function key, and a host's
//! own bindings over them.

use binnacle_core::{Axis, AxisConfig, Edge, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Control, Controls, Preset, freeze_focused, toggle_pin, unpin_focused};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

const PRESETS: [Preset; 3] = [Preset::Chords, Preset::Modes, Preset::BareKeys];

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

fn char(c: char) -> KeyEvent {
    press(KeyCode::Char(c), KeyModifiers::NONE)
}

#[test]
fn each_preset_reaches_every_control() {
    for preset in PRESETS {
        let controls = Controls::preset(preset);
        for control in Control::ALL {
            assert!(
                !controls.keys(control).is_empty(),
                "{preset:?} reaches {control:?}"
            );
        }
    }
}

#[test]
fn no_control_is_reachable_only_through_a_function_key() {
    for preset in PRESETS {
        let controls = Controls::preset(preset);
        for control in Control::ALL {
            let keys = controls.keys(control);
            assert!(
                keys.iter().any(|(code, _)| !matches!(code, KeyCode::F(_))),
                "{preset:?} reaches {control:?} only by {keys:?}"
            );
        }
    }
}

#[test]
fn every_binding_reaches_its_control_from_a_key_press() {
    for preset in PRESETS {
        let controls = Controls::preset(preset);
        for control in Control::ALL {
            for (code, modifiers) in controls.keys(control) {
                let key = press(code, modifiers);
                assert_eq!(controls.control(&key), Some(control), "{preset:?} {key:?}");
            }
        }
    }
}

#[test]
fn the_chords_preset_binds_ctrl_and_alt_chords_with_a_second_key_for_f3() {
    let controls = Controls::preset(Preset::Chords);
    let ctrl = KeyModifiers::CONTROL;
    let alt = KeyModifiers::ALT;
    let cases = [
        (press(KeyCode::Char('f'), ctrl), Control::Find),
        (press(KeyCode::F(3), KeyModifiers::NONE), Control::FindNext),
        (press(KeyCode::Char('n'), ctrl), Control::FindNext),
        (
            press(KeyCode::F(3), KeyModifiers::SHIFT),
            Control::FindPrevious,
        ),
        (press(KeyCode::Char('p'), ctrl), Control::FindPrevious),
        (press(KeyCode::Char('f'), alt), Control::OnlyMatching),
        (press(KeyCode::Char('l'), ctrl), Control::QuickFilter),
        (press(KeyCode::Down, alt), Control::ColumnFilter),
        (press(KeyCode::Right, alt), Control::RowFilter),
        (press(KeyCode::Char('k'), ctrl), Control::Clear),
        (press(KeyCode::Char('l'), alt), Control::Reapply),
        (press(KeyCode::Char('l'), ctrl | alt), Control::Reapply),
        (press(KeyCode::Esc, KeyModifiers::NONE), Control::Close),
    ];
    for (key, control) in cases {
        assert_eq!(controls.control(&key), Some(control), "{key:?}");
    }
    assert_eq!(controls.control(&char('n')), None, "bare keys type");
    assert!(Preset::Chords.typing());
}

#[test]
fn the_modes_preset_binds_vim_and_visidata_keys() {
    let controls = Controls::preset(Preset::Modes);
    let cases = [
        ('/', Control::Find),
        ('?', Control::FindBackward),
        ('n', Control::FindNext),
        ('N', Control::FindPrevious),
        ('|', Control::QuickFilter),
        ('\\', Control::ColumnText { not: false }),
        ('&', Control::ColumnText { not: true }),
        ('F', Control::ColumnFilter),
        ('R', Control::RowFilter),
        ('=', Control::KeepValue),
        ('u', Control::Remove),
        ('U', Control::Clear),
        ('i', Control::Case),
    ];
    for (c, control) in cases {
        assert_eq!(controls.control(&char(c)), Some(control), "{c}");
    }
    assert!(!Preset::Modes.typing());
}

#[test]
fn the_bare_keys_preset_binds_single_keys() {
    let controls = Controls::preset(Preset::BareKeys);
    let cases = [
        ('/', Control::Find),
        ('?', Control::FindBackward),
        ('n', Control::FindNext),
        ('N', Control::FindPrevious),
        ('\\', Control::QuickFilter),
        ('f', Control::ColumnFilter),
        ('F', Control::RowFilter),
        ('x', Control::Remove),
        ('X', Control::Clear),
        ('m', Control::OnlyMatching),
    ];
    for (c, control) in cases {
        assert_eq!(controls.control(&char(c)), Some(control), "{c}");
    }
    assert!(!Preset::BareKeys.typing());
}

#[test]
fn shift_is_carried_by_a_characters_case() {
    let controls = Controls::preset(Preset::Modes);
    let shifted = press(KeyCode::Char('N'), KeyModifiers::SHIFT);
    assert_eq!(controls.control(&shifted), Some(Control::FindPrevious));
    let question = press(KeyCode::Char('?'), KeyModifiers::SHIFT);
    assert_eq!(controls.control(&question), Some(Control::FindBackward));
    let chords = Controls::preset(Preset::Chords);
    let back = press(KeyCode::F(3), KeyModifiers::SHIFT);
    assert_eq!(chords.control(&back), Some(Control::FindPrevious));
    assert_eq!(
        chords.control(&press(KeyCode::F(3), KeyModifiers::NONE)),
        Some(Control::FindNext),
        "Shift still counts on keys that are not characters"
    );
}

#[test]
fn a_host_overrides_single_keys_over_a_preset() {
    let mut controls = Controls::preset(Preset::BareKeys);
    assert_eq!(
        controls.unbind(KeyCode::Char('m'), KeyModifiers::NONE),
        Some(Control::OnlyMatching)
    );
    controls.bind(
        KeyCode::Char('o'),
        KeyModifiers::NONE,
        Control::OnlyMatching,
    );
    controls.bind(KeyCode::Char('f'), KeyModifiers::NONE, Control::QuickFilter);
    assert_eq!(controls.control(&char('m')), None);
    assert_eq!(controls.control(&char('o')), Some(Control::OnlyMatching));
    assert_eq!(controls.control(&char('f')), Some(Control::QuickFilter));
    assert_eq!(controls.keys(Control::ColumnFilter), vec![]);
    assert_eq!(
        controls.keys(Control::QuickFilter),
        vec![
            (KeyCode::Char('\\'), KeyModifiers::NONE),
            (KeyCode::Char('f'), KeyModifiers::NONE)
        ],
        "in the order bound"
    );
    assert_eq!(controls.hint(Control::QuickFilter).as_deref(), Some("\\"));
    let chords = Controls::preset(Preset::Chords);
    assert_eq!(chords.hint(Control::Find).as_deref(), Some("Ctrl+F"));
    assert_eq!(
        chords.hint(Control::ColumnFilter).as_deref(),
        Some("Alt+Down")
    );
    assert_eq!(
        chords.hint(Control::FindPrevious).as_deref(),
        Some("Shift+F3")
    );
}

const PIN_LEFT: Control = Control::Pin {
    axis: Axis::Cols,
    edge: Edge::Leading,
};
const UNPIN: Control = Control::Unpin { axis: Axis::Cols };

#[test]
fn each_preset_pins_the_focused_column_left_and_unpins_it() {
    let alt = KeyModifiers::ALT;
    let cases = [
        (
            Preset::Chords,
            press(KeyCode::Char(','), alt),
            press(KeyCode::Char('u'), alt),
        ),
        (Preset::Modes, char('!'), press(KeyCode::Char('u'), alt)),
        (Preset::BareKeys, char('<'), char('u')),
    ];
    for (preset, pin, unpin) in cases {
        let controls = Controls::preset(preset);
        assert_eq!(controls.control(&pin), Some(PIN_LEFT), "{preset:?}");
        assert_eq!(controls.control(&unpin), Some(UNPIN), "{preset:?}");
    }
}

/// The Keys of the columns pinned left.
fn pinned(grid: &mut Grid) -> Vec<u64> {
    for (axis, range) in grid.begin_frame(Viewport::new(40.0, 10.0)) {
        let keys = IdentitySource::new(range.end).window(range.clone());
        grid.set_window(axis, range.start, &keys);
    }
    let snapshot = grid.snapshot();
    (snapshot.pinned(Axis::Cols, Edge::Leading).iter())
        .filter_map(|i| i.key.map(|k| k.0))
        .collect()
}

/// Pin toggles the focused column; Unpin unpins it and leaves an unpinned
/// one alone.
#[test]
fn pin_toggles_the_focused_column_and_unpin_unpins_it() {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(8, 6.0).header(3.0).gap(1.0),
    );
    pinned(&mut grid);
    grid.set_focus(Key(0), Key(4));
    toggle_pin(&mut grid, Axis::Cols, Edge::Leading);
    assert_eq!(pinned(&mut grid), [4]);
    grid.set_focus(Key(0), Key(2));
    toggle_pin(&mut grid, Axis::Cols, Edge::Leading);
    assert_eq!(pinned(&mut grid), [4, 2]);
    toggle_pin(&mut grid, Axis::Cols, Edge::Leading);
    assert_eq!(pinned(&mut grid), [4]);
    unpin_focused(&mut grid, Axis::Cols);
    assert_eq!(pinned(&mut grid), [4]);
    grid.set_focus(Key(0), Key(4));
    unpin_focused(&mut grid, Axis::Cols);
    assert_eq!(pinned(&mut grid), Vec::<u64>::new());
}

/// Each preset pins the focused column right and its row top or bottom,
/// unpins the row, and freezes either axis up to focus.
#[test]
fn each_preset_reaches_both_edges_of_both_axes_and_freeze() {
    use Control::{Freeze, Pin, Unpin};
    let (alt, none) = (KeyModifiers::ALT, KeyModifiers::NONE);
    let key = |c, m| press(KeyCode::Char(c), m);
    let pin = |axis, edge| Pin { axis, edge };
    let cases = [
        (
            Preset::Chords,
            [
                key('.', alt),
                key('<', alt),
                key('>', alt),
                key('U', alt),
                key('w', alt),
                key('W', alt),
            ],
        ),
        (
            Preset::Modes,
            [
                key('>', none),
                key('[', none),
                key(']', none),
                key('U', alt),
                key('z', none),
                key('Z', none),
            ],
        ),
        (
            Preset::BareKeys,
            [
                key('>', none),
                key('[', none),
                key(']', none),
                key('U', none),
                key('z', none),
                key('Z', none),
            ],
        ),
    ];
    let controls = [
        pin(Axis::Cols, Edge::Trailing),
        pin(Axis::Rows, Edge::Leading),
        pin(Axis::Rows, Edge::Trailing),
        Unpin { axis: Axis::Rows },
        Freeze { axis: Axis::Cols },
        Freeze { axis: Axis::Rows },
    ];
    for (preset, keys) in cases {
        let bound = Controls::preset(preset);
        for (key, control) in keys.iter().zip(controls) {
            assert_eq!(bound.control(key), Some(control), "{preset:?} {key:?}");
        }
    }
}

/// The Keys pinned to `edge` of `axis`.
fn pinned_at(grid: &mut Grid, axis: Axis, edge: Edge) -> Vec<u64> {
    pinned(grid);
    let snapshot = grid.snapshot();
    (snapshot.pinned(axis, edge).iter())
        .filter_map(|i| i.key.map(|k| k.0))
        .collect()
}

/// Pin toggles the focused row at either edge, pinning to one edge moves it
/// from the other, and Freeze pins the columns from the first to the
/// focused one, as many as fit with room for one more to scroll.
#[test]
fn pin_moves_between_edges_and_freeze_pins_from_the_start() {
    let mut grid = Grid::new(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(8, 6.0).header(3.0).gap(1.0),
    );
    pinned(&mut grid);
    grid.set_focus(Key(3), Key(2));
    toggle_pin(&mut grid, Axis::Rows, Edge::Trailing);
    assert_eq!(pinned_at(&mut grid, Axis::Rows, Edge::Trailing), [3]);
    toggle_pin(&mut grid, Axis::Rows, Edge::Leading);
    assert_eq!(
        pinned_at(&mut grid, Axis::Rows, Edge::Trailing),
        Vec::<u64>::new()
    );
    assert_eq!(pinned_at(&mut grid, Axis::Rows, Edge::Leading), [3]);
    unpin_focused(&mut grid, Axis::Rows);
    assert_eq!(
        pinned_at(&mut grid, Axis::Rows, Edge::Leading),
        Vec::<u64>::new()
    );
    freeze_focused(&mut grid, Axis::Cols);
    assert_eq!(pinned_at(&mut grid, Axis::Cols, Edge::Leading), [0, 1, 2]);
    // 40 wide: after the 3-wide band, four columns and their Gaps reach 31,
    // leaving 9, room for one more to scroll; a fifth would leave 2.
    grid.set_focus(Key(3), Key(6));
    freeze_focused(&mut grid, Axis::Cols);
    assert_eq!(
        pinned_at(&mut grid, Axis::Cols, Edge::Leading),
        [0, 1, 2, 3]
    );
}
