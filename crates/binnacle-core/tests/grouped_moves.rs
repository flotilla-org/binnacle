//! Seam 1 scenarios for moves and edits on grouped rows: a Leaf moves
//! within its group, a drop in another group is refused with a reason for
//! the drop caption, Summary Items never move, and their cells never edit.

use binnacle_core::{
    Action, Axis, AxisConfig, DropTarget, Effect, Grid, Grouping, Key, Level, Mode, MoveRefusal,
    PointerEvent, ViewSpec, Viewport,
};
use binnacle_testkit::{Group, Scenario, TreeTable};

const V: Viewport = Viewport {
    width: 20.0,
    height: 13.0,
};

const REGION: Key = Key(10);
const PORT: Key = Key(11);
const NORTH: Key = Key(100);
const BERGEN: Key = Key(111);

/// Everything open, one row per unit under a 1-high header band:
/// 0 North, 1 Aberdeen, 2 shipment 0, 3 shipment 3, 4 Bergen, 5 shipment 4,
/// 6 South, 7 Cadiz, 8 to 10 shipments 1, 2 and 5. Row p spans y
/// `1 + p .. 2 + p`; the row header band is 3 wide.
fn grouped() -> Scenario<TreeTable> {
    let tree = vec![
        Group::groups(
            NORTH.0,
            "North",
            vec![
                Group::leaves(110, "Aberdeen", &[0, 3]),
                Group::leaves(BERGEN.0, "Bergen", &[4]),
            ],
        ),
        Group::groups(101, "South", vec![Group::leaves(120, "Cadiz", &[1, 2, 5])]),
    ];
    let grid = Grid::new(
        AxisConfig::new(6, 1.0).header(1.0),
        AxisConfig::new(3, 5.0).header(3.0),
    );
    let mut s = Scenario::new(grid, TreeTable::new(vec![REGION, PORT], 3, tree));
    s.input(|g| {
        g.set_pin_zone(Axis::Rows, 0.0);
        let mut grouping = Grouping::rows_by(REGION);
        grouping.row_levels = vec![Level::field(REGION), Level::field(PORT)];
        g.request_view(ViewSpec::default().with_grouping(Some(grouping)));
    });
    s.frame(V);
    s.frame(V);
    s
}

/// Drags the row header at Position `from` to `y`, leaving it held.
fn drag(s: &mut Scenario<TreeTable>, from: u64, y: f32) -> DropTarget {
    let y0 = 1.5 + from as f32;
    s.input(|g| {
        g.pointer(PointerEvent::down(1.0, y0));
        g.pointer(PointerEvent::moved(1.0, y));
    });
    let snapshot = s.frame(V);
    match snapshot.mode() {
        Mode::MovingHeaders { target, .. } => target,
        mode => panic!("not moving: {mode:?}"),
    }
}

fn moves(effects: &[Effect]) -> Vec<(Vec<Key>, u64)> {
    (effects.iter())
        .filter_map(|e| match e {
            Effect::MoveRequested { keys, to, .. } => Some((keys.clone(), *to)),
            _ => None,
        })
        .collect()
}

fn rows(s: &mut Scenario<TreeTable>) -> Vec<Key> {
    s.frame(V).rows().iter().filter_map(|i| i.key).collect()
}

#[test]
fn a_leaf_moves_within_its_group() {
    let mut s = grouped();
    let mut out = s.step("everything open", V);
    let target = drag(&mut s, 3, 3.2);
    out += &s.render();
    assert_eq!(
        target,
        DropTarget::Move {
            to: 2,
            unpin: false
        }
    );
    s.input(|g| g.pointer(PointerEvent::up(1.0, 3.2)));
    out += &s.step(
        "release: shipment 3 moves before shipment 0, in Aberdeen",
        V,
    );
    insta::assert_snapshot!(out);
    let shown = rows(&mut s);
    assert_eq!(&shown[1..4], &[Key(110), Key(3), Key(0)]);
}

#[test]
fn a_drop_in_another_group_is_refused_and_asks_nothing() {
    let mut s = grouped();
    let target = drag(&mut s, 3, 6.2);
    let refused = DropTarget::Refused {
        to: 5,
        why: MoveRefusal::BetweenGroups,
    };
    assert_eq!(target, refused, "into Bergen, past its Group Item");
    let indicator = s.frame(V).drop_indicator().expect("the line shows");
    assert_eq!(indicator.target, refused);
    s.input(|g| g.pointer(PointerEvent::up(1.0, 6.2)));
    assert_eq!(moves(&s.effects()), vec![]);

    let target = drag(&mut s, 3, 7.2);
    assert_eq!(
        target,
        DropTarget::Refused {
            to: 6,
            why: MoveRefusal::BetweenGroups,
        },
        "between Bergen's last Leaf and South: a region away"
    );
    s.input(|g| g.pointer(PointerEvent::up(1.0, 7.2)));
    assert_eq!(moves(&s.effects()), vec![]);
}

#[test]
fn a_drop_at_either_end_of_its_group_moves() {
    let mut s = grouped();
    // Just after Aberdeen's Group Item, and just after its last Leaf.
    assert_eq!(
        drag(&mut s, 3, 2.6),
        DropTarget::Move {
            to: 2,
            unpin: false
        }
    );
    s.input(|g| g.pointer(PointerEvent::up(1.0, 2.6)));
    s.frame(V);
    assert_eq!(
        drag(&mut s, 2, 4.6),
        DropTarget::Move {
            to: 4,
            unpin: false
        }
    );
}

#[test]
fn summary_items_never_move() {
    let mut s = grouped();
    let target = drag(&mut s, 4, 9.2);
    assert_eq!(
        target,
        DropTarget::Refused {
            to: 8,
            why: MoveRefusal::Summary,
        }
    );
    s.input(|g| g.pointer(PointerEvent::up(1.0, 9.2)));
    assert_eq!(moves(&s.effects()), vec![]);
}

#[test]
fn a_summary_item_s_cell_never_edits() {
    let mut s = grouped();
    s.input(|g| {
        g.set_focus(NORTH, Key(1));
        g.action(Action::BeginEdit);
    });
    let asked = |e: &Effect| matches!(e, Effect::EditRequested { .. });
    assert!(!s.effects().iter().any(asked), "no edit on a Group Item");
    s.input(|g| g.edit_started(NORTH, Key(1)));
    assert_eq!(
        s.effects(),
        vec![Effect::EditCancelled {
            row: NORTH,
            col: Key(1)
        }],
        "a host's editor opened anyway is dropped at once"
    );
    assert_eq!(s.frame(V).mode(), Mode::Idle);

    s.input(|g| {
        g.set_focus(Key(4), Key(1));
        g.action(Action::BeginEdit);
    });
    assert!(s.effects().iter().any(asked), "a Leaf edits as ever");
    s.input(|g| g.edit_started(Key(4), Key(1)));
    s.pump();
    assert_eq!(
        s.frame(V).mode(),
        Mode::Editing {
            row: Key(4),
            col: Key(1)
        }
    );
}
