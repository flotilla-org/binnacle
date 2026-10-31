//! The CLI host over a Described Table: `print` in every format, one Layout
//! giving the same rows and columns everywhere, and the inline viewport on
//! a test backend.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use binnacle_core::{Axis, Key};
use binnacle_described::{DescribedSource, Message, Received, Wire};
use binnacle_format::Timestamp;
use binnacle_layout::view::{
    AxisView, Condition, ConditionKind, Direction, FilterSpec, SortColumn, ViewSpec,
};
use binnacle_layout::{JsonId, Layout, Order};
use binnacle_ratatui::cli::{
    CliTable, Format, InlineOptions, Selection, print, print_rows, run_inline_with,
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
    print(source, layout, format, &mut out).unwrap();
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
) -> (Vec<String>, Option<Selection>) {
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
) -> (Vec<String>, Option<Selection>) {
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
        Some(Selection {
            rows: vec![row(&source, "alpha")]
        })
    );
    let events = vec![key(KeyCode::Down), key(KeyCode::Enter)];
    let (_, chosen) = inline(&mut source, &Layout::default(), events, 100, 20);
    assert_eq!(
        chosen,
        Some(Selection {
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
    let mut want = vec![row(&source, "alpha"), row(&source, "charlie")];
    want.sort();
    assert_eq!(
        chosen,
        Some(Selection { rows: want }),
        "the picks, not the focus"
    );
}

#[test]
fn esc_and_q_choose_nothing() {
    let mut source = crews();
    for code in [KeyCode::Esc, KeyCode::Char('q')] {
        let (_, chosen) = inline(&mut source, &Layout::default(), vec![key(code)], 100, 20);
        assert_eq!(chosen, None);
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
    assert_eq!(chosen, None);
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
    assert_eq!(chosen, None);
}
