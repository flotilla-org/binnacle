use std::collections::HashMap;

use binnacle_core::{Action, Axis, FocusStep};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::Preset;

/// Maps crossterm key presses to Grid Actions. [`Keymap::default`] holds the
/// adapter's bindings; hosts rebind with [`Keymap::bind`] and
/// [`Keymap::unbind`], or start from [`Keymap::empty`].
///
/// Default bindings: arrows move focus one item, PageUp and PageDown a page of
/// rows, Alt+PageUp and Alt+PageDown a page of columns, Home and End to the
/// first and last row, Ctrl with an arrow to the edge that way. Shift with
/// any of these extends the selection. Ctrl+A selects all, Ctrl+Space selects
/// the whole column and Shift+Space the whole row, Esc cancels. Ctrl+C
/// copies (see [`crate::copy_text`]; a host quitting on it unbinds it). Enter
/// activates, F2 begins an edit, and any other character typed without Ctrl
/// or Alt is sent as `TypeText` (see [`Keymap::set_typing`]). In the outline
/// column (the row band of rows grouped in outline form) the arrows walk the
/// tree, Enter toggles the focused group, `*` and the digits (bound to
/// `TypeText` whatever the typing) open everything under it or the rows to
/// that depth, and Tab leaves for the body. A character
/// typed with Shift and bound only without it takes that binding, since its
/// case carries Shift.
#[derive(Clone, Debug)]
pub struct Keymap {
    bindings: HashMap<(KeyCode, KeyModifiers), Action>,
    typing: bool,
}

impl Keymap {
    pub fn empty() -> Self {
        Keymap {
            bindings: HashMap::new(),
            typing: false,
        }
    }

    /// The default bindings, with typing as `preset` has it, and what the
    /// preset adds: vim's `h j k l` moves for [`Preset::Modes`], and `V`
    /// (vim's line selection) to select the row for both
    /// [`Preset::Modes`] and [`Preset::BareKeys`], since few terminals
    /// report Shift with Space.
    pub fn preset(preset: Preset) -> Self {
        let mut keymap = Keymap::default();
        keymap.set_typing(preset.typing());
        if matches!(preset, Preset::Modes | Preset::BareKeys) {
            keymap.bind(KeyCode::Char('V'), KeyModifiers::NONE, Action::SelectRow);
        }
        if preset == Preset::Modes {
            let moves = [
                ('j', Axis::Rows, true),
                ('k', Axis::Rows, false),
                ('l', Axis::Cols, true),
                ('h', Axis::Cols, false),
            ];
            for (c, axis, forward) in moves {
                let action = Action::MoveFocus {
                    axis,
                    by: FocusStep::Item,
                    forward,
                    extend: false,
                };
                keymap.bind(KeyCode::Char(c), KeyModifiers::NONE, action);
            }
        }
        keymap
    }

    /// Whether characters with no binding, typed alone or with Shift, map to
    /// `TypeText`. On in [`Keymap::default`], off in [`Keymap::empty`].
    pub fn set_typing(&mut self, typing: bool) -> &mut Self {
        self.typing = typing;
        self
    }

    /// Binds `code` pressed with exactly `modifiers` to `action`, replacing any
    /// earlier binding.
    pub fn bind(&mut self, code: KeyCode, modifiers: KeyModifiers, action: Action) -> &mut Self {
        self.bindings.insert((code, modifiers), action);
        self
    }

    /// Removes the binding for `code` with `modifiers`, returning its Action.
    pub fn unbind(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<Action> {
        self.bindings.remove(&(code, modifiers))
    }

    /// The keys bound to `action`, in no particular order.
    pub fn keys(&self, action: &Action) -> Vec<(KeyCode, KeyModifiers)> {
        (self.bindings.iter())
            .filter(|(_, a)| *a == action)
            .map(|(&key, _)| key)
            .collect()
    }

    /// The Action for a key press or repeat, if bound. Releases map to nothing.
    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        if let Some(action) = self.bindings.get(&(event.code, event.modifiers)) {
            return Some(action.clone());
        }
        if let KeyCode::Char(_) = event.code {
            let unshifted = event.modifiers - KeyModifiers::SHIFT;
            if let Some(action) = self.bindings.get(&(event.code, unshifted)) {
                return Some(action.clone());
            }
        }
        match event.code {
            KeyCode::Char(c) if self.typing && (KeyModifiers::SHIFT).contains(event.modifiers) => {
                Some(Action::TypeText(c.to_string()))
            }
            _ => None,
        }
    }
}

impl Default for Keymap {
    fn default() -> Self {
        use FocusStep::{Edge, Item, Page};
        use KeyCode::{Down, End, Home, Left, PageDown, PageUp, Right, Up};
        let moves = [
            (Down, KeyModifiers::NONE, Axis::Rows, Item, true),
            (Up, KeyModifiers::NONE, Axis::Rows, Item, false),
            (Right, KeyModifiers::NONE, Axis::Cols, Item, true),
            (Left, KeyModifiers::NONE, Axis::Cols, Item, false),
            (PageDown, KeyModifiers::NONE, Axis::Rows, Page, true),
            (PageUp, KeyModifiers::NONE, Axis::Rows, Page, false),
            (PageDown, KeyModifiers::ALT, Axis::Cols, Page, true),
            (PageUp, KeyModifiers::ALT, Axis::Cols, Page, false),
            (End, KeyModifiers::NONE, Axis::Rows, Edge, true),
            (Home, KeyModifiers::NONE, Axis::Rows, Edge, false),
            (Down, KeyModifiers::CONTROL, Axis::Rows, Edge, true),
            (Up, KeyModifiers::CONTROL, Axis::Rows, Edge, false),
            (Right, KeyModifiers::CONTROL, Axis::Cols, Edge, true),
            (Left, KeyModifiers::CONTROL, Axis::Cols, Edge, false),
        ];
        let mut keymap = Keymap::empty();
        for (code, modifiers, axis, by, forward) in moves {
            for extend in [false, true] {
                let modifiers = if extend {
                    modifiers | KeyModifiers::SHIFT
                } else {
                    modifiers
                };
                let action = Action::MoveFocus {
                    axis,
                    by,
                    forward,
                    extend,
                };
                keymap.bind(code, modifiers, action);
            }
        }
        keymap.bind(KeyCode::Char('a'), KeyModifiers::CONTROL, Action::SelectAll);
        keymap.bind(KeyCode::Char('c'), KeyModifiers::CONTROL, Action::Copy);
        keymap.bind(
            KeyCode::Char(' '),
            KeyModifiers::CONTROL,
            Action::SelectColumn,
        );
        keymap.bind(KeyCode::Char(' '), KeyModifiers::SHIFT, Action::SelectRow);
        keymap.bind(KeyCode::Esc, KeyModifiers::NONE, Action::Cancel);
        keymap.bind(KeyCode::Enter, KeyModifiers::NONE, Action::Activate);
        keymap.bind(KeyCode::F(2), KeyModifiers::NONE, Action::BeginEdit);
        keymap.bind(KeyCode::Tab, KeyModifiers::NONE, Action::LeaveOutline);
        for c in "*0123456789".chars() {
            keymap.bind(
                KeyCode::Char(c),
                KeyModifiers::NONE,
                Action::TypeText(c.into()),
            );
        }
        keymap.set_typing(true);
        keymap
    }
}
