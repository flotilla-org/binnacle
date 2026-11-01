//! The inline table's two presentations on a test backend: full screen
//! and inline are one session, `f` switching between them.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use binnacle_described::{DescribedSource, Message, Received, Wire};
use binnacle_format::Timestamp;
use binnacle_layout::Layout;
use binnacle_ratatui::cli::{Exit, InlineOptions, Presentation, run_inline_with};
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Size};
use ratatui::{Terminal, TerminalOptions, Viewport};
use support::replay;

/// The terminal's size, and the lines of prompt above the inline viewport.
const WIDTH: u16 = 100;
const HEIGHT: u16 = 20;
const PROMPT: u16 = 2;

fn key(code: KeyCode) -> Option<Event> {
    Some(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn char(c: char) -> Option<Event> {
    key(KeyCode::Char(c))
}

fn ctrl(c: char) -> Option<Event> {
    Some(Event::Key(KeyEvent::new(
        KeyCode::Char(c),
        KeyModifiers::CONTROL,
    )))
}

/// A test backend that behaves as a terminal does where the cursor goes:
/// drawing leaves it after the last cell drawn. It keeps each frame it
/// flushes, and grows to `grow`'s size at that frame.
struct Screen {
    inner: TestBackend,
    frames: Rc<RefCell<Vec<Buffer>>>,
    grow: Option<(usize, u16)>,
}

impl Backend for Screen {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I: Iterator<Item = (u16, u16, &'a Cell)>>(
        &mut self,
        content: I,
    ) -> Result<(), Self::Error> {
        let mut last = None;
        let content = content.inspect(|&(x, y, _)| last = Some((x, y)));
        self.inner.draw(content)?;
        match last {
            Some((x, y)) => self.inner.set_cursor_position((x, y)),
            None => Ok(()),
        }
    }
    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }
    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }
    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }
    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        self.inner.get_cursor_position()
    }
    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }
    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }
    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }
    fn size(&self) -> Result<Size, Self::Error> {
        self.inner.size()
    }
    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        self.inner.window_size()
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        let mut frames = self.frames.borrow_mut();
        frames.push(self.inner.buffer().clone());
        if let Some((at, height)) = self.grow
            && frames.len() == at
        {
            self.inner.resize(WIDTH, height);
        }
        self.inner.flush()
    }
}

/// The text of each of `buffer`'s lines, without trailing spaces.
fn lines(buffer: &Buffer) -> Vec<String> {
    let area = buffer.area;
    (area.top()..area.bottom())
        .map(|y| {
            let mut line = String::new();
            let mut x = area.left();
            while x < area.right() {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
            }
            line.trim_end().to_owned()
        })
        .collect()
}

/// A run of the table on a test terminal, its inline viewport `inline`
/// lines tall below the prompt.
struct Run {
    inline: u16,
    presentation: Presentation,
    grow: Option<(usize, u16)>,
}

impl Run {
    fn new() -> Run {
        Run {
            inline: 12,
            presentation: Presentation::Inline,
            grow: None,
        }
    }

    /// Every frame flushed (the last after exit), and how the table ended.
    fn run(
        &self,
        source: &mut DescribedSource,
        events: Vec<Option<Event>>,
        options: InlineOptions<'_, DescribedSource>,
    ) -> (Vec<Buffer>, Exit) {
        let frames = Rc::new(RefCell::new(Vec::new()));
        let mut backend = Screen {
            inner: TestBackend::new(WIDTH, HEIGHT),
            frames: Rc::clone(&frames),
            grow: self.grow,
        };
        backend.set_cursor_position((0, PROMPT)).unwrap();
        let viewport = Viewport::Inline(self.inline);
        let mut terminal = Terminal::with_options(backend, TerminalOptions { viewport }).unwrap();
        let options = InlineOptions {
            presentation: self.presentation,
            origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
            ..options
        };
        let exit = run_inline_with(&mut terminal, events, source, &Layout::default(), options);
        (frames.take(), exit.unwrap())
    }

    fn events(&self, events: Vec<Option<Event>>) -> (Vec<Buffer>, Exit) {
        let mut source = replay("crews.ndjson");
        self.run(&mut source, events, InlineOptions::default())
    }
}

/// The lines of the inline viewport, at its place below the prompt.
fn inline_lines(frame: &Buffer, inline: u16) -> Vec<String> {
    let lines = lines(frame);
    let (top, bottom) = (PROMPT as usize, (PROMPT + inline) as usize);
    assert!(
        lines[bottom..].iter().all(|l| l.is_empty()),
        "nothing below the inline viewport:\n{}",
        lines.join("\n")
    );
    lines[top..bottom].to_vec()
}

#[test]
fn full_screen_draws_over_the_whole_terminal_and_inline_at_its_height() {
    let full = Run {
        presentation: Presentation::FullScreen,
        ..Run::new()
    };
    let (frames, _) = full.events(vec![key(KeyCode::Esc)]);
    let first = lines(&frames[0]);
    assert!(first[0].contains("Name"), "the header on the first line");
    assert!(first[1].contains("alpha"));
    let status = first.last().unwrap();
    assert!(
        status.contains("6 rows") && status.contains("f inline"),
        "{status}"
    );

    let (frames, _) = Run::new().events(vec![key(KeyCode::Esc)]);
    let first = lines(&frames[0]);
    assert!(first[..PROMPT as usize].iter().all(|l| l.is_empty()));
    let inline = inline_lines(&frames[0], 12);
    assert!(inline[0].contains("Name"));
    assert!(inline[11].contains("f full screen"), "{}", inline[11]);
}

/// Sorts by name descending, filters to the rows with "br" in them, picks
/// the first two, toggling the presentation at `toggles`
/// (indices into those events), then Enter.
fn session_after(toggles: &[usize]) -> (Vec<Buffer>, Exit) {
    let mut events = vec![
        char('s'),
        char('s'),
        ctrl('l'),
        char('b'),
        char('r'),
        key(KeyCode::Enter),
        key(KeyCode::Home),
        char(' '),
        key(KeyCode::Down),
        char(' '),
    ];
    for &at in toggles.iter().rev() {
        events.insert(at, char('f'));
    }
    events.push(key(KeyCode::Enter));
    Run::new().events(events)
}

#[test]
fn toggling_keeps_the_session() {
    let (plain, chosen) = session_after(&[]);
    let Exit::Chosen(selection) = &chosen else {
        panic!("{chosen:?}");
    };
    assert_eq!(selection.rows.len(), 2, "two picks");
    // Full screen and back between each step, and full screen across the
    // sort, the filter and the picks.
    let (toggled, again) = session_after(&[0, 0, 2, 6, 7, 10]);
    assert_eq!(again, chosen);
    let before_exit = |frames: &[Buffer]| frames[frames.len() - 2].clone();
    assert_eq!(
        inline_lines(&before_exit(&toggled), 12),
        inline_lines(&before_exit(&plain), 12)
    );
    // Sort, filter, focus and picks draw as styles too.
    let viewport = |frame: Buffer| -> Vec<Cell> {
        let rows = PROMPT..PROMPT + 12;
        (rows.flat_map(|y| (0..WIDTH).map(move |x| (x, y))))
            .map(|at| frame[at].clone())
            .collect()
    };
    assert_eq!(
        viewport(before_exit(&toggled)),
        viewport(before_exit(&plain))
    );
    let shown = |frames: &[Buffer]| lines(&frames[frames.len() - 1]);
    assert_eq!(shown(&toggled)[2..], shown(&plain)[2..]);
}

#[test]
fn toggling_again_and_again_draws_inline_in_the_same_place() {
    // The terminal grows while full screen the third time.
    let run = Run {
        grow: Some((6, HEIGHT + 4)),
        ..Run::new()
    };
    let mut events = vec![char('f'); 10];
    events.push(key(KeyCode::Esc));
    let (frames, _) = run.events(events);
    let first = inline_lines(&frames[0], 12);
    for (i, frame) in frames.iter().enumerate().take(11) {
        match i % 2 {
            0 => assert_eq!(inline_lines(frame, 12), first, "frame {i}"),
            _ => assert!(lines(frame)[0].contains("Name"), "frame {i} full screen"),
        }
    }
}

#[test]
fn leaving_from_full_screen_leaves_the_rows_inline_would() {
    // Three rows show inline; full screen shows all six.
    let small = Run {
        inline: 6,
        ..Run::new()
    };
    let moves = || vec![key(KeyCode::Down), key(KeyCode::Down), key(KeyCode::Down)];
    for leave in [key(KeyCode::Enter), key(KeyCode::Esc)] {
        let mut inline = moves();
        inline.push(leave.clone());
        let (inline, inline_exit) = small.events(inline);
        let mut full = vec![char('f')];
        full.extend(moves());
        full.push(leave.clone());
        let (full, full_exit) = small.events(full);
        assert!(
            lines(&full[full.len() - 2])[0].contains("Name"),
            "full screen"
        );
        assert_eq!(full_exit, inline_exit);
        let left = |frames: &[Buffer]| lines(&frames[frames.len() - 1]);
        let (inline, full) = (left(&inline), left(&full));
        assert_eq!(full[PROMPT as usize..], inline[PROMPT as usize..]);
        let left = full[PROMPT as usize..].join("\n");
        assert!(
            left.contains("delta") && !left.contains("foxtrot"),
            "{left}"
        );
    }
}

#[test]
fn f_in_the_filter_bar_or_find_types_an_f() {
    for (open, field) in [(ctrl('l'), "Filter"), (ctrl('f'), "Find")] {
        let (frames, _) = Run::new().events(vec![open, char('f'), key(KeyCode::Esc)]);
        let after = &frames[2];
        let inline = inline_lines(after, 12);
        let bar = inline.iter().find(|l| l.trim_start().starts_with(field));
        let typed = bar.unwrap().trim_start().strip_prefix(field).unwrap();
        assert!(typed.trim_start().starts_with("f▏"), "{bar:?}");
        assert!(lines(after)[..PROMPT as usize].iter().all(|l| l.is_empty()));
    }
}

#[test]
fn live_changes_while_full_screen_show_and_stay_inline() {
    let mut source = replay("crews.ndjson");
    let calls = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&calls);
    // A row comes on the third frame, the second full screen.
    let live = Box::new(move |source: &mut DescribedSource| {
        *counted.borrow_mut() += 1;
        if *counted.borrow() != 3 {
            return Some(Vec::new());
        }
        let row = [("name".to_owned(), Wire::Text("alfa-zulu".into()))].into();
        let upsert = Message::Upsert {
            key: vec![Wire::Text("alfa-zulu".into())],
            row,
        };
        match source.receive(upsert) {
            Received::Applied(notices) => Some(notices),
            other => panic!("{other:?}"),
        }
    });
    let options = InlineOptions {
        live: Some(live),
        ..InlineOptions::default()
    };
    let events = vec![char('f'), None, None, char('f'), key(KeyCode::Esc)];
    let (frames, _) = Run::new().run(&mut source, events, options);
    let has = |frame: &Buffer| lines(frame).iter().any(|l| l.contains("alfa-zulu"));
    assert!(!has(&frames[1]));
    assert!(lines(&frames[2])[0].contains("Name"), "full screen");
    assert!(has(&frames[2]), "{:#?}", lines(&frames[2]));
    assert!(
        inline_lines(&frames[4], 12)
            .iter()
            .any(|l| l.contains("alfa-zulu"))
    );
}
