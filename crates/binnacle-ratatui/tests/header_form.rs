//! Header form in the terminal: each row level a column of the band, each
//! group a span with its label kept in view, `›` where a span goes on past
//! the view or a pinned edge, a closed group's Subtotal under its span,
//! pending marks, a click on a span's mark and the tree keys between the
//! level columns.

use std::time::Duration;

use binnacle_core::{
    Axis, AxisConfig, Depth, Edge, Expand, Form, Grid, Grouping, HeaderSpan, ItemKind, Key, Level,
    Outline, Value, ViewSpec, Viewport,
};
use binnacle_ratatui::{
    Content, Keymap, MARK_DELAY, PendingMarks, RenderContext, Theme, disclosure_at, render_with,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 40, 14);
const V: Viewport = Viewport {
    width: 40.0,
    height: 14.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const ABERDEEN: Key = Key(110);
const BERGEN: Key = Key(111);
const CADIZ: Key = Key(120);

/// North holds Aberdeen (0 and 3) and Bergen (4); South holds Cadiz (1, 2
/// and 5).
fn tree() -> Vec<Group> {
    vec![
        Group::groups(
            NORTH.0,
            "North",
            vec![
                Group::leaves(ABERDEEN.0, "Aberdeen", &[0, 3]),
                Group::leaves(BERGEN.0, "Bergen", &[4]),
            ],
        ),
        Group::groups(
            SOUTH.0,
            "South",
            vec![Group::leaves(CADIZ.0, "Cadiz", &[1, 2, 5])],
        ),
    ]
}

/// The shipments in header form, a Subtotal after each open group, opened
/// as `expand` says; level columns 8 and 12 wide in a band of 24, focus on
/// shipment 0.
fn shipments(expand: Expand) -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(24.0).gap(1.0),
    );
    let table = TreeTable::new(vec![REGION, PORT], 3, tree()).subtotals_after();
    let mut s = Scenario::new(grid, table);
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = expand;
    grouping.rows.form = Form::Header;
    s.frame(V);
    s.input(|g| {
        g.set_tiers(Axis::Rows, &[8.0, 12.0]);
        g.request_view(ViewSpec::default().with_grouping(Some(grouping)));
    });
    s.frame(V);
    s.input(|g| g.set_focus(Key(0), Key(0)));
    s.frame(V);
    s
}

struct Shipments;

fn name(key: Key) -> &'static str {
    match key {
        NORTH => "North",
        SOUTH => "South",
        ABERDEEN => "Aberdeen",
        BERGEN => "Bergen",
        CADIZ => "Cadiz",
        _ => "?",
    }
}

impl Content for Shipments {
    fn cell(&self, row: Key, col: Key) -> String {
        match row.0 >= 100 {
            true => "sum".into(),
            false => format!("s{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "crew", "weight"][col.0 as usize].into()
    }

    fn row_header(&self, row: Key, _: u64) -> String {
        format!("#{}", row.0)
    }

    fn group_label(&self, _: Key, outline: &Outline) -> String {
        let value = match &outline.group.as_ref().unwrap().value {
            Value::Text(t) => t.clone(),
            Value::Blank => "(blank)".into(),
        };
        match outline.kind {
            ItemKind::Subtotal => format!("{value} total"),
            _ => value,
        }
    }

    fn span_label(&self, span: &HeaderSpan) -> String {
        name(span.key).into()
    }
}

/// The Grid as it stands (no frame, so a request stays unanswered), drawn
/// at `now` with `marks` brought up to date.
fn draw(s: &mut Scenario<TreeTable>, marks: &mut PendingMarks, now: Duration) -> String {
    let theme = Theme::classic();
    let snapshot = s.grid.snapshot();
    marks.seen(&snapshot, now);
    let mut buf = Buffer::empty(AREA);
    let ctx = RenderContext::new(&theme)
        .at(now)
        .marks(marks)
        .scrollbars(false);
    render_with(&snapshot, &Shipments, &ctx, AREA, &mut buf);
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

fn now(s: &mut Scenario<TreeTable>) -> String {
    draw(s, &mut PendingMarks::default(), Duration::ZERO)
}

#[test]
fn open_and_closed_spans_with_a_closed_groups_subtotal_under_its_span() {
    let mut s = shipments(Expand::default());
    let mut out = format!("--- everything open\n{}\n", now(&mut s));
    s.input(|g| g.collapse(Axis::Rows, ABERDEEN).unwrap());
    s.frame(V);
    out += &format!("--- Aberdeen closed\n{}\n", now(&mut s));
    let mut s = shipments(Expand::to(Depth::Levels(0)));
    out += &format!("--- every group closed\n{}", now(&mut s));
    insta::assert_snapshot!(out);
}

#[test]
fn a_scrolled_span_keeps_its_label_and_marks_where_it_goes_on() {
    let mut s = shipments(Expand::default());
    let short = Viewport {
        width: 40.0,
        height: 4.0,
    };
    s.frame(short);
    s.input(|g| g.scroll_to_item(Axis::Rows, 1));
    s.frame(short);
    let out = now(&mut s);
    insta::assert_snapshot!(out);
    let first = out.lines().nth(1).unwrap();
    assert!(first.contains("Nor…›") && first.contains("Aberdeen›"));
}

#[test]
fn a_span_breaks_at_a_pinned_row_each_part_labelled() {
    let mut s = shipments(Expand::default());
    s.input(|g| g.pin(Axis::Rows, Edge::Leading, Key(3)));
    s.frame(V);
    let out = now(&mut s);
    insta::assert_snapshot!(out);
    assert_eq!(
        out.matches("Aberdeen").count(),
        3,
        "two parts and the Subtotal"
    );
}

#[test]
fn a_toggled_span_draws_pending_once_it_has_waited() {
    let mut s = shipments(Expand::default());
    let mut marks = PendingMarks::default();
    let start = Duration::from_secs(10);
    draw(&mut s, &mut marks, start);
    s.input(|g| g.toggle(Axis::Rows, NORTH).unwrap());
    let toggled = start + Duration::from_millis(5);
    let early = draw(&mut s, &mut marks, toggled);
    assert!(!early.contains('⋯'));
    let late = draw(&mut s, &mut marks, toggled + MARK_DELAY);
    insta::assert_snapshot!(late);
    assert!(late.lines().nth(1).unwrap().contains("⋯ North"));
}

#[test]
fn a_click_on_a_spans_mark_names_its_group() {
    let mut s = shipments(Expand::default());
    let snapshot = s.frame(V);
    assert_eq!(disclosure_at(&snapshot, 1, 1), Some(NORTH));
    assert_eq!(disclosure_at(&snapshot, 9, 1), Some(ABERDEEN));
    assert_eq!(disclosure_at(&snapshot, 9, 4), Some(BERGEN));
    assert_eq!(disclosure_at(&snapshot, 1, 2), None, "below the label");
    assert_eq!(disclosure_at(&snapshot, 4, 1), None, "the label");
}

/// Whether the cell at (`x`, `y`) is drawn in the focus style.
fn focused_at(s: &mut Scenario<TreeTable>, x: u16, y: u16) -> bool {
    let theme = Theme::classic();
    let snapshot = s.frame(V);
    let mut buf = Buffer::empty(AREA);
    render_with(
        &snapshot,
        &Shipments,
        &RenderContext::new(&theme),
        AREA,
        &mut buf,
    );
    (buf[(x, y)].style().add_modifier).contains(theme.focus.add_modifier)
}

#[test]
fn the_keymap_steps_between_level_columns_and_toggles_spans() {
    let mut s = shipments(Expand::default());
    let keymap = Keymap::default();
    let press = |s: &mut Scenario<TreeTable>, code| {
        let action = keymap
            .action(&KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap();
        s.input(|g| g.action(action));
        s.frame(V);
    };
    press(&mut s, KeyCode::Left);
    let snap = s.frame(V);
    assert!(snap.outline_focused());
    assert_eq!(snap.focused_tier(), 1, "the port column");
    assert!(
        focused_at(&mut s, 12, 1) && focused_at(&mut s, 12, 2),
        "the whole of Aberdeen's span"
    );
    press(&mut s, KeyCode::Left);
    assert_eq!(
        s.frame(V).focus(Axis::Rows).and_then(|f| f.key),
        Some(TreeTable::subtotal(ABERDEEN)),
        "Left closes Aberdeen, focus on its Subtotal"
    );
    press(&mut s, KeyCode::Left);
    assert_eq!(
        s.frame(V).focused_tier(),
        0,
        "Left again: the region column"
    );
    assert!(focused_at(&mut s, 3, 1));
    press(&mut s, KeyCode::Right);
    assert_eq!(s.frame(V).focused_tier(), 1, "Right on open North steps in");
    press(&mut s, KeyCode::Enter);
    assert_eq!(s.frame(V).count(Axis::Rows), 11, "Enter opens Aberdeen");
    press(&mut s, KeyCode::Tab);
    assert!(!s.frame(V).outline_focused(), "Tab leaves for the body");
}

#[test]
fn a_groups_label_that_is_the_current_match_draws_its_span_in_the_find_style() {
    let theme = Theme::classic();
    let mut s = shipments(Expand::default());
    s.input(|g| {
        g.set_find_support(Some(binnacle_core::TextSupport::new(false)));
        g.set_find(Some(binnacle_core::FindQuery::new(
            binnacle_core::TextQuery::new("bergen"),
        )));
        g.action(binnacle_core::Action::FindNext);
    });
    s.frame(V);
    let snapshot = s.grid.snapshot();
    assert_eq!(
        snapshot.find_match(),
        Some(binnacle_core::FindMatch::new(Key(4), PORT))
    );
    let mut buf = Buffer::empty(AREA);
    render_with(
        &snapshot,
        &Shipments,
        &RenderContext::new(&theme),
        AREA,
        &mut buf,
    );
    let found = theme.find_current.bg.unwrap();
    // The band, 24 wide (the port field is also a column key here).
    let lit: Vec<(u16, u16)> = (0..AREA.height)
        .flat_map(|y| (0..24).map(move |x| (x, y)))
        .filter(|&(x, y)| buf[(x, y)].bg == found)
        .collect();
    // Bergen's span, in the port column (8 to 20) on shipment 4's line.
    let line: String = (lit.iter()).map(|&(x, y)| buf[(x, y)].symbol()).collect();
    assert!(
        lit.iter()
            .all(|&(x, y)| (8..20).contains(&x) && y == lit[0].1),
        "{lit:?}"
    );
    assert!(line.contains("Bergen"), "{line:?}");
}
