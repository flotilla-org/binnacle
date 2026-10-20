//! The header menu: a popup under the focused column's header (or under the
//! focused row) with Hide, Show only these and Choose, then the axis's
//! hidden items inline, numbered, and Show all, each shown again in its
//! place. Hide and Show only act on the picks or the selection as well as
//! the focused item, and past the axis's hide cap suggest a filter.

use std::ops::Range;

use binnacle_core::{Axis, Grid, Key, Position, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::filter::{grouped, label};
use crate::renderer::{char_count, fit};
use crate::{Content, Theme};

const WIDTH: u16 = 40;
/// How many hidden items show at once; the list scrolls past them.
const LIST_ROWS: usize = 8;

/// Hides the focused item of `axis` ([`crate::Control::Hide`]): `false` if
/// the axis does not hide, nothing there is focused, or the axis's hidden
/// items would pass its cap.
pub fn hide_focused(grid: &mut Grid, axis: Axis) -> bool {
    let snapshot = grid.snapshot();
    let key = snapshot.focus(axis).and_then(|f| f.key);
    match key.filter(|_| snapshot.hide_support(axis)) {
        Some(key) => grid.hide(axis, &[key]).is_ok(),
        None => false,
    }
}

/// What a hide or "show only" did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hid {
    /// Asked the Table Source to hide this many items.
    Hidden(usize),
    /// Nothing hidden: the axis's hidden items would number more than its
    /// cap, so the host suggests a filter ([`cap_hint`]).
    Capped(usize),
    /// Nothing to hide, or the axis does not hide.
    Nothing,
}

/// The items of `axis` that Hide and "show only" act on: the picks if there
/// are any, else the items the selection spans, else the focused item. A
/// selection comes as runs of Positions, since the Grid knows Keys only in
/// its Window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chosen {
    Keys(Vec<Key>),
    Runs(Vec<Range<Position>>),
}

impl Chosen {
    /// What `snapshot` has chosen on `axis`, if anything.
    pub fn of(snapshot: &Snapshot, axis: Axis) -> Option<Chosen> {
        let picked = snapshot.picked(axis);
        if !picked.is_empty() {
            return Some(Chosen::Keys(picked.to_vec()));
        }
        if let Some(range) = snapshot.selection_range()
            && range.len(axis) > 1
        {
            return Some(Chosen::Runs(range.span(axis).to_vec()));
        }
        let key = snapshot.focus(axis).and_then(|f| f.key)?;
        Some(Chosen::Keys(vec![key]))
    }

    /// How many items it names.
    pub fn len(&self) -> u64 {
        match self {
            Chosen::Keys(keys) => keys.len() as u64,
            Chosen::Runs(runs) => runs.iter().map(|r| r.end - r.start).sum(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn keys(
        self,
        axis: Axis,
        mut keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
    ) -> Vec<Key> {
        match self {
            Chosen::Keys(keys) => keys,
            Chosen::Runs(runs) => runs.into_iter().flat_map(|r| keys_at(axis, r)).collect(),
        }
    }
}

/// Hides the chosen items of `axis` ([`Chosen`], [`crate::Control::Hide`]).
/// `keys_at` gives the Keys at a run of Positions, from the Table Source;
/// it is asked only for a selection within the axis's hide cap.
pub fn hide_chosen(
    grid: &mut Grid,
    axis: Axis,
    keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
) -> Hid {
    let snapshot = grid.snapshot();
    let Some(chosen) = Chosen::of(&snapshot, axis).filter(|_| snapshot.hide_support(axis)) else {
        return Hid::Nothing;
    };
    let cap = snapshot.hide_cap(axis);
    if snapshot.hidden(axis).len() as u64 + chosen.len() > cap as u64 {
        return Hid::Capped(cap);
    }
    let keys = chosen.keys(axis, keys_at);
    hide(grid, axis, &keys)
}

/// Shows only the chosen items of `axis`: hides every other item shown at
/// that moment, so items arriving later are shown. `keys_at` gives the Keys
/// at a run of Positions; it is asked only within the axis's hide cap.
pub fn show_only_chosen(
    grid: &mut Grid,
    axis: Axis,
    keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
) -> Hid {
    let snapshot = grid.snapshot();
    let Some(chosen) = Chosen::of(&snapshot, axis).filter(|_| snapshot.hide_support(axis)) else {
        return Hid::Nothing;
    };
    let cap = snapshot.hide_cap(axis) as u64;
    let others = snapshot.count(axis).saturating_sub(chosen.len());
    if snapshot.hidden(axis).len() as u64 + others > cap {
        return Hid::Capped(cap as usize);
    }
    let mut keys_at = keys_at;
    let count = snapshot.count(axis);
    // A selection's others lie between its runs; picks are within the cap of
    // all there is.
    let (keep, gaps) = match chosen {
        Chosen::Keys(keys) => (keys, std::iter::once(0..count).collect()),
        Chosen::Runs(runs) => {
            let mut gaps = Vec::new();
            let mut at = 0;
            for r in runs {
                gaps.push(at..r.start);
                at = r.end;
            }
            gaps.push(at..count);
            (Vec::new(), gaps)
        }
    };
    let others: Vec<Key> = (gaps.into_iter())
        .filter(|g| g.start < g.end)
        .flat_map(|g| keys_at(axis, g))
        .filter(|k| !keep.contains(k))
        .collect();
    hide(grid, axis, &others)
}

fn hide(grid: &mut Grid, axis: Axis, keys: &[Key]) -> Hid {
    match grid.hide(axis, keys) {
        Ok(()) if keys.is_empty() => Hid::Nothing,
        Ok(()) => Hid::Hidden(keys.len()),
        Err(capped) => Hid::Capped(capped.cap),
    }
}

/// What to tell the user when a hide on `axis` passed its cap: to filter
/// instead, since a filter scales.
pub fn cap_hint(axis: Axis, cap: usize) -> String {
    let noun = match (axis, cap) {
        (Axis::Cols, 1) => "column",
        (Axis::Cols, _) => "columns",
        (Axis::Rows, 1) => "row",
        (Axis::Rows, _) => "rows",
    };
    format!("Can't hide over {} {noun}: filter", grouped(cap as u64))
}

/// The header menu of a column or row, on an axis whose Table Source hides.
/// [`HeaderMenu::open`] drops it under the focused item's header with the
/// cursor on Hide; [`HeaderMenu::open_hidden`] with it on the first hidden
/// item. While open it takes the keys: Up and Down move the cursor, Enter
/// does what it is on, `h` hides the chosen items ([`Chosen`]), `o` shows
/// only them, `c` asks for the chooser ([`HeaderMenu::take_chooser`]), `1`
/// to `9` show that hidden item, `a` shows them all, Esc closes. Hiding
/// closes it; showing leaves it open, and so does a hide past the cap,
/// which says to filter instead.
#[derive(Clone, Debug, Default)]
pub struct HeaderMenu {
    open: Option<Open>,
    /// The axis whose chooser the menu asked for, until the host takes it.
    chooser: Option<Axis>,
}

#[derive(Clone, Debug)]
struct Open {
    /// The item's own axis: the columns for a column's menu.
    axis: Axis,
    key: Key,
    /// 0 is Hide, 1 Show only these, 2 Choose, then each hidden item, then
    /// Show all.
    cursor: usize,
    /// Why the last hide hid nothing.
    hint: Option<String>,
}

/// The menu's lines before the hidden items.
const ACTIONS: usize = 3;

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
        self.open_at(snapshot, axis, ACTIONS)
    }

    fn open_at(&mut self, snapshot: &Snapshot, axis: Axis, cursor: usize) -> bool {
        let key = snapshot.focus(axis).and_then(|f| f.key);
        let Some(key) = key.filter(|_| snapshot.hide_support(axis)) else {
            return false;
        };
        let cursor = cursor.min(snapshot.hidden(axis).len() + ACTIONS);
        let hint = None;
        self.open = Some(Open {
            axis,
            key,
            cursor,
            hint,
        });
        true
    }

    /// Closes the menu: `false` if it was not open.
    pub fn close(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// The axis whose chooser the user picked from the menu, once: the host
    /// opens its [`crate::Chooser`] there.
    pub fn take_chooser(&mut self) -> Option<Axis> {
        self.chooser.take()
    }

    /// Handles a key press while open: `true` if the menu took it. Every
    /// key is taken while it is open, so none reaches the table. `keys_at`
    /// gives the Keys at a run of Positions from the Table Source, for a
    /// hide of the selection ([`hide_chosen`]).
    pub fn key(
        &mut self,
        event: &KeyEvent,
        grid: &mut Grid,
        keys_at: impl FnMut(Axis, Range<Position>) -> Vec<Key>,
    ) -> bool {
        let Some(Open { axis, cursor, .. }) = self.open.clone() else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let hidden: Vec<Key> = grid.snapshot().hidden(axis).to_vec();
        // The actions, the hidden items, then Show all while there are any.
        let last = match hidden.is_empty() {
            true => ACTIONS - 1,
            false => hidden.len() + ACTIONS,
        };
        let cursor = cursor.min(last);
        let mut moved = cursor;
        let mut act = None;
        match event.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Up => moved = cursor.checked_sub(1).unwrap_or(last),
            KeyCode::Down => moved = if cursor >= last { 0 } else { cursor + 1 },
            KeyCode::Char('h') => act = Some(0),
            KeyCode::Char('o') => act = Some(1),
            KeyCode::Char('c') => act = Some(2),
            KeyCode::Char('a') if !hidden.is_empty() => act = Some(last),
            KeyCode::Char(c @ '1'..='9') => {
                let i = c as usize - '1' as usize;
                act = (i < hidden.len()).then_some(i + ACTIONS);
            }
            KeyCode::Enter => act = Some(cursor),
            _ => {}
        }
        let hid = match act {
            Some(0) => Some(hide_chosen(grid, axis, keys_at)),
            Some(1) => Some(show_only_chosen(grid, axis, keys_at)),
            Some(2) => {
                self.chooser = Some(axis);
                self.open = None;
                return true;
            }
            Some(i) if i >= ACTIONS && i - ACTIONS < hidden.len() => {
                grid.show(axis, &[hidden[i - ACTIONS]]);
                None
            }
            Some(_) => {
                grid.show(axis, &hidden);
                None
            }
            None => None,
        };
        match hid {
            Some(Hid::Hidden(_)) => self.open = None,
            Some(Hid::Capped(cap)) => {
                if let Some(o) = &mut self.open {
                    o.hint = Some(cap_hint(axis, cap));
                }
            }
            _ => {}
        }
        if let Some(o) = &mut self.open {
            o.cursor = moved;
        }
        true
    }

    /// Draws the open menu in `area` (the table's), under the column's
    /// header or under the row, moved to fit: the actions (Hide and Show
    /// only naming how many items they reach), the hidden items numbered
    /// (scrolled to the cursor past [`LIST_ROWS`]), Show all, a hint after a
    /// hide past the cap, and the keys.
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
            return;
        };
        // `label` names an item of the other axis than the one it is given.
        let name = |key: Key| label(content, o.axis.other(), key);
        let (noun, nouns) = match o.axis {
            Axis::Cols => ("column", "columns"),
            Axis::Rows => ("row", "rows"),
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
        // Borders, the actions, the heading, the list and Show all, any
        // hint, the keys.
        let height = 2
            + ACTIONS as u16
            + 1
            + list as u16
            + u16::from(!hidden.is_empty())
            + u16::from(o.hint.is_some())
            + 1;
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
        let chosen = Chosen::of(snapshot, o.axis).map_or(1, |c| c.len());
        let title = match chosen {
            1 => format!(" {} {} ", capital(noun), name(o.key)),
            n => format!(" {} {nouns} chosen ", grouped(n)),
        };
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
        let these = match chosen {
            1 => format!("this {noun}"),
            n => format!("these {}", grouped(n)),
        };
        let actions = [
            match chosen {
                1 => format!("h Hide {noun}"),
                n => format!("h Hide {} {nouns}", grouped(n)),
            },
            format!("o Show only {these}"),
            format!("c Choose {nouns}…"),
        ];
        for (i, action) in actions.iter().enumerate() {
            let (mark, style) = at(i);
            line(row, &format!("{mark}{action}"), style);
            row += 1;
        }
        let heading = match hidden.len() {
            0 => format!(" No hidden {nouns}"),
            n => format!(" Hidden {nouns} ({n}):"),
        };
        line(row, &heading, theme.body.patch(theme.row_number));
        row += 1;
        // The window of the list that keeps the cursor in view.
        let first = (o.cursor.saturating_sub(ACTIONS))
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
            let (mark, style) = at(i + ACTIONS);
            line(row, &format!("{mark}{number} {named}"), style);
            row += 1;
        }
        if !hidden.is_empty() {
            let (mark, style) = at(hidden.len() + ACTIONS);
            line(row, &format!("{mark}a Show all"), style);
            row += 1;
        }
        if let Some(hint) = &o.hint {
            line(row, &format!(" {hint}"), theme.body.patch(theme.error));
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
