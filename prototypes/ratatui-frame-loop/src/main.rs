//! PROTOTYPE (throwaway, wayfinder ticket #5): a ratatui host driving the stub Grid
//! and In-Memory Table Source through the frame loop.
//!
//!   cargo run                 interactive (mouse + keyboard), 100k rows
//!   cargo run -- --huge       interactive, 10^15 virtual rows, no sort
//!   cargo run -- --script     scripted walkthrough, prints every frame and the state

mod grid;
mod memory;

use grid::{Action, Axis, By, Effect, Grid, Notice, Pointer, Snapshot};
use memory::{IdentitySource, MemorySource, SortKeys};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use std::time::{Duration, Instant};

const COLS: [&str; 6] = ["id", "name", "city", "amount", "qty", "code"];
const NAMES: [&str; 7] = ["ada", "bob", "cy", "dee", "eve", "fay", "gus"];
const CITIES: [&str; 5] = ["Oslo", "Lima", "Pune", "Kyiv", "Baku"];

struct Rec {
    id: i64,
    name: String,
    city: &'static str,
    amount: Option<f64>,
    qty: i64,
    code: String,
}

fn make_data(n: usize) -> Vec<Rec> {
    let mut s: u64 = 0x2545F4914F6CDD1D;
    let mut rnd = move || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    (0..n)
        .map(|i| {
            let r = rnd();
            Rec {
                id: i as i64,
                name: format!("{}{}", NAMES[(r % 7) as usize], r % 97),
                city: CITIES[((r >> 8) % 5) as usize],
                amount: if r % 11 == 0 { None } else { Some(((r >> 16) % 100_000) as f64 / 100.0) },
                qty: ((r >> 24) % 50) as i64,
                code: format!("{:04X}", (r >> 32) & 0xFFFF),
            }
        })
        .collect()
}

impl Rec {
    fn text(&self, col: u64) -> String {
        match col {
            0 => self.id.to_string(),
            1 => self.name.clone(),
            2 => self.city.to_string(),
            3 => self.amount.map_or("-".into(), |a| format!("{a:.2}")),
            4 => self.qty.to_string(),
            _ => self.code.clone(),
        }
    }
}

enum RowSource {
    Mem(MemorySource, Vec<Rec>),
    Huge(IdentitySource),
}

struct Host {
    grid: Grid,
    rows: RowSource,
    cols: IdentitySource,
    log: Vec<String>,
}

impl Host {
    fn new(huge: bool) -> Self {
        let (rows, n) = if huge {
            let n = 1_000_000_000_000_000u64;
            (RowSource::Huge(IdentitySource { count: n }), n)
        } else {
            let data = make_data(100_000);
            (RowSource::Mem(MemorySource::new(data.len()), data), 100_000)
        };
        let mut grid = Grid::new(n, COLS.len() as u64, 1.0, 9.0, 1.0);
        if huge {
            grid.row_header_w = 18.0;
        }
        Host { grid, rows, cols: IdentitySource { count: COLS.len() as u64 }, log: Vec::new() }
    }

    /// The two-phase frame: begin_frame -> fill Windows -> pull Snapshot.
    fn frame(&mut self, w: u16, h: u16) -> Snapshot {
        let req = self.grid.begin_frame(w as f32, h as f32);
        if let Some((s, n)) = req[0] {
            let keys = match &self.rows {
                RowSource::Mem(m, _) => m.window(s, n),
                RowSource::Huge(i) => i.window(s, n),
            };
            self.log.push(format!("window rows {s}..{}", s + n));
            self.grid.set_window(Axis::Rows, s, keys);
        }
        if let Some((s, n)) = req[1] {
            self.grid.set_window(Axis::Cols, s, self.cols.window(s, n));
        }
        self.grid.snapshot()
    }

    /// The pump: drain Effects, interpret them, answer back as inputs.
    fn pump(&mut self) {
        while let Some(e) = self.grid.poll_effect() {
            match (&e, &mut self.rows) {
                (Effect::SortRequested(spec), RowSource::Mem(src, data)) => {
                    let keys = spec.iter().map(|(col, _)| extract(data, *col)).collect();
                    let t = Instant::now();
                    src.apply_sort(spec, keys);
                    self.log.push(format!("{e:?} -> apply_sort {:?} -> Reordered", t.elapsed()));
                    self.grid.notice(Axis::Rows, Notice::Reordered);
                }
                (Effect::Locate { axis: Axis::Rows, key }, RowSource::Mem(src, _)) => {
                    let pos = src.locate(*key);
                    self.log.push(format!("{e:?} -> located {pos:?}"));
                    self.grid.located(Axis::Rows, *key, pos);
                }
                (Effect::Locate { axis, key }, RowSource::Huge(_)) => {
                    self.grid.located(*axis, *key, Some(*key)); // identity
                }
                (Effect::SortRequested(_), RowSource::Huge(_)) => {
                    self.log.push(format!("{e:?} ignored: this source can't sort"));
                }
                (Effect::SelectionChanged | Effect::FocusChanged, _) => {}
                _ => self.log.push(format!("{e:?}")),
            }
        }
    }

    /// App-level filter (the Grid never sees predicates): keep mask -> Reset.
    fn filter_city(&mut self, city: Option<&str>) {
        if let RowSource::Mem(src, data) = &mut self.rows {
            src.apply_filter(city.map(|c| data.iter().map(|r| r.city == c).collect()));
            let n = src.count();
            self.log.push(format!("apply_filter({city:?}) -> Reset({n})"));
            self.grid.notice(Axis::Rows, Notice::Reset(n));
        }
    }

    fn cell_text(&self, row: u64, col: u64) -> String {
        match &self.rows {
            RowSource::Mem(_, data) => data[row as usize].text(col),
            RowSource::Huge(_) => format!("{}", row.wrapping_mul(2654435761).wrapping_add(col * 7919) % 100_000),
        }
    }
}

fn extract(data: &[Rec], col: u64) -> SortKeys {
    match col {
        0 => SortKeys::I64(data.iter().map(|r| Some(r.id)).collect()),
        3 => SortKeys::F64(data.iter().map(|r| r.amount).collect()),
        4 => SortKeys::I64(data.iter().map(|r| Some(r.qty)).collect()),
        _ => SortKeys::Bytes(data.iter().map(|r| Some(r.text(col).into_bytes())).collect()),
    }
}

// ---- rendering: the host's only job is to draw what the Snapshot says --------

fn put(buf: &mut Buffer, clip: Rect, x: i32, y: i32, w: i32, text: &str, style: Style) {
    if y < clip.y as i32 || y >= clip.bottom() as i32 {
        return;
    }
    for (i, ch) in text.chars().chain(std::iter::repeat(' ')).take(w.max(0) as usize).enumerate() {
        let cx = x + i as i32;
        if cx >= clip.x as i32 && cx < clip.right() as i32 {
            buf[(cx as u16, y as u16)].set_char(ch).set_style(style);
        }
    }
}

fn render(host: &Host, snap: &Snapshot, buf: &mut Buffer, area: Rect) {
    let gx = area.x as i32;
    let gy = area.y as i32;
    let rhw = snap.row_header_w as i32;
    let chh = snap.col_header_h as i32;
    let head = Style::default().add_modifier(Modifier::BOLD).bg(Color::DarkGray);
    let cells = Rect::new(area.x + rhw as u16, area.y + chh as u16, area.width.saturating_sub(rhw as u16), area.height.saturating_sub(chh as u16));
    let col_head = Rect::new(cells.x, area.y, cells.width, chh as u16);
    let row_head = Rect::new(area.x, cells.y, rhw as u16, cells.height);

    for c in &snap.cols {
        let x = cells.x as i32 + c.offset as i32;
        let mut label = COLS[c.key as usize].to_string();
        if let Some(i) = snap.sort.iter().position(|(k, _)| *k == c.key) {
            let arrow = if snap.sort[i].1 == grid::Dir::Asc { '^' } else { 'v' };
            label = if snap.sort.len() > 1 { format!("{label}{arrow}{}", i + 1) } else { format!("{label}{arrow}") };
        }
        put(buf, col_head, x, gy, c.size as i32, &label, head);
        put(buf, col_head, x + c.size as i32, gy, 1, "|", head);
    }
    for r in &snap.rows {
        let y = cells.y as i32 + r.offset as i32;
        put(buf, row_head, gx, y, rhw - 1, &format!("{:>w$}", r.pos + 1, w = (rhw - 1) as usize), head);
        for c in &snap.cols {
            let x = cells.x as i32 + c.offset as i32;
            let st = snap.cell_state(r, c);
            let style = if st.focused {
                Style::default().add_modifier(Modifier::REVERSED)
            } else if st.selected {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };
            let mark = if st.focused { ">" } else if st.selected { "*" } else { " " };
            put(buf, cells, x, y, c.size as i32, &format!("{mark}{}", host.cell_text(r.key, c.key)), style);
            put(buf, cells, x + c.size as i32, y, 1, "|", Style::default().fg(Color::DarkGray));
        }
    }
}

fn status(host: &Host, snap: &Snapshot) -> Vec<String> {
    let f = snap.focus;
    vec![
        format!(
            "gen {} | rows {} cols {} | thumb {:.6} | mode {:?} | cursor {:?} capturing {}",
            snap.generation, snap.counts[0], snap.counts[1], snap.thumb[0], snap.mode, snap.cursor, snap.capturing
        ),
        format!(
            "sort {:?} | focus row(pos {:?} key {:?}) col(pos {:?} key {:?}) | range {:?}",
            snap.sort, f.0.pos, f.0.key, f.1.pos, f.1.key, snap.range
        ),
        format!("log: {}", host.log.iter().rev().take(3).rev().cloned().collect::<Vec<_>>().join(" ; ")),
    ]
}

// ---- scripted walkthrough ------------------------------------------------------

fn script() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let (w, h) = (72u16, 16u16);

    type Step = (&'static str, Box<dyn Fn(&mut Host)>);
    let header_x = |col: usize| 8.0 + col as f32 * 10.0 + 2.0;
    let cell = |row: f32, col: usize| (8.0 + col as f32 * 10.0 + 1.0, 1.0 + row);
    let huge_cell = |row: f32, col: usize| (18.0 + col as f32 * 10.0 + 1.0, 1.0 + row);
    let click = move |g: &mut Grid, (x, y): (f32, f32), shift: bool, clicks: u8| {
        g.pointer(Pointer::Down { x, y, clicks, shift });
        g.pointer(Pointer::Up { x, y });
    };

    let runs: Vec<(&str, bool, Vec<Step>)> = vec![
        ("Sort follows focus by Key; ranges drop on reorder", false, vec![
            ("initial frame (window filled for the first time)", Box::new(|_| {})),
            ("click cell row 3, 'name'", Box::new(move |h| click(&mut h.grid, cell(2.0, 1), false, 1))),
            ("shift-click row 6, 'amount' (range)", Box::new(move |h| click(&mut h.grid, cell(5.0, 3), true, 1))),
            ("click header 'amount' -> SortRequested -> Reordered; range dropped, focus Locate'd", Box::new(move |h| click(&mut h.grid, (header_x(3), 0.0), false, 1))),
            ("ArrowDown: focus moves from its new position (scrolls into view)", Box::new(|h| h.grid.action(Action::MoveFocus { axis: Axis::Rows, forward: true, by: By::Item, extend: false }))),
            ("click header 'amount' again -> descending", Box::new(move |h| click(&mut h.grid, (header_x(3), 0.0), false, 1))),
            ("shift-click header 'city' -> multi-sort [amount v, city ^]", Box::new(move |h| click(&mut h.grid, (header_x(2), 0.0), true, 1))),
            ("app filter: city == Oslo (keep mask, Reset; sort order kept, no re-sort)", Box::new(|h| h.filter_city(Some("Oslo")))),
            ("ArrowDown after the filter (focus was Locate'd; is the row still there?)", Box::new(|h| h.grid.action(Action::MoveFocus { axis: Axis::Rows, forward: true, by: By::Item, extend: false }))),
            ("app clears the filter (Reset back to 100000)", Box::new(|h| h.filter_city(None))),
        ]),
        ("Resize, scroll and keyboard", false, vec![
            ("initial frame", Box::new(|_| {})),
            ("drag the border after 'name' 6 cells right (Resizing, capturing)", Box::new(|h| {
                h.grid.pointer(Pointer::Down { x: 8.0 + 19.0, y: 0.0, clicks: 1, shift: false });
                h.grid.pointer(Pointer::Move { x: 8.0 + 25.0, y: 0.0 });
            })),
            ("release -> SizeChanged", Box::new(|h| h.grid.pointer(Pointer::Up { x: 8.0 + 25.0, y: 0.0 }))),
            ("wheel: scroll 5000 rows", Box::new(|h| h.grid.scroll(Axis::Rows, 5000))),
            ("Ctrl-End-ish: MoveFocus Edge (focus jumps outside the old Window)", Box::new(|h| h.grid.action(Action::MoveFocus { axis: Axis::Rows, forward: true, by: By::Edge, extend: false }))),
            ("shift-PageUp: extend selection a page up", Box::new(|h| h.grid.action(Action::MoveFocus { axis: Axis::Rows, forward: false, by: By::Page, extend: true }))),
            ("Escape: Cancel clears the range", Box::new(|h| h.grid.action(Action::Cancel))),
        ]),
        ("Huge virtual table: 10^15 rows, Scroll Anchor stays exact", true, vec![
            ("initial frame", Box::new(|_| {})),
            ("scrollbar: ScrollTo 0.5", Box::new(|h| h.grid.scroll_to(Axis::Rows, 0.5))),
            ("scroll by 1 row (exact at 5e14)", Box::new(|h| h.grid.scroll(Axis::Rows, 1))),
            ("click a cell, then header 'amount' (this source can't sort)", Box::new(move |h| {
                click(&mut h.grid, huge_cell(1.0, 0), false, 1);
                click(&mut h.grid, (18.0 + 3.0 * 10.0 + 2.0, 0.0), false, 1);
            })),
            ("MoveFocus Edge -> last row", Box::new(|h| h.grid.action(Action::MoveFocus { axis: Axis::Rows, forward: true, by: By::Edge, extend: false }))),
        ]),
    ];

    for (title, huge, steps) in runs {
        println!("\n=============== {title} ===============");
        let mut host = Host::new(huge);
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        for (i, (desc, step)) in steps.iter().enumerate() {
            step(&mut host);
            host.pump();
            let snap = host.frame(w, h);
            // A Reordered/Locate answer can arrive during the frame; pump again.
            host.pump();
            term.draw(|f| { let a = f.area(); render(&host, &snap, f.buffer_mut(), a) }).unwrap();
            println!("\n--- {}. {desc}", i + 1);
            let buf = term.backend().buffer();
            for y in 0..h {
                let line: String = (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect();
                println!("  {}", line.trim_end());
            }
            for s in status(&host, &snap) {
                println!("  > {s}");
            }
            host.log.clear();
        }
    }
}

// ---- interactive ---------------------------------------------------------------

fn interactive(huge: bool) -> std::io::Result<()> {
    use ratatui::crossterm::event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind,
    };
    use ratatui::crossterm::execute;

    let mut host = Host::new(huge);
    let mut term = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;
    let mut last_down: Option<(Instant, u16, u16, u8)> = None;

    let result = (|| -> std::io::Result<()> {
        loop {
            host.pump();
            let size = term.size()?;
            let gh = size.height.saturating_sub(3);
            let snap = host.frame(size.width, gh);
            host.pump();
            term.draw(|f| {
                let area = f.area();
                let grid_area = Rect::new(area.x, area.y, area.width, gh);
                render(&host, &snap, f.buffer_mut(), grid_area);
                for (i, s) in status(&host, &snap).iter().enumerate() {
                    let y = gh as i32 + i as i32;
                    put(f.buffer_mut(), area, 0, y, area.width as i32, s, Style::default().fg(Color::Yellow));
                }
            })?;
            let ev = event::read()?;
            match ev {
                Event::Key(k) => {
                    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
                    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                    let mv = |axis, forward, by| Action::MoveFocus { axis, forward, by, extend: shift };
                    let act = match k.code {
                        KeyCode::Char('q') if !shift => return Ok(()),
                        KeyCode::Char('c') if ctrl => return Ok(()),
                        KeyCode::Char('a') if ctrl => Some(Action::SelectAll),
                        KeyCode::Down => Some(mv(Axis::Rows, true, By::Item)),
                        KeyCode::Up => Some(mv(Axis::Rows, false, By::Item)),
                        KeyCode::Right => Some(mv(Axis::Cols, true, By::Item)),
                        KeyCode::Left => Some(mv(Axis::Cols, false, By::Item)),
                        KeyCode::PageDown => Some(mv(Axis::Rows, true, By::Page)),
                        KeyCode::PageUp => Some(mv(Axis::Rows, false, By::Page)),
                        KeyCode::End => Some(mv(Axis::Rows, true, By::Edge)),
                        KeyCode::Home => Some(mv(Axis::Rows, false, By::Edge)),
                        KeyCode::Enter => Some(Action::Activate),
                        KeyCode::Esc => Some(Action::Cancel),
                        KeyCode::Char(c) => Some(Action::TypeText(c.to_string())),
                        _ => None,
                    };
                    if let Some(a) = act {
                        host.grid.action(a);
                    }
                }
                Event::Mouse(m) => {
                    let (x, y) = (m.column as f32, m.row as f32);
                    let shift = m.modifiers.contains(KeyModifiers::SHIFT);
                    match m.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            // The host owns double-click timing and supplies the count.
                            let clicks = match last_down {
                                Some((t, lx, ly, n)) if t.elapsed() < Duration::from_millis(400) && lx == m.column && ly == m.row => n + 1,
                                _ => 1,
                            };
                            last_down = Some((Instant::now(), m.column, m.row, clicks));
                            host.grid.pointer(Pointer::Down { x, y, clicks, shift });
                        }
                        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => host.grid.pointer(Pointer::Move { x, y }),
                        MouseEventKind::Up(MouseButton::Left) => host.grid.pointer(Pointer::Up { x, y }),
                        MouseEventKind::ScrollDown => host.grid.scroll(Axis::Rows, 3),
                        MouseEventKind::ScrollUp => host.grid.scroll(Axis::Rows, -3),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    })();

    execute!(std::io::stdout(), DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--script") {
        script();
        Ok(())
    } else {
        interactive(args.iter().any(|a| a == "--huge"))
    }
}
