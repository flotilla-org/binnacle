//! A Layout restored onto a Grid over an In-Memory Table Source and read
//! back, through the scenario harness: the order goes through the source's
//! move path and the header moves since are saved with it, and what the
//! host's IDs don't name is skipped, reported, and kept on the next save.

use binnacle_core::{Axis, AxisConfig, Edge, Effect, Grid, Key, PointerEvent, Viewport};
use binnacle_layout::view::{Direction, SortColumn, ViewSpec};
use binnacle_layout::{Bands, EdgePins, IdTable, Layout, Order, Pins, Sizes};
use binnacle_testkit::{RecordTable, Scenario};
use hegel::TestCase;
use hegel::generators as gs;

const ROWS: u64 = 6;
const COLS: u64 = 4;
/// Shows every row and column.
const V: Viewport = Viewport {
    width: 100.0,
    height: 20.0,
};

fn config() -> (AxisConfig, AxisConfig) {
    (
        AxisConfig::new(ROWS, 1.0).header(1.0),
        AxisConfig::new(COLS, 6.0).header(4.0),
    )
}

fn table() -> Scenario<RecordTable> {
    let records = (0..ROWS as i64)
        .map(|r| {
            (0..COLS as i64)
                .map(|c| Some((r * 7 + c * 3) % 5))
                .collect()
        })
        .collect();
    let (rows, cols) = config();
    let mut s = Scenario::new(
        Grid::new(rows, cols),
        RecordTable::new(records, COLS as usize, 0..COLS),
    );
    s.input(|g| {
        g.set_movable(Axis::Rows, true);
        g.set_movable(Axis::Cols, true);
    });
    s.frame(V);
    s
}

/// The rows `r0`.. and columns `c0`.. by their Logical Indexes, and
/// nothing else.
fn ids() -> IdTable<String, String> {
    IdTable::new(
        (0..ROWS).map(|i| (format!("r{i}"), Key(i))),
        (0..COLS).map(|i| (format!("c{i}"), Key(i))),
    )
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The Keys of `axis` in drawn Position order.
fn order<S: binnacle_testkit::ScenarioSource>(s: &mut Scenario<S>, axis: Axis) -> Vec<u64> {
    let snapshot = s.frame(V);
    let mut items: Vec<_> = snapshot
        .drawn(axis)
        .map(|i| (i.position, i.key.unwrap().0))
        .collect();
    items.sort();
    items.into_iter().map(|(_, k)| k).collect()
}

fn moves(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|e| matches!(e, Effect::MoveRequested { .. }))
        .count()
}

/// A Layout setting only the order: each axis listed, or unset if empty.
fn ordered(rows: &[&str], cols: &[&str]) -> Layout<String, String> {
    let set = |ids: &[&str]| (!ids.is_empty()).then(|| strings(ids));
    Layout {
        order: Some(Order {
            rows: set(rows),
            cols: set(cols),
        }),
        ..Layout::default()
    }
}

#[test]
fn an_order_puts_the_listed_items_first_in_that_order_on_both_axes_one_move_after_another() {
    let mut s = table();
    let mut unplaced = Order::default();
    s.input(|g| unplaced = ordered(&["r4", "r1"], &["c2", "c0"]).restore(g, &ids()));
    assert!(unplaced.is_unset());
    assert_eq!(moves(&s.effects()), 2);
    assert_eq!(order(&mut s, Axis::Rows), [4, 1, 0, 2, 3, 5]);
    assert_eq!(order(&mut s, Axis::Cols), [2, 0, 1, 3]);
}

#[test]
fn an_order_already_in_place_asks_for_no_move() {
    let mut s = table();
    s.input(|g| {
        ordered(&["r0", "r1"], &["c0"]).restore(g, &ids());
    });
    assert_eq!(moves(&s.effects()), 0);
}

#[test]
fn ids_the_host_does_not_name_are_reported_unplaced_and_the_rest_move() {
    let mut s = table();
    let mut unplaced = Order::default();
    s.input(|g| unplaced = ordered(&[], &["gone", "c3"]).restore(g, &ids()));
    assert_eq!(
        unplaced,
        Order {
            rows: None,
            cols: Some(strings(&["gone"]))
        }
    );
    assert_eq!(order(&mut s, Axis::Cols), [3, 0, 1, 2]);
}

#[test]
fn an_order_on_an_axis_the_view_sorts_is_unplaced_and_moves_nothing() {
    let mut s = table();
    let mut view = ViewSpec::default();
    view.rows.sort = vec![SortColumn {
        id: "c1".to_owned(),
        direction: Direction::Descending,
    }];
    let layout = Layout {
        view: Some(view),
        ..ordered(&["r5", "r0"], &["c1"])
    };
    let mut unplaced = Order::default();
    s.input(|g| unplaced = layout.restore(g, &ids()));
    assert_eq!(unplaced.rows, Some(strings(&["r5", "r0"])));
    assert_eq!(unplaced.cols, None);
    let effects = s.effects();
    assert_eq!(moves(&effects), 1, "the columns move: {effects:?}");
    assert!(
        !s.grid.snapshot().applied_view().rows.sort.is_empty(),
        "the sort holds"
    );
}

#[test]
fn a_move_the_source_refuses_leaves_the_order_and_the_snapshot_says_why() {
    let (rows, cols) = config();
    let mut s = Scenario::identity(rows, cols);
    s.input(|g| g.set_movable(Axis::Cols, true));
    s.frame(V);
    let named = IdTable::new([], (0..COLS).map(|i| (format!("c{i}"), Key(i))));
    s.input(|g| {
        ordered(&[], &["c2"]).restore(g, &named);
    });
    assert_eq!(order(&mut s, Axis::Cols), [0, 1, 2, 3]);
    let refusal = s
        .grid
        .snapshot()
        .move_refusal()
        .map(|(axis, why)| (axis, why.to_owned()));
    assert_eq!(
        refusal,
        Some((Axis::Cols, "These can't be moved".to_owned()))
    );
    // Not made, the order is no arrangement, and a save keeps the restored one.
    let read = Layout::read(&mut s.grid, &named);
    assert_eq!(read.order, None);
    let saved = read.keeping(&ordered(&[], &["c2"]), &named);
    assert_eq!(saved.order, ordered(&[], &["c2"]).order);
}

/// Pins, sizes, bands, a View Spec and a column order, as a user might
/// leave them.
fn scramble(tc: &TestCase, s: &mut Scenario<RecordTable>) {
    let col = || gs::integers::<u64>().max_value(COLS - 1);
    let row = || gs::integers::<u64>().max_value(ROWS - 1);
    let edge = || gs::sampled_from(vec![Edge::Leading, Edge::Trailing]);
    for _ in 0..tc.draw(gs::integers::<usize>().max_value(2)) {
        let (c, e) = (tc.draw(col()), tc.draw(edge()));
        s.input(|g| g.pin(Axis::Cols, e, Key(c)));
    }
    for _ in 0..tc.draw(gs::integers::<usize>().max_value(2)) {
        let (r, e) = (tc.draw(row()), tc.draw(edge()));
        s.input(|g| g.pin(Axis::Rows, e, Key(r)));
    }
    for _ in 0..tc.draw(gs::integers::<usize>().max_value(3)) {
        let (c, size) = (
            tc.draw(col()),
            tc.draw(gs::integers::<u32>().min_value(1).max_value(40)),
        );
        s.input(|g| g.set_size(Axis::Cols, Key(c), size as f32 / 2.0));
    }
    if tc.draw(gs::booleans()) {
        let band = tc.draw(gs::integers::<u32>().min_value(1).max_value(9));
        s.input(|g| g.set_header(Axis::Cols, band as f32));
    }
    let moved = tc.draw(gs::vecs(col()).max_size(3));
    let moved: Vec<String> = moved.iter().map(|c| format!("c{c}")).collect();
    let mut view = ViewSpec::default();
    if tc.draw(gs::booleans()) {
        view.rows.sort = vec![SortColumn {
            id: format!("c{}", tc.draw(col())),
            direction: Direction::Ascending,
        }];
    }
    view.rows.hidden = tc
        .draw(gs::vecs(row()).max_size(2))
        .iter()
        .map(|r| format!("r{r}"))
        .collect();
    let layout = Layout {
        view: Some(view),
        order: Some(Order {
            rows: None,
            cols: (!moved.is_empty()).then_some(moved),
        }),
        ..Layout::default()
    };
    s.input(|g| {
        layout.restore(g, &ids());
    });
    s.frame(V);
}

#[hegel::test(test_cases = 150)]
fn a_layout_read_from_one_grid_restores_onto_another_and_reads_back_the_same(tc: TestCase) {
    let (mut a, mut b) = (table(), table());
    scramble(&tc, &mut a);
    scramble(&tc, &mut b);
    let saved = Layout::read(&mut a.grid, &ids());
    let copy = Layout::<String, String>::from_json(&saved.to_json()).unwrap();
    b.input(|g| {
        copy.restore(g, &ids());
    });
    b.frame(V);
    // An order axis the save leaves unset leaves b's own.
    let mut read = Layout::read(&mut b.grid, &ids());
    let unset = |o: &Option<Order<String, String>>| {
        (o.as_ref()).map_or([true, true], |o| [o.rows.is_none(), o.cols.is_none()])
    };
    if let [rows, cols] = unset(&saved.order)
        && let Some(order) = read.order.as_mut()
    {
        rows.then(|| order.rows = None);
        cols.then(|| order.cols = None);
    }
    if read.order.as_ref().is_some_and(Order::is_unset) {
        read.order = None;
    }
    assert_eq!(read, saved);
}

#[test]
fn a_grid_nothing_has_arranged_leaves_the_order_unset_so_a_default_order_holds() {
    let mut s = table();
    let read = Layout::read(&mut s.grid, &ids());
    assert_eq!(read.order, None);
    let default = ordered(&[], &["c2"]);
    let layered = Layout::layered(default.clone(), read);
    assert_eq!(layered.order, default.order);
}

/// 4 rows and 12 columns of 6 after a 4-wide row header band, in a
/// viewport 30 wide: columns 0 to 3 show at first. No pin zones, so a drop
/// near either end moves.
fn wide() -> Scenario<RecordTable> {
    wide_of(12)
}

/// As [`wide`], with `cols` columns.
fn wide_of(cols: u64) -> Scenario<RecordTable> {
    let records = (0..4)
        .map(|r| (0..cols as i64).map(|c| Some(r + c)).collect())
        .collect();
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(4, 1.0).header(1.0),
            AxisConfig::new(cols, 6.0).header(4.0),
        ),
        RecordTable::new(records, cols as usize, 0..cols),
    );
    s.input(|g| {
        g.set_movable(Axis::Cols, true);
        g.set_pin_zone(Axis::Cols, 0.0);
    });
    s.frame(NARROW);
    s
}

const NARROW: Viewport = Viewport {
    width: 30.0,
    height: 10.0,
};

/// Every column ID: `c0` onwards.
fn wide_ids() -> IdTable<String, String> {
    IdTable::new(
        (0..4).map(|i| (format!("r{i}"), Key(i))),
        (0..400).map(|i| (format!("c{i}"), Key(i))),
    )
}

/// Drags the column header at `from` to `to`, x in the viewport.
fn drag(s: &mut Scenario<RecordTable>, from: f32, to: f32) {
    s.input(|g| g.pointer(PointerEvent::down(from, 0.5)));
    s.input(|g| g.pointer(PointerEvent::moved(to, 0.5)));
    s.input(|g| g.pointer(PointerEvent::up(to, 0.5)));
    s.frame(NARROW);
}

#[test]
fn a_column_moved_on_a_table_wider_than_the_viewport_is_saved_wherever_it_scrolls() {
    let mut s = wide();
    // Column 2 (x 16..22) dropped on column 1's first half (x 10..13).
    drag(&mut s, 19.0, 11.0);
    assert_eq!(order(&mut s, Axis::Cols)[..3], [0, 2, 1]);
    // Widen column 1 and scroll to the end: the first columns are off screen.
    s.input(|g| {
        g.set_size(Axis::Cols, Key(1), 60.0);
        g.scroll_to_item(Axis::Cols, 11);
    });
    let drawn = s.frame(NARROW).drawn(Axis::Cols).count();
    assert!(drawn < 12, "{drawn} columns drawn");
    let saved = Layout::read(&mut s.grid, &wide_ids());
    let cols = saved.order.as_ref().and_then(|o| o.cols.clone());
    assert_eq!(cols, Some(strings(&["c0", "c2"])));
    // Restored onto a fresh grid, the columns come back in that order.
    let mut fresh = wide();
    fresh.input(|g| {
        saved.restore(g, &wide_ids());
    });
    assert_eq!(order(&mut fresh, Axis::Cols)[..3], [0, 2, 1]);
    assert_eq!(
        Layout::read(&mut fresh.grid, &wide_ids()).order,
        saved.order
    );
}

fn read_cols(s: &mut Scenario<RecordTable>) -> Option<Vec<String>> {
    let layout = Layout::read(&mut s.grid, &wide_ids());
    layout.order.and_then(|o| o.cols)
}

#[test]
fn a_drag_after_a_restored_order_places_the_items_before_its_drop_with_it() {
    let mut s = wide();
    s.input(|g| {
        ordered(&[], &["c3", "c1"]).restore(g, &wide_ids());
    });
    s.frame(NARROW);
    assert_eq!(order(&mut s, Axis::Cols)[..4], [3, 1, 0, 2]);
    // Column 2 (Position 3, x 22..28) dropped before Position 1 (c1).
    drag(&mut s, 25.0, 11.0);
    assert_eq!(order(&mut s, Axis::Cols)[..4], [3, 2, 1, 0]);
    assert_eq!(read_cols(&mut s), Some(strings(&["c3", "c2", "c1"])));
    // c3 (Position 0, x 4..10) dropped at the end of the view, after c0.
    drag(&mut s, 7.0, 29.5);
    assert_eq!(order(&mut s, Axis::Cols)[..4], [2, 1, 0, 3]);
    assert_eq!(read_cols(&mut s), Some(strings(&["c2", "c1", "c0", "c3"])));
}

#[test]
fn a_drag_where_the_grid_knows_not_what_lies_before_the_drop_takes_its_items_out() {
    let mut s = wide_of(400);
    // c1, c0, then c2 to c300: an arrangement reaching Position 300.
    let listed: Vec<String> = [1, 0]
        .into_iter()
        .chain(2..=300)
        .map(|i| format!("c{i}"))
        .collect();
    let layout = Layout {
        order: Some(Order {
            rows: None,
            cols: Some(listed.clone()),
        }),
        ..Layout::default()
    };
    s.input(|g| {
        layout.restore(g, &wide_ids());
    });
    s.frame(NARROW);
    // Far along, the Window holds none of the first columns.
    s.input(|g| g.scroll_to_item(Axis::Cols, 300));
    s.frame(NARROW);
    assert_eq!(order(&mut s, Axis::Cols)[0], 300);
    // c300 (x 4..10) dropped after c301 (x 10..16), past the arrangement.
    drag(&mut s, 7.0, 15.0);
    assert_eq!(order(&mut s, Axis::Cols)[..2], [301, 300]);
    assert_eq!(read_cols(&mut s), Some(listed[..300].to_vec()));
}

#[test]
fn a_sorted_axis_saves_no_order_and_a_save_keeps_the_restored_one() {
    let mut s = table();
    let layout = ordered(&["r2"], &[]);
    s.input(|g| {
        layout.restore(g, &ids());
    });
    s.frame(V);
    s.input(|g| {
        g.request_view(binnacle_core::ViewSpec::default().with_sort(
            Axis::Rows,
            binnacle_core::SortSpec::new(vec![binnacle_core::SortColumn::ascending(Key(1))]),
        ))
    });
    s.frame(V);
    let read = Layout::read(&mut s.grid, &ids());
    assert_eq!(read.order, None);
    assert_eq!(read.keeping(&layout, &ids()).order, layout.order);
}

#[test]
fn what_the_host_does_not_name_is_skipped_on_restore_and_kept_on_the_next_save() {
    let mut s = table();
    let mut view = ViewSpec::<String, String>::default();
    view.rows.sort = vec![
        SortColumn {
            id: "gone".into(),
            direction: Direction::Ascending,
        },
        SortColumn {
            id: "c1".into(),
            direction: Direction::Descending,
        },
    ];
    view.rows.hidden = strings(&["r2", "lost row"]);
    let layout = Layout {
        view: Some(view.clone()),
        order: Some(Order {
            rows: None,
            cols: Some(strings(&["gone", "c3"])),
        }),
        pins: Some(Pins {
            rows: EdgePins::default(),
            cols: EdgePins {
                leading: strings(&["gone", "c1"]),
                trailing: vec![],
            },
        }),
        sizes: Some(Sizes {
            rows: vec![],
            cols: vec![("gone".into(), 9.0), ("c2".into(), 7.0)],
        }),
        bands: Some(Bands {
            rows: 2.0,
            cols: 5.0,
        }),
        ..Layout::default()
    };
    s.input(|g| {
        layout.restore(g, &ids());
    });
    s.frame(V);
    // The Grid holds the known parts only.
    let s_ = s.grid.snapshot();
    assert_eq!(
        s_.pins(Axis::Cols, Edge::Leading).collect::<Vec<_>>(),
        [Key(1)]
    );
    assert_eq!(s_.applied_view().rows.sort.columns().len(), 1);
    let read = Layout::read(&mut s.grid, &ids());
    assert_eq!(read.pins.as_ref().unwrap().cols.leading, strings(&["c1"]));
    // Saved over the restored layout, the unknown IDs come back.
    let saved = read.keeping(&layout, &ids());
    assert_eq!(saved.view.as_ref().unwrap().rows.sort, view.rows.sort);
    assert_eq!(saved.view.as_ref().unwrap().rows.hidden, view.rows.hidden);
    assert_eq!(
        saved.pins.as_ref().unwrap().cols.leading,
        strings(&["gone", "c1"])
    );
    assert_eq!(
        saved.sizes.as_ref().unwrap().cols,
        [("c2".to_owned(), 7.0), ("gone".to_owned(), 9.0)]
    );
    // The columns' order is the layout's, unknown ID and all.
    assert_eq!(
        saved.order.as_ref().unwrap().cols,
        Some(strings(&["gone", "c3"]))
    );
    // A list the user changed since stays as they left it.
    s.input(|g| g.request_view(binnacle_core::ViewSpec::default()));
    s.frame(V);
    let changed = Layout::read(&mut s.grid, &ids()).keeping(&layout, &ids());
    assert!(changed.view.unwrap().rows.sort.is_empty());
}
