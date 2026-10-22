//! A pivot in the terminal (#209), from `GroupedMemory`: regions down the
//! rows and quarters across, each quarter spanning its value columns in the
//! column Header Band's top line, each value column's header on the line
//! below, and the All columns, one per value, taking both lines, pinned at
//! the right.

use std::borrow::Cow;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Grid, Grouping, HeaderSpan, Key, Level, Placement, Value,
    ViewSpec,
};
use binnacle_group::{Figure, GroupedMemory};
use binnacle_memory::{MemorySource, pump};
use binnacle_ratatui::{Content, RenderContext, Theme, begin_frame, render_with};
use ratatui::buffer::Buffer;
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

/// What a host draws: its column names, each region's name, each quarter's,
/// and the figure where a row and a column cross.
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
        match self
            .0
            .summary(span.key)
            .and_then(|s| s.path.last().cloned())
        {
            Some((_, Value::Text(t))) => t,
            _ => String::new(),
        }
    }
}

fn draw() -> String {
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
    grid.request_view(ViewSpec::default().with_grouping(Some(pivot())));
    let theme = Theme::classic();
    let ctx = RenderContext::new(&theme);
    for _ in 0..4 {
        pump(&mut grid, &mut source.over(&rows), |_, _| {});
        begin_frame(&mut grid, &ctx, AREA, |grid, v| {
            for (axis, range) in grid.begin_frame(v) {
                let window = source.window(&rows, axis, range.clone());
                grid.set_window(axis, range.start, &window.keys);
                if let Some(outlines) = window.outlines {
                    grid.set_outlines(axis, range.start, &outlines);
                }
            }
        });
    }
    let snapshot = grid.snapshot();
    let mut buf = Buffer::empty(AREA);
    render_with(&snapshot, &Text(&source), &ctx, AREA, &mut buf);
    (0..AREA.height)
        .map(|y| {
            let line: String = (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect();
            format!("|{line}|\n")
        })
        .collect()
}

#[test]
fn quarters_span_their_value_columns_and_the_all_columns_take_both_lines() {
    insta::assert_snapshot!(draw());
}
