//! The binnacle ratatui demo: a table from identity Table Sources.
//!
//! `binnacle-demo` shows 1000 rows; `binnacle-demo --huge` shows 10^15. The
//! wheel scrolls; arrows scroll by one item, PageUp and PageDown by a page,
//! Home and End jump to the first and last row, `m` to the middle. Press `q`,
//! Esc or Ctrl-C to quit.

use std::io;

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, render, wheel};
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

/// The Action for a key press, or `None` to quit. `page` is the rows visible.
fn key_action(code: KeyCode, page: i64) -> Option<Option<Action>> {
    let by = |axis, items| Some(Some(Action::ScrollBy { axis, items }));
    let to = |fraction| {
        Some(Some(Action::ScrollTo {
            axis: Axis::Rows,
            fraction,
        }))
    };
    match code {
        KeyCode::Char('q') | KeyCode::Esc => None,
        KeyCode::Down => by(Axis::Rows, 1),
        KeyCode::Up => by(Axis::Rows, -1),
        KeyCode::Right => by(Axis::Cols, 1),
        KeyCode::Left => by(Axis::Cols, -1),
        KeyCode::PageDown => by(Axis::Rows, page),
        KeyCode::PageUp => by(Axis::Rows, -page),
        KeyCode::Home => to(0.0),
        KeyCode::End => to(1.0),
        KeyCode::Char('m') => to(0.5),
        _ => Some(None),
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
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let mut page = 1;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                let area = frame.area();
                page = i64::from(area.height.saturating_sub(1)).max(1);
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
                    match key_action(key.code, page) {
                        _ if ctrl_c => return Ok(()),
                        None => return Ok(()),
                        Some(Some(action)) => grid.action(action),
                        Some(None) => {}
                    }
                }
                Event::Mouse(m) => {
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
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
