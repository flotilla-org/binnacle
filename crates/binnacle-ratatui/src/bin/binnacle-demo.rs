//! The binnacle ratatui demo: a static table from identity Table Sources.
//! Press `q`, Esc or Ctrl-C to quit.

use std::io;

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_ratatui::{Content, render};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

const ROWS: u64 = 1000;
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
    let rows = IdentitySource::new(ROWS);
    let cols = IdentitySource::new(COLS);
    let mut grid = Grid::new(
        AxisConfig::new(ROWS, 1.0).header(1.0),
        AxisConfig::new(COLS, 10.0).header(6.0),
    );
    let mut terminal = ratatui::init();
    let result = (|| loop {
        terminal.draw(|frame| {
            let area = frame.area();
            for (axis, range) in
                grid.begin_frame(Viewport::new(area.width.into(), area.height.into()))
            {
                let keys = match axis {
                    Axis::Rows => rows.window(range.clone()),
                    Axis::Cols => cols.window(range.clone()),
                };
                grid.set_window(axis, range.start, &keys);
            }
            render(&grid.snapshot(), &Identity, area, frame.buffer_mut());
        })?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            let ctrl_c =
                key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
            if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                return Ok(());
            }
        }
    })();
    ratatui::restore();
    result
}
