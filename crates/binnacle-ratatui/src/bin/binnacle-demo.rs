//! The binnacle ratatui demo.
//!
//! `binnacle-demo` shows 1000 generated records behind an In-Memory Table
//! Source: click a column header to sort ascending, descending, then
//! unsorted; Shift-click adds a column to the sort. A header ends in `▲` or
//! `▼` (with its place in a multi-column sort) and `~` while a sort is
//! pending. Focus and picks stay on their records. Drag a column header to
//! move the column (a selected column takes the other selected ones with
//! it); a cyan line shows where it will drop, and widths go with their
//! columns. Press Ctrl+F to show only records with a crew of 6 or more, and
//! again to show them all: focus stays on its record, or moves to the nearest
//! one left in sort order. `binnacle-demo --huge` shows 10^15 identity rows,
//! which neither sort, move nor filter.
//!
//! Typing jumps to the next record whose name starts with what was typed
//! (type-ahead through `set_focus`; pause a second to start over); in the
//! huge table, typing digits goes to that row number. Enter, F2 or a
//! double-click on a name opens a minimal inline editor: type, Backspace,
//! Enter to keep the new name, Esc to drop it. A click elsewhere keeps it.
//!
//! The wheel scrolls; the adapter's default keymap moves focus (arrows,
//! PageUp and PageDown, Home and End, Ctrl with an arrow; Shift extends the
//! selection) and the view follows it; Ctrl+A selects all, Ctrl+Space the
//! column, Esc cancels a drag or a resize; Ctrl+G scrolls to the middle and
//! Ctrl+F toggles the filter. Click to focus, Shift-click or drag to select,
//! Ctrl-click to pick rows, click a row number to select its row. Drag a
//! column header's border (the line in the one-cell Gap) to resize the
//! column; double-click it to fit the column to its visible text. The bottom
//! line shows the gesture in progress and, on the right, the Theme: Ctrl+T
//! cycles the Themes (`classic`, `catppuccin_mocha`, `mono`), and the demo
//! starts in `mono` when `NO_COLOR` is set. Press Ctrl+Q or Ctrl+C to quit.

use std::borrow::Cow;
use std::io;
use std::time::{Duration, Instant};

use binnacle_core::{
    Action, Axis, AxisConfig, EditEnd, Effect, Grid, Key, Located, Stamped, Viewport,
};
use binnacle_memory::{IdentitySource, MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    ColorSupport, Content, Keymap, Mouse, RenderContext, Theme, render_editor, render_with, status,
    wheel,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;

const RECORDS: usize = 1000;
const HUGE_ROWS: u64 = 1_000_000_000_000_000;
const HUGE_COLS: u64 = 12;

const NAMES: [&str; 8] = [
    "anchor", "bosun", "capstan", "davit", "ensign", "fathom", "galley", "hawser",
];

struct Record {
    id: i64,
    name: String,
    /// Missing for every seventh record, to show nulls sorting last.
    weight: Option<f64>,
    crew: i64,
}

impl Record {
    fn generate(i: usize) -> Record {
        // A cheap scramble so the columns are not already in order.
        let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40;
        Record {
            id: i as i64,
            name: format!("{} {}", NAMES[(h % 8) as usize], h % 97),
            weight: (!i.is_multiple_of(7)).then(|| (h % 10_000) as f64 / 10.0),
            crew: (h % 13) as i64,
        }
    }
}

const HEADERS: [&str; 4] = ["id", "name", "weight", "crew"];

/// The one editable column.
const NAME: Key = Key(1);

/// How long a pause starts type-ahead over.
const TYPE_AHEAD: Duration = Duration::from_secs(1);

/// The table the demo shows: records, or a huge identity table.
enum Table {
    Records {
        rows: Vec<Record>,
        source: Box<MemorySource<Record>>,
        filtered: bool,
    },
    Huge(Huge),
}

/// The huge table: Keys are Positions on both axes.
struct Huge {
    rows: IdentitySource,
    cols: IdentitySource,
}

impl Huge {
    fn axis(&self, axis: Axis) -> &IdentitySource {
        match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        }
    }
}

impl PumpSource for Huge {
    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.axis(axis).locate(key)
    }
}

impl Table {
    fn window(&self, axis: Axis, range: std::ops::Range<u64>) -> Vec<Key> {
        match self {
            Table::Records { source, .. } => source.window(axis, range),
            Table::Huge(huge) => huge.axis(axis).window(range),
        }
    }

    /// Drains the Grid's Effects, answering sorts, column moves and `Locate`
    /// from the source and auto-size by measuring the text drawn for `shown`
    /// rows. Returns the Effects the app itself acts on: activation, edits
    /// and typing.
    fn pump(&mut self, grid: &mut Grid, shown: &[Key]) -> Vec<Effect> {
        let mut app = Vec::new();
        let mut seen = |text: &Text, grid: &mut Grid, stamped: Stamped| match stamped.effect {
            Effect::Activated { .. }
            | Effect::EditRequested { .. }
            | Effect::EditCancelled { .. }
            | Effect::TextTyped { .. } => app.push(stamped.effect),
            _ => auto_size(text, shown, grid, stamped),
        };
        match self {
            Table::Records { rows, source, .. } => {
                let text = Text::Records(rows);
                source.pump(rows, grid, |grid, stamped| seen(&text, grid, stamped));
            }
            Table::Huge(huge) => {
                pump(grid, huge, |grid, stamped| seen(&Text::Huge, grid, stamped));
            }
        }
        app
    }

    /// The row type-ahead lands on for `typed`: in the records, the first
    /// whose name starts with it (ignoring case), searching from Position
    /// `from` and wrapping; in the huge table, the row numbered `typed`.
    fn type_ahead(&self, typed: &str, from: u64) -> Option<Key> {
        match self {
            Table::Records { rows, source, .. } => {
                let count = source.count(Axis::Rows);
                let order = source.window(Axis::Rows, 0..count);
                let typed = typed.to_lowercase();
                let matches = |k: &Key| rows[k.0 as usize].name.to_lowercase().starts_with(&typed);
                let start = from.min(count) as usize;
                let (after, before) = order.split_at(start);
                after.iter().chain(before).copied().find(matches)
            }
            Table::Huge(huge) => {
                let n: u64 = typed.parse().ok()?;
                (1..=huge.rows.count()).contains(&n).then(|| Key(n - 1))
            }
        }
    }

    /// Stores an edited name and tells the Grid what changed.
    fn commit(&mut self, grid: &mut Grid, row: Key, text: String) {
        if let Table::Records { rows, source, .. } = self {
            rows[row.0 as usize].name = text;
            if let Some(notice) = source.edited(rows, row.0 as usize, NAME) {
                grid.notify(Axis::Rows, notice);
            }
        }
    }

    fn name(&self, row: Key) -> Option<String> {
        match self {
            Table::Records { rows, .. } => Some(rows[row.0 as usize].name.clone()),
            Table::Huge(_) => None,
        }
    }

    /// Turns the crew filter on or off and tells the Grid.
    fn toggle_filter(&mut self, grid: &mut Grid) {
        if let Table::Records {
            rows,
            source,
            filtered,
        } = self
        {
            *filtered = !*filtered;
            let notice = if *filtered {
                source.filter(rows, |r| r.crew >= 6)
            } else {
                source.clear_filter()
            };
            grid.notify(Axis::Rows, notice);
        }
    }

    fn text(&self) -> Text<'_> {
        match self {
            Table::Records { rows, .. } => Text::Records(rows),
            Table::Huge(_) => Text::Huge,
        }
    }
}

/// Answers `AutoSizeRequested` for a column with the widest text drawn in
/// it: the `shown` rows' cells and its header.
fn auto_size(text: &Text, shown: &[Key], grid: &mut Grid, stamped: Stamped) {
    if let Effect::AutoSizeRequested { axis, key } = stamped.effect {
        let width = match axis {
            Axis::Cols => (shown.iter())
                .map(|&row| text.cell(row, key).chars().count())
                .chain([text.column_header(key).chars().count()])
                .max()
                .unwrap_or(1),
            Axis::Rows => 1,
        };
        grid.set_size(axis, key, width as f32);
    }
}

/// The text a [`Table`] draws, borrowed apart from its source.
enum Text<'a> {
    Records(&'a [Record]),
    Huge,
}

impl Content for Text<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        match self {
            Text::Records(rows) => {
                let r = &rows[row.0 as usize];
                match col.0 {
                    0 => r.id.to_string(),
                    1 => r.name.clone(),
                    2 => r.weight.map_or_else(|| "-".into(), |w| format!("{w:.1}")),
                    _ => r.crew.to_string(),
                }
            }
            Text::Huge => format!("r{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        match self {
            Text::Records(_) => HEADERS[col.0 as usize].to_string(),
            Text::Huge => format!("col {}", col.0),
        }
    }
}

/// The host's own editor: the edited cell and the text so far.
struct Editor {
    row: Key,
    col: Key,
    text: String,
}

/// What the demo app keeps beside the Grid: its editor and type-ahead.
#[derive(Default)]
struct App {
    editor: Option<Editor>,
    typed: String,
    typed_at: Option<Instant>,
    /// The focused cell as of the last Snapshot.
    focus: Option<(binnacle_core::FocusEnd, binnacle_core::FocusEnd)>,
}

impl App {
    /// Pumps until the queue stays empty, acting on what the app owns:
    /// activation and F2 open the editor on a name, `EditCancelled` drops
    /// it, and typing jumps ahead.
    fn pump(&mut self, table: &mut Table, grid: &mut Grid, shown: &[Key]) {
        loop {
            let effects = table.pump(grid, shown);
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                match effect {
                    Effect::Activated { row, col } | Effect::EditRequested { row, col } => {
                        if let (NAME, Some(text)) = (col, table.name(row)) {
                            grid.edit_started(row, col);
                            self.editor = Some(Editor { row, col, text });
                        }
                    }
                    Effect::EditCancelled { .. } => self.editor = None,
                    Effect::TextTyped { text } => self.type_ahead(table, grid, &text),
                    _ => {}
                }
            }
        }
    }

    fn type_ahead(&mut self, table: &Table, grid: &mut Grid, text: &str) {
        let now = Instant::now();
        let fresh = self.typed_at.is_none_or(|t| now - t > TYPE_AHEAD);
        if fresh {
            self.typed.clear();
        }
        self.typed.push_str(text);
        self.typed_at = Some(now);
        // A new search starts after the focused row; a longer prefix may
        // still match it.
        let (row, col) = self.focus.map_or((0, NAME), |(r, c)| {
            (r.position + u64::from(fresh), c.key.unwrap_or(NAME))
        });
        if let Some(found) = table.type_ahead(&self.typed, row) {
            grid.set_focus(found, col);
        }
    }

    /// Takes a key press while the editor is open; false if it is closed.
    fn edit_key(&mut self, table: &mut Table, grid: &mut Grid, code: KeyCode) -> bool {
        let Some(editor) = &mut self.editor else {
            return false;
        };
        match code {
            KeyCode::Char(c) => editor.text.push(c),
            KeyCode::Backspace => {
                editor.text.pop();
            }
            KeyCode::Enter => self.commit(table, grid),
            KeyCode::Esc => {
                self.editor = None;
                grid.edit_ended(EditEnd::Cancel);
            }
            _ => {}
        }
        true
    }

    /// Keeps the editor's text, if it is open, and closes it.
    fn commit(&mut self, table: &mut Table, grid: &mut Grid) {
        if let Some(Editor { row, col, text }) = self.editor.take() {
            debug_assert_eq!(col, NAME);
            grid.edit_ended(EditEnd::Commit);
            table.commit(grid, row, text);
        }
    }
}

fn main() -> io::Result<()> {
    let (mut table, mut grid) = if std::env::args().any(|a| a == "--huge") {
        // The row header band and the columns fit the widest row number.
        let digits = HUGE_ROWS.to_string().len() as f32;
        let mut grid = Grid::new(
            AxisConfig::new(HUGE_ROWS, 1.0).header(1.0),
            AxisConfig::new(HUGE_COLS, digits + 4.0)
                .header(digits + 1.0)
                .gap(1.0),
        );
        for c in 0..HUGE_COLS {
            grid.set_sortable(Key(c), false);
        }
        let table = Table::Huge(Huge {
            rows: IdentitySource::new(HUGE_ROWS),
            cols: IdentitySource::new(HUGE_COLS),
        });
        (table, grid)
    } else {
        let rows: Vec<Record> = (0..RECORDS).map(Record::generate).collect();
        let source = MemorySource::<Record>::new(rows.len(), HEADERS.len())
            .i64_column(Key(0), |r| Some(r.id))
            .bytes_column(Key(1), |r| Some(Cow::Borrowed(r.name.as_bytes())))
            .f64_column(Key(2), |r| r.weight)
            .i64_column(Key(3), |r| Some(r.crew));
        let grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 1.0).header(1.0),
            AxisConfig::new(HEADERS.len() as u64, 14.0)
                .header(5.0)
                .gap(1.0),
        );
        let source = Box::new(source);
        let table = Table::Records {
            rows,
            source,
            filtered: false,
        };
        (table, grid)
    };
    let mut keymap = Keymap::default();
    keymap.bind(
        KeyCode::Char('g'),
        KeyModifiers::CONTROL,
        Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        },
    );
    let mut mouse = Mouse::default();
    let mut area = ratatui::layout::Rect::default();
    // The row Keys last drawn, which auto-size measures.
    let mut shown_rows: Vec<Key> = Vec::new();
    let mut app = App::default();
    let themes = Theme::presets();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut theme = if no_color { themes.len() - 1 } else { 0 };
    let colors = ColorSupport::from_colorterm(std::env::var("COLORTERM").ok().as_deref());
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                // The Grid takes all but the bottom line, the status line.
                let full = frame.area();
                area = full;
                area.height = full.height.saturating_sub(1);
                app.pump(&mut table, &mut grid, &shown_rows);
                let viewport = Viewport::new(area.width.into(), area.height.into());
                for (axis, range) in grid.begin_frame(viewport) {
                    let keys = table.window(axis, range.clone());
                    grid.set_window(axis, range.start, &keys);
                }
                let snapshot = grid.snapshot();
                shown_rows.clear();
                shown_rows.extend(snapshot.rows().iter().filter_map(|r| r.key));
                app.focus = snapshot.focus(Axis::Rows).zip(snapshot.focus(Axis::Cols));
                let ctx = RenderContext::new(&themes[theme]).colors(colors);
                render_with(&snapshot, &table.text(), &ctx, area, frame.buffer_mut());
                if let Some(editor) = &app.editor {
                    render_editor(&snapshot, &editor.text, area, frame.buffer_mut());
                }
                let line = ratatui::layout::Rect {
                    y: full.y + area.height,
                    height: full.height - area.height,
                    ..full
                };
                frame.render_widget(ratatui::text::Line::from(status(&snapshot)), line);
                let name = ratatui::text::Line::from(themes[theme].name).right_aligned();
                frame.render_widget(name, line);
            })?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let ctrl = key.modifiers == KeyModifiers::CONTROL;
                    if ctrl && matches!(key.code, KeyCode::Char('c' | 'q')) {
                        return Ok(());
                    }
                    if app.edit_key(&mut table, &mut grid, key.code) {
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('t') {
                        theme = (theme + 1) % themes.len();
                        continue;
                    }
                    if ctrl && key.code == KeyCode::Char('f') {
                        table.toggle_filter(&mut grid);
                        continue;
                    }
                    if let Some(action) = keymap.action(&key) {
                        grid.action(action);
                    }
                }
                Event::Mouse(m) => {
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
                    } else if let Some(pointer) = mouse.event(&m, area, Instant::now()) {
                        if pointer.kind == binnacle_core::PointerKind::Down {
                            // A press would cancel the edit: keep it first.
                            app.commit(&mut table, &mut grid);
                        }
                        grid.pointer(pointer);
                    }
                }
                _ => {}
            }
        }
    })();
    execute!(io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}
