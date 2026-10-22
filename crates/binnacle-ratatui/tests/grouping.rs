//! Grouped rows in the terminal: outline form in the row Header Band (an
//! indent, the disclosure mark, a Group Item's label and count) in each
//! preset's grouping roles; nested levels with Subtotals before, after or
//! off, the Grand Total pinned, figures on Summary Items and the caption;
//! the header menu's Group rows by and Ungroup rows; and a grouping kept
//! in a saved layout.

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, GRAND_TOTAL, Grid, Grouping, ItemKind, Key, Level, OpenState,
    Outline, Placement, Support, Value, ViewSpec, Viewport,
};
use binnacle_ratatui::{
    Content, HeaderMenu, RenderContext, Theme, render_with, restore_layout, save_layout,
};
use binnacle_testkit::{GroupedTable, Scenario};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

const AREA: Rect = Rect::new(0, 0, 34, 10);
const V: Viewport = Viewport {
    width: 34.0,
    height: 10.0,
};

const REGION: Key = Key(1);
const NORTH: Key = Key(1 << 63);
const SOUTH: Key = Key((1 << 63) | 1);

/// Six shipments keyed 0 to 5 over three columns; column 1 is the region,
/// North for 0, 3 and 4 and South for 1, 2 and 5.
fn shipments() -> Scenario<GroupedTable> {
    let group = |key, name: &str| {
        (
            key,
            Outline::group(vec![], OpenState::Open, REGION, Value::Text(name.into())),
        )
    };
    let leaf = |key, parent| (Key(key), Outline::leaf(vec![parent]));
    let grouped = vec![
        group(NORTH, "North"),
        leaf(0, NORTH),
        leaf(3, NORTH),
        leaf(4, NORTH),
        group(SOUTH, "South"),
        leaf(1, SOUTH),
        leaf(2, SOUTH),
        leaf(5, SOUTH),
    ];
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 7.0).header(12.0).gap(1.0),
    );
    let mut s = Scenario::new(
        grid,
        GroupedTable::new((0..6).map(Key).collect(), 3, REGION, grouped),
    );
    s.input(|g| g.set_support(Axis::Rows, REGION, Support::default().with_group(true)));
    s.frame(V);
    s
}

/// Leaves show their Key; a Group Item's count comes from the host, as a
/// real one gets it from its Table Source.
struct Shipments;

impl Content for Shipments {
    fn cell(&self, row: Key, col: Key) -> String {
        match row.0 >> 63 {
            1 => String::new(),
            _ => format!("s{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "region", "kg"][col.0 as usize].into()
    }

    fn row_header(&self, row: Key, _: u64) -> String {
        format!("#{}", row.0)
    }

    fn group_label(&self, _: Key, outline: &Outline) -> String {
        match &outline.group.as_ref().unwrap().value {
            Value::Text(t) => format!("{t} (3)"),
            Value::Blank => "(blank) (3)".into(),
        }
    }
}

fn draw(s: &mut Scenario<GroupedTable>, theme: &Theme, menu: Option<&HeaderMenu>) -> Buffer {
    s.frame(V);
    let mut buf = Buffer::empty(AREA);
    let snapshot = s.grid.snapshot();
    render_with(
        &snapshot,
        &Shipments,
        &RenderContext::new(theme),
        AREA,
        &mut buf,
    );
    if let Some(menu) = menu {
        menu.render(&snapshot, &Shipments, theme, AREA, &mut buf);
    }
    buf
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

fn describe(style: Style) -> String {
    let colour = |c: Option<Color>| c.map_or("-".to_string(), |c| c.to_string());
    let mut out = format!("fg {} bg {}", colour(style.fg), colour(style.bg));
    if !style.add_modifier.is_empty() {
        out += &format!(" {:?}", style.add_modifier);
    }
    out
}

/// The text, then each cell's style as a letter (`.` for unstyled), then
/// what each letter stands for.
fn styled(buf: &Buffer) -> String {
    let a = buf.area;
    let mut legend: Vec<Style> = Vec::new();
    let mut map = String::new();
    for y in a.top()..a.bottom() {
        for x in a.left()..a.right() {
            let style = buf[(x, y)].style();
            if style == Style::default().fg(Color::Reset).bg(Color::Reset) {
                map.push('.');
                continue;
            }
            let i = legend.iter().position(|&s| s == style).unwrap_or_else(|| {
                legend.push(style);
                legend.len() - 1
            });
            map.push(char::from(b'a' + i as u8));
        }
        map.push('\n');
    }
    let legend: String = legend
        .iter()
        .enumerate()
        .map(|(i, &s)| format!("{}: {}\n", char::from(b'a' + i as u8), describe(s)))
        .collect();
    format!("{}\n\n{map}\n{legend}", text(buf))
}

fn grouped() -> Scenario<GroupedTable> {
    let mut s = shipments();
    s.input(|g| g.group_by(Axis::Rows, REGION));
    s.frame(V);
    s
}

#[test]
fn grouped_rows_draw_in_outline_form_in_every_preset() {
    for theme in Theme::presets() {
        let mut s = grouped();
        let buf = draw(&mut s, &theme, None);
        insta::assert_snapshot!(theme.name, styled(&buf));
    }
}

#[test]
fn every_preset_tells_group_items_apart_from_leaves() {
    for theme in Theme::presets() {
        let roles = [
            theme.group_item,
            theme.subtotal,
            theme.grand_total,
            theme.disclosure,
        ];
        for role in roles {
            assert_ne!(
                role,
                Style::default(),
                "{} leaves a role unstyled",
                theme.name
            );
        }
        assert_ne!(theme.group_item, theme.subtotal, "{}", theme.name);
        assert_ne!(theme.subtotal, theme.grand_total, "{}", theme.name);
    }
}

fn press(menu: &mut HeaderMenu, s: &mut Scenario<GroupedTable>, code: KeyCode) {
    let event = KeyEvent::new(code, KeyModifiers::NONE);
    assert!(menu.key(&event, &mut s.grid, |_, _| Vec::new()));
}

fn grouping(s: &mut Scenario<GroupedTable>) -> Option<Grouping> {
    s.grid.snapshot().applied_view().grouping.clone()
}

#[test]
fn the_column_menu_groups_the_rows_by_its_column_and_ungroups_them() {
    let mut s = shipments();
    s.input(|g| g.set_focus(Key(3), REGION));
    let mut menu = HeaderMenu::new();
    assert!(menu.open(&s.grid.snapshot(), Axis::Cols));
    let open = text(&draw(&mut s, &Theme::classic(), Some(&menu)));
    assert!(open.contains("g Group rows by this column"), "{open}");

    press(&mut menu, &mut s, KeyCode::Char('g'));
    assert!(!menu.is_open());
    s.frame(V);
    assert_eq!(grouping(&mut s), Some(Grouping::rows_by(REGION)));
    let shown = text(&draw(&mut s, &Theme::classic(), None));
    assert!(shown.contains("▾ North (3)"), "{shown}");
    assert!(
        !shown.contains("region"),
        "the region column leaves: {shown}"
    );

    // The focused shipment kept its Key, so its column menu offers to ungroup.
    s.input(|g| g.set_focus(Key(3), Key(2)));
    assert!(menu.open(&s.grid.snapshot(), Axis::Cols));
    let open = text(&draw(&mut s, &Theme::classic(), Some(&menu)));
    assert!(open.contains("g Ungroup rows"), "{open}");
    press(&mut menu, &mut s, KeyCode::Char('g'));
    s.frame(V);
    assert_eq!(grouping(&mut s), None);
}

#[test]
fn a_column_the_rows_cannot_group_by_offers_no_grouping() {
    let mut s = shipments();
    s.input(|g| g.set_focus(Key(3), Key(2)));
    let mut menu = HeaderMenu::new();
    assert!(menu.open(&s.grid.snapshot(), Axis::Cols));
    let open = text(&draw(&mut s, &Theme::classic(), Some(&menu)));
    assert!(!open.contains("Group"), "{open}");
    press(&mut menu, &mut s, KeyCode::Char('g'));
    s.frame(V);
    assert_eq!(grouping(&mut s), None);
}

#[test]
fn a_saved_layout_keeps_the_grouping() {
    let mut s = grouped();
    let layout = save_layout(&mut s.grid);
    assert_eq!(layout.view.grouping, Some(Grouping::rows_by(REGION)));

    let mut fresh = shipments();
    fresh.input(|g| assert!(restore_layout(g, &layout)));
    fresh.frame(V);
    assert_eq!(grouping(&mut fresh), Some(Grouping::rows_by(REGION)));
    let shown = text(&draw(&mut fresh, &Theme::classic(), None));
    assert!(shown.contains("▾ South (3)"), "{shown}");
}

const PORT: Key = Key(2);

/// Shipments by region, then port, as a grouping layer would send them
/// with Subtotals placed as `subtotals` says per level and the Grand Total
/// after: North holds Aberdeen (0, 3) and Bergen (4); South's one port,
/// Cadiz, is closed.
fn nested(subtotals: [Placement; 2]) -> Scenario<GroupedTable> {
    let text = |name: &str| Value::Text(name.into());
    let mut items = Vec::new();
    let mut next = 1 << 63;
    let mut key = || {
        next += 1;
        Key(next)
    };
    let regions: [(&str, &[(&str, OpenState, &[u64])]); 2] = [
        (
            "North",
            &[
                ("Aberdeen", OpenState::Open, &[0, 3]),
                ("Bergen", OpenState::Open, &[4]),
            ],
        ),
        ("South", &[("Cadiz", OpenState::Closed, &[])]),
    ];
    for (name, ports) in regions {
        let region = key();
        items.push((region, Outline::group(vec![], OpenState::Open, REGION, text(name))));
        let sub = (key(), Outline::subtotal(vec![region], REGION, text(name)));
        if subtotals[0] == Placement::Before {
            items.push(sub.clone());
        }
        for &(port_name, open, leaves) in ports {
            let port = key();
            let group = Outline::group(vec![region], open, PORT, text(port_name));
            items.push((port, group));
            let port_sub = (key(), Outline::subtotal(vec![region, port], PORT, text(port_name)));
            if subtotals[1] == Placement::Before && open == OpenState::Open {
                items.push(port_sub.clone());
            }
            for &leaf in leaves {
                items.push((Key(leaf), Outline::leaf(vec![region, port])));
            }
            if subtotals[1] == Placement::After && open == OpenState::Open {
                items.push(port_sub);
            }
        }
        if subtotals[0] == Placement::After {
            items.push(sub);
        }
    }
    items.push((GRAND_TOTAL, Outline::grand_total()));
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 7.0).header(16.0).gap(1.0),
    );
    let mut s = Scenario::new(
        grid,
        GroupedTable::new((0..6).map(Key).collect(), 3, REGION, items),
    );
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![
        Level::Field {
            field: REGION,
            subtotal: subtotals[0],
        },
        Level::Field {
            field: PORT,
            subtotal: subtotals[1],
        },
    ];
    grouping.values = vec![AggregateValue::of(Key(2), "sum"), AggregateValue::count()];
    grouping.rows.grand_total = Placement::After;
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.frame(V);
    s
}

/// Leaves show their Key; Summary Items their sum in the kg column, as a
/// host draws what its source aggregates.
struct Figures;

impl Content for Figures {
    fn cell(&self, row: Key, col: Key) -> String {
        match (row.0 >> 63, col.0) {
            (1, 2) => format!("Σ{}", row.0 & 0xff),
            (1, _) => String::new(),
            _ => format!("s{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "region", "kg"][col.0 as usize].into()
    }

    fn row_header(&self, row: Key, _: u64) -> String {
        format!("#{}", row.0)
    }
}

fn draw_figures(s: &mut Scenario<GroupedTable>, height: u16) -> String {
    let area = Rect::new(0, 0, 34, height);
    s.frame(Viewport {
        width: 34.0,
        height: f32::from(height),
    });
    let mut buf = Buffer::empty(area);
    let snapshot = s.grid.snapshot();
    let theme = Theme::classic();
    render_with(&snapshot, &Figures, &RenderContext::new(&theme), area, &mut buf);
    text(&buf)
}

#[test]
fn nested_levels_draw_their_subtotals_where_placed_and_the_grand_total_pinned() {
    let mut out = String::new();
    for subtotals in [
        [Placement::After, Placement::After],
        [Placement::Before, Placement::Off],
        [Placement::Off, Placement::Off],
    ] {
        let mut s = nested(subtotals);
        let pins: Vec<Key> = s.frame(V).pins(Axis::Rows, binnacle_core::Edge::Trailing).collect();
        assert_eq!(pins, vec![GRAND_TOTAL], "{subtotals:?}");
        out += &format!("--- Subtotals {subtotals:?}\n{}\n", draw_figures(&mut s, 14));
    }
    insta::assert_snapshot!(out);
}

#[test]
fn a_single_value_names_itself_in_the_corner_with_its_parameters() {
    struct Named;
    impl Content for Named {
        fn cell(&self, _: Key, _: Key) -> String {
            String::new()
        }
        fn column_header(&self, col: Key) -> String {
            ["id", "region", "kg"][col.0 as usize].into()
        }
    }
    let quantile = AggregateValue::of(Key(2), "quantile").with_params(vec!["0.9".into()]);
    assert_eq!(Named.value_name(&quantile), "quantile(kg, 0.9)");
    assert_eq!(Named.value_name(&AggregateValue::count()), "count");
    let label = |kind, name: &str| {
        let mut o = Outline::subtotal(vec![], REGION, Value::Text(name.into()));
        o.kind = kind;
        Named.group_label(Key(0), &o)
    };
    assert_eq!(label(ItemKind::Subtotal, "Bergen"), "Bergen total");
    assert_eq!(label(ItemKind::Group, "Bergen"), "Bergen");
    assert_eq!(Named.group_label(GRAND_TOTAL, &Outline::grand_total()), "Grand total");
}
