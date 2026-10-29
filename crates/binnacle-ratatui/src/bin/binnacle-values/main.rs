//! The binnacle values example: the shared fixture of Typed Values, every
//! type, a unit column and a time-role column, each drawn through the shared
//! formatter, as the web demo's Values page draws them. A null draws `∅`
//! dimmed, an empty text blank and an invalid value in the warning style
//! after a `!`. Numbers line up on their decimals, and the "seen" column's
//! relative times count from the fixture's own time at start-up, so its
//! "just now" turns "1m ago" within the first minute. Arrows move, `t`
//! cycles the Theme presets, `q` quits.
//!
//! Run it with `cargo run -p binnacle-ratatui --bin binnacle-values`.

mod table;

use std::io;
use std::time::{Duration, Instant};

use binnacle_ratatui::{
    ColorSupport, Keymap, Preset, RenderContext, Theme, begin_frame, render_with, wheel,
};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use ratatui::crossterm::execute;

use table::Values;

fn main() -> io::Result<()> {
    let values = Values::new();
    let mut grid = values.grid();
    let keymap = Keymap::preset(Preset::Chords);
    let start = Instant::now();
    let themes = Theme::presets();
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let mut theme = if no_color { themes.len() - 1 } else { 0 };
    let colors = ColorSupport::from_colorterm(std::env::var("COLORTERM").ok().as_deref());
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
                    .colors(colors);
                begin_frame(&mut grid, &ctx, area, |grid, viewport| {
                    values.fill(grid, viewport)
                });
                render_with(&grid.snapshot(), &values, &ctx, area, frame.buffer_mut());
                let line = ratatui::layout::Rect {
                    y: full.bottom().saturating_sub(1),
                    height: 1,
                    ..full
                };
                let text = format!("t theme  q quit | {}", themes[theme].name);
                frame.render_widget(ratatui::text::Line::from(text), line);
            })?;
            // Draws at least once a second, so relative times move on.
            if !event::poll(Duration::from_secs(1))? {
                continue;
            }
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('t') => theme = (theme + 1) % themes.len(),
                    _ => {
                        if let Some(action) = keymap.action(&key) {
                            grid.action(action);
                        }
                    }
                },
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
