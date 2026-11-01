//! The table inline below the prompt or full screen, `f` switching
//! between them, with the full widget: keys and the mouse, sort, the header
//! menu with grouping, the chooser, the quick and column filters and find.
//! Enter hands back the focused or picked rows; Esc or `q` hands back
//! nothing, and Ctrl-C or a signal says it was interrupted. Either way the
//! rows the inline viewport shows stay behind as aligned text.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use binnacle_core::{Axis, ChangeNotice, Grid, Key, Viewport};
use binnacle_format::{Align, Timestamp, Value};
use binnacle_layout::{JsonId, Layout};
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::crossterm::{execute, queue};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui::{Terminal, TerminalOptions};

use super::print::title_aligns;
use super::{CliTable, Exit, Selection, notify, partial_status, print, thousands};
use crate::{
    CellRenderer, Chooser, ColumnFilter, Content, Control, Controls, FindBar, FindStatus,
    HeaderMenu, Keymap, Mouse, Preset, QuickFilter, RenderContext, Theme, ValueRenderer,
    begin_frame, render_with, wheel,
};

/// A host's live feed: called between frames, it may feed the source and
/// returns the Change Notices to apply, or `None` to end the session as
/// Esc does (the feed failed, say). The terminal is restored before
/// [`run_inline`] returns, so the host reports why after that.
pub type Live<'a, T> = Box<dyn FnMut(&mut T) -> Option<Vec<(Axis, ChangeNotice)>> + 'a>;

/// A host's note for the status line, asked each frame: the state of its
/// feed, say "reconnecting". `None` shows nothing.
pub type Note<'a> = Box<dyn FnMut() -> Option<String> + 'a>;

/// How the live table session shows: inline below the prompt, or on the
/// alternate screen at the terminal's full size. `f` switches between
/// them while it runs; it is one Grid either way, so rows, focus, picks,
/// sort, filters and find stay as they were.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Presentation {
    /// Below the prompt, at most [`InlineOptions::max_height`] lines.
    #[default]
    Inline,
    /// On the alternate screen, over the whole terminal.
    FullScreen,
}

impl Presentation {
    /// The other presentation, which `f` switches to.
    pub fn other(self) -> Presentation {
        match self {
            Presentation::Inline => Presentation::FullScreen,
            Presentation::FullScreen => Presentation::Inline,
        }
    }
}

/// How [`run_inline`] runs.
pub struct InlineOptions<'a, T> {
    /// The most lines the inline viewport takes; half the terminal by
    /// default.
    pub max_height: Option<u16>,
    /// The presentation the session starts in; inline by default.
    pub presentation: Presentation,
    /// Called between frames while the viewport is open.
    pub live: Option<Live<'a, T>>,
    /// Asked each frame; what it gives leads the status line.
    pub note: Option<Note<'a>>,
    pub theme: Theme,
    /// How long to wait for input before the next frame.
    pub tick: Duration,
    /// Where relative times count from; now by default.
    pub origin: Option<Timestamp>,
}

impl<T> Default for InlineOptions<'_, T> {
    fn default() -> Self {
        InlineOptions {
            max_height: None,
            presentation: Presentation::Inline,
            live: None,
            note: None,
            theme: Theme::classic(),
            tick: Duration::from_millis(100),
            origin: None,
        }
    }
}

/// How tall the viewport is for `rows` rows: the header line, the rows, a
/// scrollbar line and the status line, up to `max` lines (half of
/// `terminal_rows` by default), at least 3.
pub fn inline_height(rows: u64, terminal_rows: u16, max: Option<u16>) -> u16 {
    let need = u16::try_from(rows.saturating_add(3)).unwrap_or(u16::MAX);
    need.min(max.unwrap_or(terminal_rows / 2)).max(3)
}

/// How many rows decide which side a header lines up on: the first ones in
/// display order, not only the visible ones.
const SAMPLE: u64 = 64;

/// SIGINT, the signal Ctrl-C is taken as.
const SIGINT: i32 = 2;

/// Opens `source`, presented as `layout`, in an inline viewport below the
/// prompt on this process's terminal, and returns how it ended.
/// The viewport is as tall as the rows need, up to its most; with a live
/// feed, the most, since rows may still come. The session starts in
/// `options.presentation`; full screen is the alternate screen, and the
/// inline viewport keeps its lines meanwhile. While it is open, SIGINT,
/// SIGTERM and SIGHUP close it as [`Exit::Interrupted`], and a panic
/// restores the terminal, leaving the alternate screen, before the panic
/// hook that was set runs.
/// A signal caught before the viewport has closed wins, even over an
/// Enter already pressed: the process is being told to stop.
/// While the live feed keeps bringing changes, the next frame doesn't wait
/// for input. See [`run_inline_with`].
pub fn run_inline<'a, T: CliTable + 'a>(
    source: &mut T,
    layout: &Layout<JsonId, JsonId>,
    mut options: InlineOptions<'a, T>,
) -> io::Result<Exit> {
    let (_, rows) = ratatui::crossterm::terminal::size()?;
    // A live table may grow after the viewport opens: it takes the most.
    let need = match options.live {
        Some(_) => u64::MAX,
        None => source.count(Axis::Rows),
    };
    let height = inline_height(need, rows, options.max_height);
    let signals = Signals::watch()?;
    let busy = Rc::new(Cell::new(false));
    if let Some(mut live) = options.live.take() {
        let busy = Rc::clone(&busy);
        options.live = Some(Box::new(move |source: &mut T| {
            let fed = live(source);
            busy.set(fed.as_ref().is_some_and(|n| !n.is_empty()));
            fed
        }));
    }
    // Not `ratatui::init`: its restore leaves the alternate screen, which
    // puts the cursor back where it was saved, over the rows left behind.
    ratatui::crossterm::terminal::enable_raw_mode()?;
    let raw = RawMode::new();
    let backend = ratatui::backend::CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: ratatui::Viewport::Inline(height),
        },
    )?;
    execute!(io::stdout(), EnableMouseCapture)?;
    let tick = options.tick;
    let mut failed: Option<io::Error> = None;
    let events = std::iter::from_fn(|| {
        // A signal ends the events, so the rows stay behind as on Esc.
        if signals.caught().is_some() {
            return None;
        }
        let wait = if busy.get() { Duration::ZERO } else { tick };
        let read = match event::poll(wait) {
            Ok(true) => event::read().map(Some),
            Ok(false) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(None),
            Err(e) => Err(e),
        };
        read.map_err(|e| failed = Some(e)).ok()
    });
    let result = run(&mut terminal, events, source, layout, options, screen);
    // A read error ended the events, so whatever `run_inline_with` made of
    // that end (a choice of nothing, or a draw error after it) is not the
    // cause: the read error wins.
    if let Some(e) = failed {
        return Err(e);
    }
    execute!(io::stdout(), DisableMouseCapture)?;
    drop(raw);
    match signals.caught() {
        Some(signal) => result.map(|_| Exit::Interrupted { signal }),
        None => result,
    }
}

/// Runs the inline table on `terminal` (its viewport inline, or any other
/// for tests), taking input from `events`: `None` is a frame with no input
/// (when the live feed is read), and the end of `events` leaves as Esc
/// does, as does the live feed ending. Focus starts on the first row.
///
/// - Enter returns the picked rows' Keys if any are picked (Space picks
///   the focused row), in the order they show, else the focused row's;
///   [`Exit::Left`] when there is neither, as in an empty table.
/// - Esc or `q` returns [`Exit::Left`], once no popup or bar is open to
///   take it.
/// - Ctrl-C returns [`Exit::Interrupted`] with SIGINT, whatever is open.
/// - `s` cycles the sort by the focused column: ascending, descending,
///   none.
/// - `f` switches the [`Presentation`], unless a text field has focus.
///
/// The session starts in `options.presentation`. Full screen draws over
/// the whole backend, and back inline it draws in `terminal`'s viewport
/// again; this form switches no alternate screen, which [`run_inline`]
/// does.
///
/// On the way out it clears the viewport and writes the rows it showed as
/// aligned text ([`super::print`]'s), whole: a line wider than the
/// terminal wraps, so no value is cut off, and the rows stay in the
/// scrollback. Leaving from full screen it goes back inline first, and
/// writes the rows the inline viewport would show.
pub fn run_inline_with<T: CliTable, B: Backend>(
    terminal: &mut Terminal<B>,
    events: impl IntoIterator<Item = Option<Event>>,
    source: &mut T,
    layout: &Layout<JsonId, JsonId>,
    options: InlineOptions<'_, T>,
) -> Result<Exit, B::Error> {
    run(terminal, events, source, layout, options, |_, _| Ok(()))
}

/// [`run_inline_with`], calling `screen` with the backend and the
/// Presentation being switched to as the session goes full screen (after
/// the inline viewport is cleared) and back inline (before it is drawn
/// again): [`run_inline`] enters and leaves the alternate screen there.
fn run<T: CliTable, B: Backend>(
    terminal: &mut Terminal<B>,
    events: impl IntoIterator<Item = Option<Event>>,
    source: &mut T,
    layout: &Layout<JsonId, JsonId>,
    mut options: InlineOptions<'_, T>,
    mut screen: impl FnMut(&mut B, Presentation) -> Result<(), B::Error>,
) -> Result<Exit, B::Error> {
    let origin = options.origin.unwrap_or_else(|| {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
        Timestamp::from_unix_nanos(now.map_or(0, |d| d.as_nanos() as i128))
    });
    let mut grid = print::headless(source, layout);
    focus_first(&mut grid, source);
    let mut menu = HeaderMenu::new();
    menu.set_fields(source.columns());
    let mut session = Session {
        grid,
        menu,
        chooser: Chooser::new(),
        popup: ColumnFilter::new(),
        quick: QuickFilter::new(),
        find: FindBar::new(),
        bars: crate::Layout::default(),
        shown: Vec::new(),
        theme: options.theme.clone(),
        origin,
        start: Instant::now(),
    };
    let keymap = Keymap::preset(Preset::Chords);
    let controls = Controls::preset(Preset::Chords);
    let mut mouse = Mouse::default();
    let mut presentation = Presentation::Inline;
    let mut full = FullScreen::default();
    if options.presentation == Presentation::FullScreen {
        presentation = switch(terminal, &mut full, presentation, &mut screen)?;
    }
    let mut events = events.into_iter();
    let chosen = loop {
        if let Some(live) = options.live.as_mut() {
            let Some(notices) = live(source) else {
                break Exit::Left;
            };
            notify(&mut session.grid, source, notices);
        }
        let note = options.note.as_mut().and_then(|note| note());
        let now = Instant::now();
        session.popup.tick(&mut session.grid, now);
        session.quick.tick(&mut session.grid, now);
        session.find.tick(&mut session.grid, now);
        let status = Status {
            note: note.as_deref(),
            presentation,
        };
        match presentation {
            Presentation::Inline => {
                terminal.draw(|frame| {
                    let area = frame.area();
                    session.draw(source, &status, area, frame.buffer_mut());
                })?;
            }
            Presentation::FullScreen => {
                full.draw(terminal.backend_mut(), |area, buf| {
                    session.draw(source, &status, area, buf)
                })?;
            }
        }
        let Some(input) = events.next() else {
            break Exit::Left;
        };
        let Some(event) = input else {
            continue;
        };
        let Session {
            grid,
            menu,
            chooser,
            popup,
            quick,
            find,
            bars,
            ..
        } = &mut session;
        match event {
            Event::Key(key)
                if key.kind == KeyEventKind::Press
                    && key.code == KeyCode::Char('c')
                    && key.modifiers == KeyModifiers::CONTROL =>
            {
                break Exit::Interrupted { signal: SIGINT };
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if popup.key(&key, grid) || chooser.key(&key, grid) {
                    continue;
                }
                if menu.key(&key, grid, |axis, range| source.window(axis, range).keys) {
                    if let Some(key) = menu.take_slicer() {
                        popup.open_slicer(&grid.snapshot(), key);
                    }
                    if let Some(axis) = menu.take_chooser() {
                        chooser.open(&grid.snapshot(), axis);
                    }
                    continue;
                }
                if find.key(&key, grid) || quick.key(&key, grid) {
                    continue;
                }
                let snapshot = grid.snapshot();
                let took = match controls.control(&key) {
                    Some(Control::QuickFilter) => quick.focus(&snapshot),
                    Some(Control::ColumnFilter) => popup.open(&snapshot),
                    Some(Control::RowFilter) => popup.open_row(&snapshot),
                    Some(Control::Find) => {
                        if find.open(&snapshot) {
                            find.apply(grid);
                        }
                        true
                    }
                    Some(Control::FindNext) => find.repeat(false, grid),
                    Some(Control::FindPrevious) => find.repeat(true, grid),
                    Some(Control::Close) if find.is_open() => {
                        find.close(grid);
                        true
                    }
                    Some(Control::Menu { axis }) => menu.open(&snapshot, axis),
                    Some(Control::Chooser { axis }) => chooser.open(&snapshot, axis),
                    _ => false,
                };
                if took {
                    continue;
                }
                let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
                match key.code {
                    KeyCode::Enter if plain => match chosen(grid, source) {
                        Some(selection) => break Exit::Chosen(selection),
                        None => break Exit::Left,
                    },
                    KeyCode::Esc => break Exit::Left,
                    KeyCode::Char('q') if plain => break Exit::Left,
                    KeyCode::Char('s') if key.modifiers.is_empty() => {
                        let focused = grid.snapshot().focus(Axis::Cols).and_then(|f| f.key);
                        if let Some(col) = focused {
                            grid.cycle_sort(Axis::Rows, col, false);
                        }
                    }
                    KeyCode::Char('f') if key.modifiers.is_empty() => {
                        presentation = switch(terminal, &mut full, presentation, &mut screen)?;
                    }
                    KeyCode::Char(' ') if plain => {
                        let snapshot = grid.snapshot();
                        if let Some(row) = snapshot.focus(Axis::Rows).and_then(|f| f.key) {
                            match snapshot.picked(Axis::Rows).contains(&row) {
                                true => grid.unpick(Axis::Rows, row),
                                false => grid.pick(Axis::Rows, row),
                            }
                        }
                    }
                    KeyCode::Char('m') if plain => {
                        menu.open(&grid.snapshot(), Axis::Cols);
                    }
                    _ => {
                        if let Some(action) = keymap.action(&key) {
                            grid.action(action);
                        }
                    }
                }
            }
            Event::Mouse(m) => {
                if let Some(action) = wheel(&m) {
                    grid.action(action);
                } else if let Some(pointer) = mouse.event(&m, bars.grid, Instant::now()) {
                    grid.pointer(pointer);
                }
            }
            _ => {}
        }
    };
    if presentation == Presentation::FullScreen {
        switch(terminal, &mut full, presentation, &mut screen)?;
        // What the inline viewport would show, so the rows left behind are
        // the same whichever presentation the session ended in.
        let area = terminal.get_frame().area();
        let status = Status {
            note: None,
            presentation: Presentation::Inline,
        };
        session.draw(source, &status, area, &mut Buffer::empty(area));
    }
    let Session { grid, shown, .. } = session;
    let cols = print::columns(source, &grid);
    let text = print::aligned(&*source, &shown, &cols);
    terminal.clear()?;
    let area = terminal.get_frame().area();
    let lines: Vec<&str> = (text.iter())
        .flat_map(|line| wrap(line, area.width.into()))
        .collect();
    // What the viewport can't hold goes above it, into the scrollback.
    let over = lines.len().saturating_sub(area.height.into());
    if over > 0 {
        let height = u16::try_from(over).unwrap_or(u16::MAX);
        terminal.insert_before(height, |buf| {
            for (i, line) in lines[..over].iter().enumerate().take(height.into()) {
                let rect = Rect {
                    y: buf.area.y + i as u16,
                    height: 1,
                    ..buf.area
                };
                Line::from(*line).render_to(rect, buf);
            }
        })?;
    }
    let mut end = 0;
    terminal.draw(|frame| {
        let area = frame.area();
        for (i, line) in lines[over..].iter().enumerate().take(area.height.into()) {
            let rect = Rect {
                y: area.y + i as u16,
                height: 1,
                ..area
            };
            Line::from(*line).render_to(rect, frame.buffer_mut());
            end = rect.bottom();
        }
        end = end.max(area.y);
    })?;
    // The shell writes where the cursor is: with the rows down to the
    // screen's last line, a line scrolls in below them for it.
    let bottom = terminal.size()?.height.saturating_sub(1);
    if end > bottom {
        terminal.set_cursor_position((0, bottom))?;
        terminal.backend_mut().append_lines(1)?;
    }
    terminal.set_cursor_position((0, end.min(bottom)))?;
    terminal.show_cursor()?;
    Ok(chosen)
}

/// `line` in pieces at most `width` columns wide, by display width, each
/// without trailing spaces; one piece, empty, for an empty line.
fn wrap(line: &str, width: usize) -> Vec<&str> {
    use unicode_width::UnicodeWidthChar;
    let mut pieces = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (at, c) in line.char_indices() {
        let w = c.width().unwrap_or(0);
        if used + w > width.max(1) && at > start {
            pieces.push(line[start..at].trim_end());
            (start, used) = (at, 0);
        }
        used += w;
    }
    pieces.push(line[start..].trim_end());
    pieces
}

/// Whether the full-screen presentation has the terminal on the alternate
/// screen.
static ALTERNATE: AtomicBool = AtomicBool::new(false);

/// Enters the alternate screen for the full-screen presentation, or leaves
/// it for the inline one.
fn screen(out: &mut impl Write, to: Presentation) -> io::Result<()> {
    match to {
        Presentation::FullScreen => {
            ALTERNATE.store(true, Ordering::SeqCst);
            execute!(out, EnterAlternateScreen)
        }
        Presentation::Inline => {
            execute!(out, LeaveAlternateScreen)?;
            ALTERNATE.store(false, Ordering::SeqCst);
            Ok(())
        }
    }
}

/// Leaves the alternate screen, raw mode and mouse capture and shows the
/// cursor, as on the way out; safe to call more than once.
fn restore() {
    let _ = leave(&mut io::stdout());
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
}

/// Writes to `out` what leaves the alternate screen if it shows (leaving it
/// when it doesn't would put the cursor back where it was last saved, over
/// the rows left behind) and mouse capture, and shows the cursor.
fn leave(out: &mut impl Write) -> io::Result<()> {
    if ALTERNATE.swap(false, Ordering::SeqCst) {
        queue!(out, LeaveAlternateScreen)?;
    }
    execute!(out, DisableMouseCapture, ratatui::crossterm::cursor::Show)
}

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Sync + Send + 'static>;

/// Raw mode, left when dropped, on the way out or by a panic. While held,
/// a panic restores the terminal first, then runs the panic hook that was
/// set before, so its message prints on a sane terminal; dropped, that hook
/// is set again.
struct RawMode {
    previous: Option<Arc<PanicHook>>,
    restore: fn(),
}

impl RawMode {
    fn new() -> RawMode {
        RawMode::restoring(restore)
    }

    /// As [`RawMode::new`], leaving the terminal by `restore`.
    fn restoring(restore: fn()) -> RawMode {
        let previous = Arc::new(std::panic::take_hook());
        let hook = Arc::clone(&previous);
        std::panic::set_hook(Box::new(move |info| {
            restore();
            hook(info);
        }));
        RawMode {
            previous: Some(previous),
            restore,
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        (self.restore)();
        // Not while panicking: the hook has run, and setting one then
        // aborts.
        if std::thread::panicking() {
            return;
        }
        drop(std::panic::take_hook());
        if let Some(previous) = self.previous.take() {
            match Arc::try_unwrap(previous) {
                Ok(previous) => std::panic::set_hook(previous),
                Err(previous) => std::panic::set_hook(Box::new(move |info| previous(info))),
            }
        }
    }
}

/// SIGINT, SIGTERM and SIGHUP, caught while held instead of ending the
/// process, so the terminal can be restored; the last caught is kept.
/// The handlers are installed once per process: with no viewport open, a
/// signal does what it would by default.
struct Signals {
    #[cfg(unix)]
    watch: &'static Watch,
}

/// The signal caught (0 for none), and whether no viewport is open.
#[cfg(unix)]
struct Watch {
    caught: Arc<std::sync::atomic::AtomicUsize>,
    closed: Arc<std::sync::atomic::AtomicBool>,
}

impl Signals {
    #[cfg(unix)]
    fn watch() -> io::Result<Signals> {
        use std::sync::OnceLock;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
        static WATCH: OnceLock<Result<Watch, String>> = OnceLock::new();
        let watch = WATCH.get_or_init(|| {
            let watch = Watch {
                caught: Arc::new(AtomicUsize::new(0)),
                closed: Arc::new(AtomicBool::new(true)),
            };
            for signal in [SIGINT, SIGTERM, SIGHUP] {
                let caught = Arc::clone(&watch.caught);
                signal_hook::flag::register_usize(signal, caught, signal as usize)
                    .and_then(|_| {
                        let closed = Arc::clone(&watch.closed);
                        signal_hook::flag::register_conditional_default(signal, closed)
                    })
                    .map_err(|e| e.to_string())?;
            }
            Ok(watch)
        });
        let watch = watch.as_ref().map_err(|e| io::Error::other(e.clone()))?;
        watch.caught.store(0, Ordering::SeqCst);
        watch.closed.store(false, Ordering::SeqCst);
        Ok(Signals { watch })
    }

    #[cfg(not(unix))]
    fn watch() -> io::Result<Signals> {
        Ok(Signals {})
    }

    /// The signal caught, if any.
    fn caught(&self) -> Option<i32> {
        #[cfg(unix)]
        match self.watch.caught.load(std::sync::atomic::Ordering::SeqCst) {
            0 => None,
            signal => Some(signal as i32),
        }
        #[cfg(not(unix))]
        None
    }
}

#[cfg(unix)]
impl Drop for Signals {
    fn drop(&mut self) {
        (self.watch.closed).store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(test)]
mod panic_tests {
    use std::sync::Mutex;

    use super::{Presentation, RawMode, leave, screen};

    static ORDER: Mutex<Vec<&str>> = Mutex::new(Vec::new());

    /// What the restore wrote, as the terminal would read it.
    static WRITTEN: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    /// Held by each test here, as whether the alternate screen shows is
    /// process-wide.
    static SCREEN: Mutex<()> = Mutex::new(());

    const LEAVE_ALTERNATE: &str = "\x1b[?1049l";

    fn written() -> String {
        String::from_utf8(std::mem::take(&mut *WRITTEN.lock().unwrap())).unwrap()
    }

    // One test for the guard, since the panic hook is process-wide.
    #[test]
    fn a_panic_or_a_drop_restores_the_terminal_and_a_panic_before_the_previous_hook_runs() {
        let _screen = SCREEN.lock().unwrap_or_else(|e| e.into_inner());
        // The hook is process-wide: it only records this test's own thread.
        let mine = std::thread::current().id();
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            if std::thread::current().id() == mine {
                ORDER.lock().unwrap().push("previous hook");
            }
        }));
        let mode = RawMode::restoring(|| {
            ORDER.lock().unwrap().push("restore");
            leave(&mut *WRITTEN.lock().unwrap()).unwrap();
        });
        screen(&mut Vec::new(), Presentation::FullScreen).unwrap();
        let panicked = std::panic::catch_unwind(|| panic!("the viewport failed"));
        assert!(panicked.is_err());
        assert!(
            written().contains(LEAVE_ALTERNATE),
            "a panic leaves full screen"
        );
        screen(&mut Vec::new(), Presentation::FullScreen).unwrap();
        drop(mode);
        std::panic::set_hook(original);
        assert!(
            written().contains(LEAVE_ALTERNATE),
            "a drop leaves full screen"
        );
        let order = ORDER.lock().unwrap();
        // The drop restores once more, after the panic's own.
        assert_eq!(
            order[..],
            ["restore", "previous hook", "restore"],
            "{order:?}"
        );
    }

    #[test]
    fn the_restore_leaves_the_alternate_screen_only_when_it_shows() {
        let _screen = SCREEN.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        leave(&mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(!out.contains(LEAVE_ALTERNATE), "{out:?}");
        assert!(out.contains("\x1b[?25h"), "the cursor shows: {out:?}");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::Signals;

    #[test]
    fn a_signal_while_watched_is_caught_not_fatal() {
        use signal_hook::consts::{SIGHUP, SIGTERM};
        let signals = Signals::watch().unwrap();
        assert_eq!(signals.caught(), None);
        signal_hook::low_level::raise(SIGHUP).unwrap();
        signal_hook::low_level::raise(SIGTERM).unwrap();
        assert_eq!(signals.caught(), Some(SIGTERM), "the last caught");
        drop(signals);
        let again = Signals::watch().unwrap();
        assert_eq!(again.caught(), None, "a new watch starts clear");
    }
}

/// The picked rows if any, in the order the rows show (picks the view
/// leaves out after them, in Key order), else the focused row; none in an
/// empty table.
fn chosen<T: CliTable>(grid: &mut Grid, source: &mut T) -> Option<Selection> {
    let snapshot = grid.snapshot();
    let picked = snapshot.picked(Axis::Rows).to_vec();
    let rows = match picked.is_empty() {
        false => {
            let count = source.count(Axis::Rows);
            let shown = source.window(Axis::Rows, 0..count).keys;
            let mut left: HashSet<Key> = picked.iter().copied().collect();
            let mut rows: Vec<Key> = (shown.into_iter()).filter(|k| left.remove(k)).collect();
            rows.extend(picked.iter().filter(|k| left.contains(k)));
            rows
        }
        true => snapshot
            .focus(Axis::Rows)
            .and_then(|f| f.key)
            .into_iter()
            .collect(),
    };
    (!rows.is_empty()).then_some(Selection { rows })
}

/// Focuses the first row's first shown column.
fn focus_first<T: CliTable>(grid: &mut Grid, source: &mut T) {
    let row = source.window(Axis::Rows, 0..1).keys.first().copied();
    let col = print::columns(source, grid).first().copied();
    if let (Some(row), Some(col)) = (row, col) {
        grid.set_focus(row, col);
    }
}

/// Answers the Grid's Effects, then fills its Windows, outlines and all.
fn fill<T: CliTable>(grid: &mut Grid, source: &mut T, viewport: Viewport) {
    binnacle_memory::pump(grid, source, |_, _| {});
    for (axis, range) in grid.begin_frame(viewport) {
        let window = source.window(axis, range.clone());
        grid.set_window(axis, range.start, &window.keys);
        if let Some(outlines) = window.outlines {
            grid.set_outlines(axis, range.start, &outlines);
        }
    }
}

/// What the inline table draws: each cell through the shared formatter
/// with its column's unit and role, under its title.
struct View<'a, T> {
    source: &'a T,
    origin: Timestamp,
    aligns: HashMap<Key, Align>,
}

impl<T: CliTable> View<'_, T> {
    fn renderer(&self, col: Key) -> ValueRenderer<impl Fn(Key, Key) -> Value + '_> {
        let renderer = ValueRenderer::new(self.origin, move |row, col| self.source.value(row, col))
            .role(self.source.role(col));
        match self.source.unit(col) {
            Some(unit) => renderer.unit(unit),
            None => renderer,
        }
    }
}

impl<T: CliTable> Content for View<'_, T> {
    fn cell(&self, row: Key, col: Key) -> String {
        self.renderer(col).text(row, col)
    }

    fn column_header(&self, col: Key) -> String {
        self.source.header(col)
    }

    fn column_align(&self, col: Key) -> Align {
        self.aligns.get(&col).copied().unwrap_or(Align::Start)
    }

    fn column_renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        Box::new(self.renderer(col))
    }
}

/// Draws a line of text into one row of a buffer.
trait RenderTo {
    fn render_to(self, area: Rect, buf: &mut ratatui::buffer::Buffer);
}

impl RenderTo for Line<'_> {
    fn render_to(self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        use ratatui::widgets::Widget;
        self.render(area.intersection(buf.area), buf);
    }
}

/// The live table session: the Grid and the widgets over it, drawn in
/// either presentation.
struct Session {
    grid: Grid,
    menu: HeaderMenu,
    chooser: Chooser,
    popup: ColumnFilter,
    quick: QuickFilter,
    find: FindBar,
    bars: crate::Layout,
    /// The rows the last frame drew.
    shown: Vec<Key>,
    theme: Theme,
    origin: Timestamp,
    start: Instant,
}

/// What the status line says besides the table's own state.
struct Status<'a> {
    note: Option<&'a str>,
    presentation: Presentation,
}

impl Session {
    /// Draws the table, its bars and popups and the status line in `full`.
    fn draw<T: CliTable>(&mut self, source: &mut T, status: &Status, full: Rect, buf: &mut Buffer) {
        let theme = &self.theme;
        let (find, quick) = (&mut self.find, &mut self.quick);
        let lines = 1 + u16::from(find.is_open()) + u16::from(quick.focused());
        let area = Rect {
            height: full.height.saturating_sub(lines),
            ..full
        };
        let ctx = RenderContext::new(theme).at(self.start.elapsed());
        self.bars = begin_frame(&mut self.grid, &ctx, area, |grid, viewport| {
            fill(grid, source, viewport)
        });
        let snapshot = self.grid.snapshot();
        self.shown = snapshot.drawn(Axis::Rows).filter_map(|i| i.key).collect();
        self.popup.fetch(&snapshot, |axis, key, query| {
            source.distinct(axis, key, query)
        });
        let listed = (self.chooser.axis())
            .map(|axis| source.window(axis, 0..snapshot.count(axis)).keys)
            .unwrap_or_default();
        // A header lines up with the side most of its cells do, among the
        // first rows.
        let sample = source.window(Axis::Rows, 0..snapshot.count(Axis::Rows).min(SAMPLE));
        let cols = source.columns();
        let aligns = title_aligns(&*source, &sample.keys, &cols);
        let view = View {
            source: &*source,
            origin: self.origin,
            aligns: cols.into_iter().zip(aligns).collect(),
        };
        render_with(&snapshot, &view, &ctx, area, buf);
        let mut y = area.bottom();
        let mut line = || {
            let rect = Rect {
                y,
                height: 1,
                ..full
            };
            y += 1;
            rect
        };
        if find.is_open() {
            find.render(&snapshot, &FindStatus::default(), theme, line(), buf);
        }
        if quick.focused() {
            quick.render(&snapshot, theme, line(), buf);
        }
        let text = crate::status(&snapshot);
        let text = match text.is_empty() {
            true => {
                let shown = snapshot.count(Axis::Rows);
                let rows = match source.partial() {
                    Some(came) => partial_status(shown, came),
                    None => format!("{} rows", thousands(shown)),
                };
                let switch = match status.presentation {
                    Presentation::Inline => "f full screen",
                    Presentation::FullScreen => "f inline",
                };
                format!(
                    "{rows} | Enter choose  Space pick  s sort  m menu  ^F find  ^L filter  {switch}  Esc quit"
                )
            }
            false => text,
        };
        let text = match status.note {
            Some(note) => format!("{note} | {text}"),
            None => text,
        };
        Line::from(text).render_to(line(), buf);
        self.popup
            .render(&snapshot, &view, theme, self.bars.grid, buf);
        self.menu
            .render(&snapshot, &view, theme, self.bars.grid, buf);
        self.chooser.fetch(&snapshot, &view, |_, _| listed);
        self.chooser.render(&snapshot, theme, self.bars.grid, buf);
    }
}

/// Draws the full-screen presentation straight to the backend, as a
/// `Terminal` would, since a `Terminal`'s viewport is fixed when it is made:
/// only the cells that changed since the last frame, all of them after a
/// resize.
#[derive(Default)]
struct FullScreen {
    last: Buffer,
}

impl FullScreen {
    fn draw<B: Backend>(
        &mut self,
        backend: &mut B,
        render: impl FnOnce(Rect, &mut Buffer),
    ) -> Result<(), B::Error> {
        let area = Rect::from((Position::ORIGIN, backend.size()?));
        if area != self.last.area {
            backend.hide_cursor()?;
            backend.clear()?;
            self.last = Buffer::empty(area);
        }
        let mut next = Buffer::empty(area);
        render(area, &mut next);
        backend.draw(self.last.diff(&next).into_iter())?;
        backend.flush()?;
        self.last = next;
        Ok(())
    }
}

/// Switches the session from `from` to the other presentation and returns
/// it. Going full screen clears the inline viewport first; coming back, the
/// cursor goes to the viewport's top before it is cleared and drawn again,
/// so it takes the same lines however often the session switches.
fn switch<B: Backend>(
    terminal: &mut Terminal<B>,
    full: &mut FullScreen,
    from: Presentation,
    screen: &mut impl FnMut(&mut B, Presentation) -> Result<(), B::Error>,
) -> Result<Presentation, B::Error> {
    let to = from.other();
    let top = terminal.get_frame().area().as_position();
    match to {
        Presentation::FullScreen => {
            terminal.clear()?;
            terminal.set_cursor_position(top)?;
            screen(terminal.backend_mut(), to)?;
            *full = FullScreen::default();
        }
        Presentation::Inline => {
            screen(terminal.backend_mut(), to)?;
            terminal.set_cursor_position(top)?;
            terminal.clear()?;
        }
    }
    Ok(to)
}
