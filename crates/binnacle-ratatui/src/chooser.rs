//! The chooser: the find and filter checklist over an axis's own items, with
//! a search, a tick for each item shown and none for each hidden. A tick
//! shows the item, an untick hides it, at once. The columns the rows group
//! by have left the body: the columns' chooser lists them last, tagged
//! "grouped", with no tick.

use std::ops::Range;

use binnacle_core::{Axis, Edge, Grid, Key, Position, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::filter::{count, field_text, grouped, label};
use crate::menu::cap_hint;
use crate::renderer::{char_count, fit};
use crate::{Content, Theme};

/// The popup's width in cells, borders included.
const WIDTH: u16 = 40;
/// How many shown items the chooser lists; it says so when there are more.
const LIMIT: u64 = 1000;
/// The most list rows the popup draws.
const LIST_ROWS: usize = 12;

/// The chooser of an axis whose Table Source hides: every item shown (the
/// first [`LIMIT`] of them, in the axis's order) then every hidden one, each
/// ticked while shown. The host hands it the items with
/// [`Chooser::fetch`] after opening it.
///
/// While open it takes every key: characters search the items' labels,
/// Backspace and Ctrl+U edit the search, Up, Down, PageUp, PageDown, Home
/// and End move, Space ticks the item under the cursor, or the head of the
/// list ("(Show all)", or "(Show all matching …)" while searching) to show
/// or hide every item listed, and Enter or Esc close it. A hide past the
/// axis's cap hides nothing and says to filter instead. The list keeps the
/// order it opened with, so items do not jump as they are ticked.
#[derive(Clone, Debug, Default)]
pub struct Chooser {
    open: Option<Open>,
}

#[derive(Clone, Debug)]
struct Open {
    axis: Axis,
    /// The items and their labels, `None` until fetched.
    items: Option<Vec<(Key, String)>>,
    /// Whether there were more shown items than listed.
    partial: bool,
    search: String,
    /// The list row under the cursor: 0 is the head.
    cursor: usize,
    /// Why the last untick hid nothing.
    hint: Option<String>,
}

/// The fields the rows group by, which the columns' chooser lists as
/// grouped: none on the rows' chooser.
fn grouped_by(snapshot: &Snapshot, axis: Axis) -> Vec<Key> {
    let view = snapshot.requested_view().unwrap_or(snapshot.applied_view());
    match (axis, &view.grouping) {
        (Axis::Cols, Some(g)) => g
            .levels(Axis::Rows)
            .iter()
            .filter_map(|l| l.field_key())
            .collect(),
        _ => Vec::new(),
    }
}

impl Open {
    /// The listed items the search matches, ignoring case.
    fn matching(&self) -> Vec<(Key, &str)> {
        let search = self.search.to_lowercase();
        (self.items.iter().flatten())
            .filter(|(_, l)| l.to_lowercase().contains(&search))
            .map(|(k, l)| (*k, l.as_str()))
            .collect()
    }

    fn hide(&mut self, grid: &mut Grid, keys: &[Key]) {
        self.hint = match grid.hide(self.axis, keys) {
            Ok(()) => None,
            Err(capped) => Some(cap_hint(self.axis, capped.cap)),
        };
    }
}

impl Chooser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// The axis it is open on, if it is.
    pub fn axis(&self) -> Option<Axis> {
        self.open.as_ref().map(|o| o.axis)
    }

    /// Opens the chooser of `axis`: `false` if its Table Source does not hide.
    pub fn open(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        if !snapshot.hide_support(axis) {
            return false;
        }
        self.open = Some(Open {
            axis,
            items: None,
            partial: false,
            search: String::new(),
            cursor: 1,
            hint: None,
        });
        true
    }

    /// Closes it: `false` if it was not open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Hands it the items to list: the first time after opening, calls
    /// `keys_at` for the Keys of the shown items, labelled by `content`;
    /// later, adds items hidden since (from elsewhere) at the end. Call it
    /// before each render while open.
    pub fn fetch(
        &mut self,
        snapshot: &Snapshot,
        content: &impl Content,
        keys_at: impl FnOnce(Axis, Range<Position>) -> Vec<Key>,
    ) {
        let Some(o) = &mut self.open else {
            return;
        };
        let axis = o.axis;
        // `label` names an item of the other axis than the one it is given.
        let named = |key: Key| label(content, axis.other(), key);
        let items = o.items.get_or_insert_with(|| {
            let count = snapshot.count(axis);
            o.partial = count > LIMIT;
            let shown = keys_at(axis, 0..count.min(LIMIT));
            shown.into_iter().map(|k| (k, named(k))).collect()
        });
        for key in (snapshot.hidden(axis).iter().copied()).chain(grouped_by(snapshot, axis)) {
            if !items.iter().any(|(k, _)| *k == key) {
                items.push((key, named(key)));
            }
        }
    }

    /// Takes a key press while open: `true` if it did, which is every key.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        let Some(o) = &mut self.open else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let snapshot = grid.snapshot();
        let hidden: Vec<Key> = snapshot.hidden(o.axis).to_vec();
        let grouped = grouped_by(&snapshot, o.axis);
        let listed: Vec<Key> = o.matching().into_iter().map(|(k, _)| k).collect();
        let last = listed.len();
        // The head ticks every item listed but the grouped ones.
        let matching: Vec<Key> = (listed.iter().copied())
            .filter(|k| !grouped.contains(k))
            .collect();
        let plain = KeyModifiers::SHIFT.contains(event.modifiers);
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        let page = LIST_ROWS.saturating_sub(1);
        match event.code {
            KeyCode::Esc | KeyCode::Enter => return self.close(),
            KeyCode::Up => o.cursor = o.cursor.saturating_sub(1),
            KeyCode::Down => o.cursor = (o.cursor + 1).min(last),
            KeyCode::PageUp => o.cursor = o.cursor.saturating_sub(page),
            KeyCode::PageDown => o.cursor = (o.cursor + page).min(last),
            KeyCode::Home => o.cursor = 0,
            KeyCode::End => o.cursor = last,
            KeyCode::Char(' ') if plain && o.cursor == 0 => {
                if matching.iter().all(|k| !hidden.contains(k)) {
                    o.hide(grid, &matching);
                } else {
                    grid.show(o.axis, &matching);
                    o.hint = None;
                }
            }
            KeyCode::Char(' ') if plain => {
                if let Some(&key) = listed.get(o.cursor - 1).filter(|k| !grouped.contains(k)) {
                    if hidden.contains(&key) {
                        grid.show(o.axis, &[key]);
                        o.hint = None;
                    } else {
                        o.hide(grid, &[key]);
                    }
                }
            }
            KeyCode::Char('u') if ctrl => {
                o.search.clear();
                o.cursor = 1;
            }
            KeyCode::Char(c) if plain => {
                o.search.push(c);
                o.cursor = 1;
            }
            KeyCode::Backspace => {
                o.search.pop();
                o.cursor = 1;
            }
            _ => {}
        }
        true
    }

    /// Draws the open chooser in `area` (the table's), at the body's top
    /// left, as tall as its list needs and `area` allows: the search, the
    /// count of items listed, the head and the items (pinned ones tagged),
    /// how many are shown, any hint, and the keys.
    pub fn render(&self, snapshot: &Snapshot, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let area = area.intersection(buf.area);
        let Some(o) = &self.open else {
            return;
        };
        let (noun, nouns) = match o.axis {
            Axis::Cols => ("column", "columns"),
            Axis::Rows => ("row", "rows"),
        };
        let hidden = snapshot.hidden(o.axis);
        let pinned: Vec<Key> = (snapshot.pinned(o.axis, Edge::Leading).iter())
            .filter_map(|i| i.key)
            .collect();
        let grouped_by = grouped_by(snapshot, o.axis);
        let matching = o.matching();
        // Borders, the search, the rule, the shown count, any hint, the keys.
        let fixed = 2 + 1 + 1 + 1 + u16::from(o.hint.is_some()) + 1;
        let left = snapshot.header(Axis::Cols).ceil() as u16;
        let top = snapshot.header(Axis::Rows).ceil() as u16;
        let room = usize::from(area.height.saturating_sub(top + fixed));
        let list = (matching.len() + 1).min(LIST_ROWS).min(room).max(1);
        let width = WIDTH.min(area.width);
        let height = (fixed + list as u16).min(area.height);
        if width < 12 || height < fixed {
            return;
        }
        let left = left.min(area.width - width);
        let top = top.min(area.height - height);
        let popup = Rect::new(area.x + left, area.y + top, width, height);
        let border = theme.body.patch(theme.border);
        let inner = usize::from(width - 2);
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
        let title = format!(" Choose {nouns} ");
        let rule = inner.saturating_sub(char_count(&title) as usize + 1);
        let mut x = popup.x;
        put(0, &mut x, "┌─", border);
        put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
        put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);
        let mut row = 1;

        let search = "Search ";
        let field = field_text(
            &o.search,
            inner.saturating_sub(char_count(search) as usize + 2),
            true,
        );
        let mut x = popup.x;
        put(row, &mut x, "│ ", border);
        put(row, &mut x, search, theme.body.add_modifier(Modifier::BOLD));
        put(
            row,
            &mut x,
            &field,
            theme.body.add_modifier(Modifier::UNDERLINED),
        );
        put(row, &mut x, " │", border);
        row += 1;

        let listed = match &o.items {
            None => String::new(),
            Some(_) if o.partial => {
                format!(" first {} shown; type to search ", grouped(LIMIT))
            }
            Some(items) => format!(" {} {nouns} ", grouped(items.len() as u64)),
        };
        let listed = fit(&listed, u32::try_from(inner.saturating_sub(2)).unwrap_or(0));
        let mut x = popup.x;
        put(row, &mut x, "├─", border);
        put(row, &mut x, &listed, theme.body.patch(theme.loading));
        let rest = inner.saturating_sub(1 + char_count(&listed) as usize);
        put(row, &mut x, &format!("{}┤", "─".repeat(rest)), border);
        row += 1;

        let width = inner.saturating_sub(2);
        let start = (o.cursor + 1).saturating_sub(list);
        for i in start..start + list {
            let (mark, text, tag, style) = if i == 0 {
                let ticks = || matching.iter().filter(|(k, _)| !grouped_by.contains(k));
                let all = ticks().all(|(k, _)| !hidden.contains(k));
                let some = ticks().any(|(k, _)| !hidden.contains(k));
                let mark = match (all, some) {
                    (true, _) => "[x]",
                    (false, true) => "[-]",
                    _ => "[ ]",
                };
                let text = match o.search.as_str() {
                    "" => "(Show all)".to_string(),
                    s => format!("(Show all matching \"{s}\")"),
                };
                (mark, text, "", theme.body.add_modifier(Modifier::BOLD))
            } else if let Some(&(key, text)) = matching.get(i - 1) {
                let mark = match (grouped_by.contains(&key), hidden.contains(&key)) {
                    (true, _) => "   ",
                    (false, true) => "[ ]",
                    (false, false) => "[x]",
                };
                let named = match o.axis {
                    Axis::Cols => text.to_string(),
                    Axis::Rows => format!("{noun} {text}"),
                };
                let tag = match () {
                    _ if grouped_by.contains(&key) => "grouped",
                    _ if pinned.contains(&key) => "pinned",
                    _ => "",
                };
                (mark, named, tag, theme.body)
            } else if i == 1 && o.items.is_some() {
                let none = format!("No {nouns} match");
                ("   ", none, "", theme.body.patch(theme.loading))
            } else {
                ("   ", String::new(), "", theme.body)
            };
            let room = width.saturating_sub(4 + char_count(tag) as usize);
            let text = fit(&text, u32::try_from(room).unwrap_or(0));
            let pad = room.saturating_sub(char_count(&text) as usize);
            let style = match i == o.cursor {
                true => style.add_modifier(Modifier::REVERSED),
                false => style,
            };
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(row, &mut x, &format!("{mark} {text}"), style);
            put(row, &mut x, &" ".repeat(pad), theme.body);
            put(row, &mut x, tag, theme.body.patch(theme.loading));
            put(row, &mut x, " │", border);
            row += 1;
        }

        let mut lines = Vec::new();
        let shown = format!("{} shown", count(snapshot, o.axis, (noun, nouns)));
        lines.push((shown, theme.body.patch(theme.loading)));
        if let Some(hint) = &o.hint {
            lines.push((hint.clone(), theme.body.patch(theme.error)));
        }
        let keys = "Space tick  Enter, Esc close".to_string();
        lines.push((keys, theme.body.patch(theme.row_number)));
        for (text, style) in lines {
            let text = fit(&text, u32::try_from(inner - 1).unwrap_or(0));
            let pad = (inner - 1).saturating_sub(char_count(&text) as usize);
            let mut x = popup.x;
            put(row, &mut x, "│ ", border);
            put(row, &mut x, &text, style);
            put(row, &mut x, &" ".repeat(pad), theme.body);
            put(row, &mut x, "│", border);
            row += 1;
        }
        let mut x = popup.x;
        put(row, &mut x, &format!("└{}┘", "─".repeat(inner)), border);
    }
}
