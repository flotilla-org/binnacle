//! A pivot in the terminal (#209), from `GroupedMemory`: regions down the
//! rows and quarters across, each quarter spanning its value columns in the
//! column Header Band's top line, each value column's header on the line
//! below, and the All columns, one per value, taking both lines, pinned at
//! the right. A quarter closes (#210) to its total columns under its span,
//! one per value (#229), as a region over the values closes to its own.

use std::borrow::Cow;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Form, Grid, Grouping, HeaderSpan, Key, Level, Placement,
    Value, ViewSpec,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::{MemorySource, pump};
use binnacle_ratatui::{
    Content, Keymap, RenderContext, Theme, begin_frame, column_disclosure_at, render_with,
};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 72, 8);

const REGION: Key = Key(0);
const QUARTER: Key = Key(1);
const WEIGHT: Key = Key(2);
const HEADERS: [&str; 3] = ["region", "quarter", "weight"];

struct Ship {
    region: &'static str,
    quarter: &'static str,
    weight: f64,
}

const SHIPS: [Ship; 5] = [
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
        region: "North",
        quarter: "Q1",
        weight: 8.0,
    },
    Ship {
        region: "South",
        quarter: "Q1",
        weight: 16.0,
    },
];

fn source() -> GroupedMemory<Ship> {
    let source = MemorySource::<Ship>::new(SHIPS.len(), HEADERS.len())
        .bytes_column(REGION, |s| Some(Cow::Borrowed(s.region.as_bytes())))
        .text_column(REGION, |s| Some(Cow::Borrowed(s.region)))
        .bytes_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter.as_bytes())))
        .text_column(QUARTER, |s| Some(Cow::Borrowed(s.quarter)))
        .f64_column(WEIGHT, |s| Some(s.weight));
    GroupedMemory::new(source)
}

/// Regions down, quarters then the values across, the Grand Totals after.
fn pivot() -> Grouping {
    let mut grouping = Grouping::rows_by(REGION);
    grouping.col_levels = vec![Level::field(QUARTER), Level::Values];
    grouping.values = vec![AggregateValue::of(WEIGHT, "sum"), AggregateValue::count()];
    grouping.rows.grand_total = Placement::After;
    grouping.cols.grand_total = Placement::After;
    grouping
}

/// What a host draws: its column names, each region's name, each quarter's
/// (on its span and its totals of each value), and the figure where a row
/// and a column cross.
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

    fn span_label(&self, span: &HeaderSpan) -> String {
        self.group_name(span.axis, span.key).unwrap_or_default()
    }

    fn group_name(&self, _: Axis, group: Key) -> Option<String> {
        match self.0.summary(group)?.path.last()? {
            (_, Value::Text(t)) => Some(t.clone()),
            _ => None,
        }
    }
}

/// Frames the grid until the layer has answered.
fn settle(grid: &mut Grid, source: &mut GroupedMemory<Ship>, rows: &[Ship], ctx: &RenderContext) {
    for _ in 0..4 {
        pump(grid, &mut source.over(rows), |_, _| {});
        begin_frame(grid, ctx, AREA, |grid, v| {
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

/// Draws the pivot after `act` has had its way with the Grid.
fn draw_after(act: impl FnOnce(&mut Grid)) -> Buffer {
    draw_pivot(pivot(), act)
}

/// Draws `grouping` after `act` has had its way with the Grid.
fn draw_pivot(grouping: Grouping, act: impl FnOnce(&mut Grid)) -> Buffer {
    let rows: Vec<Ship> = SHIPS.into_iter().collect();
    let mut source = source();
    let mut grid = Grid::new(
        AxisConfig::new(rows.len() as u64, 1.0).header(2.0),
        AxisConfig::new(HEADERS.len() as u64, 9.0)
            .header(14.0)
            .gap(1.0),
    );
    // The quarters' line, then the value columns' own: the values
    // pseudo-level, innermost, is named on the corner's top line.
    grid.set_tiers(Axis::Cols, &[1.0, 0.0]);
    grid.request_view(ViewSpec::default().with_grouping(Some(grouping)));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    settle(&mut grid, &mut source, &rows, &ctx);
    act(&mut grid);
    settle(&mut grid, &mut source, &rows, &ctx);
    let snapshot = grid.snapshot();
    let mut buf = Buffer::empty(AREA);
    render_with(&snapshot, &Text(&source), &ctx, AREA, &mut buf);
    buf
}

fn text(buf: &Buffer) -> String {
    (0..AREA.height)
        .map(|y| {
            let line: String = (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect();
            format!("|{line}|\n")
        })
        .collect()
}

fn draw() -> String {
    text(&draw_after(|_| {}))
}

#[test]
fn quarters_span_their_value_columns_and_the_all_columns_take_both_lines() {
    insta::assert_snapshot!(draw());
}

/// The first quarter's span: Q1, the first row's first.
fn first_quarter(grid: &mut Grid) -> Key {
    let snapshot = grid.snapshot();
    let first = snapshot.spans(Axis::Cols).iter().min_by_key(|s| s.first);
    first.expect("the quarters span their columns").key
}

#[test]
fn a_closed_quarter_shows_a_total_column_per_value_under_its_span() {
    let drawn = draw_after(|grid| {
        let q1 = first_quarter(grid);
        let snapshot = grid.snapshot();
        assert_eq!(
            column_disclosure_at(&snapshot, 14, 0),
            Some(q1),
            "Q1's mark"
        );
        assert_eq!(column_disclosure_at(&snapshot, 16, 0), None, "its label");
        assert_eq!(column_disclosure_at(&snapshot, 14, 2), None, "the body");
        grid.collapse(Axis::Cols, q1).unwrap();
    });
    insta::assert_snapshot!(text(&drawn));
}

/// The column tier focus is in, if it is in the column Header Band.
fn focused_tier(grid: &mut Grid) -> Option<u32> {
    let snapshot = grid.snapshot();
    (snapshot.band_focused(Axis::Cols)).then(|| snapshot.focused_tier())
}

#[test]
fn up_from_the_first_row_enters_the_column_tiers_where_left_steps_out_then_closes_a_quarter() {
    let key = |code| Keymap::default().action(&KeyEvent::new(code, KeyModifiers::NONE));
    let drawn = draw_after(|grid| {
        let snapshot = grid.snapshot();
        let row = snapshot.rows().first().and_then(|i| i.key).unwrap();
        let col = snapshot.cols().first().and_then(|i| i.key).unwrap();
        grid.set_focus(row, col);
        grid.action(key(KeyCode::Up).unwrap());
        // The values' line, the innermost tier, has nothing to open: Left
        // steps out to the quarter, then closes it.
        assert_eq!(focused_tier(grid), Some(1), "Up enters the innermost tier");
        grid.action(key(KeyCode::Left).unwrap());
        assert_eq!(focused_tier(grid), Some(0));
        grid.action(key(KeyCode::Left).unwrap());
    });
    let top: String = (0..AREA.width).map(|x| drawn[(x, 0)].symbol()).collect();
    assert!(top.contains("▸ Q1"), "Left closes Q1: {top}");
    let focus = Theme::classic().focus.add_modifier;
    let focused = |x, y| (drawn[(x, y)].style().add_modifier).contains(focus);
    assert!(focused(16, 0), "the focus is drawn on Q1's span");
    assert!(!focused(16, 2), "and not in the body");
}

#[test]
fn a_closed_region_over_the_values_shows_a_total_row_per_value() {
    let mut grouping = pivot();
    grouping.row_levels = vec![Level::field(REGION), Level::Values];
    grouping.col_levels = vec![Level::field(QUARTER)];
    let drawn = draw_pivot(grouping, |grid| {
        let north = grid.snapshot().rows().first().and_then(|i| i.key).unwrap();
        grid.collapse(Axis::Rows, north).unwrap();
    });
    insta::assert_snapshot!(text(&drawn));
}

#[test]
fn in_header_form_a_closed_region_names_each_value_under_its_span() {
    let mut grouping = pivot();
    // A total row after each open region too, so closing North keeps the
    // rows in place (Collapsed), its totals left under its span.
    let region = Level::Field {
        field: REGION,
        subtotal: Placement::After,
    };
    grouping.row_levels = vec![region, Level::Values];
    grouping.col_levels = vec![Level::field(QUARTER)];
    grouping.rows.form = Form::Header;
    let drawn = draw_pivot(grouping, |grid| {
        grid.set_tiers(Axis::Rows, &[7.0]);
        let snapshot = grid.snapshot();
        let north = snapshot.outline(Axis::Rows, 0).unwrap().ancestors[0];
        grid.collapse(Axis::Rows, north).unwrap();
    });
    insta::assert_snapshot!(text(&drawn));
}
