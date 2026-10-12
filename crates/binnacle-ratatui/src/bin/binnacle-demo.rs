//! The binnacle ratatui demo.
//!
//! `binnacle-demo` shows 1000 generated records behind an In-Memory Table
//! Source: click a column header to sort ascending, descending, then
//! unsorted; Shift-click adds a column to the sort. A header ends in `▲` or
//! `▼` (with its place in a multi-column sort) and `~` while a sort is
//! pending. Focus and picks stay on their records. `binnacle-demo --huge`
//! shows 10^15 identity rows, which do not sort.
//!
//! The wheel scrolls; the adapter's default keymap moves focus (arrows,
//! PageUp and PageDown, Home and End, Ctrl with an arrow; Shift extends the
//! selection) and the view follows it; Ctrl+A selects all, Ctrl+Space the
//! column, Esc cancels a drag or a resize; `m` scrolls to the middle. Click to
//! focus, Shift-click or drag to select, Ctrl-click to pick rows, click a row
//! number to select its row. Drag a column header's border (the line in the
//! one-cell Gap) to resize the column; double-click it to fit the column to
//! its visible text. The bottom line shows the gesture in progress. Press `q`
//! or Ctrl-C to quit.

use std::borrow::Cow;
use std::io;
use std::time::Instant;

use binnacle_core::{Action, Axis, AxisConfig, Effect, Grid, Key, Stamped, Viewport};
use binnacle_memory::{IdentitySource, MemorySource};
use binnacle_ratatui::{Content, Keymap, Mouse, render, status, wheel};
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

/// The table the demo shows: records, or a huge identity table.
enum Table {
    Records {
        rows: Vec<Record>,
        source: Box<MemorySource<Record>>,
    },
    Huge {
        rows: IdentitySource,
        cols: IdentitySource,
    },
}

impl Table {
    fn window(&self, axis: Axis, range: std::ops::Range<u64>) -> Vec<Key> {
        match (self, axis) {
            (Table::Records { source, .. }, _) => source.window(axis, range),
            (Table::Huge { rows, .. }, Axis::Rows) => rows.window(range),
            (Table::Huge { cols, .. }, Axis::Cols) => cols.window(range),
        }
    }

    /// Drains the Grid's Effects, answering sorts and `Locate` from the
    /// source and auto-size by measuring the text drawn for `shown` rows. The
    /// other Effects need nothing here.
    fn pump(&mut self, grid: &mut Grid, shown: &[Key]) {
        match self {
            Table::Records { rows, source } => {
                let text = Text::Records(rows);
                source.pump(rows, grid, |grid, stamped| {
                    auto_size(&text, shown, grid, stamped)
                });
            }
            Table::Huge { .. } => {
                while let Some(stamped) = grid.poll_effect() {
                    auto_size(&Text::Huge, shown, grid, stamped);
                }
            }
        }
    }

    fn text(&self) -> Text<'_> {
        match self {
            Table::Records { rows, .. } => Text::Records(rows),
            Table::Huge { .. } => Text::Huge,
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
        let table = Table::Huge {
            rows: IdentitySource::new(HUGE_ROWS),
            cols: IdentitySource::new(HUGE_COLS),
        };
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
        (Table::Records { rows, source }, grid)
    };
    let mut keymap = Keymap::default();
    keymap.bind(
        KeyCode::Char('m'),
        KeyModifiers::NONE,
        Action::ScrollTo {
            axis: Axis::Rows,
            fraction: 0.5,
        },
    );
    let mut mouse = Mouse::default();
    let mut area = ratatui::layout::Rect::default();
    // The row Keys last drawn, which auto-size measures.
    let mut shown_rows: Vec<Key> = Vec::new();
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                // The Grid takes all but the bottom line, the status line.
                let full = frame.area();
                area = full;
                area.height = full.height.saturating_sub(1);
                table.pump(&mut grid, &shown_rows);
                let viewport = Viewport::new(area.width.into(), area.height.into());
                for (axis, range) in grid.begin_frame(viewport) {
                    let keys = table.window(axis, range.clone());
                    grid.set_window(axis, range.start, &keys);
                }
                let snapshot = grid.snapshot();
                shown_rows.clear();
                shown_rows.extend(snapshot.rows().iter().filter_map(|r| r.key));
                render(&snapshot, &table.text(), area, frame.buffer_mut());
                let line = ratatui::layout::Rect {
                    y: full.y + area.height,
                    height: full.height - area.height,
                    ..full
                };
                frame.render_widget(ratatui::text::Line::from(status(&snapshot)), line);
            })?;
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let ctrl_c = key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl_c || key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                    if let Some(action) = keymap.action(&key) {
                        grid.action(action);
                    }
                }
                Event::Mouse(m) => {
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
                    } else if let Some(pointer) = mouse.event(&m, area, Instant::now()) {
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
