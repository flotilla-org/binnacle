//! Grouped rows in the terminal: outline form in the row Header Band (an
//! indent, the disclosure mark, a Group Item's label and count) in each
//! preset's grouping roles; the header menu's Group rows by and Ungroup
//! rows; and a grouping kept in a saved layout.

use binnacle_core::{
    Axis, AxisConfig, Grid, Grouping, Key, OpenState, Outline, Support, Value, Viewport,
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
