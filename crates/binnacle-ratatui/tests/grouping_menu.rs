//! The grouping menu in the terminal (#212), over `GroupedMemory`: a level
//! named in the corner opens its menu, which removes or moves the level
//! (the rows' quarter moved to the columns makes a pivot, and moved back
//! the grouped table), collapses to it or expands through it, and removes
//! or adds values; a column's menu groups by its column and adds values
//! over it; a pivot column's menu acts on its column level; the column
//! chooser lists the columns the rows group by as "grouped".

use std::borrow::Cow;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Depth, Grid, Grouping, Key, Level, ViewSpec, Viewport,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::{MemorySource, PumpSource, pump};
use binnacle_ratatui::{
    Chooser, Content, HeaderMenu, RenderContext, Theme, begin_frame, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 60, 16);

const REGION: Key = Key(0);
const QUARTER: Key = Key(1);
const WEIGHT: Key = Key(2);
const HEADERS: [&str; 3] = ["region", "quarter", "weight"];

struct Ship {
    region: &'static str,
    quarter: &'static str,
    weight: f64,
}

const SHIPS: [Ship; 4] = [
    Ship {
        region: "North",
        quarter: "Q1",
        weight: 1.0,
    },
    Ship {
        region: "South",
        quarter: "Q2",
        weight: 2.0,
    },
    Ship {
        region: "North",
        quarter: "Q2",
        weight: 4.0,
    },
    Ship {
        region: "South",
        quarter: "Q1",
        weight: 8.0,
    },
];

/// The ships behind the grouping layer, a Grid over them and the menu,
/// knowing the three columns as the fields.
struct Table {
    rows: Vec<Ship>,
    source: GroupedMemory<Ship>,
    grid: Grid,
    menu: HeaderMenu,
}

impl Table {
    /// The ships grouped as `grouping` asks, applied.
    fn new(grouping: Grouping) -> Table {
        let rows: Vec<Ship> = SHIPS.into_iter().collect();
        let source = MemorySource::<Ship>::new(rows.len(), HEADERS.len())
            .bytes_column(REGION, |s| Some(Cow::Borrowed(s.region.as_bytes())))
            .text_column(REGION, |s| Some(Cow::Borrowed(s.region)))
            .bytes_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter.as_bytes())))
            .text_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter)))
            .f64_column(WEIGHT, |s| Some(s.weight));
        let mut source = GroupedMemory::new(source);
        let mut grid = Grid::new(
            AxisConfig::new(rows.len() as u64, 1.0).header(2.0),
            AxisConfig::new(HEADERS.len() as u64, 9.0)
                .header(14.0)
                .gap(1.0),
        );
        for c in 0..HEADERS.len() as u64 {
            grid.set_aggregates(Axis::Rows, Key(c), source.aggregates(Axis::Rows, Key(c)));
            let support = source.over(&rows).support(Axis::Rows, Key(c));
            grid.set_support(Axis::Rows, Key(c), support);
        }
        grid.set_tiers(Axis::Cols, &[1.0, 0.0]);
        grid.request_view(ViewSpec::default().with_grouping(Some(grouping)));
        let mut menu = HeaderMenu::new();
        menu.set_fields(vec![REGION, QUARTER, WEIGHT]);
        let mut table = Table {
            rows,
            source,
            grid,
            menu,
        };
        table.settle();
        table
    }

    /// Answers the requests and fills the Windows until they settle.
    fn settle(&mut self) {
        let theme = Theme::classic();
        let ctx = RenderContext::new(&theme);
        for _ in 0..4 {
            pump(&mut self.grid, &mut self.source.over(&self.rows), |_, _| {});
            let (source, rows) = (&mut self.source, &self.rows);
            begin_frame(&mut self.grid, &ctx, AREA, |grid, v: Viewport| {
                for (axis, range) in grid.begin_frame(v) {
                    let window = source.window(rows, axis, range.clone());
                    grid.set_window(axis, range.start, &window.keys);
                    if let Some(outlines) = window.outlines {
                        grid.set_outlines(axis, range.start, &outlines);
                    }
                }
            });
        }
    }

    /// The grouping applied: each axis's levels by field name ("values"
    /// for the pseudo-level), then the values.
    fn applied(&mut self) -> (Vec<&'static str>, Vec<&'static str>, Vec<String>) {
        let grouping = self.grid.snapshot().applied_view().grouping.clone();
        let grouping = grouping.expect("grouped");
        let names = |axis| -> Vec<&'static str> {
            (grouping.levels(axis).iter())
                .map(|l| l.field_key().map_or("values", |f| HEADERS[f.0 as usize]))
                .collect()
        };
        let values = grouping
            .values
            .iter()
            .map(|v| Text(&self.source).value_name(v));
        (names(Axis::Rows), names(Axis::Cols), values.collect())
    }

    fn press(&mut self, code: KeyCode) {
        let event = KeyEvent::new(code, KeyModifiers::NONE);
        assert!(self.menu.key(&event, &mut self.grid, |_, _| Vec::new()));
        self.settle();
    }

    /// The table with the menu over it, each line between bars.
    fn draw(&mut self) -> String {
        let snapshot = self.grid.snapshot();
        let theme = Theme::classic();
        let mut buf = Buffer::empty(AREA);
        let text = Text(&self.source);
        render_with(
            &snapshot,
            &text,
            &RenderContext::new(&theme),
            AREA,
            &mut buf,
        );
        self.menu.render(&snapshot, &text, &theme, AREA, &mut buf);
        lines(&buf)
    }
}

fn lines(buf: &Buffer) -> String {
    (0..AREA.height)
        .map(|y| {
            let line: String = (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect();
            format!("|{line}|\n")
        })
        .collect()
}

/// Its column names, each group's value, and the figures.
struct Text<'a>(&'a GroupedMemory<Ship>);

impl Content for Text<'_> {
    fn cell(&self, row: Key, col: Key) -> String {
        match self.0.cell(row, col) {
            Some(Figure::Number(n)) => format!("{n}"),
            Some(Figure::Count(n)) => n.to_string(),
            _ => String::new(),
        }
    }

    fn column_header(&self, col: Key) -> String {
        HEADERS
            .get(col.0 as usize)
            .map_or_else(String::new, |h| h.to_string())
    }
}

/// The rows by region then quarter, counted.
fn by_region_then_quarter() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(QUARTER)];
    grouping
}

/// Regions down, quarters then the values across.
fn pivot() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.col_levels = vec![Level::field(QUARTER), Level::Values];
    grouping.values = vec![AggregateValue::of(WEIGHT, "sum"), AggregateValue::count()];
    grouping
}

#[test]
fn the_rows_quarter_level_moved_to_the_columns_makes_a_pivot_and_moved_back_the_grouped_table() {
    let mut t = Table::new(by_region_then_quarter());
    assert!(
        t.menu
            .open_level(&t.grid.snapshot(), Axis::Rows, Some(QUARTER))
    );
    insta::assert_snapshot!("row_level_menu", t.draw());
    t.press(KeyCode::Char('m'));
    assert!(!t.menu.is_open());
    assert_eq!(
        t.applied(),
        (vec!["region"], vec!["quarter"], vec!["count".into()])
    );

    assert!(
        t.menu
            .open_level(&t.grid.snapshot(), Axis::Cols, Some(QUARTER))
    );
    let open = t.draw();
    assert!(open.contains("Column level quarter"), "{open}");
    t.press(KeyCode::Char('m'));
    let applied = (vec!["region", "quarter"], vec![], vec!["count".into()]);
    assert_eq!(t.applied(), applied);
}

#[test]
fn a_level_with_no_such_level_does_not_open() {
    let mut t = Table::new(by_region_then_quarter());
    let snapshot = t.grid.snapshot();
    assert!(!t.menu.open_level(&snapshot, Axis::Cols, Some(QUARTER)));
    assert!(!t.menu.open_level(&snapshot, Axis::Rows, Some(WEIGHT)));
    assert!(
        !t.menu.open_level(&snapshot, Axis::Rows, None),
        "one value: no values level"
    );
}

#[test]
fn removing_a_level_keeps_the_rest_and_collapsing_closes_its_groups() {
    let mut t = Table::new(by_region_then_quarter());
    assert!(
        t.menu
            .open_level(&t.grid.snapshot(), Axis::Rows, Some(REGION))
    );
    t.press(KeyCode::Char('-'));
    let snapshot = t.grid.snapshot();
    let grouping = snapshot.applied_view().grouping.as_ref().unwrap();
    assert_eq!(grouping.rows.expand.depth, Depth::Levels(0));
    assert!(
        t.menu
            .open_level(&t.grid.snapshot(), Axis::Rows, Some(REGION))
    );
    t.press(KeyCode::Char('x'));
    assert_eq!(t.applied(), (vec!["quarter"], vec![], vec!["count".into()]));
}

#[test]
fn a_column_menu_adds_a_value_over_its_column_from_its_list() {
    let mut t = Table::new(by_region_then_quarter());
    let row = t.grid.snapshot().strip(Axis::Rows)[0].key.unwrap();
    t.grid.set_focus(row, WEIGHT);
    assert!(t.menu.open(&t.grid.snapshot(), Axis::Cols));
    t.press(KeyCode::Char('v'));
    insta::assert_snapshot!("add_a_value", t.draw());
    // Down from Add a value to its third: the sum.
    for _ in 0..3 {
        t.press(KeyCode::Down);
    }
    t.press(KeyCode::Enter);
    assert!(!t.menu.is_open());
    let values = vec!["count".to_string(), "sum(weight)".into()];
    assert_eq!(t.applied(), (vec!["region", "quarter"], vec![], values));
}

#[test]
fn a_pivot_column_menu_acts_on_its_column_level_and_the_values() {
    let mut t = Table::new(pivot());
    let snapshot = t.grid.snapshot();
    let (row, col) = (
        snapshot.strip(Axis::Rows)[0].key,
        snapshot.strip(Axis::Cols)[0].key,
    );
    t.grid.set_focus(row.unwrap(), col.unwrap());
    assert!(t.menu.open(&t.grid.snapshot(), Axis::Cols));
    let open = t.draw();
    for line in [
        "x Remove level",
        "m Move to rows",
        "- Collapse to quarter",
        "+ Expand through quarter",
    ] {
        assert!(open.contains(line), "{line}: {open}");
    }
    // Past the pin lines and the level's to the second value.
    for _ in 0..8 {
        t.press(KeyCode::Down);
    }
    let at = t.draw();
    assert!(at.contains("▸  Remove value count"), "{at}");
    t.press(KeyCode::Enter);
    assert_eq!(
        t.applied(),
        (vec!["region"], vec!["quarter"], vec!["sum(weight)".into()])
    );
}

#[test]
fn the_values_level_moves_to_the_rows() {
    let mut t = Table::new(pivot());
    assert!(t.menu.open_level(&t.grid.snapshot(), Axis::Cols, None));
    let open = t.draw();
    assert!(open.contains(" Values "), "{open}");
    assert!(!open.contains("Remove level"), "{open}");
    t.press(KeyCode::Char('m'));
    let values = vec!["sum(weight)".to_string(), "count".into()];
    assert_eq!(
        t.applied(),
        (vec!["region", "values"], vec!["quarter"], values)
    );
}

#[test]
fn the_chooser_lists_the_columns_the_rows_group_by_as_grouped_with_no_tick() {
    let mut t = Table::new(Grouping::rows_by(REGION));
    t.grid.set_hide_support(Axis::Cols, true);
    t.settle();
    let mut chooser = Chooser::new();
    let snapshot = t.grid.snapshot();
    assert!(chooser.open(&snapshot, Axis::Cols));
    let shown: Vec<Key> = snapshot
        .strip(Axis::Cols)
        .iter()
        .filter_map(|i| i.key)
        .collect();
    chooser.fetch(&snapshot, &Text(&t.source), |_, _| shown);
    let mut buf = Buffer::empty(AREA);
    chooser.render(&snapshot, &Theme::classic(), AREA, &mut buf);
    insta::assert_snapshot!("chooser_grouped", lines(&buf));
    // Down past the head and the two columns to region: Space does nothing.
    for code in [KeyCode::Down, KeyCode::Down, KeyCode::Char(' ')] {
        assert!(chooser.key(&KeyEvent::new(code, KeyModifiers::NONE), &mut t.grid));
    }
    assert!(t.grid.snapshot().hidden(Axis::Cols).is_empty());
}
