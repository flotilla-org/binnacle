//! The CLI host over a Described Table: `print` in every format, one Layout
//! giving the same rows and columns everywhere, and the inline viewport on
//! a test backend.

mod support;

#[path = "../examples/binnacle-tables/feed.rs"]
mod feed;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use binnacle_core::{Axis, Key};
use binnacle_described::{Decoder, DescribedSource, Encoding, Message, Received, Wire};
use binnacle_described_http::Event as Wired;
use binnacle_format::Timestamp;
use binnacle_layout::view::{
    AxisView, Condition, ConditionKind, Direction, FilterSpec, SortColumn, ViewSpec,
};
use binnacle_layout::{JsonId, Layout, Order};
use binnacle_ratatui::cli::{
    CliTable, Exit, Format, InlineOptions, Selection, print, print_rows, run_inline_with,
};
use ratatui::backend::{Backend, ClearType, TestBackend, WindowSize};
use ratatui::buffer::{Buffer, Cell};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Position, Size};
use ratatui::{Terminal, TerminalOptions, Viewport};
use serde_json::json;
use support::replay;

/// The crews table with three more rows: wide characters (CJK and emoji),
/// a multi-line note, and nulls.
fn crews() -> DescribedSource {
    let mut source = replay("crews.ndjson");
    for row in [
        json!({"name": "hotel 日本", "phase": "Running", "cpus": 16, "load": 12.5, "note": "first line\nsecond line", "host": "東京", "spec": {}}),
        json!({"name": "india 🚢", "phase": "Done", "cpus": 3, "note": "🚢🚢 ok", "host": null, "spec": {"a": [1]}}),
        json!({"name": "juliet", "spec": {}}),
    ] {
        let key = vec![Wire::from_json(&row["name"])];
        let Wire::Map(row) = Wire::from_json(&row) else {
            unreachable!()
        };
        assert!(matches!(
            source.receive(Message::Upsert { key, row }),
            Received::Applied(_)
        ));
    }
    source
}

fn printed(
    source: &mut DescribedSource,
    layout: &Layout<JsonId, JsonId>,
    format: Format,
) -> String {
    let mut out = Vec::new();
    let mut note = Vec::new();
    print(source, layout, format, &mut out, &mut note, None).unwrap();
    assert!(note.is_empty(), "a complete table has no note");
    String::from_utf8(out).unwrap()
}

fn id(name: &str) -> JsonId {
    JsonId::from(name)
}

#[test]
fn print_writes_each_format() {
    let layout = Layout::default();
    for (name, format) in [
        ("aligned", Format::Aligned),
        ("tsv", Format::Tsv),
        ("json", Format::Json),
        ("jsonl", Format::Jsonl),
    ] {
        let text = printed(&mut crews(), &layout, format);
        insta::assert_snapshot!(format!("print_{name}"), text);
    }
}

#[test]
fn aligned_text_has_no_trailing_spaces_and_aligns_by_display_width() {
    let text = printed(&mut crews(), &Layout::default(), Format::Aligned);
    assert!(text.lines().all(|l| l == l.trim_end()), "{text}");
    let lines: Vec<&str> = text.lines().collect();
    let column = |line: &str, at: &str| {
        let byte = line.find(at).unwrap();
        unicode_width::UnicodeWidthStr::width(&line[..byte])
    };
    let header = column(lines[0], "Phase");
    let hotel = lines.iter().find(|l| l.contains("hotel")).unwrap();
    let india = lines.iter().find(|l| l.contains("india")).unwrap();
    assert_eq!(column(hotel, "Running"), header, "after CJK");
    assert_eq!(column(india, "Done"), header, "after an emoji");
    assert!(!text.contains("second line"), "cut at the first line break");
}

/// A Layout with a sort, a filter, a hidden column and a column order.
fn arranged() -> Layout<JsonId, JsonId> {
    Layout {
        view: Some(ViewSpec {
            rows: AxisView {
                filter: FilterSpec {
                    quick: None,
                    conditions: vec![Condition {
                        id: id("phase"),
                        kind: ConditionKind::OneOf {
                            values: vec![Some("Running".into()), Some("Done".into())],
                        },
                        not: false,
                    }],
                },
                sort: vec![SortColumn {
                    id: id("load"),
                    direction: Direction::Descending,
                }],
                hidden: Vec::new(),
            },
            cols: AxisView {
                hidden: vec![id("spec"), id("started")],
                ..AxisView::default()
            },
            grouping: None,
        }),
        order: Some(Order {
            rows: None,
            cols: Some(vec![id("load"), id("name")]),
        }),
        ..Layout::default()
    }
}

/// The names in `text`'s rows, in order, and its column names.
const NAMES: [&str; 6] = [
    "hotel 日本",
    "delta",
    "echo",
    "alpha",
    "foxtrot",
    "india 🚢",
];
const FIELDS: [&str; 6] = ["load", "name", "phase", "cpus", "note", "host"];

fn names_in(text: &str) -> Vec<&'static str> {
    let mut found: Vec<(usize, &str)> = (NAMES.iter())
        .chain(["bravo", "charlie", "juliet"].iter())
        .filter_map(|n| text.find(n.split(' ').next().unwrap()).map(|at| (at, *n)))
        .collect();
    found.sort();
    found.into_iter().map(|(_, n)| n).collect()
}

#[test]
fn one_layout_gives_the_same_rows_and_columns_in_every_format() {
    let layout = arranged();
    let aligned = printed(&mut crews(), &layout, Format::Aligned);
    let header = aligned.lines().next().unwrap();
    let titles: Vec<&str> = header
        .split("  ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    assert_eq!(titles, ["Load", "Name", "Phase", "CPUs", "note", "Host"]);
    assert_eq!(names_in(&aligned), NAMES);
    let tsv = printed(&mut crews(), &layout, Format::Tsv);
    let fields: Vec<&str> = tsv.lines().next().unwrap().split('\t').collect();
    assert_eq!(fields, FIELDS);
    assert_eq!(names_in(&tsv), NAMES);
    for format in [Format::Json, Format::Jsonl] {
        let text = printed(&mut crews(), &layout, format);
        let rows: Vec<serde_json::Value> = match format {
            Format::Json => serde_json::from_str(&text).unwrap(),
            _ => text
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect(),
        };
        let names: Vec<&str> = rows.iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(names, NAMES);
        let first = text.lines().find(|l| l.contains('{')).unwrap();
        let keys: Vec<usize> = FIELDS
            .iter()
            .map(|f| first.find(&format!("\"{f}\"")).unwrap())
            .collect();
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "keys in column order: {first}"
        );
        assert!(!first.contains("\"spec\""), "hidden columns left out");
    }
    // The viewport's first frame shows the same.
    let mut source = crews();
    let (frames, _) = inline(&mut source, &layout, vec![key(KeyCode::Esc)], 100, 14);
    let first = &frames[0];
    let header = first.lines().find(|l| l.contains("Name")).unwrap();
    let at = |t: &str| header.find(t).unwrap();
    assert!(
        at("Load") < at("Name") && at("Name") < at("Phase") && at("CPUs") < at("Host"),
        "{header}"
    );
    assert!(
        !header.contains("Spec") && !header.contains("Started"),
        "{header}"
    );
    assert_eq!(names_in(first), NAMES, "{first}");
}

#[test]
fn machine_output_writes_raw_values() {
    let mut source = crews();
    let text = printed(&mut source, &Layout::default(), Format::Jsonl);
    let echo: serde_json::Value =
        serde_json::from_str(text.lines().find(|l| l.contains("echo")).unwrap()).unwrap();
    assert_eq!(echo["cpus"], json!(18446744073709551615u64), "u64 exact");
    assert_eq!(echo["started"], json!("2026-10-29T10:00:00.000000001Z"));
    let charlie = text.lines().find(|l| l.contains("charlie")).unwrap();
    assert!(
        charlie.contains("\"cpus\":\"many\""),
        "invalid as its raw text"
    );
    assert!(charlie.contains("\"started\":\"soon\""));
    let tsv = printed(&mut source, &Layout::default(), Format::Tsv);
    let hotel = tsv.lines().find(|l| l.contains("hotel")).unwrap();
    assert!(hotel.contains("first line\\nsecond line"), "{hotel}");
    let bravo = tsv.lines().find(|l| l.starts_with("bravo")).unwrap();
    assert!(bravo.contains("\t\t"), "nulls empty: {bravo}");
}

/// A key press.
fn key(code: KeyCode) -> Option<Event> {
    Some(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

/// A test backend that keeps each frame it flushes.
struct Recording {
    inner: TestBackend,
    frames: Rc<RefCell<Vec<Buffer>>>,
}

impl Backend for Recording {
    type Error = <TestBackend as Backend>::Error;

    fn draw<'a, I: Iterator<Item = (u16, u16, &'a Cell)>>(
        &mut self,
        content: I,
    ) -> Result<(), Self::Error> {
        self.inner.draw(content)
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
        self.frames.borrow_mut().push(self.inner.buffer().clone());
        self.inner.flush()
    }
}

fn text(buffer: &Buffer) -> String {
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
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs the inline table on a `width`×`height` test terminal with an inline
/// viewport of 12 lines below 2 lines of prompt; returns each frame's text
/// (the last after exit) and the choice.
fn inline(
    source: &mut DescribedSource,
    layout: &Layout<JsonId, JsonId>,
    events: Vec<Option<Event>>,
    width: u16,
    height: u16,
) -> (Vec<String>, Exit) {
    let options = InlineOptions {
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    inline_with(source, layout, events, width, height, options)
}

fn inline_with(
    source: &mut DescribedSource,
    layout: &Layout<JsonId, JsonId>,
    events: Vec<Option<Event>>,
    width: u16,
    height: u16,
    options: InlineOptions<'_, DescribedSource>,
) -> (Vec<String>, Exit) {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let mut backend = Recording {
        inner: TestBackend::new(width, height),
        frames: Rc::clone(&frames),
    };
    backend.set_cursor_position((0, 2)).unwrap();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(12),
        },
    )
    .unwrap();
    let chosen = run_inline_with(&mut terminal, events, source, layout, options).unwrap();
    let frames = frames.borrow().iter().map(text).collect();
    (frames, chosen)
}

fn row(source: &DescribedSource, name: &str) -> Key {
    source.row_key(&[Wire::Text(name.into())]).unwrap()
}

#[test]
fn the_viewport_draws_the_table_inline_below_the_prompt() {
    let mut source = crews();
    let (frames, _) = inline(
        &mut source,
        &Layout::default(),
        vec![key(KeyCode::Esc)],
        100,
        20,
    );
    let lines: Vec<&str> = frames[0].lines().collect();
    assert!(
        lines[..2].iter().all(|l| l.is_empty()),
        "the prompt's lines are left alone"
    );
    assert!(
        lines[2].contains("Name") && lines[2].contains("Phase"),
        "{}",
        frames[0]
    );
    assert!(lines[3].contains("alpha"));
    assert!(
        lines[14..].iter().all(|l| l.is_empty()),
        "nothing below the viewport"
    );
    insta::assert_snapshot!(frames[0]);
}

#[test]
fn enter_chooses_the_focused_row_and_with_picks_the_picks() {
    let mut source = crews();
    let (_, chosen) = inline(
        &mut source,
        &Layout::default(),
        vec![key(KeyCode::Enter)],
        100,
        20,
    );
    assert_eq!(
        chosen,
        Exit::Chosen(Selection {
            rows: vec![row(&source, "alpha")]
        })
    );
    let events = vec![key(KeyCode::Down), key(KeyCode::Enter)];
    let (_, chosen) = inline(&mut source, &Layout::default(), events, 100, 20);
    assert_eq!(
        chosen,
        Exit::Chosen(Selection {
            rows: vec![row(&source, "bravo")]
        })
    );
    let events = vec![
        key(KeyCode::Char(' ')),
        key(KeyCode::Down),
        key(KeyCode::Down),
        key(KeyCode::Char(' ')),
        key(KeyCode::Down),
        key(KeyCode::Enter),
    ];
    let (_, chosen) = inline(&mut source, &Layout::default(), events, 100, 20);
    let want = vec![row(&source, "alpha"), row(&source, "charlie")];
    assert_eq!(
        chosen,
        Exit::Chosen(Selection { rows: want }),
        "the picks, not the focus"
    );
}

#[test]
fn esc_and_q_choose_nothing() {
    let mut source = crews();
    for code in [KeyCode::Esc, KeyCode::Char('q')] {
        let (_, chosen) = inline(&mut source, &Layout::default(), vec![key(code)], 100, 20);
        assert_eq!(chosen, Exit::Left);
    }
}

#[test]
fn after_exit_the_viewport_holds_the_rows_shown_as_aligned_text() {
    let mut source = crews();
    let layout = arranged();
    let (frames, _) = inline(&mut source, &layout, vec![key(KeyCode::Esc)], 100, 20);
    let last = frames.last().unwrap();
    let rows: Vec<Key> = NAMES.iter().map(|n| row(&source, n)).collect();
    let cols: Vec<Key> = FIELDS
        .iter()
        .map(|f| source.column_key(f).unwrap())
        .collect();
    let mut want = Vec::new();
    print_rows(&source, &rows, &cols, Format::Aligned, &mut want).unwrap();
    let want = String::from_utf8(want).unwrap();
    let shown: Vec<&str> = last.lines().skip(2).take(want.lines().count()).collect();
    assert_eq!(shown, want.lines().collect::<Vec<_>>(), "{last}");
    assert!(last.lines().skip(2 + 7).all(|l| l.is_empty()), "{last}");
}

#[test]
fn a_change_notice_from_the_live_feed_shows_in_the_next_frame() {
    let mut source = crews();
    let calls = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&calls);
    let live = Box::new(move |source: &mut DescribedSource| {
        *counted.borrow_mut() += 1;
        if *counted.borrow() != 2 {
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
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    let events = vec![None, None, key(KeyCode::Esc)];
    let (frames, _) = inline_with(&mut source, &Layout::default(), events, 100, 20, options);
    assert!(!frames[0].contains("alfa-zulu"));
    assert!(frames[1].contains("alfa-zulu"), "{}", frames[1]);
    assert!(*calls.borrow() >= 2);
}

/// The rows chosen and the last frame after focus moves down twice in a
/// view sorted by CPUs, the live feed sending `resyncs` after the moves:
/// each a `resync` or a stream end, then, a frame later, the table again,
/// so the source is unsynced in between, as over a network.
fn chosen_after(resyncs: &[bool]) -> (Exit, String) {
    let mut source = replay("crews.ndjson");
    let fields: Vec<String> = (source.columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    let flags = binnacle_ratatui::cli::Flags::parse(["--sort", "-cpus"]).unwrap();
    let layout = flags.apply(&Layout::default(), &fields).unwrap();
    // Each resync is two steps: its start (`Some(stream_end)`), then the
    // table again (`None`).
    let mut steps: Vec<Option<bool>> = (resyncs.iter())
        .flat_map(|&end| [Some(end), None])
        .rev()
        .collect();
    let frames = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&frames);
    let live = Box::new(move |source: &mut DescribedSource| {
        *counted.borrow_mut() += 1;
        // The first three frames: the table, then each move.
        if *counted.borrow() <= 3 {
            return Some(Vec::new());
        }
        let received: Vec<Received> = match steps.pop() {
            None => Vec::new(),
            Some(Some(true)) => vec![source.ended()],
            Some(Some(false)) => vec![source.receive(Message::Resync)],
            Some(None) => (support::messages("crews.ndjson").into_iter())
                .map(|m| source.receive(m))
                .collect(),
        };
        let mut notices = Vec::new();
        for received in received {
            if let Received::Applied(n) = received {
                notices.extend(n);
            }
        }
        Some(notices)
    });
    let options = InlineOptions {
        live: Some(live),
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    let mut events = vec![None, key(KeyCode::Down), key(KeyCode::Down)];
    events.extend(resyncs.iter().flat_map(|_| [None, None]));
    events.push(key(KeyCode::Enter));
    let (frames, chosen) = inline_with(&mut source, &layout, events, 100, 20, options);
    (chosen, frames[frames.len() - 2].clone())
}

#[test]
fn a_resync_and_a_reconnect_keep_the_sort_and_the_focus() {
    let mut source = replay("crews.ndjson");
    let flags = binnacle_ratatui::cli::Flags::parse(["--sort", "-cpus"]).unwrap();
    let fields: Vec<String> = (source.columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    let layout = flags.apply(&Layout::default(), &fields).unwrap();
    let jsonl = printed(&mut source, &layout, Format::Jsonl);
    let third: serde_json::Value = serde_json::from_str(jsonl.lines().nth(2).unwrap()).unwrap();
    let third = row(&source, third["name"].as_str().unwrap());

    let (chosen, frame) = chosen_after(&[false, true]);
    assert_eq!(chosen, Exit::Chosen(Selection { rows: vec![third] }));
    assert_eq!((chosen, frame), chosen_after(&[]));
}

#[test]
fn the_hosts_note_leads_the_status_line_while_it_has_one() {
    let mut source = crews();
    let frames = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&frames);
    let note = Box::new(move || {
        *counted.borrow_mut() += 1;
        (*counted.borrow() == 1).then(|| "reconnecting (503)".to_owned())
    });
    let options = InlineOptions {
        note: Some(note),
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    let events = vec![None, key(KeyCode::Esc)];
    let (frames, _) = inline_with(&mut source, &Layout::default(), events, 100, 20, options);
    assert!(
        frames[0].contains("reconnecting (503) | 9 rows"),
        "{}",
        frames[0]
    );
    assert!(!frames[1].contains("reconnecting"), "{}", frames[1]);
}

#[test]
fn a_live_feed_that_ends_closes_the_viewport_choosing_nothing() {
    let mut source = crews();
    let calls = Rc::new(RefCell::new(0));
    let counted = Rc::clone(&calls);
    let live = Box::new(move |_: &mut DescribedSource| {
        *counted.borrow_mut() += 1;
        (*counted.borrow() < 2).then(Vec::new)
    });
    let options = InlineOptions {
        live: Some(live),
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    // Enter would choose alpha, had the feed not ended first.
    let events = vec![None, key(KeyCode::Enter)];
    let (frames, chosen) = inline_with(&mut source, &Layout::default(), events, 100, 20, options);
    assert_eq!(chosen, Exit::Left);
    assert_eq!(*calls.borrow(), 2);
    assert!(frames.last().unwrap().contains("alpha"), "the rows stay");
}

#[test]
fn the_described_source_names_rows_by_primary_key_and_columns_by_field() {
    let source = crews();
    let alpha = row(&source, "alpha");
    let ids: &dyn binnacle_layout::IdMap<JsonId, JsonId> = &source;
    let alpha_id = ids.row_id(alpha).unwrap();
    assert_eq!(alpha_id.value(), &json!(["alpha"]));
    assert_eq!(ids.row_key(&alpha_id), Some(alpha));
    let load = source.column_key("load").unwrap();
    assert_eq!(ids.col_id(load), Some(id("load")));
    assert_eq!(CliTable::field(&source, load), "load");
    assert_eq!(CliTable::header(&source, load), "Load");
    assert_eq!(CliTable::count(&source, Axis::Rows), 9);
}

#[test]
fn json_writes_non_finite_numbers_and_bytes_as_strings() {
    use binnacle_described::{Field, FieldType, RowsMessage, Schema, Scope, TableMessage};
    let mut source = DescribedSource::new("t", Scope::new("n"));
    source.receive(Message::Table(TableMessage {
        schema: Schema {
            fields: vec![
                Field::new("id", FieldType::Integer),
                Field::new("x", FieldType::Number),
                Field::new("raw", FieldType::Other("binary".into())),
            ],
            primary_key: vec!["id".into()],
            foreign_keys: Vec::new(),
        },
        incarnation: None,
        table: None,
        sequence: None,
    }));
    let rows = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY]
        .into_iter()
        .enumerate()
        .map(|(i, x)| {
            [
                ("id".to_owned(), Wire::Int(i as i128)),
                ("x".to_owned(), Wire::Float(x)),
                ("raw".to_owned(), Wire::Bytes(vec![0xde, 0xad, i as u8])),
            ]
            .into()
        })
        .collect();
    source.receive(Message::Rows(RowsMessage {
        rows,
        cursor: None,
        partial: false,
        sequence: None,
    }));
    let text = printed(&mut source, &Layout::default(), Format::Jsonl);
    assert_eq!(
        text,
        "{\"id\":0,\"x\":\"NaN\",\"raw\":\"dead00\"}\n{\"id\":1,\"x\":\"inf\",\"raw\":\"dead01\"}\n{\"id\":2,\"x\":\"-inf\",\"raw\":\"dead02\"}\n"
    );
    let tsv = printed(&mut source, &Layout::default(), Format::Tsv);
    assert_eq!(tsv.lines().nth(1), Some("0\tNaN\tdead00"));
}

#[test]
fn the_clap_group_converts_to_the_parsers_input() {
    use binnacle_ratatui::cli::{FlagArgs, Flags, Interactive};
    let args = FlagArgs {
        sort: vec!["-cpus".into(), "name".into()],
        wheres: vec!["phase==Running".into()],
        wide: true,
        no_interactive: true,
        format: Some("tsv".into()),
        ..FlagArgs::default()
    };
    let want = Flags::parse([
        "--sort",
        "-cpus,name",
        "--where",
        "phase==Running",
        "--wide",
        "--no-interactive",
        "--format",
        "tsv",
    ])
    .unwrap();
    assert_eq!(args.flags().unwrap(), want);
    assert_eq!(want.interactive, Interactive::Never);
}

#[test]
fn a_title_with_control_characters_prints_without_them() {
    use binnacle_described::{Field, RowsMessage, Scope, TableMessage};
    let mut title = Field::new("x", binnacle_described::FieldType::String)
        .properties()
        .clone();
    title.insert("title".into(), json!("red\u{1b}[31m title\u{7}"));
    let mut source = DescribedSource::new("t", Scope::new("n"));
    source.receive(Message::Table(TableMessage {
        schema: binnacle_described::Schema {
            fields: vec![Field::from_properties(title).unwrap()],
            primary_key: vec!["x".into()],
            foreign_keys: Vec::new(),
        },
        incarnation: None,
        table: None,
        sequence: None,
    }));
    let row = [("x".to_owned(), Wire::Text("a\u{1b}]52;c;bad\u{7}".into()))].into();
    source.receive(Message::Rows(RowsMessage {
        rows: vec![row],
        cursor: None,
        partial: false,
        sequence: None,
    }));
    let text = printed(&mut source, &Layout::default(), Format::Aligned);
    assert!(
        !text.contains('\u{1b}') && !text.contains('\u{7}'),
        "{text:?}"
    );
    assert!(text.starts_with("red"), "{text:?}");
    let tsv = printed(&mut source, &Layout::default(), Format::Tsv);
    assert_eq!(tsv.lines().nth(1), Some("a\\x1b]52;c;bad\\x07"), "{tsv:?}");
}

#[test]
fn enter_in_an_empty_table_chooses_nothing() {
    let mut source = crews();
    let flags = binnacle_ratatui::cli::Flags::parse(["--where", "name==nobody"]).unwrap();
    let fields: Vec<String> = source
        .columns()
        .iter()
        .map(|c| c.name().to_owned())
        .collect();
    let layout = flags.apply(&Layout::default(), &fields).unwrap();
    let (_, chosen) = inline(&mut source, &layout, vec![key(KeyCode::Enter)], 100, 20);
    assert_eq!(chosen, Exit::Left);
}

/// The crews table as a server over its row cap sends it: the first page
/// with a cursor to more.
fn partial_crews() -> DescribedSource {
    let mut source = DescribedSource::new("crews", binnacle_described::Scope::new("flotilla"));
    let _ = source.poll_request();
    for message in support::messages("crews.ndjson") {
        let message = match message {
            Message::Rows(mut rows) => {
                rows.cursor = Some("page-2".into());
                Message::Rows(rows)
            }
            other => other,
        };
        source.receive(message);
    }
    assert!(source.status().partial);
    source
}

/// What `print` writes to stdout and to the note for `source` as `layout`
/// presents it, with the host's `hint`.
fn printed_partial(
    source: &mut DescribedSource,
    layout: &Layout<JsonId, JsonId>,
    hint: Option<&str>,
) -> (String, String) {
    let (mut out, mut note) = (Vec::new(), Vec::new());
    print(source, layout, Format::Jsonl, &mut out, &mut note, hint).unwrap();
    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(note).unwrap(),
    )
}

/// The partial crews table filtered to the Running phase: its layout, and
/// how many rows that leaves of the 6 that came.
fn running_of_partial() -> (DescribedSource, Layout<JsonId, JsonId>, usize) {
    let mut source = partial_crews();
    let base = source.presentation(None, None);
    let flags = binnacle_ratatui::cli::Flags::parse(["--where", "phase==Running"]).unwrap();
    let layout = flags.apply_to(&mut source, &base, |_| None).unwrap();
    let shown = printed_partial(&mut source, &layout, None)
        .0
        .lines()
        .count();
    assert!(0 < shown && shown < 6, "the filter drops some: {shown}");
    (source, layout, shown)
}

#[test]
fn a_partial_table_prints_its_rows_then_says_so_apart() {
    let (out, note) = printed_partial(&mut partial_crews(), &Layout::default(), None);
    assert_eq!(out.lines().count(), 6);
    assert_eq!(note, "showing 6 rows read; the table has more\n");
}

#[test]
fn the_partial_note_takes_the_hosts_hint_and_never_suggests_where() {
    let hint = "narrow the server's read with --scope-where";
    let (_, note) = printed_partial(&mut partial_crews(), &Layout::default(), Some(hint));
    assert_eq!(
        note,
        format!("showing 6 rows read; the table has more; {hint}\n")
    );
    let (_, bare) = printed_partial(&mut partial_crews(), &Layout::default(), None);
    assert!(!bare.contains("--where"), "{bare}");
}

#[test]
fn under_a_filter_the_partial_note_says_both_numbers() {
    let (mut source, layout, shown) = running_of_partial();
    let hint = "narrow the server's read with --scope-where";
    let (out, note) = printed_partial(&mut source, &layout, Some(hint));
    assert_eq!(out.lines().count(), shown);
    assert_eq!(
        note,
        format!("showing {shown} of 6 rows read; the table has more; {hint}\n")
    );
    assert!(!note.contains("--where"), "{note}");
}

#[test]
fn the_viewport_of_a_partial_table_says_of_more() {
    let mut source = partial_crews();
    let (frames, _) = inline(
        &mut source,
        &Layout::default(),
        vec![key(KeyCode::Esc)],
        100,
        20,
    );
    assert!(
        frames[0].contains("6 of more rows | Enter"),
        "{}",
        frames[0]
    );
    let (mut source, layout, shown) = running_of_partial();
    let (frames, _) = inline(&mut source, &layout, vec![key(KeyCode::Esc)], 100, 20);
    let want = format!("{shown} shown of 6 read, more on the server | Enter");
    assert!(frames[0].contains(&want), "{}", frames[0]);
    let (frames, _) = inline(
        &mut crews(),
        &Layout::default(),
        vec![key(KeyCode::Esc)],
        100,
        20,
    );
    assert!(!frames[0].contains("of more"), "{}", frames[0]);
}

/// The binnacle-tables example, built now: a run of this test alone must
/// not use a binary an earlier build left. Cargo does nothing when it is
/// fresh.
fn example() -> std::path::PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let built = std::process::Command::new(cargo)
        .args([
            "build",
            "-p",
            "binnacle-described",
            "--example",
            "binnacle-tables",
        ])
        .status()
        .unwrap();
    assert!(built.success());
    let deps = std::env::current_exe().unwrap();
    let examples = deps.parent().unwrap().parent().unwrap().join("examples");
    examples.join(format!("binnacle-tables{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn binnacle_tables_over_the_row_cap_prints_the_rows_and_notes_it_on_stderr() {
    use binnacle_testkit::fake_tables::FakeTables;
    let fake = FakeTables::new()
        .generate("big", 10_500)
        .generate("small", 50)
        .start()
        .unwrap();
    let url = fake.url().unwrap();
    let run = |table: &str| {
        std::process::Command::new(example())
            .args(["--url", &url, "--table", table, "--format", "jsonl"])
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    };
    let big = run("big");
    let stderr = String::from_utf8_lossy(&big.stderr);
    assert_eq!(big.status.code(), Some(0), "{stderr}");
    assert_eq!(String::from_utf8_lossy(&big.stdout).lines().count(), 10_000);
    assert_eq!(
        stderr,
        "binnacle-tables: showing 10,000 rows read; the table has more; narrow the server's read with --scope-where\n"
    );
    let small = run("small");
    assert_eq!(small.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&small.stdout).lines().count(), 50);
    assert!(
        small.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&small.stderr)
    );
}

/// The crews table with the generic default Layout, which pins the key
/// (name) and state (phase) columns leading, patched with `args`.
fn pinned_with(args: &[&str]) -> (DescribedSource, Layout<JsonId, JsonId>) {
    let mut source = replay("crews.ndjson");
    let base = source.presentation(None, None);
    let pins = base.pins.as_ref().unwrap();
    assert_eq!(
        pins.cols.leading,
        [id("name"), id("phase")],
        "the base pins"
    );
    let flags = binnacle_ratatui::cli::Flags::parse(args).unwrap();
    let layout = flags.apply_to(&mut source, &base, |_| None).unwrap();
    (source, layout)
}

#[test]
fn columns_sets_the_order_over_the_bases_pins() {
    let (mut source, layout) = pinned_with(&["--columns", "host,phase,name"]);
    let aligned = printed(&mut source, &layout, Format::Aligned);
    let titles: Vec<&str> = aligned.lines().next().unwrap().split_whitespace().collect();
    assert_eq!(titles, ["Host", "Phase", "Name"]);
    let tsv = printed(&mut source, &layout, Format::Tsv);
    assert_eq!(tsv.lines().next(), Some("host\tphase\tname"));
    let jsonl = printed(&mut source, &layout, Format::Jsonl);
    let first = jsonl.lines().next().unwrap();
    let at = |f: &str| first.find(&format!("\"{f}\"")).unwrap();
    assert!(
        at("host") < at("phase") && at("phase") < at("name"),
        "{first}"
    );
    // The base pinned two of the named: the list's first two are pinned.
    let pins = layout.pins.as_ref().unwrap();
    assert_eq!(pins.cols.leading, [id("host"), id("phase")]);
    // A pin on a column the list leaves out goes.
    let (mut source, layout) = pinned_with(&["--columns", "cpus,name"]);
    assert_eq!(layout.pins.as_ref().unwrap().cols.leading, [id("cpus")]);
    let tsv = printed(&mut source, &layout, Format::Tsv);
    assert_eq!(tsv.lines().next(), Some("cpus\tname"));
    // In the viewport too.
    let (frames, _) = inline(&mut source, &layout, vec![key(KeyCode::Esc)], 100, 20);
    let header = frames[0].lines().find(|l| l.contains("Name")).unwrap();
    assert!(header.find("CPUs") < header.find("Name"), "{header}");
}

#[test]
fn a_comparison_the_column_cant_apply_is_an_error() {
    let mut source = replay("crews.ndjson");
    let base = Layout::default();
    let apply = |source: &mut DescribedSource, clause: &str| {
        let flags = binnacle_ratatui::cli::Flags::parse(["--where", clause]).unwrap();
        flags.apply_to(source, &base, |_| None)
    };
    let error = apply(&mut source, "phase>3").unwrap_err().to_string();
    assert!(
        error.contains("`phase`") && error.contains("string"),
        "names the column and its type: {error}"
    );
    let error = apply(&mut source, "cpus>2026-10-29T00:00:00Z").unwrap_err();
    assert!(error.to_string().contains("integer"), "{error}");
    assert!(apply(&mut source, "cpus>3").is_ok());
    assert!(apply(&mut source, "started>2026-10-29T00:00:00Z").is_ok());
    assert!(apply(&mut source, "phase==Running").is_ok());
}

/// A key press with `modifiers`.
fn chord(code: KeyCode, modifiers: KeyModifiers) -> Option<Event> {
    Some(Event::Key(KeyEvent::new(code, modifiers)))
}

#[test]
fn ctrl_c_leaves_choosing_nothing_and_says_it_was_interrupted() {
    let mut source = crews();
    let ctrl_c = chord(KeyCode::Char('c'), KeyModifiers::CONTROL);
    // Over picks, and with the find bar open, as well.
    let events = vec![
        key(KeyCode::Char(' ')),
        chord(KeyCode::Char('f'), KeyModifiers::CONTROL),
        ctrl_c,
        key(KeyCode::Enter),
    ];
    let (frames, exit) = inline(&mut source, &Layout::default(), events, 100, 20);
    assert_eq!(exit, Exit::Interrupted { signal: 2 });
    assert_eq!(exit.status(), Some(130));
    assert_eq!(exit.selection(), None);
    assert!(frames.last().unwrap().contains("alpha"), "the rows stay");
}

/// The names in the first frame after `events`, in the order they show.
fn names_after(events: Vec<Option<Event>>) -> (Vec<&'static str>, String) {
    let mut events = events;
    events.push(key(KeyCode::Esc));
    let count = events.len();
    let (frames, _) = inline(&mut crews(), &Layout::default(), events, 100, 20);
    let frame = frames[count - 1].clone();
    (names_in(&frame), frame)
}

#[test]
fn s_cycles_the_focused_columns_sort() {
    // Focus starts on Name; Right moves it to Phase, then Started, CPUs.
    let right = || key(KeyCode::Right);
    let s = || key(KeyCode::Char('s'));
    let on_cpus = || vec![right(), right(), right()];
    let (unsorted, _) = names_after(on_cpus());
    let (up, frame) = names_after([on_cpus(), vec![s()]].concat());
    assert!(frame.contains('▲'), "{frame}");
    let (down, frame) = names_after([on_cpus(), vec![s(), s()]].concat());
    assert!(frame.contains('▼'), "{frame}");
    let (off, _) = names_after([on_cpus(), vec![s(), s(), s()]].concat());
    // By CPUs: 2 bravo, 3 india, 4 alpha, 8 foxtrot, 16 hotel, then the
    // huge echo, with the nulls and charlie's invalid value apart.
    let by_cpus = [
        "bravo",
        "india 🚢",
        "alpha",
        "foxtrot",
        "hotel 日本",
        "echo",
    ];
    let numbered = |names: &[&str]| -> Vec<String> {
        (names.iter())
            .filter(|n| by_cpus.contains(n))
            .map(|n| n.to_string())
            .collect()
    };
    assert_eq!(numbered(&up), by_cpus, "ascending: {up:?}");
    let mut reversed = by_cpus;
    reversed.reverse();
    assert_eq!(numbered(&down), reversed, "descending: {down:?}");
    assert_ne!(up, unsorted);
    assert_eq!(off, unsorted, "then none");
}

#[test]
fn picks_come_back_in_the_order_they_show() {
    let fields: Vec<String> = (crews().columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    // By name descending, echo is the 5th row and charlie the 7th; one way
    // or the other, Key order is not the order shown.
    for (sort, (charlie, echo), want) in [
        ("-name", (6, 4), ["echo", "charlie"]),
        ("name", (2, 4), ["charlie", "echo"]),
    ] {
        let mut source = crews();
        let flags = binnacle_ratatui::cli::Flags::parse(["--sort", sort]).unwrap();
        let layout = flags.apply(&Layout::default(), &fields).unwrap();
        // Pick charlie, then echo.
        let mut events = vec![key(KeyCode::Down); charlie];
        events.push(key(KeyCode::Char(' ')));
        let (from, to, step) = match echo > charlie {
            true => (charlie, echo, KeyCode::Down),
            false => (echo, charlie, KeyCode::Up),
        };
        events.extend(vec![key(step); to - from]);
        events.push(key(KeyCode::Char(' ')));
        events.push(key(KeyCode::Enter));
        let (_, exit) = inline(&mut source, &layout, events, 100, 20);
        let want: Vec<Key> = want.iter().map(|n| row(&source, n)).collect();
        assert_eq!(
            exit,
            Exit::Chosen(Selection { rows: want }),
            "sorted {sort}"
        );
    }
}

#[test]
fn the_rows_left_behind_wrap_rather_than_cut_at_a_narrow_terminal() {
    let mut source = crews();
    let flags = binnacle_ratatui::cli::Flags::parse(["--wide"]).unwrap();
    let fields: Vec<String> = (source.columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    let layout = flags.apply(&Layout::default(), &fields).unwrap();
    let (frames, _) = inline(&mut source, &layout, vec![key(KeyCode::Esc)], 50, 40);
    let last = frames.last().unwrap();
    assert!(
        last.lines()
            .all(|l| unicode_width::UnicodeWidthStr::width(l) <= 50),
        "{last}"
    );
    // Every value of the right-aligned CPUs column is there, whole, under
    // its header.
    let joined: String = last.lines().collect::<Vec<_>>().concat();
    assert!(joined.contains("CPUs"), "{last}");
    for cpus in ["18446744073709551615", "many"] {
        assert!(joined.contains(cpus), "{cpus}: {last}");
    }
    let alpha = last.lines().position(|l| l.starts_with("alpha")).unwrap();
    let bravo = last.lines().position(|l| l.starts_with("bravo")).unwrap();
    let alpha_row: String = last.lines().skip(alpha).take(bravo - alpha).collect();
    assert!(alpha_row.contains(" 4 "), "alpha's 4: {alpha_row}");
}

#[test]
fn wide_characters_end_their_column_at_the_same_display_column() {
    use unicode_width::UnicodeWidthStr;
    let text = printed(&mut crews(), &Layout::default(), Format::Aligned);
    let lines: Vec<&str> = text.lines().collect();
    // Where the column after `column` starts: one past its widest cell and
    // the two spaces, by display width.
    let next = |line: &str, after: &str| {
        let end = line.find(after).unwrap() + after.len();
        let rest = &line[end..];
        UnicodeWidthStr::width(&line[..end]) + (rest.len() - rest.trim_start().len())
    };
    let hotel = lines.iter().find(|l| l.starts_with("hotel")).unwrap();
    let india = lines.iter().find(|l| l.starts_with("india")).unwrap();
    let alpha = lines.iter().find(|l| l.starts_with("alpha")).unwrap();
    let phase = next(hotel, "hotel 日本");
    assert_eq!(
        phase,
        "hotel 日本".width() + 2,
        "the widest name sets the width"
    );
    assert_eq!(next(india, "india 🚢"), phase);
    assert_eq!(next(alpha, "alpha"), phase);
    // The Host column after its widest, 東京, and 🚢🚢 ok in note.
    let host_at = |line: &str, value: &str| {
        let byte = line.rfind(value).unwrap();
        UnicodeWidthStr::width(&line[..byte])
    };
    assert_eq!(
        host_at(hotel, "東京"),
        host_at(alpha, "feta"),
        "after first line"
    );
    let spec_at = |line: &str| host_at(line, "{");
    assert_eq!(spec_at(hotel), spec_at(alpha), "after 東京");
    assert_eq!(spec_at(india), spec_at(alpha), "after 🚢🚢 ok");
}

#[test]
fn a_title_aligns_as_its_columns_cells() {
    use unicode_width::UnicodeWidthStr;
    let text = printed(&mut crews(), &Layout::default(), Format::Aligned);
    let lines: Vec<&str> = text.lines().collect();
    let end = |line: &str, value: &str| {
        let byte = line.find(value).unwrap() + value.len();
        UnicodeWidthStr::width(&line[..byte])
    };
    let start =
        |line: &str, value: &str| UnicodeWidthStr::width(&line[..line.find(value).unwrap()]);
    let echo = lines.iter().find(|l| l.starts_with("echo")).unwrap();
    assert_eq!(
        end(lines[0], "CPUs"),
        end(echo, "18446744073709551615"),
        "a number column's, right"
    );
    assert_eq!(end(lines[0], "Load"), end(echo, "2%"));
    assert_eq!(
        start(lines[0], "Phase"),
        start(echo, "Done"),
        "a text column's, left"
    );
}

/// A terminal `width`×`height` with the viewport on its last lines, the
/// inline table run on it, then a shell writing `SHELL$` where the cursor
/// stands, as a shell's next output does. Returns the terminal's lines
/// (the scrollback before them is not included).
fn inline_then_shell(
    source: &mut DescribedSource,
    layout: &Layout<JsonId, JsonId>,
    width: u16,
    height: u16,
) -> Vec<String> {
    let options = InlineOptions {
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    let frames = Rc::new(RefCell::new(Vec::new()));
    let mut backend = Recording {
        inner: TestBackend::new(width, height),
        frames: Rc::clone(&frames),
    };
    backend.set_cursor_position((0, 2)).unwrap();
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(12),
        },
    )
    .unwrap();
    run_inline_with(
        &mut terminal,
        vec![key(KeyCode::Esc)],
        source,
        layout,
        options,
    )
    .unwrap();
    let backend = terminal.backend_mut();
    let at = backend.get_cursor_position().unwrap();
    let cells: Vec<Cell> = (("SHELL$").chars())
        .map(|c| {
            let mut cell = Cell::default();
            cell.set_char(c);
            cell
        })
        .collect();
    backend
        .draw(
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| (at.x + i as u16, at.y, c)),
        )
        .unwrap();
    backend.flush().unwrap();
    text(frames.borrow().last().unwrap())
        .lines()
        .map(str::to_owned)
        .collect()
}

/// `line` in pieces at most `width` columns wide, as the rows left behind
/// wrap.
fn wrapped(line: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;
    let (mut pieces, mut piece, mut used) = (Vec::new(), String::new(), 0);
    for c in line.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width && !piece.is_empty() {
            pieces.push(std::mem::take(&mut piece).trim_end().to_owned());
            used = 0;
        }
        piece.push(c);
        used += w;
    }
    pieces.push(piece.trim_end().to_owned());
    pieces
}

#[test]
fn the_shells_next_output_leaves_the_rows_left_behind_whole() {
    let flags = binnacle_ratatui::cli::Flags::parse(["--wide"]).unwrap();
    let wide = crews();
    let fields: Vec<String> = (wide.columns().iter())
        .map(|c| c.name().to_owned())
        .collect();
    let wide_layout = flags.apply(&Layout::default(), &fields).unwrap();
    for (layout, width) in [
        (&wide_layout, 40),
        (&wide_layout, 50),
        (&Layout::default(), 60),
    ] {
        // The terminal ends at the viewport's last line (14) and beyond it,
        // so the wrapped rows reach the bottom for some heights.
        for height in 14..=30 {
            let mut source = crews();
            let expected: Vec<String> = printed(&mut source, layout, Format::Aligned)
                .lines()
                .flat_map(|l| wrapped(l, width.into()))
                .collect();
            let lines = inline_then_shell(&mut source, layout, width, height);
            let shell = lines.iter().position(|l| l == "SHELL$").unwrap_or_else(|| {
                panic!("{width}x{height}: the shell's line was not written alone:\n{lines:#?}")
            });
            let above = &lines[..shell];
            let k = above.len().min(expected.len());
            assert_eq!(
                above[above.len() - k..],
                expected[expected.len() - k..],
                "{width}x{height}"
            );
            assert!(
                lines[shell + 1..].iter().all(String::is_empty),
                "{width}x{height}: {lines:#?}"
            );
        }
    }
}

#[test]
fn the_viewports_headers_line_up_with_their_cells() {
    let (frames, _) = inline(
        &mut crews(),
        &Layout::default(),
        vec![key(KeyCode::Esc)],
        140,
        20,
    );
    let frame = &frames[0];
    let lines: Vec<&str> = frame.lines().collect();
    let header = lines.iter().find(|l| l.contains("CPUs")).unwrap();
    let alpha = lines.iter().find(|l| l.contains("alpha")).unwrap();
    // Columns, not bytes: the header holds multi-byte borders.
    let column = |line: &str, at: &str| line[..line.find(at).unwrap()].chars().count();
    let end = column(header, "CPUs") + "CPUs".len();
    assert_eq!(
        alpha.chars().nth(end - 1),
        Some('4'),
        "CPUs ends over alpha's 4:\n{frame}"
    );
    let phase = column(header, "Phase");
    let under: String = alpha.chars().skip(phase).collect();
    assert!(under.starts_with("Running"), "{frame}");
}

/// The error a corrupted live message decodes to.
fn corrupted() -> binnacle_described::DecodeError {
    let mut decoder = Decoder::new(Encoding::Ndjson);
    decoder.feed(b"{\"type\": nonsense\n");
    decoder.next().unwrap().unwrap_err()
}

/// Runs `drain` over `events` for a generous budget.
fn drained(source: &mut DescribedSource, events: Vec<Wired>, notes: &mut feed::Notes) {
    let mut events: VecDeque<Wired> = events.into();
    let fed = feed::drain(
        source,
        || events.pop_front(),
        |_| {},
        notes,
        Duration::from_secs(5),
    );
    assert_eq!(fed, Ok(Vec::new()));
}

#[test]
fn the_retrying_note_shows_the_status_and_the_bodys_first_line() {
    let retry = |status, body: &str| Wired::Retrying {
        attempt: 2,
        wait: Duration::from_millis(500),
        status,
        body: body.as_bytes().to_vec(),
    };
    let mut notes = feed::Notes::default();
    let body = "\n  upstream is down  \nsecond line";
    drained(&mut crews(), vec![retry(Some(503), body)], &mut notes);
    assert_eq!(
        notes.line().as_deref(),
        Some("reconnecting (503 upstream is down), retry 2 in 500 ms")
    );
    drained(&mut crews(), vec![retry(None, "refused")], &mut notes);
    assert_eq!(
        notes.line().as_deref(),
        Some("reconnecting (refused), retry 2 in 500 ms")
    );
}

#[test]
fn a_corrupted_live_message_shows_on_the_status_line_with_a_count() {
    let mut source = crews();
    let mut notes = feed::Notes::default();
    drained(
        &mut source,
        vec![Wired::Undecodable(corrupted())],
        &mut notes,
    );
    let one = notes.line().unwrap();
    assert!(
        one.starts_with("a live message could not be decoded: "),
        "{one}"
    );
    drained(
        &mut source,
        vec![
            Wired::Undecodable(corrupted()),
            Wired::Undecodable(corrupted()),
        ],
        &mut notes,
    );
    let three = notes.line().unwrap();
    assert!(
        three.starts_with("3 live messages could not be decoded, the latest: "),
        "{three}"
    );
    // The viewport shows it, as `binnacle-tables` hands it over.
    let shared = RefCell::new(notes);
    let options = InlineOptions {
        note: Some(Box::new(|| shared.borrow().line())),
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    let (frames, _) = inline_with(
        &mut source,
        &Layout::default(),
        vec![key(KeyCode::Esc)],
        160,
        20,
        options,
    );
    assert!(
        frames[0].contains("3 live messages could not be decoded"),
        "{}",
        frames[0]
    );
}

#[test]
fn a_burst_of_changes_yields_to_a_key_press_between_frames() {
    let mut source = crews();
    let queued = Rc::new(RefCell::new(100_000u32));
    let left = Rc::clone(&queued);
    let live = Box::new(move |source: &mut DescribedSource| {
        let mut notes = feed::Notes::default();
        let next = || {
            let mut left = left.borrow_mut();
            *left = left.checked_sub(1)?;
            let name = format!("burst-{left}");
            let row = [("name".to_owned(), Wire::Text(name.clone()))].into();
            let key = vec![Wire::Text(name)];
            Some(Wired::Message(Message::Upsert { key, row }))
        };
        feed::drain(source, next, |_| {}, &mut notes, Duration::from_millis(5)).ok()
    });
    let options = InlineOptions {
        live: Some(live),
        origin: Some(Timestamp::from_unix_secs(1_793_232_000)),
        ..InlineOptions::default()
    };
    // The key press comes with the second frame; the burst is far from over.
    let events = vec![None, key(KeyCode::Esc)];
    let (frames, exit) = inline_with(&mut source, &Layout::default(), events, 100, 20, options);
    assert_eq!(exit, Exit::Left);
    assert!(frames.len() >= 3, "frames drew between the burst's parts");
    let left = *queued.borrow();
    assert!(
        0 < left && left < 100_000,
        "each frame took some of the burst and left the rest: {left}"
    );
}
