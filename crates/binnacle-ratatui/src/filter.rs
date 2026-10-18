//! Find and filter controls: the keys that reach them, by preset, the quick
//! filter field, the column filter popup and the conditions line.

use std::collections::HashMap;

use binnacle_core::{
    Axis, Condition, ConditionKind, ConditionState, Grid, Key, Snapshot, TextQuery,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::renderer::{char_count, fit};
use crate::{Content, Theme};

/// A find or filter control a key reaches, outside the Grid's Actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Control {
    /// Focus the quick filter field.
    QuickFilter,
    /// Open the focused column's filter popup.
    ColumnFilter,
}

/// A set of bindings for the find and filter controls, to suit how a
/// terminal user already works.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Preset {
    /// Ctrl chords, as in a desktop application: Ctrl+L for the quick
    /// filter, Alt+Down for the focused column's filter popup.
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
                controls.bind(KeyCode::Down, KeyModifiers::ALT, Control::ColumnFilter);
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
            put(&mut x, text, toggle_style(theme, on));
            put(&mut x, " ", theme.body);
        }
    }
}

/// A column's filter popup, dropped under the column header: the column's
/// Text condition, "Contains" or "Does not contain", with its text and the
/// case (`Aa`) and regex (`.*`, where the column takes regexes) toggles.
/// Opens on a column that takes Text conditions; each change edits the
/// column's first Text condition (adding one, or removing it once the text
/// is empty).
///
/// While open it takes every key: characters and Backspace edit, Ctrl+U
/// clears, Alt+N toggles not, Alt+C case, Alt+R regex, Enter keeps the
/// condition and Esc puts back the one from before; both close it.
#[derive(Clone, Debug, Default)]
pub struct ColumnFilter {
    open: Option<Open>,
}

#[derive(Clone, Debug)]
struct Open {
    key: Key,
    query: Query,
    not: bool,
    /// Whether the column takes regexes.
    regex: bool,
    /// The condition last asked for, which the next change replaces.
    sent: Option<Condition>,
    /// The column's condition when the popup opened, for Esc.
    before: Option<Condition>,
}

/// The popup's width in cells, borders included.
const POPUP_WIDTH: u16 = 44;

impl ColumnFilter {
    pub fn new() -> Self {
        ColumnFilter::default()
    }

    /// The column the popup is open on, if it is open.
    pub fn column(&self) -> Option<Key> {
        self.open.as_ref().map(|o| o.key)
    }

    /// Opens on the focused column; returns whether it did.
    pub fn open(&mut self, snapshot: &Snapshot) -> bool {
        match snapshot.focus(Axis::Cols).and_then(|f| f.key) {
            Some(key) => self.open_column(snapshot, key),
            None => false,
        }
    }

    /// Opens on column `key` if it takes Text conditions; returns whether it did.
    pub fn open_column(&mut self, snapshot: &Snapshot, key: Key) -> bool {
        let Some(support) = snapshot.support(Axis::Rows, key).text else {
            return false;
        };
        let current = snapshot
            .conditions(Axis::Rows)
            .into_iter()
            .find(|(c, state)| {
                *state != ConditionState::Removing
                    && c.key == key
                    && matches!(c.kind, ConditionKind::Text(_))
            })
            .map(|(c, _)| c);
        let query = match current.as_ref().map(|c| &c.kind) {
            Some(ConditionKind::Text(q)) => Query {
                text: q.text.clone(),
                case_sensitive: q.case_sensitive,
                regex: q.regex && support.regex,
            },
            _ => Query::default(),
        };
        self.open = Some(Open {
            key,
            query,
            not: current.as_ref().is_some_and(|c| c.not),
            regex: support.regex,
            sent: current.clone(),
            before: current,
        });
        true
    }

    /// Takes a key press while open, editing `grid`'s rows conditions as the
    /// condition changes; false (taking nothing) when closed.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        let Some(o) = &mut self.open else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let q = &mut o.query;
        let alt = event.modifiers == KeyModifiers::ALT;
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        match event.code {
            KeyCode::Enter => self.open = None,
            KeyCode::Esc => {
                match (o.sent.take(), o.before.take()) {
                    (Some(from), Some(to)) => grid.replace_condition(Axis::Rows, &from, to),
                    (Some(from), None) => grid.remove_condition(Axis::Rows, &from),
                    (None, Some(to)) => grid.add_condition(Axis::Rows, to),
                    (None, None) => {}
                }
                self.open = None;
            }
            KeyCode::Char('n') if alt => {
                o.not = !o.not;
                o.apply(grid);
            }
            KeyCode::Char('c') if alt => {
                q.case_sensitive = !q.case_sensitive;
                o.apply(grid);
            }
            KeyCode::Char('r') if alt && o.regex => {
                q.regex = !q.regex;
                o.apply(grid);
            }
            KeyCode::Char('u') if ctrl => {
                q.text.clear();
                o.apply(grid);
            }
            KeyCode::Char(c) if KeyModifiers::SHIFT.contains(event.modifiers) => {
                q.text.push(c);
                o.apply(grid);
            }
            KeyCode::Backspace => {
                q.text.pop();
                o.apply(grid);
            }
            _ => {}
        }
        true
    }

    /// Draws the popup while open, under its column's header in the Grid's
    /// `area` (or at the body's left while the column is out of view),
    /// moved left to fit.
    pub fn render(
        &self,
        snapshot: &Snapshot,
        content: &impl Content,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let Some(o) = &self.open else {
            return;
        };
        let area = area.intersection(buf.area);
        let header = snapshot
            .headers(Axis::Cols)
            .find(|h| h.key == Some(o.key))
            .map_or(snapshot.header(Axis::Cols), |h| {
                h.rect.x.max(snapshot.header(Axis::Cols))
            });
        let width = POPUP_WIDTH.min(area.width);
        let height = 5.min(area.height);
        let left = (header.max(0.0).ceil() as u16).min(area.width - width);
        let top = (snapshot.header(Axis::Rows).ceil() as u16).min(area.height - height);
        let popup = Rect::new(area.x + left, area.y + top, width, height);
        if popup.width < 4 || popup.height < 3 {
            return;
        }
        let border = theme.body.patch(theme.border);
        let inner = usize::from(popup.width - 2);
        let mut put = |row: u16, x: &mut u16, text: &str, style: Style| {
            for ch in text.chars() {
                if *x >= popup.right() {
                    return;
                }
                buf[(*x, popup.y + row)].set_char(ch).set_style(style);
                *x += 1;
            }
        };
        let title = format!(" Filter {} ", content.column_header(o.key));
        let title = fit(&title, u32::try_from(inner).unwrap_or(0));
        let rule = inner.saturating_sub(char_count(&title) as usize + 1);
        let mut x = popup.x;
        put(0, &mut x, "┌─", border);
        put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
        put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);

        let how = if o.not {
            "Does not contain "
        } else {
            "Contains "
        };
        let mut toggles = vec![("Aa", o.query.case_sensitive)];
        if o.regex {
            toggles.push((".*", o.query.regex));
        }
        let right: usize = toggles.iter().map(|(t, _)| t.len() + 1).sum();
        let room = inner.saturating_sub(char_count(how) as usize + right + 2);
        let chars: Vec<char> = o.query.text.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(room.saturating_sub(1))..]
            .iter()
            .collect();
        let field = format!("{tail}▏");
        let field = format!("{field:<room$}");
        let mut x = popup.x;
        put(1, &mut x, "│ ", border);
        put(1, &mut x, how, theme.body.add_modifier(Modifier::BOLD));
        put(
            1,
            &mut x,
            &field,
            theme.body.add_modifier(Modifier::UNDERLINED),
        );
        put(1, &mut x, " ", theme.body);
        for (text, on) in toggles {
            put(1, &mut x, text, toggle_style(theme, on));
            put(1, &mut x, " ", theme.body);
        }
        put(1, &mut x, "│", border);

        // The OneOf checklist goes here, between the Text condition and the keys.
        let mut keys = String::from("Alt+N not  Alt+C case");
        if o.regex {
            keys += "  Alt+R regex";
        }
        let hints = [keys, "Enter keep  Esc undo  Ctrl+U clear".to_string()];
        for (i, hint) in hints.iter().enumerate().take(usize::from(popup.height) - 3) {
            let row = 2 + i as u16;
            let hint = fit(hint, u32::try_from(inner - 1).unwrap_or(0));
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            let pad = (inner - 1).saturating_sub(char_count(&hint) as usize);
            put(row, &mut x, &hint, theme.body.patch(theme.loading));
            put(row, &mut x, &" ".repeat(pad), theme.body);
            put(row, &mut x, "│", border);
        }
        let mut x = popup.x;
        let bottom = popup.height - 1;
        put(bottom, &mut x, &format!("└{}┘", "─".repeat(inner)), border);
    }
}

impl Open {
    fn apply(&mut self, grid: &mut Grid) {
        let q = &self.query;
        let query = TextQuery::new(q.text.clone())
            .case_sensitive(q.case_sensitive)
            .regex(q.regex);
        let to = Condition::text(self.key, query).not(self.not);
        match &self.sent {
            Some(from) => grid.replace_condition(Axis::Rows, from, to.clone()),
            None => grid.add_condition(Axis::Rows, to.clone()),
        }
        self.sent = (!q.text.is_empty()).then_some(to);
    }
}

fn toggle_style(theme: &Theme, on: bool) -> Style {
    if on {
        theme
            .sort_marker
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        theme.body.add_modifier(Modifier::DIM)
    }
}

/// Draws the rows' column conditions into `area`'s first line, after a
/// `Where` label: a chip for each, such as `port: not bergen` or
/// `name: /^an/ Aa`, dimmed with a `~` while being added and struck through
/// while being removed. Draws nothing while there are none.
pub fn render_conditions(
    snapshot: &Snapshot,
    content: &impl Content,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let area = area.intersection(buf.area);
    let listed = snapshot.conditions(Axis::Rows);
    if area.is_empty() || listed.is_empty() {
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
    put(
        &mut x,
        " Where ",
        theme.header.add_modifier(Modifier::REVERSED),
    );
    for (condition, state) in listed {
        put(&mut x, " ", theme.body);
        let mut chip = chip_text(&content.column_header(condition.key), &condition);
        let style = theme.sort_marker.add_modifier(Modifier::REVERSED);
        let style = match state {
            ConditionState::Applied => style,
            ConditionState::Adding => {
                chip.push('~');
                style.add_modifier(Modifier::DIM)
            }
            _ => style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT),
        };
        put(&mut x, &format!(" {chip} "), style);
    }
}

/// A chip's text: `label: text`, with `not ` before the text, the text in
/// slashes for a regex, and ` Aa` when it matches case.
fn chip_text(label: &str, c: &Condition) -> String {
    let ConditionKind::Text(q) = &c.kind else {
        return format!("{label}: …");
    };
    let text = if q.regex {
        format!("/{}/", q.text)
    } else {
        q.text.clone()
    };
    let not = if c.not { "not " } else { "" };
    let case = if q.case_sensitive { " Aa" } else { "" };
    format!("{label}: {not}{text}{case}")
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
