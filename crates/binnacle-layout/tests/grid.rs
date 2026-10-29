//! A Layout restored onto a Grid over an In-Memory Table Source and read
//! back, through the scenario harness: the order goes through the source's
//! move path, and what the host's IDs don't name is skipped, reported, and
//! kept on the next save.

use binnacle_core::{Axis, AxisConfig, Edge, Effect, Grid, Key, Viewport};
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

fn ordered(rows: &[&str], cols: &[&str]) -> Layout<String, String> {
    Layout {
        order: Some(Order {
            rows: strings(rows),
            cols: strings(cols),
        }),
        ..Layout::default()
    }
}

#[test]
fn an_order_puts_the_listed_items_first_in_that_order_on_both_axes_one_move_after_another() {
    let mut s = table();
    let mut unplaced = Order::default();
    s.input(|g| unplaced = ordered(&["r4", "r1"], &["c2", "c0"]).restore(g, &ids()));
    assert!(unplaced.is_empty());
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
            rows: vec![],
            cols: strings(&["gone"])
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
    assert_eq!(unplaced.rows, strings(&["r5", "r0"]));
    assert!(unplaced.cols.is_empty());
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
            rows: vec![],
            cols: moved,
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
    assert_eq!(Layout::read(&mut b.grid, &ids()), saved);
}

#[test]
fn an_order_is_read_while_every_item_is_drawn_and_no_sort_orders_them() {
    let mut s = table();
    let read = |s: &mut Scenario<RecordTable>| Layout::read(&mut s.grid, &ids()).order.unwrap();
    assert_eq!(read(&mut s).cols, strings(&["c0", "c1", "c2", "c3"]));
    assert_eq!(read(&mut s).rows.len(), ROWS as usize);
    s.input(|g| {
        g.request_view(binnacle_core::ViewSpec::default().with_sort(
            Axis::Rows,
            binnacle_core::SortSpec::new(vec![binnacle_core::SortColumn::ascending(Key(1))]),
        ))
    });
    s.frame(V);
    assert!(
        read(&mut s).rows.is_empty(),
        "sorted rows have no order of their own"
    );
    // A viewport too short for every row reads no row order.
    s.frame(Viewport {
        width: 100.0,
        height: 4.0,
    });
    s.input(|g| g.request_view(binnacle_core::ViewSpec::default()));
    s.frame(Viewport {
        width: 100.0,
        height: 4.0,
    });
    assert!(read(&mut s).rows.is_empty());
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
            rows: vec![],
            cols: strings(&["gone", "c3"]),
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
    // The order is sorted away on the rows; the columns' starts with the layout's.
    assert_eq!(
        saved.order.as_ref().unwrap().cols[..2],
        strings(&["gone", "c3"])
    );
    // A list the user changed since stays as they left it.
    s.input(|g| g.request_view(binnacle_core::ViewSpec::default()));
    s.frame(V);
    let changed = Layout::read(&mut s.grid, &ids()).keeping(&layout, &ids());
    assert!(changed.view.unwrap().rows.sort.is_empty());
}
