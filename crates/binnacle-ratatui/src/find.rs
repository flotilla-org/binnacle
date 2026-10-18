//! The find bar: one line holding the Find Query's text and toggles, the
//! count, and the keys that step between matches.

use std::time::{Duration, Instant};

use binnacle_core::{Action, Axis, FindDirection, FindQuery, Grid, Snapshot, TextQuery};
use binnacle_memory::FindCount;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::Theme;
use crate::filter::{Pause, toggle_style};
use crate::renderer::{char_count, fit};

/// What the Table Source last said about the Find Query in force, which the
/// host keeps (the Grid holds only the query and the current match): the
/// count, where the last step landed among the matches, whether it is still
/// finding, and why it failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FindStatus {
    pub count: Option<FindCount>,
    /// The last step's 0-based index among the matches, if the source gave one.
    pub index: Option<u64>,
    /// Whether the last step wrapped past the end (or the start).
    pub wrapped: bool,
    /// Whether the source is still working out the Find Query in force (no
    /// count yet), so the bar says "finding…".
    pub finding: bool,
    /// The source's message if it failed or rejected the Find Query (an
    /// invalid regex, say): the bar says "No matches" in the error style.
    pub error: Option<String>,
}

/// The find bar: `Find`, the text, the case (`Aa`) and regex (`.*`, where
/// the Table Source takes regexes) toggles, the order (`rows` or `cols`),
/// then the count ("3 of 120", "120 matches", "No matches") and a note when
/// a step wraps. Opens only while the Grid has find support; each change
/// sets the Grid's Find Query, so the host's marks follow as it types: typing
/// once a pause has passed (see [`FindBar::tick`]), the toggles and steps at
/// once.
///
/// While focused it takes every key: characters and Backspace edit, Ctrl+U
/// clears, Alt+C toggles case, Alt+R regex, Alt+B the order, Enter or Ctrl+N
/// step to the next match, Shift+Enter or Ctrl+P to the previous, Alt+F shows
/// only matching rows (it sets the rows' quick filter to the find's text;
/// again clears it), and Esc leaves the bar with find still open, so F3
/// steps from the table. [`FindBar::close`] closes find.
#[derive(Clone, Debug, Default)]
pub struct FindBar {
    text: String,
    case_sensitive: bool,
    regex: bool,
    by_cols: bool,
    open: bool,
    focused: bool,
    /// Whether the Table Source takes regexes, as of opening.
    regex_support: bool,
    /// Whether Alt+F set the quick filter, so the next clears it.
    only: bool,
    /// Which way the last step went, for the wrap note.
    stepped: Option<FindDirection>,
    pause: Pause,
}

impl FindBar {
    pub fn new() -> Self {
        FindBar::default()
    }

    /// How long typing waits before setting the Find Query:
    /// [`crate::TYPING_PAUSE`] unless set; zero sets it on every key.
    pub fn pause(mut self, pause: Duration) -> Self {
        self.pause.pause = pause;
        self
    }

    /// Sets the Find Query typed, once its pause has passed by `now`. Call
    /// it from the host's loop, waking by [`FindBar::deadline`].
    pub fn tick(&mut self, grid: &mut Grid, now: Instant) {
        if self.pause.due(now) {
            self.apply(grid);
        }
    }

    /// When typed text waits to be sent until, if it does.
    pub fn deadline(&self) -> Option<Instant> {
        self.pause.deadline()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Opens find, or takes focus again while it is open, if the Grid has
    /// find support; returns whether it did. Follow it with
    /// [`FindBar::apply`] to find the bar's text again.
    pub fn open(&mut self, snapshot: &Snapshot) -> bool {
        let Some(support) = snapshot.find_support() else {
            return false;
        };
        self.regex_support = support.regex;
        self.regex &= support.regex;
        self.open = true;
        self.focused = true;
        true
    }

    /// Applies the query after [`FindBar::open`]: the Grid's Find Query
    /// becomes the bar's text (none while empty).
    pub fn apply(&self, grid: &mut Grid) {
        grid.set_find(self.query().filter(|_| self.open));
    }

    /// Closes find: the Grid's Find Query and its marks go.
    pub fn close(&mut self, grid: &mut Grid) {
        self.open = false;
        self.focused = false;
        self.stepped = None;
        self.pause.flush();
        grid.set_find(None);
    }

    /// The Find Query the bar holds, `None` while its text is empty.
    pub fn query(&self) -> Option<FindQuery> {
        if self.text.is_empty() {
            return None;
        }
        let query = TextQuery::new(self.text.clone())
            .case_sensitive(self.case_sensitive)
            .regex(self.regex);
        let by = if self.by_cols { Axis::Cols } else { Axis::Rows };
        Some(FindQuery::new(query).by(by))
    }

    /// Steps to the next or previous match while find is open, as F3 and
    /// Shift+F3 do from the table; returns whether find was open.
    pub fn step(&mut self, direction: FindDirection, grid: &mut Grid) -> bool {
        if !self.open {
            return false;
        }
        if self.pause.flush() {
            self.apply(grid);
        }
        self.stepped = Some(direction);
        grid.action(match direction {
            FindDirection::Next => Action::FindNext,
            FindDirection::Previous => Action::FindPrevious,
        });
        true
    }

    /// Takes a key press while focused; false (taking nothing) otherwise.
    pub fn key(&mut self, event: &KeyEvent, grid: &mut Grid) -> bool {
        if !self.focused || event.kind == KeyEventKind::Release {
            return self.focused;
        }
        let alt = event.modifiers == KeyModifiers::ALT;
        let ctrl = event.modifiers == KeyModifiers::CONTROL;
        let shift = event.modifiers == KeyModifiers::SHIFT;
        match event.code {
            KeyCode::Esc => self.focused = false,
            KeyCode::Enter if shift => {
                self.step(FindDirection::Previous, grid);
            }
            KeyCode::Enter => {
                self.step(FindDirection::Next, grid);
            }
            KeyCode::Char('n') if ctrl => {
                self.step(FindDirection::Next, grid);
            }
            KeyCode::Char('p') if ctrl => {
                self.step(FindDirection::Previous, grid);
            }
            KeyCode::Char('f') if alt => {
                if self.pause.flush() {
                    self.apply(grid);
                }
                let quick = self.query().filter(|_| !self.only).map(|q| q.query);
                self.only = quick.is_some();
                grid.set_quick_filter(Axis::Rows, quick);
            }
            KeyCode::Char('c') if alt => {
                self.case_sensitive = !self.case_sensitive;
                self.edited(grid);
            }
            KeyCode::Char('r') if alt && self.regex_support => {
                self.regex = !self.regex;
                self.edited(grid);
            }
            KeyCode::Char('b') if alt => {
                self.by_cols = !self.by_cols;
                self.edited(grid);
            }
            KeyCode::Char('u') if ctrl => {
                self.text.clear();
                self.typed(grid);
            }
            KeyCode::Char(c) if KeyModifiers::SHIFT.contains(event.modifiers) => {
                self.text.push(c);
                self.typed(grid);
            }
            KeyCode::Backspace => {
                self.text.pop();
                self.typed(grid);
            }
            _ => {}
        }
        true
    }

    fn edited(&mut self, grid: &mut Grid) {
        self.stepped = None;
        self.pause.flush();
        self.apply(grid);
    }

    /// Text typed: sent once the pause allows.
    fn typed(&mut self, grid: &mut Grid) {
        self.stepped = None;
        if self.pause.typed() {
            self.apply(grid);
        }
    }

    /// Draws the bar into `area`'s first line while find is open: `Find`,
    /// the text (ending in a caret while focused), the toggles and order,
    /// then the count and any wrap note. `status` is what the Table Source
    /// last said; a step's index shows only while there is a current match.
    pub fn render(
        &self,
        snapshot: &Snapshot,
        status: &FindStatus,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let area = area.intersection(buf.area);
        if !self.open || area.is_empty() {
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
        let width = usize::from(area.width);
        put(&mut x, &" ".repeat(width), theme.body);
        x = area.x;
        put(
            &mut x,
            " Find ",
            theme.header.add_modifier(Modifier::REVERSED),
        );
        put(&mut x, " ", theme.body);
        let field = theme.body.add_modifier(Modifier::UNDERLINED);
        let room = 24.min(width.saturating_sub(8));
        let shown = if self.focused {
            // Shows the end of a long text, then the caret.
            let chars: Vec<char> = self.text.chars().collect();
            let tail: String = chars[chars.len().saturating_sub(room.saturating_sub(1))..]
                .iter()
                .collect();
            format!("{tail}▏")
        } else {
            self.text.clone()
        };
        let shown = fit(&shown, u32::try_from(room).unwrap_or(u32::MAX));
        let pad = room.saturating_sub(char_count(&shown) as usize);
        put(&mut x, &shown, field);
        put(&mut x, &" ".repeat(pad), field);
        put(&mut x, " ", theme.body);
        let mut toggles = vec![("Aa", self.case_sensitive)];
        if self.regex_support {
            toggles.push((".*", self.regex));
        }
        for (text, on) in toggles {
            put(&mut x, text, toggle_style(theme, on));
            put(&mut x, " ", theme.body);
        }
        let by = if self.by_cols { "cols" } else { "rows" };
        put(&mut x, by, theme.body.add_modifier(Modifier::DIM));
        put(&mut x, "  ", theme.body);
        let current = snapshot.find_match().is_some();
        let waiting = self.pause.typed.is_some() || status.finding;
        let count = match status.error {
            _ if waiting => "finding…".to_string(),
            Some(_) => "No matches".to_string(),
            None => count_text(snapshot.find().is_some(), status, current),
        };
        let none = status.count.is_some_and(|c| c.total == 0 && !c.partial);
        let style = if waiting {
            theme.body.patch(theme.loading)
        } else if status.error.is_some() {
            theme.body.patch(theme.error)
        } else if none {
            theme
                .body
                .patch(theme.sort_marker)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.body.add_modifier(Modifier::BOLD)
        };
        put(&mut x, &count, style);
        if let Some(message) = status.error.as_ref().filter(|_| !waiting) {
            put(
                &mut x,
                &format!(": {message}"),
                theme.body.patch(theme.error),
            );
        }
        if status.wrapped && current {
            let note = match self.stepped {
                Some(FindDirection::Previous) => "  wrapped to the end",
                _ => "  wrapped to the start",
            };
            put(&mut x, note, theme.body.patch(theme.find_current));
        }
        let hints = if self.focused {
            "  Enter next  S-Enter/^P prev  Esc table"
        } else {
            "  F3 next  S-F3 prev  ^F edit"
        };
        put(&mut x, hints, theme.body.patch(theme.loading));
    }
}

/// "3 of 120" once a step has landed on a current match, else "120
/// matches"; "No matches"; a partial count with "+".
fn count_text(open: bool, status: &FindStatus, current: bool) -> String {
    let Some(c) = status.count.filter(|_| open) else {
        return match status.index.filter(|_| current) {
            Some(i) => format!("match {}", i + 1),
            None => String::new(),
        };
    };
    if c.total == 0 && !c.partial {
        return "No matches".into();
    }
    let plus = if c.partial { "+" } else { "" };
    let total = format!("{}{plus}", crate::filter::grouped(c.total));
    match status.index.filter(|_| current) {
        Some(i) => format!("{} of {total}", crate::filter::grouped(i + 1)),
        None if c.total == 1 && !c.partial => "1 match".into(),
        None => format!("{total} matches"),
    }
}
