//! The header menu: a popup under the focused column's header (or under the
//! focused row) with Hide, then the axis's hidden items inline, numbered,
//! and Show all, each shown again in its place.

use binnacle_core::{Axis, Grid, Key, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::filter::label;
use crate::renderer::{char_count, fit};
use crate::{Content, Theme};

const WIDTH: u16 = 32;
/// How many hidden items show at once; the list scrolls past them.
const LIST_ROWS: usize = 8;

/// Hides the focused item of `axis` ([`crate::Control::Hide`]): `false` if
/// the axis does not hide or nothing there is focused.
pub fn hide_focused(grid: &mut Grid, axis: Axis) -> bool {
    let snapshot = grid.snapshot();
    let key = snapshot.focus(axis).and_then(|f| f.key);
    match key.filter(|_| snapshot.hide_support(axis)) {
        Some(key) => {
            grid.hide(axis, &[key]);
            true
        }
        None => false,
    }
}

/// The header menu of a column or row, on an axis whose Table Source hides.
/// [`HeaderMenu::open`] drops it under the focused item's header with the
/// cursor on Hide; [`HeaderMenu::open_hidden`] with it on the first hidden
/// item. While open it takes the keys: Up and Down move the cursor, Enter
/// does what it is on, `h` hides the item, `1` to `9` show that hidden item,
/// `a` shows them all, Esc closes. Hiding closes it; showing leaves it open.
#[derive(Clone, Debug, Default)]
pub struct HeaderMenu {
    open: Option<Open>,
}

#[derive(Clone, Copy, Debug)]
struct Open {
    /// The item's own axis: the columns for a column's menu.
    axis: Axis,
    key: Key,
    /// 0 is Hide, then each hidden item, then Show all.
    cursor: usize,
}

impl HeaderMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Opens the menu of the focused item of `axis` (`Axis::Cols` for the
    /// focused column's): `false` if the axis does not hide or nothing there
    /// is focused.
    pub fn open(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        self.open_at(snapshot, axis, 0)
    }

    /// Opens the menu with the cursor on the first hidden item, as
    /// [`HeaderMenu::open`] does.
    pub fn open_hidden(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        self.open_at(snapshot, axis, 1)
    }

    fn open_at(&mut self, snapshot: &Snapshot, axis: Axis, cursor: usize) -> bool {
        let key = snapshot.focus(axis).and_then(|f| f.key);
        let Some(key) = key.filter(|_| snapshot.hide_support(axis)) else {
            return false;
        };
        let cursor = cursor.min(snapshot.hidden(axis).len() + 1);
        self.open = Some(Open { axis, key, cursor });
        true
    }

    /// Closes the menu: `false` if it was not open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Handles a key press while open: `true` if the menu took it. Every
    /// key is taken while it is open, so none reaches the table.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        let Some(Open { axis, key, cursor }) = self.open else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let hidden: Vec<Key> = grid.snapshot().hidden(axis).to_vec();
        // Hide, the hidden items, then Show all while there are any.
        let last = if hidden.is_empty() {
            0
        } else {
            hidden.len() + 1
        };
        let cursor = cursor.min(last);
        let mut moved = cursor;
        let mut hide = false;
        match event.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Up => moved = cursor.checked_sub(1).unwrap_or(last),
            KeyCode::Down => moved = if cursor >= last { 0 } else { cursor + 1 },
            KeyCode::Char('h') => hide = true,
            KeyCode::Char('a') => grid.show(axis, &hidden),
            KeyCode::Char(c @ '1'..='9') => {
                if let Some(&k) = hidden.get(c as usize - '1' as usize) {
                    grid.show(axis, &[k]);
                }
            }
            KeyCode::Enter => match cursor {
                0 => hide = true,
                i if i <= hidden.len() => grid.show(axis, &[hidden[i - 1]]),
                _ => grid.show(axis, &hidden),
            },
            _ => {}
        }
        if hide {
            grid.hide(axis, &[key]);
            self.open = None;
        } else if let Some(o) = &mut self.open {
            o.cursor = moved;
        }
        true
    }

    /// Draws the open menu in `area` (the table's), under the column's
    /// header or under the row, moved to fit: Hide, then the hidden items
    /// numbered (scrolled to the cursor past [`LIST_ROWS`]), Show all, and
    /// the keys.
    pub fn render(
        &self,
        snapshot: &Snapshot,
        content: &impl Content,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let area = area.intersection(buf.area);
        let Some(o) = self.open else {
            return;
        };
        // `label` names an item of the other axis than the one it is given.
        let name = |key: Key| label(content, o.axis.other(), key);
        let noun = match o.axis {
            Axis::Cols => "column",
            Axis::Rows => "row",
        };
        let hidden = snapshot.hidden(o.axis);
        let (left, top) = match o.axis {
            Axis::Cols => {
                let x = (snapshot.headers(Axis::Cols))
                    .find(|h| h.key == Some(o.key))
                    .map_or(snapshot.header(Axis::Cols), |h| {
                        h.rect.x.max(snapshot.header(Axis::Cols))
                    });
                (x, snapshot.header(Axis::Rows))
            }
            Axis::Rows => {
                let y = (snapshot.headers(Axis::Rows))
                    .find(|h| h.key == Some(o.key))
                    .map_or(0.0, |h| h.rect.y + h.rect.height);
                (
                    snapshot.header(Axis::Cols),
                    y.max(snapshot.header(Axis::Rows)),
                )
            }
        };
        let list = hidden.len().min(LIST_ROWS);
        // Borders, Hide, the heading, the list and Show all, the keys.
        let height = 2 + 1 + 1 + list as u16 + u16::from(!hidden.is_empty()) + 1;
        let width = WIDTH.min(area.width);
        let height = height.min(area.height);
        if width < 8 || height < 4 {
            return;
        }
        let left = (left.max(0.0).ceil() as u16).min(area.width - width);
        let top = (top.max(0.0).ceil() as u16).min(area.height - height);
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
        let title = format!(" {} {} ", capital(noun), name(o.key));
        let title = fit(&title, u32::try_from(inner.saturating_sub(1)).unwrap_or(0));
        let rule = inner.saturating_sub(char_count(&title) as usize + 1);
        let mut x = popup.x;
        put(0, &mut x, "┌─", border);
        put(0, &mut x, &title, theme.body.add_modifier(Modifier::BOLD));
        put(0, &mut x, &format!("{}┐", "─".repeat(rule)), border);
        let mut row = 1;
        let mut line = |row: u16, text: &str, style: Style| {
            let mut x = popup.x;
            put(row, &mut x, "│", border);
            let text = fit(text, u32::try_from(inner).unwrap_or(0));
            let pad = inner.saturating_sub(char_count(&text) as usize);
            put(row, &mut x, &format!("{text}{}", " ".repeat(pad)), style);
            put(row, &mut x, "│", border);
        };
        // The cursor's line is reversed and marked, for terminals without colour.
        let at = |i: usize| match o.cursor == i {
            true => ('▸', theme.body.add_modifier(Modifier::REVERSED)),
            false => (' ', theme.body),
        };
        let (mark, style) = at(0);
        line(row, &format!("{mark}h Hide {noun}"), style);
        row += 1;
        let heading = match hidden.len() {
            0 => format!(" No hidden {noun}s"),
            n => format!(" Hidden {noun}s ({n}):"),
        };
        line(row, &heading, theme.body.patch(theme.row_number));
        row += 1;
        // The window of the list that keeps the cursor in view.
        let first = (o.cursor.saturating_sub(1))
            .saturating_sub(list.saturating_sub(1))
            .min(hidden.len().saturating_sub(list));
        for (i, &key) in hidden.iter().enumerate().skip(first).take(list) {
            let number = if i < 9 {
                format!("{}", i + 1)
            } else {
                " ".into()
            };
            // A row's label is its number, so it is named a row.
            let named = match o.axis {
                Axis::Cols => name(key),
                Axis::Rows => format!("row {}", name(key)),
            };
            let (mark, style) = at(i + 1);
            line(row, &format!("{mark}{number} {named}"), style);
            row += 1;
        }
        if !hidden.is_empty() {
            let (mark, style) = at(hidden.len() + 1);
            line(row, &format!("{mark}a Show all"), style);
            row += 1;
        }
        let keys = "↑↓ Enter  Esc close";
        line(row, &format!(" {keys}"), theme.body.patch(theme.row_number));
        row += 1;
        let mut x = popup.x;
        put(row, &mut x, &format!("└{}┘", "─".repeat(inner)), border);
    }
}

fn capital(noun: &str) -> String {
    let mut chars = noun.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
        .unwrap_or_default()
}
