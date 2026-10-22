//! Summary Items as the user meets them in the terminal: a click on a
//! Group Item's row header selects its group and Shift-click stretches by
//! whole groups, Ctrl+C asks for a copy that `copy_text` writes as the
//! grid draws it (labels first, a closed group's items left out), and a
//! plain table copies its cells alone.

use std::time::Instant;

use binnacle_core::{AxisConfig, Effect, Expand, Grid, Grouping, Key, Level, ViewSpec, Viewport};
use binnacle_ratatui::{
    Content, CopiedRow, Keymap, Mouse, RenderContext, Theme, copy_text, render_with,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const BERGEN: Key = Key(1001);

const AREA: Rect = Rect::new(0, 0, 40, 12);
const V: Viewport = Viewport {
    width: 40.0,
    height: 12.0,
};

/// North (its ports Aberdeen, with shipments 0 and 1, and Bergen, with 2,
/// closed) and South (Cadiz, with 3), every other group open.
fn shipments(grouped: bool) -> Scenario<TreeTable> {
    let tree = vec![
        Group::groups(
            1002,
            "North",
            vec![
                Group::leaves(1000, "Aberdeen", &[0, 1]),
                Group::leaves(1001, "Bergen", &[2]),
            ],
        ),
        Group::groups(1004, "South", vec![Group::leaves(1003, "Cadiz", &[3])]),
    ];
    let grid = Grid::new(
        AxisConfig::new(4, 1.0).header(1.0),
        AxisConfig::new(2, 8.0).header(14.0).gap(1.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 2, tree));
    s.frame(V);
    if grouped {
        let mut grouping = Grouping::rows_by(REGION);
        grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
        grouping.rows.expand = Expand {
            depth: binnacle_core::Depth::All,
            except: vec![BERGEN],
        };
        s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
        s.frame(V);
    }
    s
}

/// Leaves show their Key and column; Summary Items nothing in the body.
struct Shipments;

impl Content for Shipments {
    fn cell(&self, row: Key, col: Key) -> String {
        match row.0 >= 1000 {
            true => String::new(),
            false => format!("s{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "kg"][col.0 as usize].into()
    }

    fn row_header(&self, row: Key, _: u64) -> String {
        format!("#{}", row.0)
    }
}

/// A left click at (`x`, `y`) in the grid's area, through the adapter's mouse.
fn click(s: &mut Scenario<TreeTable>, x: u16, y: u16, modifiers: KeyModifiers) {
    let mut mouse = Mouse::default();
    let now = Instant::now();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let m = MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers,
        };
        if let Some(pointer) = mouse.event(&m, AREA, now) {
            s.input(|g| g.pointer(pointer));
        }
    }
    s.frame(V);
}

/// Ctrl+C through the default keymap, and the copy it asks for as
/// `copy_text` writes it, the host resolving Positions through its rows.
fn copy(s: &mut Scenario<TreeTable>) -> Option<String> {
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = Keymap::default().action(&ctrl_c).expect("Ctrl+C is bound");
    s.input(|g| g.action(action));
    let shown = s.source.shown().to_vec();
    s.effects().into_iter().find_map(|e| {
        let Effect::CopyRequested {
            rows,
            cols,
            row_labels,
            ..
        } = e
        else {
            return None;
        };
        let rows: Vec<CopiedRow> = (rows.into_iter().flatten())
            .map(|position| match shown.get(position as usize) {
                Some((key, outline)) => CopiedRow {
                    key: *key,
                    position,
                    outline: Some(outline.clone()),
                },
                // Plain rows are keyed by Position.
                None => CopiedRow {
                    key: Key(position),
                    position,
                    outline: None,
                },
            })
            .collect();
        let cols: Vec<Key> = cols.into_iter().flatten().map(Key).collect();
        Some(copy_text(&Shipments, &rows, &cols, row_labels))
    })
}

fn selected(s: &mut Scenario<TreeTable>) -> Option<(Vec<u64>, Vec<u64>)> {
    let snap = s.frame(V);
    let range = snap.selection_range()?;
    let flat = |runs: &[std::ops::Range<u64>]| runs.iter().flat_map(|r| r.clone()).collect();
    Some((flat(&range.rows), flat(&range.cols)))
}

fn draw(s: &mut Scenario<TreeTable>) -> String {
    s.frame(V);
    let mut buf = Buffer::empty(AREA);
    let theme = Theme::classic();
    render_with(
        &s.grid.snapshot(),
        &Shipments,
        &RenderContext::new(&theme),
        AREA,
        &mut buf,
    );
    (AREA.top()..AREA.bottom())
        .map(|y| {
            let line: String = (AREA.left()..AREA.right())
                .map(|x| buf[(x, y)].symbol())
                .collect();
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_click_on_a_group_items_label_selects_its_group_and_ctrl_c_copies_it_as_drawn() {
    let mut s = shipments(true);
    let drawn = draw(&mut s);
    // North's label, a row below the column headers.
    click(&mut s, 6, 1, KeyModifiers::NONE);
    assert_eq!(
        selected(&mut s),
        Some(((0..5).collect(), vec![0, 1])),
        "{drawn}"
    );
    assert_eq!(
        copy(&mut s).as_deref(),
        Some("North\t\t\nAberdeen\t\t\n#0\ts0c0\ts0c1\n#1\ts1c0\ts1c1\nBergen\t\t")
    );
}

#[test]
fn shift_click_on_another_group_item_stretches_by_whole_groups() {
    let mut s = shipments(true);
    // Aberdeen, then Shift on South: Aberdeen's group through South's.
    click(&mut s, 8, 2, KeyModifiers::NONE);
    assert_eq!(selected(&mut s), Some(((1..4).collect(), vec![0, 1])));
    click(&mut s, 6, 6, KeyModifiers::SHIFT);
    assert_eq!(selected(&mut s), Some(((1..8).collect(), vec![0, 1])));
    let text = copy(&mut s).unwrap();
    let labels: Vec<&str> = text
        .lines()
        .map(|l| l.split('\t').next().unwrap())
        .collect();
    assert_eq!(
        labels,
        ["Aberdeen", "#0", "#1", "Bergen", "South", "Cadiz", "#3"]
    );
}

#[test]
fn a_plain_table_copies_the_focused_cell_alone_without_labels() {
    let mut s = shipments(false);
    // The second row's kg cell.
    click(&mut s, 24, 2, KeyModifiers::NONE);
    assert_eq!(copy(&mut s).as_deref(), Some("s1c1"));
}
