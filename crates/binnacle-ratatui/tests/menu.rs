//! Hiding in the terminal: the hide key, the header menu's Hide, Show only
//! these, its hidden items inline and Show all, and the chooser, over an
//! In-Memory Table Source that hides both axes.

use binnacle_core::{
    Action, Axis, AxisConfig, Edge, FocusStep, Grid, Key, Modifiers, PointerEvent, Viewport,
};
use binnacle_memory::{MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    Chooser, Content, Control, Controls, HeaderMenu, Hid, Preset, RenderContext, Theme,
    hide_chosen, hide_focused, render_with, show_only_chosen,
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
        let source = &self.source;
        let event = KeyEvent::new(code, KeyModifiers::NONE);
        let took = menu.key(&event, &mut self.grid, |axis, range| {
            source.window(axis, range)
        });
        self.frame();
        took
    }

    fn choose(&mut self, chooser: &mut Chooser, code: KeyCode) -> bool {
        let took = chooser.key(&KeyEvent::new(code, KeyModifiers::NONE), &mut self.grid);
        self.frame();
        self.fetch(chooser);
        took
    }

    fn fetch(&mut self, chooser: &mut Chooser) {
        let source = &self.source;
        chooser.fetch(&self.grid.snapshot(), &Labels, |axis, range| {
            source.window(axis, range)
        });
    }

    fn hide_chosen(&mut self, axis: Axis) -> Hid {
        let source = &self.source;
        let hid = hide_chosen(&mut self.grid, axis, |axis, range| {
            source.window(axis, range)
        });
        self.frame();
        hid
    }

    fn show_only(&mut self, axis: Axis) -> Hid {
        let source = &self.source;
        let hid = show_only_chosen(&mut self.grid, axis, |axis, range| {
            source.window(axis, range)
        });
        self.frame();
        hid
    }

    /// Ctrl-clicks the header of the item of `axis` at `position`, picking it.
    fn pick(&mut self, axis: Axis, position: u64) {
        let (x, y) = match axis {
            Axis::Cols => (4.0 + 13.0 * position as f32 + 1.0, 0.5),
            Axis::Rows => (1.0, 1.5 + position as f32),
        };
        let m = Modifiers::CTRL;
        self.grid.pointer(PointerEvent::down(x, y).modifiers(m));
        self.grid.pointer(PointerEvent::up(x, y).modifiers(m));
        self.frame();
    }

    fn chooser_screen(&mut self, chooser: &mut Chooser) -> String {
        self.frame();
        self.fetch(chooser);
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
        chooser.render(&snapshot, &theme, buf.area, &mut buf);
        text(&buf)
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
        (key('o', alt), Control::Chooser { axis: Axis::Cols }),
        (
            key('C', alt | KeyModifiers::SHIFT),
            Control::Chooser { axis: Axis::Rows },
        ),
        (
            key('O', alt | KeyModifiers::SHIFT),
            Control::Chooser { axis: Axis::Rows },
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
    let modes = Controls::preset(Preset::Modes);
    let cols = Control::Chooser { axis: Axis::Cols };
    let rows = Control::Chooser { axis: Axis::Rows };
    assert_eq!(modes.control(&key('C', KeyModifiers::SHIFT)), Some(cols));
    assert_eq!(modes.control(&key('c', alt)), Some(rows));
    let bare = Controls::preset(Preset::BareKeys);
    assert_eq!(bare.control(&key('c', none)), Some(cols));
    assert_eq!(bare.control(&key('C', KeyModifiers::SHIFT)), Some(rows));
}

#[test]
fn hide_reaches_the_picks_or_the_selection_as_well_as_the_focused_item() {
    let mut t = Table::new(true);
    t.focus(0, 0);
    t.pick(Axis::Cols, 0);
    t.pick(Axis::Cols, 2);
    assert_eq!(t.hide_chosen(Axis::Cols), Hid::Hidden(2));
    assert_eq!(t.shown(Axis::Cols), [1]);
    // Records 1 to 3, selected by a range.
    t.focus(1, 1);
    t.grid.action(Action::MoveFocus {
        axis: Axis::Rows,
        by: binnacle_core::FocusStep::Item,
        forward: true,
        extend: true,
    });
    t.grid.action(Action::MoveFocus {
        axis: Axis::Rows,
        by: binnacle_core::FocusStep::Item,
        forward: true,
        extend: true,
    });
    t.frame();
    assert_eq!(t.hide_chosen(Axis::Rows), Hid::Hidden(3));
    assert_eq!(t.shown(Axis::Rows), [0, 4, 5]);
    assert_eq!(
        t.hide_chosen(Axis::Rows),
        Hid::Hidden(1),
        "then the focused row"
    );
    assert_eq!(t.shown(Axis::Rows).len(), 2);
}

#[test]
fn a_selection_across_a_pinned_row_hides_and_keeps_every_row_it_covers() {
    // Row 4 pinned first: a range from it down two rows in drawn order covers
    // rows 4, 0 and 1, two runs of Positions.
    let selected = || {
        let mut t = Table::new(true);
        t.grid.pin(Axis::Rows, Edge::Leading, Key(4));
        t.focus(4, 0);
        for _ in 0..2 {
            t.grid.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward: true,
                extend: true,
            });
        }
        t.frame();
        t
    };
    let mut t = selected();
    assert_eq!(t.hide_chosen(Axis::Rows), Hid::Hidden(3));
    assert_eq!(t.shown(Axis::Rows), [2, 3, 5]);
    let mut t = selected();
    assert_eq!(t.show_only(Axis::Rows), Hid::Hidden(3));
    assert_eq!(t.shown(Axis::Rows), [0, 1, 4]);
}

#[test]
fn show_only_these_hides_every_other_item_shown_on_either_axis() {
    let mut t = Table::new(true);
    t.focus(0, 0);
    t.pick(Axis::Cols, 1);
    assert_eq!(t.show_only(Axis::Cols), Hid::Hidden(2));
    assert_eq!(t.shown(Axis::Cols), [1]);
    assert_eq!(t.hidden(Axis::Cols), [0, 2]);
    t.pick(Axis::Rows, 2);
    t.pick(Axis::Rows, 4);
    assert_eq!(t.show_only(Axis::Rows), Hid::Hidden(4));
    assert_eq!(t.shown(Axis::Rows), [2, 4]);
}

#[test]
fn past_the_hide_cap_nothing_hides_and_the_menu_suggests_a_filter() {
    let mut t = Table::new(true);
    t.grid.set_hide_cap(Axis::Rows, 2);
    t.focus(0, 0);
    t.grid.action(Action::SelectAll);
    t.frame();
    // Asking for the selection's Keys would be wasted past the cap.
    let hid = hide_chosen(&mut t.grid, Axis::Rows, |_, _| panic!("asked for Keys"));
    assert_eq!(hid, Hid::Capped(2));
    t.focus(3, 0);
    let mut menu = HeaderMenu::new();
    assert!(menu.open(&t.grid.snapshot(), Axis::Rows));
    t.press(&mut menu, KeyCode::Char('o'));
    assert!(menu.is_open(), "a refused hide leaves it open");
    assert_eq!(t.hidden(Axis::Rows), Vec::<u64>::new());
    insta::assert_snapshot!(t.screen(&menu));
    t.press(&mut menu, KeyCode::Char('c'));
    assert!(!menu.is_open());
    assert_eq!(
        menu.take_chooser(),
        Some(Axis::Rows),
        "Choose asks for the chooser"
    );
    assert_eq!(menu.take_chooser(), None);
}

#[test]
fn the_chooser_lists_every_column_ticked_while_shown_and_ticks_show_and_hide() {
    let mut t = Table::new(true);
    t.focus(0, 0);
    let mut chooser = Chooser::new();
    assert!(chooser.open(&t.grid.snapshot(), Axis::Cols));
    let mut out = format!("open\n{}\n", t.chooser_screen(&mut chooser));
    t.choose(&mut chooser, KeyCode::Down);
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.shown(Axis::Cols), [0, 2]);
    out += &format!(
        "\nport unticked: hidden, and kept in its place\n{}\n",
        t.chooser_screen(&mut chooser)
    );
    t.choose(&mut chooser, KeyCode::Char('r'));
    out += &format!("\nsearching \"r\"\n{}\n", t.chooser_screen(&mut chooser));
    t.choose(&mut chooser, KeyCode::Home);
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.shown(Axis::Cols), [0, 1, 2], "the head shows every match");
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.shown(Axis::Cols), [0], "then hides them");
    t.choose(&mut chooser, KeyCode::Backspace);
    t.choose(&mut chooser, KeyCode::Home);
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.shown(Axis::Cols), [0, 1, 2]);
    assert!(t.choose(&mut chooser, KeyCode::Esc));
    assert!(!chooser.is_open());
    insta::assert_snapshot!(out);
}

#[test]
fn the_row_chooser_hides_rows_and_says_so_past_the_cap() {
    let mut t = Table::new(true);
    t.grid.set_hide_cap(Axis::Rows, 1);
    t.focus(0, 0);
    let mut chooser = Chooser::new();
    assert!(chooser.open(&t.grid.snapshot(), Axis::Rows));
    t.fetch(&mut chooser);
    t.choose(&mut chooser, KeyCode::Down);
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.hidden(Axis::Rows), [1]);
    t.choose(&mut chooser, KeyCode::Down);
    t.choose(&mut chooser, KeyCode::Char(' '));
    assert_eq!(t.hidden(Axis::Rows), [1], "past the cap of 1");
    insta::assert_snapshot!(t.chooser_screen(&mut chooser));
}

#[test]
fn no_chooser_opens_on_an_axis_whose_source_cannot_hide() {
    let mut t = Table::new(false);
    assert!(!Chooser::new().open(&t.grid.snapshot(), Axis::Cols));
}

#[test]
fn on_a_column_major_table_the_chooser_and_show_only_act_on_its_records() {
    use binnacle_memory::Transposed;
    let rows: Vec<Record> = (0..4).map(|r| [r, 10 * r, 100 * r]).collect();
    let mut source = (0..3).fold(MemorySource::<Record>::new(4, 3), |s, c| {
        s.i64_column(Key(c as u64), move |r: &Record| Some(r[c]))
    });
    // The records are the columns, the fields the rows.
    let mut grid = Grid::new(
        AxisConfig::new(3, 1.0).header(1.0),
        AxisConfig::new(4, 8.0).header(5.0).gap(1.0),
    );
    for axis in Axis::ALL {
        grid.set_hide_support(axis, Transposed(source.over(&rows)).hide_support(axis));
    }
    let frame = |grid: &mut Grid, source: &mut MemorySource<Record>| {
        for _ in 0..3 {
            for (axis, range) in grid.begin_frame(Viewport::new(W.into(), H.into())) {
                let keys = source.window(axis.other(), range.clone());
                grid.set_window(axis, range.start, &keys);
            }
            pump(grid, &mut Transposed(source.over(&rows)), |_, _| {});
        }
    };
    frame(&mut grid, &mut source);
    grid.set_focus(Key(0), Key(2));
    frame(&mut grid, &mut source);
    let keys_at = |axis: Axis, range| source.window(axis.other(), range);
    assert_eq!(
        show_only_chosen(&mut grid, Axis::Cols, keys_at),
        Hid::Hidden(3)
    );
    frame(&mut grid, &mut source);
    assert_eq!(
        source.window(Axis::Rows, 0..source.count(Axis::Rows)),
        [Key(2)]
    );
    let mut chooser = Chooser::new();
    assert!(chooser.open(&grid.snapshot(), Axis::Cols));
    struct Records;
    impl Content for Records {
        fn cell(&self, _: Key, _: Key) -> String {
            String::new()
        }
        fn column_header(&self, col: Key) -> String {
            format!("record {}", col.0)
        }
    }
    chooser.fetch(&grid.snapshot(), &Records, |axis, range| {
        source.window(axis.other(), range)
    });
    // The record shown first, then the three hidden: tick record 0 back.
    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    chooser.key(&down, &mut grid);
    chooser.key(
        &KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
        &mut grid,
    );
    frame(&mut grid, &mut source);
    assert_eq!(
        source.window(Axis::Rows, 0..source.count(Axis::Rows)),
        [Key(0), Key(2)]
    );
}
