//! Hiding in the terminal: the hide key, and the header menu's Hide, its
//! hidden items inline and Show all, over an In-Memory Table Source that
//! hides both axes.

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_memory::{MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    Content, Control, Controls, HeaderMenu, Preset, RenderContext, Theme, hide_focused, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

const W: u16 = 56;
const H: u16 = 12;

type Record = [i64; 3];

struct Labels;

impl Content for Labels {
    fn cell(&self, row: Key, col: Key) -> String {
        format!("r{}c{}", row.0, col.0)
    }

    fn column_header(&self, col: Key) -> String {
        ["name", "port", "crew"][col.0 as usize].to_string()
    }
}

struct Table {
    rows: Vec<Record>,
    source: MemorySource<Record>,
    grid: Grid,
}

impl Table {
    /// Six records over three columns 12 wide, both axes hiding unless `hides` is off.
    fn new(hides: bool) -> Table {
        let rows: Vec<Record> = (0..6).map(|r| [r, 10 * r, 100 * r]).collect();
        let source = (0..3).fold(MemorySource::<Record>::new(6, 3), |s, c| {
            s.i64_column(Key(c as u64), move |r: &Record| Some(r[c]))
        });
        let grid = Grid::new(
            AxisConfig::new(6, 1.0).header(1.0),
            AxisConfig::new(3, 12.0).header(4.0).gap(1.0),
        );
        let mut table = Table { rows, source, grid };
        for axis in Axis::ALL {
            let support = table.source.over(&table.rows).hide_support(axis);
            table.grid.set_hide_support(axis, hides && support);
        }
        table.frame();
        table
    }

    /// Runs frames, answering the source's requests, until it settles.
    fn frame(&mut self) {
        for _ in 0..3 {
            for (axis, range) in self.grid.begin_frame(Viewport::new(W.into(), H.into())) {
                let keys = self.source.window(axis, range.clone());
                self.grid.set_window(axis, range.start, &keys);
            }
            pump(&mut self.grid, &mut self.source.over(&self.rows), |_, _| {});
        }
    }

    fn focus(&mut self, row: u64, col: u64) {
        self.grid.set_focus(Key(row), Key(col));
        self.frame();
    }

    fn press(&mut self, menu: &mut HeaderMenu, code: KeyCode) -> bool {
        let took = menu.key(&KeyEvent::new(code, KeyModifiers::NONE), &mut self.grid);
        self.frame();
        took
    }

    fn hidden(&mut self, axis: Axis) -> Vec<u64> {
        self.grid
            .snapshot()
            .hidden(axis)
            .iter()
            .map(|k| k.0)
            .collect()
    }

    fn shown(&mut self, axis: Axis) -> Vec<u64> {
        let count = self.source.count(axis);
        self.source
            .window(axis, 0..count)
            .iter()
            .map(|k| k.0)
            .collect()
    }

    fn screen(&mut self, menu: &HeaderMenu) -> String {
        self.frame();
        let snapshot = self.grid.snapshot();
        let mut buf = Buffer::empty(Rect::new(0, 0, W, H));
        let theme = Theme::classic();
        render_with(
            &snapshot,
            &Labels,
            &RenderContext::new(&theme),
            buf.area,
            &mut buf,
        );
        menu.render(&snapshot, &Labels, &theme, buf.area, &mut buf);
        text(&buf)
    }
}

fn text(buf: &Buffer) -> String {
    let a = buf.area;
    (a.top()..a.bottom())
        .map(|y| {
            let line: String = (a.left()..a.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_hide_key_hides_the_focused_column_and_focus_goes_to_its_neighbour() {
    let mut t = Table::new(true);
    t.focus(2, 1);
    assert!(hide_focused(&mut t.grid, Axis::Cols));
    t.frame();
    assert_eq!(t.hidden(Axis::Cols), [1]);
    assert_eq!(t.shown(Axis::Cols), [0, 2]);
    let focus = t.grid.snapshot().focus(Axis::Cols).and_then(|f| f.key);
    assert_eq!(focus, Some(Key(2)));
    assert!(hide_focused(&mut t.grid, Axis::Rows));
    t.frame();
    assert_eq!(t.shown(Axis::Rows), [0, 1, 3, 4, 5]);
}

#[test]
fn the_menu_hides_its_column_then_lists_the_hidden_ones_to_show_again() {
    let mut t = Table::new(true);
    let mut menu = HeaderMenu::new();
    t.focus(0, 0);
    assert!(menu.open(&t.grid.snapshot(), Axis::Cols));
    let mut out = format!("menu on name\n{}\n", t.screen(&menu));
    assert!(t.press(&mut menu, KeyCode::Enter), "Enter on Hide");
    assert!(!menu.is_open(), "hiding closes it");
    t.focus(0, 2);
    assert!(hide_focused(&mut t.grid, Axis::Cols));
    t.frame();
    assert_eq!(t.shown(Axis::Cols), [1]);
    assert!(menu.open_hidden(&t.grid.snapshot(), Axis::Cols));
    out += &format!(
        "\nname and crew hidden; the menu at the list\n{}\n",
        t.screen(&menu)
    );
    assert!(t.press(&mut menu, KeyCode::Char('2')), "2 shows crew");
    assert_eq!(t.hidden(Axis::Cols), [0]);
    assert_eq!(t.shown(Axis::Cols), [1, 2]);
    assert!(menu.is_open(), "showing leaves it open");
    t.press(&mut menu, KeyCode::Down);
    out += &format!("\nthe cursor on Show all\n{}\n", t.screen(&menu));
    t.press(&mut menu, KeyCode::Enter);
    assert_eq!(t.hidden(Axis::Cols), Vec::<u64>::new());
    assert_eq!(t.shown(Axis::Cols), [0, 1, 2]);
    out += &format!("\nall shown again, in their places\n{}\n", t.screen(&menu));
    assert!(t.press(&mut menu, KeyCode::Char('x')), "it takes every key");
    assert!(t.press(&mut menu, KeyCode::Esc));
    assert!(!menu.is_open());
    assert!(!t.press(&mut menu, KeyCode::Esc), "closed, it takes none");
    insta::assert_snapshot!(out);
}

#[test]
fn the_row_menu_hides_rows_and_its_cursor_wraps() {
    let mut t = Table::new(true);
    let mut menu = HeaderMenu::new();
    t.focus(1, 0);
    assert!(hide_focused(&mut t.grid, Axis::Rows));
    t.frame();
    t.focus(3, 0);
    assert!(menu.open(&t.grid.snapshot(), Axis::Rows));
    t.press(&mut menu, KeyCode::Up);
    t.press(&mut menu, KeyCode::Up);
    let out = t.screen(&menu);
    insta::assert_snapshot!(out);
    t.press(&mut menu, KeyCode::Char('h'));
    assert_eq!(t.hidden(Axis::Rows), [1, 3]);
    assert_eq!(t.shown(Axis::Rows), [0, 2, 4, 5]);
    assert!(menu.open_hidden(&t.grid.snapshot(), Axis::Rows));
    t.press(&mut menu, KeyCode::Char('a'));
    assert_eq!(t.shown(Axis::Rows), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn nothing_hides_and_no_menu_opens_on_an_axis_whose_source_cannot_hide() {
    let mut t = Table::new(false);
    let mut menu = HeaderMenu::new();
    t.focus(0, 0);
    assert!(!hide_focused(&mut t.grid, Axis::Cols));
    assert!(!menu.open(&t.grid.snapshot(), Axis::Cols));
    assert!(!menu.open_hidden(&t.grid.snapshot(), Axis::Rows));
    t.frame();
    assert_eq!(t.shown(Axis::Cols), [0, 1, 2]);
}

#[test]
fn each_preset_reaches_hiding_with_its_own_keys() {
    let alt = KeyModifiers::ALT;
    let none = KeyModifiers::NONE;
    let key = |c: char, m: KeyModifiers| KeyEvent::new(KeyCode::Char(c), m);
    let chords = Controls::preset(Preset::Chords);
    let cases = [
        (key('0', alt), Control::Hide { axis: Axis::Cols }),
        (key('9', alt), Control::Hide { axis: Axis::Rows }),
        (
            key(')', alt | KeyModifiers::SHIFT),
            Control::ShowHidden { axis: Axis::Cols },
        ),
        (
            key('(', alt | KeyModifiers::SHIFT),
            Control::ShowHidden { axis: Axis::Rows },
        ),
        (key('m', alt), Control::Menu { axis: Axis::Cols }),
        (
            KeyEvent::new(KeyCode::F(10), KeyModifiers::SHIFT),
            Control::Menu { axis: Axis::Cols },
        ),
        (
            key('M', alt | KeyModifiers::SHIFT),
            Control::Menu { axis: Axis::Rows },
        ),
    ];
    for (event, control) in cases {
        assert_eq!(chords.control(&event), Some(control), "{event:?}");
    }
    assert_eq!(
        chords.hint(Control::Hide { axis: Axis::Cols }).as_deref(),
        Some("Alt+0")
    );
    assert_eq!(
        chords.hint(Control::Menu { axis: Axis::Cols }).as_deref(),
        Some("Alt+M")
    );
    for preset in [Preset::Modes, Preset::BareKeys] {
        let controls = Controls::preset(preset);
        let cases = [
            ('-', Control::Hide { axis: Axis::Cols }),
            ('_', Control::Hide { axis: Axis::Rows }),
            ('+', Control::ShowHidden { axis: Axis::Cols }),
            ('*', Control::ShowHidden { axis: Axis::Rows }),
            ('.', Control::Menu { axis: Axis::Cols }),
            (',', Control::Menu { axis: Axis::Rows }),
        ];
        for (c, control) in cases {
            assert_eq!(
                controls.control(&key(c, none)),
                Some(control),
                "{preset:?} {c}"
            );
        }
    }
}
