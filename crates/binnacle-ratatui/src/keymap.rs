use std::collections::HashMap;

use binnacle_core::{Action, Axis, FocusStep};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Maps crossterm key presses to Grid Actions. [`Keymap::default`] holds the
/// adapter's bindings; hosts rebind with [`Keymap::bind`] and
/// [`Keymap::unbind`], or start from [`Keymap::empty`].
///
/// Default bindings: arrows move focus one item, PageUp and PageDown a page of
/// rows, Home and End to the first and last row, Ctrl with an arrow to the
/// edge that way. Shift with any of these extends the selection. Ctrl+A
/// selects all, Ctrl+Space selects the whole column, Esc cancels. Enter
/// activates, F2 begins an edit, and any other character typed without Ctrl
/// or Alt is sent as `TypeText` (see [`Keymap::set_typing`]).
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

    /// The Action for a key press or repeat, if bound. Releases map to nothing.
    pub fn action(&self, event: &KeyEvent) -> Option<Action> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        if let Some(action) = self.bindings.get(&(event.code, event.modifiers)) {
            return Some(action.clone());
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
        keymap.bind(
            KeyCode::Char(' '),
            KeyModifiers::CONTROL,
            Action::SelectColumn,
        );
        keymap.bind(KeyCode::Esc, KeyModifiers::NONE, Action::Cancel);
        keymap.bind(KeyCode::Enter, KeyModifiers::NONE, Action::Activate);
        keymap.bind(KeyCode::F(2), KeyModifiers::NONE, Action::BeginEdit);
        keymap.set_typing(true);
        keymap
    }
}
