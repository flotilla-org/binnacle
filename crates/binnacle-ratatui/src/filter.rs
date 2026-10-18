//! Find and filter controls: the keys that reach them, by preset, and the
//! quick filter field.

use std::collections::HashMap;

use binnacle_core::{Axis, Grid, Snapshot, TextQuery};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::Theme;
use crate::renderer::{char_count, fit};

/// A find or filter control a key reaches, outside the Grid's Actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// Focus the quick filter field.
    QuickFilter,
}

/// A set of bindings for the find and filter controls, to suit how a
/// terminal user already works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Ctrl chords, as in a desktop application: Ctrl+L for the quick filter.
    Chords,
}

/// Maps crossterm key presses to [`Control`]s, starting from a [`Preset`];
/// hosts rebind with [`Controls::bind`] and [`Controls::unbind`]. Check it
/// before the [`crate::Keymap`], so its bindings win.
#[derive(Clone, Debug)]
pub struct Controls {
    bindings: HashMap<(KeyCode, KeyModifiers), Control>,
}

impl Controls {
    pub fn preset(preset: Preset) -> Self {
        let mut controls = Controls {
            bindings: HashMap::new(),
        };
        match preset {
            Preset::Chords => {
                controls.bind(
                    KeyCode::Char('l'),
                    KeyModifiers::CONTROL,
                    Control::QuickFilter,
                );
            }
        }
        controls
    }

    /// Binds `code` pressed with exactly `modifiers` to `control`, replacing
    /// any earlier binding.
    pub fn bind(&mut self, code: KeyCode, modifiers: KeyModifiers, control: Control) -> &mut Self {
        self.bindings.insert((code, modifiers), control);
        self
    }

    /// Removes the binding for `code` with `modifiers`, returning its Control.
    pub fn unbind(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Option<Control> {
        self.bindings.remove(&(code, modifiers))
    }

    /// The Control for a key press or repeat, if bound.
    pub fn control(&self, event: &KeyEvent) -> Option<Control> {
        if event.kind == KeyEventKind::Release {
            return None;
        }
        self.bindings.get(&(event.code, event.modifiers)).copied()
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
/// filter, searching the shown columns.
///
/// While focused it takes every key: characters and Backspace edit, Ctrl+U
/// clears, Alt+C toggles case, Alt+R regex (if the rows take regexes), Enter
/// keeps the filter and Esc puts back the one from before focus; both leave
/// the field.
#[derive(Clone, Debug, Default)]
pub struct QuickFilter {
    query: Query,
    /// The query when the field took focus, for Esc.
    before: Option<Query>,
    /// Whether the rows take regexes, as of focus.
    regex: bool,
    placeholder: String,
}

impl QuickFilter {
    pub fn new() -> Self {
        QuickFilter::default()
    }

    /// What the empty field says while unfocused, such as how to reach it.
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn focused(&self) -> bool {
        self.before.is_some()
    }

    /// Takes focus if the rows have a quick filter; returns whether it did.
    pub fn focus(&mut self, snapshot: &Snapshot) -> bool {
        let Some(support) = snapshot.quick_filter(Axis::Rows) else {
            return false;
        };
        self.regex = support.regex;
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
        match event.code {
            KeyCode::Enter => self.before = None,
            KeyCode::Esc => {
                self.query = self.before.take().unwrap_or_default();
                self.apply(grid);
            }
            KeyCode::Char('c') if alt => {
                q.case_sensitive = !q.case_sensitive;
                self.apply(grid);
            }
            KeyCode::Char('r') if alt && self.regex => {
                q.regex = !q.regex;
                self.apply(grid);
            }
            KeyCode::Char('u') if ctrl => {
                q.text.clear();
                self.apply(grid);
            }
            KeyCode::Char(c) if KeyModifiers::SHIFT.contains(event.modifiers) => {
                q.text.push(c);
                self.apply(grid);
            }
            KeyCode::Backspace => {
                q.text.pop();
                self.apply(grid);
            }
            _ => {}
        }
        true
    }

    fn apply(&self, grid: &mut Grid) {
        let q = &self.query;
        let query = TextQuery::new(q.text.clone())
            .case_sensitive(q.case_sensitive)
            .regex(q.regex);
        grid.set_quick_filter(Axis::Rows, Some(query));
    }

    /// Draws the field into `area`'s first line: `Filter`, the text (ending
    /// in a caret while focused), then the toggles at the right, lit when on.
    /// Draws nothing while the rows have no quick filter.
    pub fn render(&self, snapshot: &Snapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        let Some(support) = snapshot.quick_filter(Axis::Rows) else {
            return;
        };
        if area.is_empty() {
            return;
        }
        let mut toggles = vec![("Aa", self.query.case_sensitive)];
        if support.regex {
            toggles.push((".*", self.query.regex));
        }
        let label = " Filter ";
        let right: usize = toggles.iter().map(|(t, _)| t.len() + 1).sum::<usize>() + 1;
        let width = usize::from(area.width);
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
        } else if self.query.text.is_empty() {
            self.placeholder.clone()
        } else {
            self.query.text.clone()
        };
        let style = if !self.focused() && self.query.text.is_empty() {
            theme.body.patch(theme.loading).add_modifier(Modifier::DIM)
        } else {
            field
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
            let style = if on {
                theme
                    .sort_marker
                    .add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                theme.body.add_modifier(Modifier::DIM)
            };
            put(&mut x, text, style);
            put(&mut x, " ", theme.body);
        }
    }
}

/// The row count for a status line: "1,204 of 5,001 rows" when the Table
/// Source gave an unfiltered total that differs, else "5,001 rows".
pub fn row_count(snapshot: &Snapshot) -> String {
    let count = snapshot.count(Axis::Rows);
    let noun = if count == 1 { "row" } else { "rows" };
    match snapshot.total(Axis::Rows) {
        Some(total) if total != count => {
            format!("{} of {} {noun}", grouped(count), grouped(total))
        }
        _ => format!("{} {noun}", grouped(count)),
    }
}

/// `n` with commas between groups of three digits.
fn grouped(n: u64) -> String {
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
