//! The binnacle ratatui demo: a table from identity Table Sources.
//!
//! `binnacle-demo` shows 1000 rows; `binnacle-demo --huge` shows 10^15. The
//! wheel scrolls; the adapter's default keymap moves focus (arrows, PageUp and
//! PageDown, Home and End, Ctrl with an arrow; Shift extends the selection) and
//! the view follows it; Ctrl+A selects all, Ctrl+Space the column, Esc
//! cancels a drag; `m` scrolls to the middle. Click to focus, Shift-click or
//! drag to select, Ctrl-click to pick rows, click a row number to select its
//! row. Press `q` or Ctrl-C to quit.

use std::io;
use std::time::Instant;

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, Keymap, Mouse, render, wheel};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;

const ROWS: u64 = 1000;
const HUGE_ROWS: u64 = 1_000_000_000_000_000;
const COLS: u64 = 12;

struct Identity;

impl Content for Identity {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        format!("col {}", col.0)
    }
}

fn main() -> io::Result<()> {
    let rows = if std::env::args().any(|a| a == "--huge") {
        HUGE_ROWS
    } else {
        ROWS
    };
    let row_source = IdentitySource::new(rows);
    let col_source = IdentitySource::new(COLS);
    // The row header band and the columns fit the widest row number.
    let digits = rows.to_string().len() as f32;
    let mut grid = Grid::new(
        AxisConfig::new(rows, 1.0).header(1.0),
        AxisConfig::new(COLS, (digits + 4.0).max(10.0))
            .header(digits + 1.0)
            .gap(1.0),
    );
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
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                area = frame.area();
                // Nothing here acts on Effects (FocusChanged, SelectionChanged)
                // yet; drain them.
                while grid.poll_effect().is_some() {}
                let viewport = Viewport::new(area.width.into(), area.height.into());
                for (axis, range) in grid.begin_frame(viewport) {
                    let keys = match axis {
                        Axis::Rows => row_source.window(range.clone()),
                        Axis::Cols => col_source.window(range.clone()),
                    };
                    grid.set_window(axis, range.start, &keys);
                }
                render(&grid.snapshot(), &Identity, area, frame.buffer_mut());
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
