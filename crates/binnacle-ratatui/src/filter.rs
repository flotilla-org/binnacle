//! Find, filter and hiding controls: the keys that reach them, by preset,
//! the quick filter field, the column filter popup and the conditions line.

use std::time::{Duration, Instant};

use binnacle_core::{
    Axis, Blanks, Compared, Condition, ConditionKind, ConditionState, Distinct, DistinctQuery,
    Edge, FilterSpec, Grid, Key, Level, Placement, Snapshot, TextQuery, TextSupport, Value,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::checklist::{Column, Shown};
use crate::compare::{self, BETWEEN, STEPS};
use crate::renderer::{char_count, fit};
use crate::{Content, Theme};

/// A find, filter, pinning or hiding control a key reaches, outside the
/// Grid's Actions. Every [`Preset`] reaches each of [`Control::ALL`]; the
/// host carries it out with the method each names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// Focus the quick filter field ([`QuickFilter::focus`]).
    QuickFilter,
    /// Open the focused column's filter popup or drawer, or focus a drawer
    /// already shown ([`ColumnFilter::open`]).
    ColumnFilter,
    /// Open the focused row's filter popup or drawer, filtering the columns
    /// by its values ([`ColumnFilter::open_row`]).
    RowFilter,
    /// Open the focused column's filter at its text, set to contain or (with
    /// `not`) not contain it ([`ColumnFilter::open_text`]).
    ColumnText { not: bool },
    /// Keep only the rows whose value in the focused column is the focused
    /// cell's ([`keep_value`], with the value the host reads).
    KeepValue,
    /// Remove the focused column's conditions or, with none there, the last
    /// condition ([`remove_conditions`]).
    Remove,
    /// Clear the quick filter and every condition ([`clear_filters`]).
    Clear,
    /// Open the find bar, or focus it again ([`crate::FindBar::open`]).
    Find,
    /// Open the find bar to step backwards: Enter steps to the previous
    /// match and [`Control::FindNext`] goes backwards too
    /// ([`crate::FindBar::open_backward`]).
    FindBackward,
    /// Step on to the next match while find is open, the way find was opened
    /// ([`crate::FindBar::repeat`]).
    FindNext,
    /// Step back the other way ([`crate::FindBar::repeat`] reversed).
    FindPrevious,
    /// Show only the rows that match the find, or show them all again
    /// ([`crate::FindBar::only_matching`]).
    OnlyMatching,
    /// Toggle whether find matches case ([`crate::FindBar::toggle_case`]).
    Case,
    /// Apply the View Spec again ([`binnacle_core::Grid::reapply_view`]), so
    /// rows the user's edits left unmatched are filtered.
    Reapply,
    /// Close an open filter drawer, else find; with neither open, the key
    /// goes on to the [`crate::Keymap`] ([`ColumnFilter::close`],
    /// [`crate::FindBar::close`]).
    Close,
    /// Pin the focused item of `axis` to `edge`, or unpin it if it is pinned
    /// there already ([`crate::toggle_pin`]).
    Pin { axis: Axis, edge: Edge },
    /// Unpin the focused item of `axis` ([`crate::unpin_focused`]).
    Unpin { axis: Axis },
    /// Hide the chosen items of `axis`: the picks, else the selection, else
    /// the focused item ([`crate::hide_chosen`]).
    Hide { axis: Axis },
    /// Open the focused item's header menu at the hidden items of `axis`
    /// ([`crate::HeaderMenu::open_hidden`]).
    ShowHidden { axis: Axis },
    /// Open the focused item's header menu, on the column or the row
    /// ([`crate::HeaderMenu::open`]).
    Menu { axis: Axis },
    /// Pin every item of `axis` from the start up to the focused one, as
    /// many as fit ([`crate::freeze_focused`]).
    Freeze { axis: Axis },
    /// Open the chooser of `axis`, every item with a tick for shown
    /// ([`crate::Chooser::open`]).
    Chooser { axis: Axis },
}

impl Control {
    /// Every Control, each of which every preset reaches.
    pub const ALL: [Control; 32] = [
        Control::QuickFilter,
        Control::ColumnFilter,
        Control::RowFilter,
        Control::ColumnText { not: false },
        Control::ColumnText { not: true },
        Control::KeepValue,
        Control::Remove,
        Control::Clear,
        Control::Find,
        Control::FindBackward,
        Control::FindNext,
        Control::FindPrevious,
        Control::OnlyMatching,
        Control::Case,
        Control::Reapply,
        Control::Close,
        Control::Pin {
            axis: Axis::Cols,
            edge: Edge::Leading,
        },
        Control::Pin {
            axis: Axis::Cols,
            edge: Edge::Trailing,
        },
        Control::Pin {
            axis: Axis::Rows,
            edge: Edge::Leading,
        },
        Control::Pin {
            axis: Axis::Rows,
            edge: Edge::Trailing,
        },
        Control::Unpin { axis: Axis::Cols },
        Control::Hide { axis: Axis::Cols },
        Control::Hide { axis: Axis::Rows },
        Control::ShowHidden { axis: Axis::Cols },
        Control::ShowHidden { axis: Axis::Rows },
        Control::Menu { axis: Axis::Cols },
        Control::Menu { axis: Axis::Rows },
        Control::Unpin { axis: Axis::Rows },
        Control::Freeze { axis: Axis::Cols },
        Control::Freeze { axis: Axis::Rows },
        Control::Chooser { axis: Axis::Cols },
        Control::Chooser { axis: Axis::Rows },
    ];
}

/// A set of bindings for the find and filter controls, to suit how a
/// terminal user already works. Each reaches every [`Control`], and none only
/// through a function key, since macOS sends media keys for the top row by
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Ctrl and Alt chords, as in a desktop application, with typing left to
    /// the [`crate::Keymap`]: Ctrl+F find, Alt+Shift+F find backwards, F3 or
    /// Ctrl+N next, Shift+F3 or Ctrl+P previous, Alt+F show only matching
    /// rows, Alt+C case, Ctrl+L the quick filter, Alt+Down the focused
    /// column's filter popup and Alt+Right the focused row's, Alt+T and Alt+N
    /// the column's text set to contain or not, Alt+= keep the focused value, Ctrl+D remove, Ctrl+K clear, Alt+L (or
    /// Excel's Ctrl+Alt+L) apply the filter again, Esc close, Alt+, and Alt+.
    /// pin the focused column left and right, Alt+< and Alt+> its row to the
    /// top and bottom, Alt+U and Alt+Shift+U unpin the column and the row,
    /// and Alt+W and Alt+Shift+W freeze the columns and rows up to focus.
    /// Alt+0 and Alt+9 hide the focused column and row (Excel's Ctrl+0 and
    /// Ctrl+9, as terminals drop Ctrl with a digit), Alt+) and Alt+( list the
    /// hidden ones, Alt+M (or Shift+F10) and Alt+Shift+M open the column's
    /// and the row's menu, and Alt+O and Alt+Shift+C (or Alt+Shift+O, where
    /// the terminal passes it) their choosers.
    Chords,
    /// vim and VisiData: `/` and `?` find forwards and backwards, `n` and `N`
    /// step, `|` the quick filter, `\` and `&` the focused column's text set
    /// to contain or not, `F` its filter popup, `R` the focused row's, `=`
    /// keep the focused value, `u` and `U` remove and clear, `i` case, `m` show only matching rows,
    /// `r` apply the filter again, Esc close, `!` pin the focused column
    /// left or unpin it (VisiData's key column) and `>` right, `[` and `]`
    /// its row to the top and bottom, Alt+U and Alt+Shift+U unpin the column
    /// and the row, `z` and `Z` freeze the columns and rows up to focus, `-`
    /// and `_` hide the column and the row (VisiData's `-`), `+` and `*` list
    /// the hidden ones, `.` and `,` the menus, `C` (VisiData's column sheet)
    /// and Alt+C the choosers. Typing is off.
    Modes,
    /// Single keys with no modes: `/ ? n N` find, `\` the quick filter, `f`
    /// the filter drawer (see [`ColumnFilter::drawer`]), `F` the focused
    /// row's, `|` and `&` the column's text set to contain or not, `=` keep the focused value, `x` and `X` remove
    /// and clear, `m` show only matching rows, `i` case, `r` apply the filter
    /// again, Esc close, `<` and `>` pin the focused column left and right,
    /// `[` and `]` its row to the top and bottom, `u` and `U` unpin the
    /// column and the row, `z` and `Z` freeze the columns and rows up to
    /// focus, `-` and `_` hide the column and the row, `+` and `*` list the
    /// hidden ones, `.` and `,` the menus, `c` and `C` the choosers. Typing
    /// is off.
    BareKeys,
}

impl Preset {
    /// Whether the [`crate::Keymap`] behind the preset should turn
    /// characters into typing ([`crate::Keymap::set_typing`]): only chords
    /// leave the bare keys free for it.
    pub fn typing(self) -> bool {
        self == Preset::Chords
    }
}

/// Maps crossterm key presses to [`Control`]s, starting from a [`Preset`];
/// hosts rebind with [`Controls::bind`] and [`Controls::unbind`]. Check it
/// before the [`crate::Keymap`], so its bindings win. A character's case
/// carries Shift, so Shift is ignored on characters.
#[derive(Clone, Debug)]
pub struct Controls {
    /// In the order bound, so the first binding of a Control is its hint.
    bindings: Vec<((KeyCode, KeyModifiers), Control)>,
}

impl Controls {
    pub fn preset(preset: Preset) -> Self {
        use Control::*;
        use KeyCode::{Char, Down, Esc, F, Right};
        const NONE: KeyModifiers = KeyModifiers::NONE;
        const CTRL: KeyModifiers = KeyModifiers::CONTROL;
        const ALT: KeyModifiers = KeyModifiers::ALT;
        const SHIFT: KeyModifiers = KeyModifiers::SHIFT;
        const PIN_LEFT: Control = Pin {
            axis: Axis::Cols,
            edge: Edge::Leading,
        };
        const UNPIN: Control = Unpin { axis: Axis::Cols };
        const PIN_RIGHT: Control = Pin {
            axis: Axis::Cols,
            edge: Edge::Trailing,
        };
        const PIN_TOP: Control = Pin {
            axis: Axis::Rows,
            edge: Edge::Leading,
        };
        const PIN_BOTTOM: Control = Pin {
            axis: Axis::Rows,
            edge: Edge::Trailing,
        };
        const UNPIN_ROW: Control = Unpin { axis: Axis::Rows };
        const FREEZE: Control = Freeze { axis: Axis::Cols };
        const FREEZE_ROWS: Control = Freeze { axis: Axis::Rows };
        let mut controls = Controls {
            bindings: Vec::new(),
        };
        let bindings: &[(KeyCode, KeyModifiers, Control)] = match preset {
            Preset::Chords => &[
                (Char('l'), CTRL, QuickFilter),
                (Down, ALT, ColumnFilter),
                (Right, ALT, RowFilter),
                (Char('t'), ALT, ColumnText { not: false }),
                (Char('n'), ALT, ColumnText { not: true }),
                (Char('='), ALT, KeepValue),
                (Char('d'), CTRL, Remove),
                (Char('k'), CTRL, Clear),
                (Char('f'), CTRL, Find),
                (Char('F'), ALT, FindBackward),
                (F(3), NONE, FindNext),
                (Char('n'), CTRL, FindNext),
                (F(3), SHIFT, FindPrevious),
                (Char('p'), CTRL, FindPrevious),
                (Char('f'), ALT, OnlyMatching),
                (Char('c'), ALT, Case),
                (Char('l'), ALT, Reapply),
                (Char('l'), CTRL.union(ALT), Reapply),
                (Esc, NONE, Close),
                (Char(','), ALT, PIN_LEFT),
                (Char('.'), ALT, PIN_RIGHT),
                (Char('<'), ALT, PIN_TOP),
                (Char('>'), ALT, PIN_BOTTOM),
                (Char('u'), ALT, UNPIN),
                (Char('0'), ALT, Hide { axis: Axis::Cols }),
                (Char('9'), ALT, Hide { axis: Axis::Rows }),
                (Char(')'), ALT, ShowHidden { axis: Axis::Cols }),
                (Char('('), ALT, ShowHidden { axis: Axis::Rows }),
                (Char('m'), ALT, Menu { axis: Axis::Cols }),
                (F(10), SHIFT, Menu { axis: Axis::Cols }),
                (Char('M'), ALT, Menu { axis: Axis::Rows }),
                (Char('U'), ALT, UNPIN_ROW),
                (Char('w'), ALT, FREEZE),
                (Char('W'), ALT, FREEZE_ROWS),
                (Char('o'), ALT, Chooser { axis: Axis::Cols }),
                // Terminals send Alt+Shift+O as ESC O, which reads as SS3.
                (Char('C'), ALT, Chooser { axis: Axis::Rows }),
                (Char('O'), ALT, Chooser { axis: Axis::Rows }),
            ],
            Preset::Modes => &[
                (Char('|'), NONE, QuickFilter),
                (Char('F'), NONE, ColumnFilter),
                (Char('R'), NONE, RowFilter),
                (Char('\\'), NONE, ColumnText { not: false }),
                (Char('&'), NONE, ColumnText { not: true }),
                (Char('='), NONE, KeepValue),
                (Char('u'), NONE, Remove),
                (Char('U'), NONE, Clear),
                (Char('/'), NONE, Find),
                (Char('?'), NONE, FindBackward),
                (Char('n'), NONE, FindNext),
                (Char('N'), NONE, FindPrevious),
                (Char('m'), NONE, OnlyMatching),
                (Char('i'), NONE, Case),
                (Char('r'), NONE, Reapply),
                (Esc, NONE, Close),
                (Char('!'), NONE, PIN_LEFT),
                (Char('>'), NONE, PIN_RIGHT),
                (Char('['), NONE, PIN_TOP),
                (Char(']'), NONE, PIN_BOTTOM),
                (Char('u'), ALT, UNPIN),
                (Char('-'), NONE, Hide { axis: Axis::Cols }),
                (Char('_'), NONE, Hide { axis: Axis::Rows }),
                (Char('+'), NONE, ShowHidden { axis: Axis::Cols }),
                (Char('*'), NONE, ShowHidden { axis: Axis::Rows }),
                (Char('.'), NONE, Menu { axis: Axis::Cols }),
                (Char(','), NONE, Menu { axis: Axis::Rows }),
                (Char('U'), ALT, UNPIN_ROW),
                (Char('z'), NONE, FREEZE),
                (Char('Z'), NONE, FREEZE_ROWS),
                (Char('C'), NONE, Chooser { axis: Axis::Cols }),
                (Char('c'), ALT, Chooser { axis: Axis::Rows }),
            ],
            Preset::BareKeys => &[
                (Char('\\'), NONE, QuickFilter),
                (Char('f'), NONE, ColumnFilter),
                (Char('F'), NONE, RowFilter),
                (Char('|'), NONE, ColumnText { not: false }),
                (Char('&'), NONE, ColumnText { not: true }),
                (Char('='), NONE, KeepValue),
                (Char('x'), NONE, Remove),
                (Char('X'), NONE, Clear),
                (Char('/'), NONE, Find),
                (Char('?'), NONE, FindBackward),
                (Char('n'), NONE, FindNext),
                (Char('N'), NONE, FindPrevious),
                (Char('m'), NONE, OnlyMatching),
                (Char('i'), NONE, Case),
                (Char('r'), NONE, Reapply),
                (Esc, NONE, Close),
                (Char('<'), NONE, PIN_LEFT),
                (Char('>'), NONE, PIN_RIGHT),
                (Char('['), NONE, PIN_TOP),
                (Char(']'), NONE, PIN_BOTTOM),
                (Char('u'), NONE, UNPIN),
                (Char('-'), NONE, Hide { axis: Axis::Cols }),
                (Char('_'), NONE, Hide { axis: Axis::Rows }),
                (Char('+'), NONE, ShowHidden { axis: Axis::Cols }),
                (Char('*'), NONE, ShowHidden { axis: Axis::Rows }),
                (Char('.'), NONE, Menu { axis: Axis::Cols }),
                (Char(','), NONE, Menu { axis: Axis::Rows }),
                (Char('U'), NONE, UNPIN_ROW),
                (Char('z'), NONE, FREEZE),
                (Char('Z'), NONE, FREEZE_ROWS),
                (Char('c'), NONE, Chooser { axis: Axis::Cols }),
                (Char('C'), NONE, Chooser { axis: Axis::Rows }),
            ],
        };
        for &(code, modifiers, control) in bindings {
            controls.bind(code, modifiers, control);
        }
        controls
    }

    /// Binds `code` pressed with exactly `modifiers` to `control`, replacing
    /// any earlier binding.
    pub fn bind(&mut self, code: KeyCode, modifiers: KeyModifiers, control: Control) -> &mut Self {
        let key = chord(code, modifiers);
        match self.bindings.iter_mut().find(|(k, _)| *k == key) {
            Some(binding) => binding.1 = control,
            None => self.bindings.push((key, control)),
        }
        self
    }

    /// Removes the binding for `code` with `modifiers`, returning its Control.
    pub fn unbind(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<Control> {
        let key = chord(code, modifiers);
        let at = self.bindings.iter().position(|(k, _)| *k == key)?;
        Some(self.bindings.remove(at).1)
    }

    /// The Control for a key press or repeat, if bound.
    pub fn control(&self, event: &KeyEvent) -> Option<Control> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        let key = chord(event.code, event.modifiers);
        (self.bindings.iter())
            .find(|(k, _)| *k == key)
            .map(|&(_, control)| control)
    }

    /// The keys bound to `control`, in the order bound.
    pub fn keys(&self, control: Control) -> Vec<(KeyCode, KeyModifiers)> {
        (self.bindings.iter())
            .filter(|(_, c)| *c == control)
            .map(|&(key, _)| key)
            .collect()
    }

    /// The first key bound to `control`, named for a hint, such as `Ctrl+F`,
    /// `Alt+Down`, `Shift+F3` or `/`.
    pub fn hint(&self, control: Control) -> Option<String> {
        let (code, modifiers) = *self.keys(control).first()?;
        let mut name = String::new();
        for (modifier, label) in [
            (KeyModifiers::CONTROL, "Ctrl+"),
            (KeyModifiers::ALT, "Alt+"),
            (KeyModifiers::SHIFT, "Shift+"),
        ] {
            if modifiers.contains(modifier) {
                name += label;
            }
        }
        match code {
            KeyCode::Char(c) if !modifiers.is_empty() && c.is_ascii_uppercase() => {
                name += "Shift+";
                name.push(c);
            }
            KeyCode::Char(c) if !modifiers.is_empty() => name.push(c.to_ascii_uppercase()),
            KeyCode::Char(c) => name.push(c),
            KeyCode::F(n) => name += &format!("F{n}"),
            code => name += &format!("{code:?}"),
        }
        Some(name)
    }
}

/// A binding's key: Shift dropped from a character, whose case carries it.
fn chord(code: KeyCode, mut modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    if matches!(code, KeyCode::Char(_)) {
        modifiers.remove(KeyModifiers::SHIFT);
    }
    (code, modifiers)
}

/// How long a field that writes a Text Query waits after the last key typed
/// before it asks, unless the host sets another pause.
pub const TYPING_PAUSE: Duration = Duration::from_millis(150);

/// A field's typing pause: text typed waits until `pause` has passed with no
/// more, then [`Pause::due`] says to send it. A zero pause sends at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pause {
    pub(crate) pause: Duration,
    pub(crate) typed: Option<Instant>,
}

impl Default for Pause {
    fn default() -> Self {
        Pause {
            pause: TYPING_PAUSE,
            typed: None,
        }
    }
}

impl Pause {
    /// Text was typed: whether to send it now (no pause).
    pub(crate) fn typed(&mut self) -> bool {
        if self.pause.is_zero() {
            return true;
        }
        self.typed = Some(Instant::now());
        false
    }

    /// Whether typed text waits and its pause has passed by `now`; it stops
    /// waiting if so.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        let due = self.deadline().is_some_and(|at| now >= at);
        if due {
            self.typed = None;
        }
        due
    }

    /// Whether typed text waits to be sent; it stops waiting.
    pub(crate) fn flush(&mut self) -> bool {
        self.typed.take().is_some()
    }

    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.typed.map(|at| at + self.pause)
    }
}

/// The text, case and regex options a quick filter field holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Query {
    text: String,
    case_sensitive: bool,
    regex: bool,
}

/// The rows' quick filter field: one line holding the text and the case
/// (`Aa`) and regex (`.*`) toggles, drawn by [`QuickFilter::render`] only
/// while the rows have a quick filter. Each change sets the Grid's quick
/// filter, searching the shown columns: typing once a pause has passed (see
/// [`QuickFilter::tick`]), the toggles, Esc and Enter at once. Unfocused, it
/// shows the quick filter in force, which another control (the find bar's
/// Alt+F) may have set.
///
/// While focused it takes every key: characters and Backspace edit, Ctrl+U
/// clears, Alt+C toggles case, Alt+R regex (if the rows take regexes), Enter
/// keeps the filter and Esc puts back the one from before focus; both leave
/// the field.
///
/// Made [`QuickFilter::on`] the columns, the same field filters the columns,
/// searching the shown rows.
///
/// A quick filter the Table Source did not apply (an invalid regex, say)
/// turns the field red, with the message the host gives
/// [`QuickFilter::set_error`] after it, and the text kept.
#[derive(Clone, Debug, Default)]
pub struct QuickFilter {
    query: Query,
    /// The query when the field took focus, for Esc.
    before: Option<Query>,
    /// Whether the rows take regexes, as of focus.
    regex: bool,
    placeholder: String,
    pause: Pause,
    error: Option<String>,
    /// Whether it filters the columns, not the rows.
    columns: bool,
}

impl QuickFilter {
    pub fn new() -> Self {
        QuickFilter::default()
    }

    /// Makes it filter `axis`: the rows unless set.
    pub fn on(mut self, axis: Axis) -> Self {
        self.columns = axis == Axis::Cols;
        self
    }

    /// The axis it filters.
    pub fn axis(&self) -> Axis {
        if self.columns { Axis::Cols } else { Axis::Rows }
    }

    /// What the empty field says while unfocused, such as how to reach it.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// How long typing waits before setting the quick filter:
    /// [`TYPING_PAUSE`] unless set; zero sets it on every key.
    pub fn pause(mut self, pause: Duration) -> Self {
        self.pause.pause = pause;
        self
    }

    /// Sets the quick filter typed, once its pause has passed by `now`. Call
    /// it from the host's loop, waking by [`QuickFilter::deadline`].
    pub fn tick(&mut self, grid: &mut Grid, now: Instant) {
        if self.pause.due(now) {
            self.apply(grid);
        }
    }

    /// When typed text waits to be sent until, if it does.
    pub fn deadline(&self) -> Option<Instant> {
        self.pause.deadline()
    }

    /// The Table Source's message for a quick filter it did not apply,
    /// shown while the field's quick filter stays not applied.
    pub fn set_error(&mut self, message: Option<String>) {
        self.error = message;
    }

    pub fn focused(&self) -> bool {
        self.before.is_some()
    }

    /// Takes focus if its axis has a quick filter; returns whether it did.
    pub fn focus(&mut self, snapshot: &Snapshot) -> bool {
        let Some(support) = snapshot.quick_filter(self.axis()) else {
            return false;
        };
        self.regex = support.regex;
        if !self.focused() {
            self.query = in_force(snapshot, self.axis()).unwrap_or(self.query.clone());
        }
        self.before.get_or_insert_with(|| self.query.clone());
        true
    }

    /// The text typed so far.
    pub fn text(&self) -> &str {
        &self.query.text
    }

    /// Takes a key press while focused, setting `grid`'s quick filter as the
    /// query changes; false (taking nothing) when unfocused.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        if !self.focused() || event.kind == KeyEventKind::Release {
            return self.focused();
        }
        let q = &mut self.query;
        let alt = event.modifiers == KeyModifiers::ALT;
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        let typed = match event.code {
            KeyCode::Enter => {
                self.before = None;
                if self.pause.flush() {
                    self.apply(grid);
                }
                false
            }
            KeyCode::Esc => {
                self.pause.flush();
                self.query = self.before.take().unwrap_or_default();
                self.apply(grid);
                false
            }
            KeyCode::Char('c') if alt => {
                q.case_sensitive = !q.case_sensitive;
                self.pause.flush();
                self.apply(grid);
                false
            }
            KeyCode::Char('r') if alt && self.regex => {
                q.regex = !q.regex;
                self.pause.flush();
                self.apply(grid);
                false
            }
            KeyCode::Char('u') if ctrl => {
                q.text.clear();
                true
            }
            KeyCode::Char(c) if KeyModifiers::SHIFT.contains(event.modifiers) => {
                q.text.push(c);
                true
            }
            KeyCode::Backspace => {
                q.text.pop();
                true
            }
            _ => false,
        };
        if typed && self.pause.typed() {
            self.apply(grid);
        }
        true
    }

    fn apply(&self, grid: &mut Grid) {
        let q = &self.query;
        let query = TextQuery::new(q.text.clone())
            .case_sensitive(q.case_sensitive)
            .regex(q.regex);
        grid.set_quick_filter(self.axis(), Some(query));
    }

    /// Draws the field into `area`'s first line: `Filter`, the text (ending
    /// in a caret while focused), then the toggles at the right, lit when on.
    /// A quick filter still pending is dimmed; one not applied is
    /// drawn in the error style, followed by the message. Draws nothing while
    /// the rows have no quick filter.
    pub fn render(&self, snapshot: &Snapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        let axis = self.axis();
        let Some(support) = snapshot.quick_filter(axis) else {
            return;
        };
        if area.is_empty() {
            return;
        }
        let query = match in_force(snapshot, axis) {
            Some(q) if !self.focused() => q,
            _ => self.query.clone(),
        };
        let mut toggles = vec![("Aa", query.case_sensitive)];
        if support.regex {
            toggles.push((".*", query.regex));
        }
        let state = quick_state(snapshot, axis);
        // Not applied only while the field shows what was asked for.
        let state = match state {
            QuickState::NotApplied(asked) if asked != query => QuickState::Settled,
            state => state,
        };
        let label = match axis {
            Axis::Rows => " Filter ",
            Axis::Cols => " Filter columns ",
        };
        let width = usize::from(area.width);
        let message = match (&state, &self.error) {
            (QuickState::NotApplied(_), Some(m)) => {
                let m = format!("✗ {m} ");
                fit(&m, u32::try_from(width / 2).unwrap_or(0)).into_owned()
            }
            (QuickState::NotApplied(_), None) => "✗ not applied ".to_string(),
            _ => String::new(),
        };
        let right: usize = toggles.iter().map(|(t, _)| t.len() + 1).sum::<usize>()
            + 1
            + char_count(&message) as usize;
        let room = width.saturating_sub(char_count(label) as usize + right + 1);
        let blank = Style::default();
        let mut x = area.x;
        let mut put = |x: &mut u16, text: &str, style: Style| {
            for ch in text.chars() {
                if *x >= area.right() {
                    return;
                }
                buf[(*x, area.y)].set_char(ch).set_style(style);
                *x += 1;
            }
        };
        put(&mut x, &" ".repeat(width), theme.body);
        x = area.x;
        put(&mut x, label, theme.header.add_modifier(Modifier::REVERSED));
        put(&mut x, " ", theme.body);
        let field = theme.body.add_modifier(Modifier::UNDERLINED);
        let shown = if self.focused() {
            // Shows the end of a long text, then the caret.
            let chars: Vec<char> = self.query.text.chars().collect();
            let tail: String = chars[chars.len().saturating_sub(room.saturating_sub(1))..]
                .iter()
                .collect();
            format!("{tail}▏")
        } else if query.text.is_empty() {
            self.placeholder.clone()
        } else {
            query.text.clone()
        };
        let style = if !self.focused() && query.text.is_empty() {
            theme.body.patch(theme.loading).add_modifier(Modifier::DIM)
        } else {
            match state {
                QuickState::NotApplied(_) => field.patch(theme.error),
                QuickState::Pending => field.add_modifier(Modifier::DIM),
                QuickState::Settled => field,
            }
        };
        let shown = fit(&shown, u32::try_from(room).unwrap_or(u32::MAX));
        let pad = room.saturating_sub(char_count(&shown) as usize);
        put(&mut x, &shown, style);
        put(
            &mut x,
            &" ".repeat(pad),
            if self.focused() { field } else { blank },
        );
        put(&mut x, " ", theme.body);
        for (text, on) in toggles {
            put(&mut x, text, toggle_style(theme, on));
            put(&mut x, " ", theme.body);
        }
        let message_style = match state {
            QuickState::NotApplied(_) => theme.body.patch(theme.error),
            _ => theme.body.patch(theme.loading),
        };
        put(&mut x, &message, message_style);
    }
}

/// Where an axis's quick filter stands between the latest request and the
/// applied View Spec.
#[derive(Clone, Debug, PartialEq, Eq)]
enum QuickState {
    /// Applied as asked.
    Settled,
    /// Asked for, an answer still due.
    Pending,
    /// Asked for (this query), answered, and not applied.
    NotApplied(Query),
}

fn quick_state(snapshot: &Snapshot, axis: Axis) -> QuickState {
    let Some(requested) = snapshot.requested_view() else {
        return QuickState::Settled;
    };
    let asked = &requested.axis(axis).filter.quick;
    if *asked == snapshot.applied_view().axis(axis).filter.quick {
        return QuickState::Settled;
    }
    if snapshot.view_pending() {
        return QuickState::Pending;
    }
    QuickState::NotApplied(asked.as_ref().map_or_else(Query::default, Query::of))
}

/// `axis`'s quick filter as last asked for (the request still unanswered,
/// else the one applied), or an empty query for none; `None` while the axis
/// has no quick filter.
fn in_force(snapshot: &Snapshot, axis: Axis) -> Option<Query> {
    snapshot.quick_filter(axis)?;
    let view = snapshot.requested_view().unwrap_or(snapshot.applied_view());
    Some(
        view.axis(axis)
            .filter
            .quick
            .as_ref()
            .map_or_else(Query::default, Query::of),
    )
}

impl Query {
    fn of(q: &TextQuery) -> Self {
        Query {
            text: q.text.clone(),
            case_sensitive: q.case_sensitive,
            regex: q.regex,
        }
    }
}

/// A column's filter popup, dropped under the column header. Where the
/// column takes Text conditions, its Text condition: "Contains" or "Does not
/// contain", with its text and the case (`Aa`) and regex (`.*`, where the
/// column takes regexes) toggles. Where it compares numbers or dates, its
/// comparison: "Any value", "Equal to" to "At least", or "Between", with
/// the value (or both ends) typed as a number or a `YYYY-MM-DD` date, and
/// asked for once each field holds one. Where it takes OneOf conditions, the
/// checklist of its values: a search, "(Select all)" (or "(Select all
/// matching …)" while searching), each value with its count, "(Blanks)"
/// first, and notes on whether the list is capped and narrowed. Each change
/// edits the column's conditions at once, the checklist's by the rules of
/// the list-of-values filter (#107); the host hands it the values with
/// [`ColumnFilter::fetch`].
///
/// Opened on a row ([`ColumnFilter::open_row`]), the same popup filters the
/// columns by that row's values, dropped beside the row header.
///
/// While open it takes every key. Tab and Shift+Tab move between the text,
/// the comparison's fields, the search and the list. In a field or the
/// search, characters and Backspace edit and Ctrl+U clears; in the
/// comparison's fields, Left and Right step the comparison; in the list,
/// Up, Down, PageUp,
/// PageDown, Home and End move, Space ticks, and characters search. Ctrl+A
/// ticks the head of the list; Alt+N toggles not, Alt+C case, Alt+R regex.
/// Enter keeps the conditions and Esc puts back the ones from before; both
/// close it. Typing in the text or the comparison sets the condition once a
/// pause has passed (see [`ColumnFilter::tick`]); every other change, at
/// once.
///
/// A Text condition the Table Source did not apply (an invalid regex, say)
/// turns the text red, as one does a comparison's fields, with the message the host gives
/// [`ColumnFilter::set_error`] under it.
///
/// Made a [`ColumnFilter::drawer`], it is drawn down the table's right
/// instead, in the [`ColumnFilter::width`] the host leaves it, and stays
/// shown once opened: Enter and Esc keep the conditions and give focus back
/// to the table, where the drawer follows the focused column (a column with
/// no filters leaves it empty) until [`ColumnFilter::open`] focuses it again
/// or [`ColumnFilter::close`] closes it.
#[derive(Clone, Debug, Default)]
pub struct ColumnFilter {
    open: Option<Open>,
    pause: Pause,
    error: Option<String>,
    drawer: bool,
    /// The axis a drawer filters and the item it is shown on, while shown.
    shown: Option<(Axis, Key)>,
    /// Whether a shown drawer has focus.
    focused: bool,
}

/// Where the popup's keys go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Text,
    /// The comparison's value, or the low end of a Between.
    Compare,
    /// A Between's high end.
    High,
    Search,
    List,
}

#[derive(Clone, Debug)]
struct Open {
    /// The axis filtered: the rows for a column's popup, the columns for a
    /// row's.
    axis: Axis,
    key: Key,
    /// How the column takes Text conditions, if it does.
    text: Option<TextSupport>,
    /// Whether the column takes OneOf conditions.
    one_of: bool,
    /// What the column compares, if it does.
    compared: Option<Compared>,
    query: Query,
    not: bool,
    /// The comparison row's step in [`STEPS`] and its fields.
    step: usize,
    low: String,
    high: String,
    /// The column's conditions as last asked for, which the next change replaces.
    column: Column,
    /// The column's conditions when the popup opened, for Esc.
    before: Column,
    zone: Zone,
    search: String,
    /// The list row under the cursor: 0 is the head.
    cursor: usize,
    /// The first list row drawn.
    scroll: usize,
    /// The search and narrowing the values answer, once asked.
    asked: Option<(String, FilterSpec)>,
    /// The values, `None` until the source lists them.
    listed: Option<Listed>,
}

/// The values the source listed, the blank first if there are blanks.
#[derive(Clone, Debug)]
struct Listed {
    values: Vec<Value>,
    counts: Vec<Option<u64>>,
    partial: bool,
    narrowed: bool,
}

impl From<Distinct> for Listed {
    fn from(d: Distinct) -> Self {
        let mut values = Vec::new();
        let mut counts = Vec::new();
        if let Some(blanks) = d.blanks {
            values.push(Value::Blank);
            counts.push(match blanks {
                Blanks::Counted(n) => Some(n),
                _ => None,
            });
        }
        for v in d.values {
            values.push(Value::Text(v.text));
            counts.push(v.count);
        }
        Listed {
            values,
            counts,
            partial: d.partial,
            narrowed: d.narrowed,
        }
    }
}

/// The popup's width in cells, borders included.
const POPUP_WIDTH: u16 = 44;
/// How many values the checklist asks for; it says so when there are more.
const LIMIT: usize = 1000;
/// The most list rows the popup draws.
const LIST_ROWS: usize = 10;

impl ColumnFilter {
    pub fn new() -> Self {
        ColumnFilter::default()
    }

    /// How long typing in the text waits before setting the condition:
    /// [`TYPING_PAUSE`] unless set; zero sets it on every key.
    pub fn pause(mut self, pause: Duration) -> Self {
        self.pause.pause = pause;
        self
    }

    /// Sets the Text condition typed, once its pause has passed by `now`.
    /// Call it from the host's loop, waking by [`ColumnFilter::deadline`].
    pub fn tick(&mut self, grid: &mut Grid, now: Instant) {
        if self.pause.due(now)
            && let Some(o) = &mut self.open
        {
            o.apply_typed(grid);
        }
    }

    /// When typed text waits to be sent until, if it does.
    pub fn deadline(&self) -> Option<Instant> {
        self.pause.deadline()
    }

    /// The Table Source's message for a Text condition it did not apply,
    /// shown while the popup's condition stays not applied.
    pub fn set_error(&mut self, message: Option<String>) {
        self.error = message;
    }

    /// Draws it as a drawer down the table's right, not a popup.
    pub fn drawer(mut self) -> Self {
        self.drawer = true;
        self
    }

    /// The cells wide a shown drawer is, which the host takes from the
    /// table's right; 0 for a popup or a closed drawer.
    pub fn width(&self) -> u16 {
        if self.shown.is_some() { POPUP_WIDTH } else { 0 }
    }

    /// Whether it takes keys: a popup while open, a drawer while focused.
    pub fn focused(&self) -> bool {
        match self.drawer {
            true => self.focused && self.open.is_some(),
            false => self.open.is_some(),
        }
    }

    /// Whether it is open in slicer mode ([`ColumnFilter::open_slicer`]).
    pub fn slicing(&self) -> bool {
        self.open.as_ref().is_some_and(|o| o.column.slicing)
    }

    /// The column it is open on, if it is open on a column.
    pub fn column(&self) -> Option<Key> {
        self.open
            .as_ref()
            .filter(|o| o.axis == Axis::Rows)
            .map(|o| o.key)
    }

    /// The column or row it is open on, if it is open.
    pub fn item(&self) -> Option<Key> {
        self.open.as_ref().map(|o| o.key)
    }

    /// The axis it filters while open: [`Axis::Rows`] on a column,
    /// [`Axis::Cols`] on a row.
    pub fn filtering(&self) -> Option<Axis> {
        self.open.as_ref().map(|o| o.axis)
    }

    /// Opens on the focused column, or focuses a drawer already shown on
    /// it; returns whether it took focus. A drawer opened on a column with
    /// no filters shows empty and takes no focus.
    pub fn open(&mut self, snapshot: &Snapshot) -> bool {
        self.open_focused(snapshot, Axis::Rows)
    }

    /// Opens on the focused row, to filter the columns by its values
    /// ([`Control::RowFilter`]), or focuses a drawer already shown on it;
    /// returns whether it took focus.
    pub fn open_row(&mut self, snapshot: &Snapshot) -> bool {
        self.open_focused(snapshot, Axis::Cols)
    }

    /// Opens on the focused item of `axis`'s other axis, to filter `axis`.
    fn open_focused(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        let Some(key) = snapshot.focus(axis.other()).and_then(|f| f.key) else {
            return false;
        };
        if self.drawer && self.shown == Some((axis, key)) && self.open.is_some() {
            self.focused = true;
            return true;
        }
        self.open_on(snapshot, axis, key)
    }

    /// Closes it, keeping the conditions; returns whether it was open (or a
    /// drawer shown).
    pub fn close(&mut self) -> bool {
        let was = self.open.is_some() || self.shown.is_some();
        self.open = None;
        self.shown = None;
        self.focused = false;
        was
    }

    /// Opens on column `key` if it takes Text, OneOf or comparison
    /// conditions; returns whether it did.
    pub fn open_column(&mut self, snapshot: &Snapshot, key: Key) -> bool {
        self.open_on(snapshot, Axis::Rows, key)
    }

    /// Opens on `key`, an item of `axis`'s other axis, to filter `axis` by
    /// it, if `axis` takes Text, OneOf or comparison conditions on it;
    /// returns whether it did.
    pub fn open_on(&mut self, snapshot: &Snapshot, axis: Axis, key: Key) -> bool {
        let opened = self.show(snapshot, axis, key);
        if self.drawer {
            self.shown = Some((axis, key));
            self.focused = opened;
        }
        opened
    }

    /// Opens in slicer mode on column `key`, shown, hidden or grouped by:
    /// it edits the grouping's slicers on the column instead of the rows'
    /// conditions, with the same keys, titled "Slice by" the column. Returns
    /// whether the column takes Text, OneOf or comparison conditions.
    pub fn open_slicer(&mut self, snapshot: &Snapshot, key: Key) -> bool {
        let opened = self.put(snapshot, sliced(snapshot, key));
        if self.drawer {
            self.shown = Some((Axis::Rows, key));
            self.focused = opened;
        }
        opened
    }

    /// Puts `key`'s conditions on `axis` in it, or nothing if it takes none;
    /// returns whether it takes any.
    fn show(&mut self, snapshot: &Snapshot, axis: Axis, key: Key) -> bool {
        self.put(snapshot, Column::of(&asked(snapshot, axis), axis, key))
    }

    /// Puts `column` in it, or nothing if it takes no conditions; returns
    /// whether it takes any.
    fn put(&mut self, snapshot: &Snapshot, column: Column) -> bool {
        let (axis, key) = (column.axis, column.key);
        let support = snapshot.support(axis, key);
        if support.text.is_none() && !support.one_of && support.compare.is_none() {
            if self.drawer {
                self.open = None;
            }
            return false;
        }
        let mut open = Open {
            axis,
            key,
            text: support.text,
            one_of: support.one_of,
            compared: support.compare,
            query: Query::default(),
            not: false,
            step: 0,
            low: String::new(),
            high: String::new(),
            before: column.clone(),
            column,
            zone: match (support.text, support.compare) {
                (Some(_), _) => Zone::Text,
                (None, Some(_)) => Zone::Compare,
                (None, None) => Zone::Search,
            },
            search: String::new(),
            cursor: 0,
            scroll: 0,
            asked: None,
            listed: None,
        };
        open.show_text();
        open.show_compare();
        self.open = Some(open);
        true
    }

    /// Opens on the focused column at its text, set to contain or (with
    /// `not`) not contain what it holds, which is asked for at once if that
    /// changes it ([`Control::ColumnText`]); returns whether the column takes
    /// Text conditions.
    pub fn open_text(&mut self, grid: &mut Grid, not: bool) -> bool {
        let snapshot = grid.snapshot();
        let key = snapshot.focus(Axis::Cols).and_then(|f| f.key);
        let takes = key.is_some_and(|k| snapshot.support(Axis::Rows, k).text.is_some());
        if !takes || !self.open(&snapshot) {
            return false;
        }
        let Some(o) = &mut self.open else {
            return false;
        };
        o.zone = Zone::Text;
        if o.not != not {
            o.not = not;
            if !o.query.text.is_empty() {
                o.apply_text(grid);
            }
        }
        true
    }

    /// Hands the checklist its values: calls `ask` with the axis filtered,
    /// the column (or row) and the question when the search, or the
    /// narrowing (the axis's applied filter less the item's own
    /// conditions), changed since it last asked. A `None` answer lists
    /// nothing. Call it before each render while open.
    pub fn fetch(
        &mut self,
        snapshot: &Snapshot,
        ask: impl FnOnce(Axis, Key, &DistinctQuery) -> Option<Distinct>,
    ) {
        self.follow(snapshot);
        let Some(o) = &mut self.open else {
            return;
        };
        if !o.one_of {
            return;
        }
        let applied = &snapshot.applied_view().axis(o.axis).filter;
        let others: Vec<Condition> = (applied.conditions.conditions().iter())
            .filter(|c| c.key != o.key)
            .cloned()
            .collect();
        let narrow = FilterSpec::default()
            .with_quick(applied.quick.clone())
            .with_conditions(others.into());
        let question = (o.search.clone(), narrow);
        if o.asked.as_ref() == Some(&question) {
            return;
        }
        let query = DistinctQuery::new(LIMIT)
            .search(question.0.clone())
            .narrow(question.1.clone());
        o.listed = ask(o.axis, o.key, &query).map(Listed::from);
        o.asked = Some(question);
        o.cursor = o.cursor.min(o.rows().saturating_sub(1));
    }

    /// While a drawer's table has focus: shows the focused column (or row),
    /// and its conditions as edited elsewhere.
    fn follow(&mut self, snapshot: &Snapshot) {
        let Some((axis, shown)) = self.shown.filter(|_| !self.focused) else {
            return;
        };
        let key = snapshot.focus(axis.other()).and_then(|f| f.key);
        let key = key.unwrap_or(shown);
        let slicing = self.slicing();
        let column = match slicing {
            true => sliced(snapshot, key),
            false => Column::of(&asked(snapshot, axis), axis, key),
        };
        let same = self.open.as_ref().map(|o| (o.key, &o.column));
        if key != shown || same.is_some_and(|same| same != (key, &column)) {
            match slicing && key == shown {
                true => self.put(snapshot, column),
                false => self.show(snapshot, axis, key),
            };
            self.shown = Some((axis, key));
        }
    }

    /// Takes a key press while focused, editing `grid`'s rows conditions as
    /// they change; false (taking nothing) otherwise.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        if !self.focused() {
            return false;
        }
        let Some(o) = &mut self.open else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let pause = &mut self.pause;
        // Anything but typing in a field sends what waits first; Alt+N, C
        // and R send typed text with their own change, and Esc drops it.
        let alt_key = event.modifiers == KeyModifiers::ALT;
        let typing = matches!(o.zone, Zone::Text | Zone::Compare | Zone::High)
            && matches!(event.code, KeyCode::Char(_) | KeyCode::Backspace)
            && !alt_key;
        if (event.code == KeyCode::Esc && !self.drawer) || (alt_key && o.zone == Zone::Text) {
            pause.flush();
        } else if !typing && pause.flush() {
            o.apply_typed(grid);
        }
        let alt = event.modifiers == KeyModifiers::ALT;
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        let plain = KeyModifiers::SHIFT.contains(event.modifiers);
        let text = o.text.is_some();
        match event.code {
            KeyCode::Enter | KeyCode::Esc if self.drawer => self.focused = false,
            KeyCode::Enter => self.open = None,
            KeyCode::Esc => {
                let (text, one_of) = (o.before.text.clone(), o.before.one_of.clone());
                let column = o.column.write(grid, text, one_of);
                column.write_compare(grid, o.before.compare.clone());
                self.open = None;
            }
            KeyCode::Tab => o.zone = o.next_zone(true),
            KeyCode::BackTab => o.zone = o.next_zone(false),
            KeyCode::Char('n') if alt && text => {
                o.not = !o.not;
                o.apply_text(grid);
            }
            KeyCode::Char('c') if alt && text => {
                o.query.case_sensitive = !o.query.case_sensitive;
                o.apply_text(grid);
            }
            KeyCode::Char('r') if alt && o.text.is_some_and(|t| t.regex) => {
                o.query.regex = !o.query.regex;
                o.apply_text(grid);
            }
            KeyCode::Char('a') if ctrl && o.one_of => o.tick_row(grid, 0),
            KeyCode::Char('u') if ctrl => o.edit(grid, pause, |t| t.clear()),
            KeyCode::Char(' ') if plain && o.zone == Zone::List => o.tick_row(grid, o.cursor),
            KeyCode::Char(c) if plain => o.edit(grid, pause, |t| t.push(c)),
            KeyCode::Backspace => o.edit(grid, pause, |t| {
                t.pop();
            }),
            KeyCode::Left | KeyCode::Right if matches!(o.zone, Zone::Compare | Zone::High) => {
                let n = STEPS.len();
                o.step = match event.code {
                    KeyCode::Right => (o.step + 1) % n,
                    _ => (o.step + n - 1) % n,
                };
                o.zone = Zone::Compare;
                o.apply_compare(grid);
            }
            KeyCode::Down if o.zone == Zone::Search => o.zone = Zone::List,
            KeyCode::Up if o.zone == Zone::List && o.cursor == 0 => o.zone = Zone::Search,
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
                if o.zone == Zone::List =>
            {
                o.move_cursor(event.code)
            }
            _ => {}
        }
        true
    }

    /// Draws the popup while open, under its column's header in the Grid's
    /// `area` (or at the body's left while the column is out of view),
    /// moved left to fit, and as tall as its list needs and `area` allows.
    /// A row's popup is drawn at the body's left under the row (or under
    /// the header band while the row is out of view), moved up to fit.
    /// A drawer fills `area`, which the host places at the table's right,
    /// [`ColumnFilter::width`] wide; while the table has focus it shows no
    /// caret or cursor.
    pub fn render(
        &self,
        snapshot: &Snapshot,
        content: &impl Content,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let area = area.intersection(buf.area);
        let Some(o) = &self.open else {
            if let Some((axis, key)) = self.shown {
                let label = label(content, axis, key);
                render_empty(&label, noun(axis), theme, area, buf);
            }
            return;
        };
        let focused = self.focused();
        // Where it drops from: under the column header, or under the row.
        let (header, top) = match o.axis {
            Axis::Rows => {
                let x = (snapshot.headers(Axis::Cols))
                    .find(|h| h.key == Some(o.key))
                    .map_or(snapshot.header(Axis::Cols), |h| {
                        h.rect.x.max(snapshot.header(Axis::Cols))
                    });
                (x, snapshot.header(Axis::Rows))
            }
            Axis::Cols => {
                let y = (snapshot.headers(Axis::Rows))
                    .find(|h| h.key == Some(o.key))
                    .map_or(0.0, |h| h.rect.y + h.rect.height);
                (
                    snapshot.header(Axis::Cols),
                    y.max(snapshot.header(Axis::Rows)),
                )
            }
        };
        let mut hints = Vec::new();
        if let Some(t) = o.text {
            let mut keys = String::from("Alt+N not  Alt+C case");
            if t.regex {
                keys += "  Alt+R regex";
            }
            hints.push(keys);
        }
        match (o.compared, o.one_of) {
            (Some(_), true) => hints.push("Left, Right comparison".to_string()),
            (Some(_), false) => hints.push("Left, Right comparison  Tab move".to_string()),
            _ => {}
        }
        if o.one_of {
            hints.push("Tab move  Space tick  Ctrl+A all".to_string());
        }
        hints.push(match self.drawer {
            true => "Enter, Esc table  Ctrl+U clear".to_string(),
            false => "Enter keep  Esc undo  Ctrl+U clear".to_string(),
        });
        if !focused {
            hints = vec![format!("Follows the focused {}", noun(o.axis))];
        }
        // The column's Text condition and comparison as listed, if they are.
        let listed = match o.column.slicing {
            true => snapshot.slicers(),
            false => snapshot.conditions(o.axis),
        };
        let state_of = |c: &Option<Condition>| {
            let c = c.as_ref()?;
            (listed.iter())
                .find(|(listed, _)| listed == c)
                .map(|(_, state)| *state)
        };
        let state = state_of(&o.column.text);
        let compare_state = state_of(&o.column.compare);
        let rejected = state == Some(ConditionState::NotApplied) && self.pause.typed.is_none();
        let compare_rejected =
            compare_state == Some(ConditionState::NotApplied) && self.pause.typed.is_none();
        // Borders, the text row and any error under it, the comparison row
        // and any error under it, the search, rule and note rows, the hints.
        let fixed = 2
            + u16::from(o.text.is_some())
            + u16::from(rejected)
            + u16::from(o.compared.is_some())
            + u16::from(compare_rejected)
            + 3 * u16::from(o.one_of)
            + hints.len() as u16;
        let top = (top.ceil() as u16).min(area.height);
        let room = usize::from(area.height.saturating_sub(top + fixed));
        // The head and the values, a line saying there are none, or one
        // saying none were listed.
        let none = o.listed.as_ref().is_some_and(|l| l.values.is_empty());
        let list = match o.one_of {
            true if self.drawer => usize::from(area.height.saturating_sub(fixed)),
            true => (o.rows() + usize::from(none)).clamp(1, LIST_ROWS).min(room),
            false => 0,
        };
        let popup = if self.drawer {
            area
        } else {
            let width = POPUP_WIDTH.min(area.width);
            let height = (fixed + list as u16).min(area.height);
            let left = (header.max(0.0).ceil() as u16).min(area.width - width);
            let top = top.min(area.height - height);
            Rect::new(area.x + left, area.y + top, width, height)
        };
        if popup.width < 4 || popup.height < 3 {
            return;
        }
        let border = theme.body.patch(theme.border);
        let inner = usize::from(popup.width - 2);
        let mut put = |row: u16, x: &mut u16, text: &str, style: Style| {
            if row >= popup.height {
                return;
            }
            for ch in text.chars() {
                if *x >= popup.right() {
                    return;
                }
                buf[(*x, popup.y + row)].set_char(ch).set_style(style);
                *x += 1;
            }
        };
        let title = match o.axis {
            Axis::Rows if o.column.slicing => {
                format!(" Slice by {} ", label(content, o.axis, o.key))
            }
            Axis::Rows => format!(" Filter {} ", label(content, o.axis, o.key)),
            Axis::Cols => format!(" Filter columns by {} ", label(content, o.axis, o.key)),
        };
        let title = fit(&title, u32::try_from(inner).unwrap_or(0));
        let rule = inner.saturating_sub(char_count(&title) as usize + 1);
        let mut x = popup.x;
        put(0, &mut x, "┌─", border);
        put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
        put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);
        let mut row = 1;

        if o.text.is_some() {
            let how = if o.not {
                "Does not contain "
            } else {
                "Contains "
            };
            let mut toggles = vec![("Aa", o.query.case_sensitive)];
            if o.text.is_some_and(|t| t.regex) {
                toggles.push((".*", o.query.regex));
            }
            let right: usize = toggles.iter().map(|(t, _)| t.len() + 1).sum();
            let room = inner.saturating_sub(char_count(how) as usize + right + 2);
            let field = field_text(&o.query.text, room, focused && o.zone == Zone::Text);
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(row, &mut x, how, theme.body.add_modifier(Modifier::BOLD));
            let underlined = theme.body.add_modifier(Modifier::UNDERLINED);
            let underlined = match state {
                _ if rejected => underlined.patch(theme.error),
                Some(ConditionState::Adding) => underlined.add_modifier(Modifier::DIM),
                _ => underlined,
            };
            put(row, &mut x, &field, underlined);
            put(row, &mut x, " ", theme.body);
            for (text, on) in toggles {
                put(row, &mut x, text, toggle_style(theme, on));
                put(row, &mut x, " ", theme.body);
            }
            put(row, &mut x, "│", border);
            row += 1;
            if rejected {
                let message = self.error.as_deref().unwrap_or("not applied");
                let message = fit(
                    &format!("✗ {message}"),
                    u32::try_from(inner - 1).unwrap_or(0),
                )
                .into_owned();
                let pad = (inner - 1).saturating_sub(char_count(&message) as usize);
                let mut x = popup.x;
                put(row, &mut x, "│ ", border);
                put(row, &mut x, &message, theme.body.patch(theme.error));
                put(row, &mut x, &" ".repeat(pad), theme.body);
                put(row, &mut x, "│", border);
                row += 1;
            }
        }

        if let Some(compared) = o.compared {
            let step = format!("‹{}› ", STEPS[o.step]);
            let underlined = theme.body.add_modifier(Modifier::UNDERLINED);
            let underlined = match compare_state {
                _ if compare_rejected => underlined.patch(theme.error),
                Some(ConditionState::Adding) => underlined.add_modifier(Modifier::DIM),
                _ => underlined,
            };
            let placeholder = match compared {
                Compared::Numbers => "number",
                Compared::Dates => "YYYY-MM-DD",
            };
            let room = inner.saturating_sub(char_count(&step) as usize + 2);
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            let on = focused && matches!(o.zone, Zone::Compare | Zone::High);
            let step_style = match on && o.zone == Zone::Compare {
                true => theme.body.add_modifier(Modifier::BOLD | Modifier::REVERSED),
                false => theme.body.add_modifier(Modifier::BOLD),
            };
            put(row, &mut x, &step, step_style);
            // A field's text and its style: the placeholder, dimmed, while
            // empty and not typed in.
            let field = |text: &str, room: usize, zone: Zone| {
                let typing = focused && o.zone == zone;
                if text.is_empty() && !typing {
                    let shown = format!("{placeholder:<room$}");
                    let shown = fit(&shown, u32::try_from(room).unwrap_or(0)).into_owned();
                    (shown, underlined.patch(theme.loading))
                } else {
                    (field_text(text, room, typing), underlined)
                }
            };
            let parts = match o.step {
                0 => vec![(" ".repeat(room), theme.body)],
                BETWEEN => {
                    let half = room.saturating_sub(5) / 2;
                    vec![
                        field(&o.low, half, Zone::Compare),
                        (" and ".to_string(), theme.body),
                        field(&o.high, room.saturating_sub(5 + half), Zone::High),
                    ]
                }
                _ => vec![field(&o.low, room, Zone::Compare)],
            };
            for (text, style) in parts {
                put(row, &mut x, &text, style);
            }
            put(row, &mut x, " │", border);
            row += 1;
            if compare_rejected {
                let message = self.error.as_deref().unwrap_or("not applied");
                let message = fit(
                    &format!("✗ {message}"),
                    u32::try_from(inner - 1).unwrap_or(0),
                )
                .into_owned();
                let pad = (inner - 1).saturating_sub(char_count(&message) as usize);
                let mut x = popup.x;
                put(row, &mut x, "│ ", border);
                put(row, &mut x, &message, theme.body.patch(theme.error));
                put(row, &mut x, &" ".repeat(pad), theme.body);
                put(row, &mut x, "│", border);
                row += 1;
            }
        }

        if o.one_of {
            let label = "Search ";
            let room = inner.saturating_sub(char_count(label) as usize + 2);
            let field = field_text(&o.search, room, focused && o.zone == Zone::Search);
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(row, &mut x, label, theme.body.add_modifier(Modifier::BOLD));
            put(
                row,
                &mut x,
                &field,
                theme.body.add_modifier(Modifier::UNDERLINED),
            );
            put(row, &mut x, " │", border);
            row += 1;

            let count = o.listed.as_ref().map(|l| match l.partial {
                true => format!(" first {} values; type to search ", grouped(LIMIT as u64)),
                false => format!(" {} values ", grouped(l.values.len() as u64)),
            });
            let count = count.unwrap_or_default();
            let count = fit(&count, u32::try_from(inner.saturating_sub(2)).unwrap_or(0));
            let mut x = popup.x;
            put(row, &mut x, "├─", border);
            put(row, &mut x, &count, theme.body.patch(theme.loading));
            let rest = inner.saturating_sub(1 + char_count(&count) as usize);
            put(row, &mut x, &format!("{}┤", "─".repeat(rest)), border);
            row += 1;

            let width = inner.saturating_sub(2);
            let start = o
                .scroll
                .min(o.cursor)
                .max((o.cursor + 1).saturating_sub(list));
            for i in start..start + list {
                let (line, style) = o.list_row(i, width, theme);
                let mut x = popup.x;
                put(row, &mut x, "│ ", border);
                let style = if focused && i == o.cursor && o.zone == Zone::List {
                    style.add_modifier(Modifier::REVERSED)
                } else {
                    style
                };
                put(row, &mut x, &line, style);
                put(row, &mut x, " │", border);
                row += 1;
            }

            let note = match &o.listed {
                Some(l) if l.narrowed => "narrowed by the other filters",
                Some(_) => "showing all values",
                None => "",
            };
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(
                row,
                &mut x,
                &format!("{note:<width$}"),
                theme.body.patch(theme.loading),
            );
            put(row, &mut x, " │", border);
            row += 1;
        }

        // A drawer keeps its hints at the bottom.
        let blank = format!("{:<inner$}", "");
        while self.drawer && usize::from(row) + hints.len() + 1 < usize::from(popup.height) {
            let mut x = popup.x;
            put(row, &mut x, "│", border);
            put(row, &mut x, &blank, theme.body);
            put(row, &mut x, "│", border);
            row += 1;
        }
        for hint in &hints {
            let hint = fit(hint, u32::try_from(inner - 1).unwrap_or(0));
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            let pad = (inner - 1).saturating_sub(char_count(&hint) as usize);
            put(row, &mut x, &hint, theme.body.patch(theme.loading));
            put(row, &mut x, &" ".repeat(pad), theme.body);
            put(row, &mut x, "│", border);
            row += 1;
        }
        let mut x = popup.x;
        let bottom = popup.height - 1;
        put(bottom, &mut x, &format!("└{}┘", "─".repeat(inner)), border);
    }
}

/// `axis`'s conditions as last asked for, less those being removed.
fn asked(snapshot: &Snapshot, axis: Axis) -> Vec<Condition> {
    (snapshot.conditions(axis).into_iter())
        .filter(|(_, state)| *state != ConditionState::Removing)
        .map(|(c, _)| c)
        .collect()
}

/// The slicers on column `key` as last asked for, less those being removed.
fn sliced(snapshot: &Snapshot, key: Key) -> Column {
    let slicers: Vec<Condition> = (snapshot.slicers().into_iter())
        .filter(|(_, state)| *state != ConditionState::Removing)
        .map(|(c, _)| c)
        .collect();
    Column::sliced(&slicers, key)
}

/// What a popup filtering `axis` is open on: "column" or "row".
fn noun(axis: Axis) -> &'static str {
    match axis {
        Axis::Rows => "column",
        Axis::Cols => "row",
    }
}

/// The name of `key`, an item of `axis`'s other axis, for a title or chip.
pub(crate) fn label(content: &impl Content, axis: Axis, key: Key) -> String {
    match axis {
        Axis::Rows => content.column_header(key),
        Axis::Cols => content.row_label(key),
    }
}

/// Removes the focused column's conditions on the rows and the focused
/// row's on the columns or, with none there, the last condition on the
/// rows, else on the columns ([`Control::Remove`]).
pub fn remove_conditions(grid: &mut Grid) {
    let snapshot = grid.snapshot();
    let on = |axis: Axis| -> Vec<Condition> {
        let focused = snapshot.focus(axis.other()).and_then(|f| f.key);
        let asked = asked(&snapshot, axis);
        asked
            .into_iter()
            .filter(|c| Some(c.key) == focused)
            .collect()
    };
    let mut gone: Vec<(Axis, Condition)> = [Axis::Rows, Axis::Cols]
        .into_iter()
        .flat_map(|axis| on(axis).into_iter().map(move |c| (axis, c)))
        .collect();
    if gone.is_empty() {
        gone = [Axis::Rows, Axis::Cols]
            .into_iter()
            .find_map(|axis| asked(&snapshot, axis).pop().map(|c| (axis, c)))
            .into_iter()
            .collect();
    }
    for (axis, c) in &gone {
        grid.remove_condition(*axis, c);
    }
}

/// Clears both axes' quick filters and conditions in one request
/// ([`Control::Clear`]); does nothing while there are none.
pub fn clear_filters(grid: &mut Grid) {
    let snapshot = grid.snapshot();
    let mut spec = snapshot
        .requested_view()
        .unwrap_or(snapshot.applied_view())
        .clone();
    if spec.rows.filter.is_empty() && spec.cols.filter.is_empty() {
        return;
    }
    spec.rows.filter = FilterSpec::default();
    spec.cols.filter = FilterSpec::default();
    grid.request_view(spec);
}

/// Keeps only the rows whose value in column `key` is `value`
/// ([`Control::KeepValue`], with the focused cell's value as the host reads
/// it): the column's OneOf becomes `value` alone, and its Text condition
/// stays. Returns whether the column takes OneOf conditions.
pub fn keep_value(grid: &mut Grid, key: Key, value: Value) -> bool {
    let snapshot = grid.snapshot();
    if !snapshot.support(Axis::Rows, key).one_of {
        return false;
    }
    let column = Column::of(&asked(&snapshot, Axis::Rows), Axis::Rows, key);
    let one_of = Condition::one_of(key, vec![value]);
    column.write(grid, column.text.clone(), Some(one_of));
    true
}

/// A drawer shown on a column (or row) with no filters: its title and a
/// note.
fn render_empty(label: &str, noun: &str, theme: &Theme, area: Rect, buf: &mut Buffer) {
    if area.width < 4 || area.height < 3 {
        return;
    }
    let border = theme.body.patch(theme.border);
    let inner = usize::from(area.width - 2);
    let mut put = |row: u16, x: &mut u16, text: &str, style: Style| {
        for ch in text.chars() {
            if *x >= area.right() {
                return;
            }
            buf[(*x, area.y + row)].set_char(ch).set_style(style);
            *x += 1;
        }
    };
    let title = format!(" Filter {label} ");
    let title = fit(&title, u32::try_from(inner).unwrap_or(0));
    let rule = inner.saturating_sub(char_count(&title) as usize + 1);
    let mut x = area.x;
    put(0, &mut x, "┌─", border);
    put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
    put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);
    for row in 1..area.height - 1 {
        let note = match row {
            1 => format!("No filters on this {noun}"),
            _ => String::new(),
        };
        let note = fit(&note, u32::try_from(inner - 1).unwrap_or(0));
        let mut x = area.x;
        put(row, &mut x, "│ ", border);
        put(
            row,
            &mut x,
            &format!("{note:<w$}", w = inner - 1),
            theme.body.patch(theme.loading),
        );
        put(row, &mut x, "│", border);
    }
    let mut x = area.x;
    put(
        area.height - 1,
        &mut x,
        &format!("└{}┘", "─".repeat(inner)),
        border,
    );
}

/// A field's text in `room` cells: the end of a long text then a caret
/// while focused, padded.
pub(crate) fn field_text(text: &str, room: usize, focused: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let shown: String = if focused {
        let tail: String = chars[chars.len().saturating_sub(room.saturating_sub(1))..]
            .iter()
            .collect();
        format!("{tail}▏")
    } else {
        fit(text, u32::try_from(room).unwrap_or(0)).into_owned()
    };
    let pad = room.saturating_sub(char_count(&shown) as usize);
    format!("{shown}{}", " ".repeat(pad))
}

impl Open {
    /// The values shown, as the checklist's rules read them.
    fn shown(&self) -> Shown<'_> {
        let listed = self.listed.as_ref();
        Shown {
            values: listed.map_or(&[][..], |l| &l.values),
            partial: listed.is_some_and(|l| l.partial),
            search: &self.search,
        }
    }

    /// How many list rows there are: the head and each value, or none
    /// until listed.
    fn rows(&self) -> usize {
        self.listed.as_ref().map_or(0, |l| 1 + l.values.len())
    }

    /// List row `i` in `width` cells and its style: a tick box, the value
    /// and its count at the right.
    fn list_row(&self, i: usize, width: usize, theme: &Theme) -> (String, Style) {
        let Some(listed) = &self.listed else {
            return (
                format!("{:<width$}", "No values listed"),
                theme.body.patch(theme.loading),
            );
        };
        let shown = self.shown();
        let (mark, label, count, style) = if i == 0 {
            let on = self.column.head_ticked(&shown);
            let some = listed.values.iter().any(|v| self.column.ticked(v));
            let mark = match (on, some && shown.search.is_empty()) {
                (true, _) => "[x]",
                (false, true) => "[-]",
                _ => "[ ]",
            };
            let label = match shown.search {
                "" => "(Select all)".to_string(),
                s => format!("(Select all matching \"{s}\")"),
            };
            (mark, label, None, theme.body.add_modifier(Modifier::BOLD))
        } else if let Some(value) = listed.values.get(i - 1) {
            let mark = if self.column.ticked(value) {
                "[x]"
            } else {
                "[ ]"
            };
            let label = match value {
                Value::Blank => "(Blanks)".to_string(),
                Value::Text(t) if t.is_empty() => "\"\"".to_string(),
                Value::Text(t) => t.clone(),
            };
            let style = if self.column.excluded(value) {
                theme.body.add_modifier(Modifier::DIM)
            } else {
                theme.body
            };
            (mark, label, listed.counts[i - 1], style)
        } else if listed.values.is_empty() && i == 1 {
            return (
                format!("{:<width$}", "No values"),
                theme.body.patch(theme.loading),
            );
        } else {
            return (" ".repeat(width), theme.body);
        };
        let count = count.map(grouped).unwrap_or_default();
        let room = width.saturating_sub(4 + char_count(&count) as usize + 1);
        let label = fit(&label, u32::try_from(room).unwrap_or(0));
        let pad =
            width.saturating_sub(4 + char_count(&label) as usize + char_count(&count) as usize);
        (format!("{mark} {label}{}{count}", " ".repeat(pad)), style)
    }

    /// The zone after (or before) this one, skipping those the column lacks.
    fn next_zone(&self, forward: bool) -> Zone {
        let mut zones = Vec::new();
        if self.text.is_some() {
            zones.push(Zone::Text);
        }
        if self.compared.is_some() {
            zones.push(Zone::Compare);
            if self.step == BETWEEN {
                zones.push(Zone::High);
            }
        }
        if self.one_of {
            zones.extend([Zone::Search, Zone::List]);
        }
        let at = zones.iter().position(|z| *z == self.zone).unwrap_or(0);
        let n = zones.len();
        zones[if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        }]
    }

    /// Edits the text or a comparison field (sent once `pause` allows) or,
    /// elsewhere, the search with `f`.
    fn edit(&mut self, grid: &mut Grid, pause: &mut Pause, f: impl FnOnce(&mut String)) {
        let field = match self.zone {
            Zone::Text => Some(&mut self.query.text),
            Zone::Compare if self.step > 0 => Some(&mut self.low),
            Zone::High => Some(&mut self.high),
            _ => None,
        };
        if let Some(field) = field {
            f(field);
            if pause.typed() {
                self.apply_typed(grid);
            }
        } else if self.one_of {
            f(&mut self.search);
            self.cursor = 0;
            self.scroll = 0;
        }
    }

    /// Asks for the condition the field being typed in shows.
    fn apply_typed(&mut self, grid: &mut Grid) {
        match self.zone {
            Zone::Compare | Zone::High => self.apply_compare(grid),
            _ => self.apply_text(grid),
        }
    }

    /// Asks for the comparison the comparison row shows, keeping its not,
    /// or none while a field holds no value.
    fn apply_compare(&mut self, grid: &mut Grid) {
        let Some(compared) = self.compared else {
            return;
        };
        let not = self.column.compare.as_ref().is_some_and(|c| c.not);
        let to = compare::comparison(self.step, compared, &self.low, &self.high)
            .map(|c| Condition::compare(self.key, c).not(not));
        if to != self.column.compare {
            self.column = self.column.write_compare(grid, to);
        }
    }

    /// Shows the column's comparison in the comparison row.
    fn show_compare(&mut self) {
        let shown = match self.column.compare.as_ref().map(|c| &c.kind) {
            Some(ConditionKind::Compare(c)) => compare::fields(c),
            _ => (0, String::new(), String::new()),
        };
        (self.step, self.low, self.high) = shown;
    }

    /// Asks for the Text condition the text row shows, or none for no text.
    fn apply_text(&mut self, grid: &mut Grid) {
        let q = &self.query;
        let query = TextQuery::new(q.text.clone())
            .case_sensitive(q.case_sensitive)
            .regex(q.regex);
        let to = (!q.text.is_empty()).then(|| Condition::text(self.key, query).not(self.not));
        let one_of = self.column.one_of.clone();
        self.column = self.column.write(grid, to, one_of);
    }

    /// Shows the column's Text condition in the text row.
    fn show_text(&mut self) {
        let regex = self.text.is_some_and(|t| t.regex);
        let text = self.column.text.as_ref();
        self.query = match text.map(|c| &c.kind) {
            Some(ConditionKind::Text(q)) => Query {
                text: q.text.clone(),
                case_sensitive: q.case_sensitive,
                regex: q.regex && regex,
            },
            _ => Query::default(),
        };
        self.not = text.is_some_and(|c| c.not);
    }

    /// Ticks or unticks list row `i` (0 the head), whichever it is not.
    fn tick_row(&mut self, grid: &mut Grid, i: usize) {
        let shown = self.shown();
        let column = if i == 0 {
            let on = !self.column.head_ticked(&shown);
            self.column.tick_head(grid, on, &shown)
        } else if let Some(value) = shown.values.get(i - 1) {
            let on = !self.column.ticked(value);
            self.column.tick(grid, value, on, &shown)
        } else {
            return;
        };
        self.column = column;
        self.show_text();
    }

    fn move_cursor(&mut self, code: KeyCode) {
        let last = self.rows().saturating_sub(1);
        self.cursor = match code {
            KeyCode::Up => self.cursor.saturating_sub(1),
            KeyCode::Down => (self.cursor + 1).min(last),
            KeyCode::PageUp => self.cursor.saturating_sub(LIST_ROWS - 1),
            KeyCode::PageDown => (self.cursor + LIST_ROWS - 1).min(last),
            KeyCode::Home => 0,
            _ => last,
        };
        self.scroll = self
            .scroll
            .min(self.cursor)
            .max((self.cursor + 1).saturating_sub(LIST_ROWS));
    }
}

pub(crate) fn toggle_style(theme: &Theme, on: bool) -> Style {
    if on {
        theme
            .sort_marker
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        theme.body.add_modifier(Modifier::DIM)
    }
}

/// Draws the conditions into `area`'s first line: the rows' after a
/// `Where` label, then the columns' after a `Columns where` label, a chip
/// for each, such as `port: not bergen` or `name: /^an/ Aa` (a column's
/// conditions named by their row's [`Content::row_label`]), dimmed with a
/// `~` while being added, struck through while being removed, and in the
/// error style with a `!` when the Table Source did not apply it. Then
/// the grouping's slicers after a `Sliced by` label, chips as the
/// conditions', and each level and value of the grouping the source left
/// out, such as `Grouped by port (Subtotal before)!` or `median(weight)!`,
/// in the error style. Draws nothing while there are none.
pub fn render_conditions(
    snapshot: &Snapshot,
    content: &impl Content,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let area = area.intersection(buf.area);
    let listed = [Axis::Rows, Axis::Cols].map(|axis| snapshot.conditions(axis));
    let slicers = snapshot.slicers();
    let not_applied = |state: &ConditionState| *state == ConditionState::NotApplied;
    let left_out: Vec<String> = (Axis::ALL.into_iter())
        .flat_map(|axis| {
            let levels = snapshot.levels(axis).into_iter();
            levels
                .filter(|(_, state)| not_applied(state))
                .map(move |(level, _)| level_text(content, axis, &level))
        })
        .chain(
            (snapshot.values().into_iter())
                .filter(|(_, state)| not_applied(state))
                .map(|(value, _)| content.value_name(&value)),
        )
        .collect();
    if area.is_empty()
        || (listed.iter().all(Vec::is_empty) && slicers.is_empty() && left_out.is_empty())
    {
        return;
    }
    let mut x = area.x;
    let mut put = |x: &mut u16, text: &str, style: Style| {
        for ch in text.chars() {
            if *x >= area.right() {
                return;
            }
            buf[(*x, area.y)].set_char(ch).set_style(style);
            *x += 1;
        }
    };
    put(&mut x, &" ".repeat(usize::from(area.width)), theme.body);
    x = area.x;
    let axes = [(Axis::Rows, " Where "), (Axis::Cols, " Columns where ")];
    for ((axis, title), listed) in axes.into_iter().zip(listed) {
        if listed.is_empty() {
            continue;
        }
        if x > area.x {
            put(&mut x, " ", theme.body);
        }
        put(&mut x, title, theme.header.add_modifier(Modifier::REVERSED));
        chips(&mut put, &mut x, content, theme, axis, listed);
    }
    if !slicers.is_empty() {
        if x > area.x {
            put(&mut x, " ", theme.body);
        }
        let title = " Sliced by ";
        put(&mut x, title, theme.header.add_modifier(Modifier::REVERSED));
        chips(&mut put, &mut x, content, theme, Axis::Rows, slicers);
    }
    let error = theme
        .body
        .patch(theme.error)
        .add_modifier(Modifier::REVERSED);
    for text in left_out {
        put(&mut x, " ", theme.body);
        put(&mut x, &format!(" {text}! "), error);
    }
}

/// A level of `axis`'s grouping for its chip: `Grouped by port` on the
/// rows, `Pivoted by port` on the columns, with where its Subtotal goes.
fn level_text(content: &impl Content, axis: Axis, level: &Level) -> String {
    let Level::Field { field, subtotal } = level else {
        return "Values level".into();
    };
    let by = match axis {
        Axis::Rows => "Grouped by",
        Axis::Cols => "Pivoted by",
    };
    let name = label(content, Axis::Rows, *field);
    match subtotal {
        Placement::Before => format!("{by} {name} (Subtotal before)"),
        Placement::After => format!("{by} {name} (Subtotal after)"),
        _ => format!("{by} {name}"),
    }
}

/// Puts a chip for each of `axis`'s `listed` conditions.
fn chips(
    put: &mut impl FnMut(&mut u16, &str, Style),
    x: &mut u16,
    content: &impl Content,
    theme: &Theme,
    axis: Axis,
    listed: Vec<(Condition, ConditionState)>,
) {
    for (condition, state) in listed {
        put(x, " ", theme.body);
        let mut chip = chip_text(&label(content, axis, condition.key), &condition);
        let style = theme.sort_marker.add_modifier(Modifier::REVERSED);
        let style = match state {
            ConditionState::Applied => style,
            ConditionState::Adding => {
                chip.push('~');
                style.add_modifier(Modifier::DIM)
            }
            ConditionState::NotApplied => {
                chip.push('!');
                theme
                    .body
                    .patch(theme.error)
                    .add_modifier(Modifier::REVERSED)
            }
            _ => style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT),
        };
        put(x, &format!(" {chip} "), style);
    }
}

/// A chip's text: `label: text`, with `not ` before the text, the text in
/// slashes for a regex, and ` Aa` when it matches case.
fn chip_text(label: &str, c: &Condition) -> String {
    let not = if c.not { "not " } else { "" };
    match &c.kind {
        ConditionKind::Text(q) => {
            let text = if q.regex {
                format!("/{}/", q.text)
            } else {
                q.text.clone()
            };
            let case = if q.case_sensitive { " Aa" } else { "" };
            format!("{label}: {not}{text}{case}")
        }
        ConditionKind::OneOf(values) => format!("{label}: {not}{}", listed_text(values)),
        ConditionKind::Compare(c) => format!("{label}: {not}{}", compare::text(c)),
        // The source's own language, passed through as typed.
        ConditionKind::Native(text) => format!("{label}: {not}‹{text}›"),
        _ => format!("{label}: …"),
    }
}

/// A OneOf's values for its chip: the first three, then how many more;
/// "nothing" for none.
fn listed_text(values: &[Value]) -> String {
    if values.is_empty() {
        return "nothing".to_string();
    }
    let named: Vec<&str> = (values.iter().take(3))
        .map(|v| match v {
            Value::Blank => "(Blanks)",
            Value::Text(t) if t.is_empty() => "\"\"",
            Value::Text(t) => t,
        })
        .collect();
    let more = values.len() - named.len();
    let more = if more > 0 {
        format!(" +{more}")
    } else {
        String::new()
    };
    named.join(", ") + &more
}

/// The row count for a status line: "1,204 of 5,001 rows" when the Table
/// Source gave an unfiltered total that differs, else "5,001 rows";
/// "filtering…" while a request that changes the rows' filter is pending.
pub fn row_count(snapshot: &Snapshot) -> String {
    count(snapshot, Axis::Rows, ("row", "rows"))
}

/// The count of `axis` for a status line, naming its items by `noun`
/// (singular, plural), as [`row_count`] does the rows': "5 of 12 fields"
/// when the axis is filtered.
pub fn count(snapshot: &Snapshot, axis: Axis, noun: (&str, &str)) -> String {
    let filter = |v: &binnacle_core::ViewSpec| v.axis(axis).filter.clone();
    let requested = snapshot.requested_view().map(filter);
    if snapshot.view_pending() && requested != Some(filter(snapshot.applied_view())) {
        return "filtering…".into();
    }
    let count = snapshot.count(axis);
    let noun = if count == 1 { noun.0 } else { noun.1 };
    match snapshot.total(axis) {
        Some(total) if total != count => {
            format!("{} of {} {noun}", grouped(count), grouped(total))
        }
        _ => format!("{} {noun}", grouped(count)),
    }
}

/// `n` with commas between groups of three digits.
pub(crate) fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(d);
    }
    out
}
