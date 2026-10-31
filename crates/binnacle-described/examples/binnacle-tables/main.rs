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
//! say `crates/binnacle-described/tests/data/rows-hosts.ndjson`, or live
//! against a table server with
//! `-- --url http://127.0.0.1:7777 --table hosts` (or `--socket <path>`),
//! with `--namespace` (default `flotilla`), `--where <clause>` (repeated)
//! and `--token` or `--cookie`. Live, the rows update as they change, and a
//! stream that ends is opened again.

mod table;

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use binnacle_core::{Axis, Grouping, ViewSpec};
use binnacle_described::Scope;
use binnacle_described_http::{Auth, Blocking, Endpoint, Event as Wire, Options};
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

const USAGE: &str = "usage: binnacle-tables <fixture.ndjson|fixture.cborseq>\n       binnacle-tables (--url <base> | --socket <path>) --table <name> [--namespace <ns>] [--where <clause>]... [--token <t> | --cookie <c>]";

/// Where the rows come from.
enum Rows {
    Fixture(PathBuf),
    Live {
        options: Options,
        table: String,
        scope: Scope,
    },
}

fn usage(message: &str) -> ! {
    eprintln!("binnacle-tables: {message}\n{USAGE}");
    std::process::exit(2);
}

fn args() -> Rows {
    let mut args = std::env::args().skip(1);
    let (mut endpoint, mut table, mut namespace) = (None, None, "flotilla".to_owned());
    let (mut wheres, mut auth) = (Vec::new(), Auth::None);
    let mut fixture = None;
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| usage(&format!("{arg} needs a value")))
        };
        match arg.as_str() {
            "--url" => endpoint = Some(Endpoint::Tcp(value())),
            "--socket" => endpoint = Some(Endpoint::Unix(value().into())),
            "--table" => table = Some(value()),
            "--namespace" => namespace = value(),
            "--where" => wheres.push(value()),
            "--token" => auth = Auth::Bearer(value()),
            "--cookie" => auth = Auth::Cookie(value()),
            "-h" | "--help" => usage("help"),
            flag if flag.starts_with("--") => usage(&format!("unknown flag {flag}")),
            path => fixture = Some(PathBuf::from(path)),
        }
    }
    match (fixture, endpoint, table) {
        (Some(path), None, None) => Rows::Fixture(path),
        (None, Some(endpoint), Some(table)) => {
            let scope = (wheres.into_iter()).fold(Scope::new(namespace), Scope::with_where);
            Rows::Live {
                options: Options::new(endpoint).auth(auth),
                table,
                scope,
            }
        }
        (None, Some(_), None) => usage("--table is needed"),
        _ => usage("a fixture, or --url or --socket"),
    }
}

fn main() -> io::Result<()> {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    let origin = Timestamp::from_unix_nanos(now.map_or(0, |d| d.as_nanos() as i128));
    let (mut tables, transport) = match args() {
        Rows::Fixture(path) => {
            let tables = Tables::load(&path, origin).unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1);
            });
            (tables, None)
        }
        Rows::Live {
            options,
            table,
            scope,
        } => (
            Tables::live(&table, scope, origin),
            Some(Blocking::new(options)?),
        ),
    };
    let mut grid = tables.grid();
    let mut failed: Option<String> = None;
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
            if let Some(transport) = &transport {
                if let Some(request) = tables.source.poll_request() {
                    transport.open(request);
                }
                while let Some(event) = transport.try_recv() {
                    match event {
                        Wire::Message(message) => {
                            if tables.apply(&mut grid, message) {
                                grid = tables.grid();
                            }
                        }
                        Wire::Ended => {
                            tables.source.ended();
                        }
                        Wire::Failed { status, body } => {
                            let body = String::from_utf8_lossy(&body).into_owned();
                            failed = Some(format!("{status} {body}"));
                        }
                        _ => {}
                    }
                    if let Some(request) = tables.source.poll_request() {
                        transport.open(request);
                    }
                }
            }
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
                let status = failed.clone().unwrap_or_else(|| tables.status());
                let text = format!(
                    "{} | {} | s sort  g group  t theme  q quit | {}",
                    tables.source.table(),
                    status,
                    themes[theme].name
                );
                frame.render_widget(ratatui::text::Line::from(text), line);
            })?;
            let wait = match next.take() {
                _ if animating(&grid.snapshot()) => Duration::from_millis(50),
                Some(due) => due.saturating_sub(start.elapsed()),
                None => Duration::from_secs(60),
            };
            // Live rows are applied between frames.
            let wait = match transport {
                Some(_) => wait.min(Duration::from_millis(50)),
                None => wait,
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
