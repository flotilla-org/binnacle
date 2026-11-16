//! A null group reads `(null)`, dim as a null cell is; an empty one reads
//! `(empty)`, in the ordinary style. Each Subtotal reads `(null) total` or
//! `(empty) total`, and the checklist lists the two as separate values.

use binnacle_core::{Axis, AxisConfig, Grid, Key, OpenState, Outline, Support, Value, Viewport};
use binnacle_ratatui::{
    Content, EMPTY_LABEL, NULL_LABEL, RenderContext, Theme, render_with, value_label,
};
use binnacle_testkit::{GroupedTable, Scenario};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

const AREA: Rect = Rect::new(0, 0, 40, 9);
const V: Viewport = Viewport {
    width: 40.0,
    height: 9.0,
};
const REGION: Key = Key(1);
const NULL: Key = Key(1 << 63);
const EMPTY: Key = Key((1 << 63) | 1);
const NORTH: Key = Key((1 << 63) | 2);

/// Four shipments, one in a null group, two in an empty one, one in North.
fn shipments() -> Scenario<GroupedTable> {
    let group = |key, value| {
        (
            key,
            Outline::group(vec![], OpenState::Closed, REGION, value),
        )
    };
    let subtotal = |key, value| (key, Outline::subtotal(vec![], REGION, value));
    let grouped = vec![
        group(NULL, Value::Blank),
        (
            Key(1 << 62),
            Outline::subtotal(vec![], REGION, Value::Blank),
        ),
        group(EMPTY, Value::Text(String::new())),
        subtotal(Key((1 << 62) | 1), Value::Text(String::new())),
        group(NORTH, Value::Text("North".into())),
    ];
    let grid = Grid::new(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(2, 7.0).header(18.0),
    );
    let mut s = Scenario::new(
        grid,
        GroupedTable::new((0..4).map(Key).collect(), 2, REGION, grouped),
    );
    s.input(|g| g.set_support(Axis::Rows, REGION, Support::default().with_group(true)));
    s.frame(V);
    s.input(|g| g.group_by(Axis::Rows, REGION));
    s.frame(V);
    s
}

struct Plain;

impl Content for Plain {
    fn cell(&self, _: Key, _: Key) -> String {
        String::new()
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "region"][col.0 as usize].into()
    }
}

fn draw(s: &mut Scenario<GroupedTable>) -> Buffer {
    s.frame(V);
    let mut buf = Buffer::empty(AREA);
    let theme = Theme::classic();
    let snapshot = s.grid.snapshot();
    render_with(
        &snapshot,
        &Plain,
        &RenderContext::new(&theme),
        AREA,
        &mut buf,
    );
    buf
}

fn line(buf: &Buffer, y: u16) -> String {
    (0..AREA.width).map(|x| buf[(x, y)].symbol()).collect()
}

#[test]
fn a_null_group_reads_null_dim_and_an_empty_one_reads_empty() {
    let mut s = shipments();
    let buf = draw(&mut s);
    let lines: Vec<String> = (0..AREA.height).map(|y| line(&buf, y)).collect();
    insta::assert_snapshot!(
        lines
            .iter()
            .map(|l| l.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    );
    let row = |label: &str| {
        let y = lines.iter().position(|l| l.contains(label)).unwrap() as u16;
        let at = lines[y as usize].find(label).unwrap();
        let x = lines[y as usize][..at].chars().count() as u16;
        (x, y)
    };
    let theme = Theme::classic();
    let dim = |label: &str| {
        let (x, y) = row(label);
        (x..x + label.chars().count() as u16).all(|x| buf[(x, y)].fg == theme.dim.fg.unwrap())
    };
    assert!(dim(NULL_LABEL), "the null group is dim");
    assert!(!dim(EMPTY_LABEL), "the empty group is not");
    let (x, y) = row("North");
    assert_ne!(buf[(x, y)].fg, theme.dim.fg.unwrap());
    assert!(!buf[(x, y)].modifier.contains(Modifier::DIM));
}

#[test]
fn values_read_as_null_and_empty() {
    assert_eq!(value_label(&Value::Blank), "(null)");
    assert_eq!(value_label(&Value::Text(String::new())), "(empty)");
    assert_eq!(value_label(&Value::Text("Bergen".into())), "Bergen");
}
