//! Opening and closing groups in the terminal: closed groups' marks, a
//! toggle's pending mark drawn only once it has waited 150 ms, focus in the
//! outline column, a click on a mark, and the tree keys.

use std::time::Duration;

use binnacle_core::{
    Action, Axis, AxisConfig, Expand, FocusStep, Grid, Grouping, Key, Level, Outline, Value,
    ViewSpec, Viewport,
};
use binnacle_ratatui::{
    Content, Keymap, MARK_DELAY, PendingMarks, Preset, RenderContext, Theme, disclosure_at,
    render_with,
};
use binnacle_testkit::{Group, Scenario, TreeTable};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 30, 12);
const V: Viewport = Viewport {
    width: 30.0,
    height: 12.0,
};

const REGION: Key = Key(1);
const PORT: Key = Key(2);
const NORTH: Key = Key(100);
const SOUTH: Key = Key(101);
const CADIZ: Key = Key(120);

/// North holds Aberdeen (0 and 3) and Bergen (4); South holds Cadiz (1, 2
/// and 5) and an empty port, which can't open.
fn tree() -> Vec<Group> {
    vec![
        Group::groups(
            NORTH.0,
            "North",
            vec![
                Group::leaves(110, "Aberdeen", &[0, 3]),
                Group::leaves(111, "Bergen", &[4]),
            ],
        ),
        Group::groups(
            SOUTH.0,
            "South",
            vec![
                Group::leaves(CADIZ.0, "Cadiz", &[1, 2, 5]),
                Group::leaves(130, "Empty", &[]),
            ],
        ),
    ]
}

/// The shipments by region and port, North closed, focus on South's row.
fn shipments() -> Scenario<TreeTable> {
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 6.0).header(12.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree()));
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
    grouping.rows.expand = Expand::default().with(0, NORTH, false);
    s.frame(V);
    s.input(|g| g.request_view(ViewSpec::default().with_grouping(Some(grouping))));
    s.frame(V);
    s.input(|g| g.set_focus(SOUTH, Key(0)));
    s.frame(V);
    s
}

struct Shipments;

impl Content for Shipments {
    fn cell(&self, row: Key, col: Key) -> String {
        match row.0 >= 100 {
            true => String::new(),
            false => format!("s{}c{}", row.0, col.0),
        }
    }

    fn column_header(&self, col: Key) -> String {
        ["id", "region", "port"][col.0 as usize].into()
    }

    fn group_label(&self, _: Key, outline: &Outline) -> String {
        match &outline.group.as_ref().unwrap().value {
            Value::Text(t) => t.clone(),
            Value::Blank => "(blank)".into(),
        }
    }
}

/// The Grid as it stands (no frame, so a request stays unanswered), drawn
/// at `now` with `marks` brought up to date.
fn draw(s: &mut Scenario<TreeTable>, marks: &mut PendingMarks, now: Duration) -> String {
    let theme = Theme::classic();
    let snapshot = s.grid.snapshot();
    marks.seen(&snapshot, now);
    let mut buf = Buffer::empty(AREA);
    let ctx = RenderContext::new(&theme).at(now).marks(marks);
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

#[test]
fn a_toggle_draws_its_mark_pending_only_once_it_has_waited_and_until_answered() {
    let mut s = shipments();
    let mut marks = PendingMarks::default();
    let start = Duration::from_secs(10);
    let mut out = format!(
        "--- North closed, the empty port with no mark\n{}\n",
        draw(&mut s, &mut marks, start)
    );
    s.input(|g| g.toggle(Axis::Rows, SOUTH).unwrap());
    let toggled = start + Duration::from_millis(5);
    out += &format!(
        "--- South toggled: no pending mark yet\n{}\n",
        draw(&mut s, &mut marks, toggled)
    );
    let early = toggled + MARK_DELAY - Duration::from_millis(1);
    let still = draw(&mut s, &mut marks, early);
    assert!(!still.contains('⋯'), "149 ms on, no pending mark");
    let late = toggled + MARK_DELAY;
    out += &format!(
        "--- 150 ms on, still unanswered: pending\n{}\n",
        draw(&mut s, &mut marks, late)
    );
    s.frame(V);
    let answered = late + Duration::from_millis(10);
    out += &format!(
        "--- answered: closed\n{}",
        draw(&mut s, &mut marks, answered)
    );
    insta::assert_snapshot!(out);
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
fn the_outline_column_takes_focus_from_the_first_column_and_tab_gives_it_back() {
    let mut s = shipments();
    let keymap = Keymap::default();
    let key = |code| {
        keymap
            .action(&KeyEvent::new(code, KeyModifiers::NONE))
            .unwrap()
    };
    // South's row is line 2: its band cell from 0, its first cell from 12.
    assert!(focused_at(&mut s, 12, 2) && !focused_at(&mut s, 0, 2));
    s.input(|g| g.action(key(KeyCode::Left)));
    assert!(s.grid.snapshot().outline_focused());
    assert!(
        focused_at(&mut s, 0, 2) && !focused_at(&mut s, 12, 2),
        "the band cell"
    );
    s.input(|g| g.action(key(KeyCode::Tab)));
    assert!(!s.grid.snapshot().outline_focused());
    assert!(
        focused_at(&mut s, 12, 2) && !focused_at(&mut s, 0, 2),
        "the body again"
    );
}

#[test]
fn a_click_on_a_mark_names_its_group_and_one_on_the_label_or_a_group_with_no_mark_does_not() {
    let mut s = shipments();
    let snapshot = s.frame(V);
    // Rows: header, North, South, Cadiz, its three shipments, Empty.
    assert_eq!(disclosure_at(&snapshot, 0, 1), Some(NORTH));
    assert_eq!(disclosure_at(&snapshot, 1, 2), Some(SOUTH));
    assert_eq!(disclosure_at(&snapshot, 2, 3), Some(CADIZ));
    assert_eq!(
        disclosure_at(&snapshot, 0, 3),
        None,
        "Cadiz's mark is indented"
    );
    assert_eq!(disclosure_at(&snapshot, 4, 2), None, "South's label");
    assert_eq!(
        disclosure_at(&snapshot, 2, 7),
        None,
        "the empty port can't open"
    );
    assert_eq!(disclosure_at(&snapshot, 2, 4), None, "a shipment");
    assert_eq!(disclosure_at(&snapshot, 14, 2), None, "the body");
    s.input(|g| g.toggle(Axis::Rows, NORTH).unwrap());
    let snapshot = s.frame(V);
    assert_eq!(
        snapshot.outline(Axis::Rows, 0).map(|o| o.open),
        Some(binnacle_core::OpenState::Open),
        "the toggle opened North"
    );
}

#[test]
fn the_tree_keys_reach_the_grid_whatever_the_preset_types() {
    for preset in [Preset::Chords, Preset::Modes, Preset::BareKeys] {
        let keymap = Keymap::preset(preset);
        let action = |code| keymap.action(&KeyEvent::new(code, KeyModifiers::NONE));
        assert_eq!(
            action(KeyCode::Tab),
            Some(Action::LeaveOutline),
            "{preset:?}"
        );
        assert_eq!(
            action(KeyCode::Char('*')),
            Some(Action::TypeText("*".into()))
        );
        let shifted = keymap.action(&KeyEvent::new(KeyCode::Char('*'), KeyModifiers::SHIFT));
        assert_eq!(
            shifted,
            Some(Action::TypeText("*".into())),
            "* typed with Shift"
        );
        for d in '0'..='9' {
            assert_eq!(action(KeyCode::Char(d)), Some(Action::TypeText(d.into())));
        }
        assert_eq!(action(KeyCode::Enter), Some(Action::Activate));
        let left = Action::MoveFocus {
            axis: Axis::Cols,
            by: FocusStep::Item,
            forward: false,
            extend: false,
        };
        assert_eq!(action(KeyCode::Left), Some(left));
    }
}
