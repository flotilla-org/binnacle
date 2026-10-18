//! Find and filter controls: the keys that reach them, by preset, the quick
//! filter field, the column filter popup and the conditions line.

use std::collections::HashMap;

use binnacle_core::{
    Axis, Blanks, Condition, ConditionKind, ConditionState, Distinct, DistinctQuery, FilterSpec,
    Grid, Key, Snapshot, TextQuery, TextSupport, Value,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::checklist::{Column, Shown};
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

/// A column's filter popup, dropped under the column header. Where the
/// column takes Text conditions, its Text condition: "Contains" or "Does not
/// contain", with its text and the case (`Aa`) and regex (`.*`, where the
/// column takes regexes) toggles. Where it takes OneOf conditions, the
/// checklist of its values: a search, "(Select all)" (or "(Select all
/// matching …)" while searching), each value with its count, "(Blanks)"
/// first, and notes on whether the list is capped and narrowed. Each change
/// edits the column's conditions at once, the checklist's by the rules of
/// the list-of-values filter (#107); the host hands it the values with
/// [`ColumnFilter::fetch`].
///
/// While open it takes every key. Tab and Shift+Tab move between the text,
/// the search and the list. In the text or the search, characters and
/// Backspace edit and Ctrl+U clears; in the list, Up, Down, PageUp,
/// PageDown, Home and End move, Space ticks, and characters search. Ctrl+A
/// ticks the head of the list; Alt+N toggles not, Alt+C case, Alt+R regex.
/// Enter keeps the conditions and Esc puts back the ones from before; both
/// close it.
#[derive(Clone, Debug, Default)]
pub struct ColumnFilter {
    open: Option<Open>,
}

/// Where the popup's keys go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    Text,
    Search,
    List,
}

#[derive(Clone, Debug)]
struct Open {
    key: Key,
    /// How the column takes Text conditions, if it does.
    text: Option<TextSupport>,
    /// Whether the column takes OneOf conditions.
    one_of: bool,
    query: Query,
    not: bool,
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

    /// Opens on column `key` if it takes Text or OneOf conditions; returns
    /// whether it did.
    pub fn open_column(&mut self, snapshot: &Snapshot, key: Key) -> bool {
        let support = snapshot.support(Axis::Rows, key);
        if support.text.is_none() && !support.one_of {
            return false;
        }
        let asked: Vec<Condition> = (snapshot.conditions(Axis::Rows).into_iter())
            .filter(|(_, state)| *state != ConditionState::Removing)
            .map(|(c, _)| c)
            .collect();
        let column = Column::of(&asked, key);
        let mut open = Open {
            key,
            text: support.text,
            one_of: support.one_of,
            query: Query::default(),
            not: false,
            before: column.clone(),
            column,
            zone: if support.text.is_some() {
                Zone::Text
            } else {
                Zone::Search
            },
            search: String::new(),
            cursor: 0,
            scroll: 0,
            asked: None,
            listed: None,
        };
        open.show_text();
        self.open = Some(open);
        true
    }

    /// Hands the checklist its values: calls `ask` with the column and the
    /// question when the search, or the narrowing (the applied rows filter
    /// less the column's own conditions), changed since it last asked. A
    /// `None` answer lists nothing. Call it before each render while open.
    pub fn fetch(
        &mut self,
        snapshot: &Snapshot,
        ask: impl FnOnce(Key, &DistinctQuery) -> Option<Distinct>,
    ) {
        let Some(o) = &mut self.open else {
            return;
        };
        if !o.one_of {
            return;
        }
        let applied = &snapshot.applied_view().rows.filter;
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
        o.listed = ask(o.key, &query).map(Listed::from);
        o.asked = Some(question);
        o.cursor = o.cursor.min(o.rows().saturating_sub(1));
    }

    /// Takes a key press while open, editing `grid`'s rows conditions as
    /// they change; false (taking nothing) when closed.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        let Some(o) = &mut self.open else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let alt = event.modifiers == KeyModifiers::ALT;
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        let plain = KeyModifiers::SHIFT.contains(event.modifiers);
        let text = o.text.is_some();
        match event.code {
            KeyCode::Enter => self.open = None,
            KeyCode::Esc => {
                let (text, one_of) = (o.before.text.clone(), o.before.one_of.clone());
                o.column.write(grid, text, one_of);
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
            KeyCode::Char('u') if ctrl => o.edit(grid, |t| t.clear()),
            KeyCode::Char(' ') if plain && o.zone == Zone::List => o.tick_row(grid, o.cursor),
            KeyCode::Char(c) if plain => o.edit(grid, |t| t.push(c)),
            KeyCode::Backspace => o.edit(grid, |t| {
                t.pop();
            }),
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
        let mut hints = Vec::new();
        if let Some(t) = o.text {
            let mut keys = String::from("Alt+N not  Alt+C case");
            if t.regex {
                keys += "  Alt+R regex";
            }
            hints.push(keys);
        }
        if o.one_of {
            hints.push("Tab move  Space tick  Ctrl+A all".to_string());
        }
        hints.push("Enter keep  Esc undo  Ctrl+U clear".to_string());
        // Borders, the text row, the search, rule and note rows, the hints.
        let fixed = 2 + u16::from(o.text.is_some()) + 3 * u16::from(o.one_of) + hints.len() as u16;
        let top = (snapshot.header(Axis::Rows).ceil() as u16).min(area.height);
        let room = usize::from(area.height.saturating_sub(top + fixed));
        // The head and the values, a line saying there are none, or one
        // saying none were listed.
        let none = o.listed.as_ref().is_some_and(|l| l.values.is_empty());
        let list = if o.one_of {
            (o.rows() + usize::from(none)).clamp(1, LIST_ROWS).min(room)
        } else {
            0
        };
        let width = POPUP_WIDTH.min(area.width);
        let height = (fixed + list as u16).min(area.height);
        let left = (header.max(0.0).ceil() as u16).min(area.width - width);
        let top = top.min(area.height - height);
        let popup = Rect::new(area.x + left, area.y + top, width, height);
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
        let title = format!(" Filter {} ", content.column_header(o.key));
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
            let field = field_text(&o.query.text, room, o.zone == Zone::Text);
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(row, &mut x, how, theme.body.add_modifier(Modifier::BOLD));
            let underlined = theme.body.add_modifier(Modifier::UNDERLINED);
            put(row, &mut x, &field, underlined);
            put(row, &mut x, " ", theme.body);
            for (text, on) in toggles {
                put(row, &mut x, text, toggle_style(theme, on));
                put(row, &mut x, " ", theme.body);
            }
            put(row, &mut x, "│", border);
            row += 1;
        }

        if o.one_of {
            let label = "Search ";
            let room = inner.saturating_sub(char_count(label) as usize + 2);
            let field = field_text(&o.search, room, o.zone == Zone::Search);
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
                let style = if i == o.cursor && o.zone == Zone::List {
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

/// A field's text in `room` cells: the end of a long text then a caret
/// while focused, padded.
fn field_text(text: &str, room: usize, focused: bool) -> String {
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

    /// Edits the text or, elsewhere, the search with `f`.
    fn edit(&mut self, grid: &mut Grid, f: impl FnOnce(&mut String)) {
        if self.zone == Zone::Text {
            f(&mut self.query.text);
            self.apply_text(grid);
        } else if self.one_of {
            f(&mut self.search);
            self.cursor = 0;
            self.scroll = 0;
        }
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
