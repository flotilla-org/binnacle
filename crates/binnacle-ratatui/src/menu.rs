//! The header menu: a popup under the focused column's header (or under the
//! focused row) with Pin left and Pin right (Pin top and Pin bottom for a
//! row), or Unpin on a pinned item, and Freeze up to here; then, where the
//! axis hides, Hide, Show only these and Choose, the axis's hidden items
//! inline, numbered, and Show all, each shown again in its place. Hide and
//! Show only act on the picks or the selection as well as the focused item,
//! and past the axis's hide cap suggest a filter.

use std::ops::Range;

use binnacle_core::{Axis, Edge, Grid, Key, Position, Snapshot};
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

/// The header menu of a column or row. [`HeaderMenu::open`] drops it under
/// the focused item's header with the cursor on its first line;
/// [`HeaderMenu::open_hidden`], on an axis whose Table Source hides, with it
/// on the first hidden item. While open it takes the keys: Up and Down move
/// the cursor, Enter does what it is on, `l` and `r` pin a column left and
/// right (`t` and `b` a row to the top and bottom), `u` unpins it (offered
/// wherever it is pinned, clipped or not), `f` freezes up to it, `h` hides
/// the chosen items ([`Chosen`]), `o` shows only them, `c` asks for the
/// chooser ([`HeaderMenu::take_chooser`]), `1` to `9` show that hidden item,
/// `a` shows them all, Esc closes. Pinning, unpinning, freezing and hiding
/// close it; showing leaves it open, and so does a hide past the cap, which
/// says to filter instead.
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
    /// An index into [`entries`].
    cursor: usize,
    /// Why the last hide hid nothing.
    hint: Option<String>,
}

/// What a line of the menu does.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Entry {
    Pin(Edge),
    Unpin,
    Freeze,
    Hide,
    ShowOnly,
    Choose,
    Show(Key),
    ShowAll,
}

impl Entry {
    /// The key that picks it, but for a hidden item's number.
    fn key(self, axis: Axis) -> char {
        match (self, axis) {
            (Entry::Pin(Edge::Leading), Axis::Cols) => 'l',
            (Entry::Pin(Edge::Trailing), Axis::Cols) => 'r',
            (Entry::Pin(Edge::Leading), Axis::Rows) => 't',
            (Entry::Pin(Edge::Trailing), Axis::Rows) => 'b',
            (Entry::Unpin, _) => 'u',
            (Entry::Freeze, _) => 'f',
            (Entry::Hide, _) => 'h',
            (Entry::ShowOnly, _) => 'o',
            (Entry::Choose, _) => 'c',
            (Entry::Show(_) | Entry::ShowAll, _) => 'a',
        }
    }
}

/// The menu's lines for `key` on `axis`, in order, as the web's menu lists
/// them: Pin at the leading edge, or Unpin wherever it is pinned; Pin at the
/// trailing edge unless pinned; Freeze unless pinned at the trailing edge;
/// then, on an axis that hides, Hide, Show only and Choose, each hidden
/// item and Show all.
fn entries(snapshot: &Snapshot, axis: Axis, key: Key) -> Vec<Entry> {
    let pinned_at = |edge| (snapshot.pinned(axis, edge).iter()).any(|i| i.key == Some(key));
    let trailing = pinned_at(Edge::Trailing);
    let mut entries = if pinned_at(Edge::Leading) || trailing {
        vec![Entry::Unpin]
    } else {
        vec![Entry::Pin(Edge::Leading), Entry::Pin(Edge::Trailing)]
    };
    if !trailing {
        entries.push(Entry::Freeze);
    }
    if snapshot.hide_support(axis) {
        entries.extend([Entry::Hide, Entry::ShowOnly, Entry::Choose]);
        let hidden = snapshot.hidden(axis);
        entries.extend(hidden.iter().map(|&k| Entry::Show(k)));
        if !hidden.is_empty() {
            entries.push(Entry::ShowAll);
        }
    }
    entries
}

impl HeaderMenu {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// Opens the menu of the focused item of `axis` (`Axis::Cols` for the
    /// focused column's): `false` if nothing there is focused.
    pub fn open(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        let Some(key) = snapshot.focus(axis).and_then(|f| f.key) else {
            return false;
        };
        self.open = Some(Open {
            axis,
            key,
            cursor: 0,
            hint: None,
        });
        true
    }

    /// Opens the menu with the cursor on the first hidden item (or Choose,
    /// with none), as [`HeaderMenu::open`] does: `false` as well if the axis
    /// does not hide.
    pub fn open_hidden(&mut self, snapshot: &Snapshot, axis: Axis) -> bool {
        if !snapshot.hide_support(axis) || !self.open(snapshot, axis) {
            return false;
        }
        if let Some(o) = &mut self.open {
            let entries = entries(snapshot, axis, o.key);
            let choose = entries.iter().position(|&e| e == Entry::Choose);
            o.cursor = choose.map_or(0, |i| (i + 1).min(entries.len() - 1));
        }
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
        let Some(Open {
            axis, key, cursor, ..
        }) = self.open.clone()
        else {
            return false;
        };
        if event.kind == KeyEventKind::Release {
            return true;
        }
        let snapshot = grid.snapshot();
        let entries = entries(&snapshot, axis, key);
        let hidden: Vec<Key> = snapshot.hidden(axis).to_vec();
        let last = entries.len() - 1;
        let cursor = cursor.min(last);
        let mut moved = cursor;
        let mut act = None;
        match event.code {
            KeyCode::Esc => return self.close(),
            KeyCode::Up => moved = cursor.checked_sub(1).unwrap_or(last),
            KeyCode::Down => moved = if cursor >= last { 0 } else { cursor + 1 },
            KeyCode::Enter => act = Some(entries[cursor]),
            KeyCode::Char(c @ '1'..='9') => {
                act = (hidden.get(c as usize - '1' as usize)).map(|&k| Entry::Show(k));
            }
            KeyCode::Char(c) => {
                act = (entries.iter().copied())
                    .find(|&e| !matches!(e, Entry::Show(_)) && e.key(axis) == c);
            }
            _ => {}
        }
        let mut closes = false;
        let hid = match act {
            Some(Entry::Pin(edge)) => {
                grid.pin(axis, edge, key);
                closes = true;
                None
            }
            Some(Entry::Unpin) => {
                grid.unpin(axis, key);
                closes = true;
                None
            }
            Some(Entry::Freeze) => {
                grid.freeze(axis, key);
                closes = true;
                None
            }
            Some(Entry::Hide) => Some(hide_chosen(grid, axis, keys_at)),
            Some(Entry::ShowOnly) => Some(show_only_chosen(grid, axis, keys_at)),
            Some(Entry::Choose) => {
                self.chooser = Some(axis);
                self.open = None;
                return true;
            }
            Some(Entry::Show(k)) => {
                grid.show(axis, &[k]);
                None
            }
            Some(Entry::ShowAll) => {
                grid.show(axis, &hidden);
                None
            }
            None => None,
        };
        if closes {
            self.open = None;
        }
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
    /// header or under the row, moved to fit: the pin lines, then where the
    /// axis hides the actions (Hide and Show only naming how many items
    /// they reach), the hidden items numbered
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
        let entries = entries(snapshot, o.axis, o.key);
        let hides = entries.contains(&Entry::Hide);
        let hidden = if hides { snapshot.hidden(o.axis) } else { &[] };
        // The lines above the heading: the pin lines and the actions.
        let above = entries.len() - hidden.len() - usize::from(!hidden.is_empty());
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
        // Borders, the lines above the heading, the heading, the list and
        // Show all, any hint, the keys.
        let height = 2
            + above as u16
            + u16::from(hides)
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
        for (i, &entry) in entries.iter().take(above).enumerate() {
            let label = match entry {
                Entry::Pin(Edge::Leading) if o.axis == Axis::Cols => "Pin left".into(),
                Entry::Pin(Edge::Trailing) if o.axis == Axis::Cols => "Pin right".into(),
                Entry::Pin(Edge::Leading) => "Pin top".into(),
                Entry::Pin(Edge::Trailing) => "Pin bottom".into(),
                Entry::Unpin => "Unpin".into(),
                Entry::Freeze => "Freeze up to here".into(),
                Entry::Hide => match chosen {
                    1 => format!("Hide {noun}"),
                    n => format!("Hide {} {nouns}", grouped(n)),
                },
                Entry::ShowOnly => format!("Show only {these}"),
                Entry::Choose => format!("Choose {nouns}…"),
                Entry::Show(_) | Entry::ShowAll => continue,
            };
            let (mark, style) = at(i);
            line(row, &format!("{mark}{} {label}", entry.key(o.axis)), style);
            row += 1;
        }
        if hides {
            let heading = match hidden.len() {
                0 => format!(" No hidden {nouns}"),
                n => format!(" Hidden {nouns} ({n}):"),
            };
            line(row, &heading, theme.body.patch(theme.row_number));
            row += 1;
        }
        // The window of the list that keeps the cursor in view.
        let first = (o.cursor.saturating_sub(above))
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
            let (mark, style) = at(above + i);
            line(row, &format!("{mark}{number} {named}"), style);
            row += 1;
        }
        if !hidden.is_empty() {
            let (mark, style) = at(above + hidden.len());
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
