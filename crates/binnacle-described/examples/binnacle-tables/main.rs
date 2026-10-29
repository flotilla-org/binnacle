//! The binnacle tables example: a recorded Described Table, replayed from
//! its rows stream into the Described Table source and drawn with the
//! ratatui adapter. Headers show each field's title, cells draw through the
//! shared formatter (a null as a dim `∅`, a time column relative to now),
//! nested columns start hidden and the key and state columns pinned
//! leading. A click on a header sorts by it, Shift adds it to the sort; `s`
//! cycles the sort of the focused column, `g` groups the rows by it (again
//! to ungroup), `t` cycles the Theme presets and `q` quits.
//!
//! Run it with
//! `cargo run -p binnacle-described --example binnacle-tables -- <fixture.ndjson|.cborseq>`,
//! say `crates/binnacle-described/tests/data/rows-hosts.ndjson`.

mod table;

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use binnacle_core::{Axis, Grouping, ViewSpec};
use binnacle_format::Timestamp;
use binnacle_ratatui::{
    ColorSupport, Keymap, Mouse, NextFrame, Preset, RenderContext, Theme, animating, begin_frame,
    render_with, wheel,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use ratatui::crossterm::execute;

use table::Tables;

fn main() -> io::Result<()> {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: binnacle-tables <fixture.ndjson|fixture.cborseq>");
        std::process::exit(2);
    };
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    let origin = Timestamp::from_unix_nanos(now.map_or(0, |d| d.as_nanos() as i128));
    let mut tables = Tables::load(&path, origin).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let mut grid = tables.grid();
    let keymap = Keymap::preset(Preset::Chords);
    let start = Instant::now();
    let themes = Theme::presets();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut theme = if no_color { themes.len() - 1 } else { 0 };
    let colors = ColorSupport::from_colorterm(std::env::var("COLORTERM").ok().as_deref());
    let next = NextFrame::new();
    let mut mouse = Mouse::default();
    let mut layout = binnacle_ratatui::Layout::default();
    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableMouseCapture)?;
    let result = (|| {
        loop {
            terminal.draw(|frame| {
                let full = frame.area();
                let mut area = full;
                area.height = full.height.saturating_sub(1);
                let ctx = RenderContext::new(&themes[theme])
                    .at(start.elapsed())
                    .colors(colors)
                    .next_frame(&next);
                layout = begin_frame(&mut grid, &ctx, area, |grid, viewport| {
                    tables.fill(grid, viewport)
                });
                render_with(&grid.snapshot(), &tables, &ctx, area, frame.buffer_mut());
                let line = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(1),
                    height: 1,
                    ..full
                };
                let text = format!(
                    "{} | {} | s sort  g group  t theme  q quit | {}",
                    tables.source.table(),
                    tables.status(),
                    themes[theme].name
                );
                frame.render_widget(ratatui::text::Line::from(text), line);
            })?;
            let wait = match next.take() {
                _ if animating(&grid.snapshot()) => Duration::from_millis(50),
                Some(due) => due.saturating_sub(start.elapsed()),
                None => Duration::from_secs(60),
            };
            if !event::poll(wait)? {
                continue;
            }
            let focused = grid.snapshot().focus(Axis::Cols).and_then(|f| f.key);
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('t') => theme = (theme + 1) % themes.len(),
                    KeyCode::Char('s') => {
                        if let Some(col) = focused {
                            grid.cycle_sort(Axis::Rows, col, false);
                        }
                    }
                    KeyCode::Char('g') => {
                        let mut spec: ViewSpec = grid.snapshot().applied_view().clone();
                        spec.grouping = match (spec.grouping, focused) {
                            (None, Some(col)) => Some(Grouping::rows_by(col)),
                            _ => None,
                        };
                        grid.request_view(spec);
                    }
                    _ => {
                        if let Some(action) = keymap.action(&key) {
                            grid.action(action);
                        }
                    }
                },
                Event::Mouse(m) => {
                    if let Some(action) = wheel(&m) {
                        grid.action(action);
                    } else if let Some(pointer) = mouse.event(&m, layout.grid, Instant::now()) {
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
