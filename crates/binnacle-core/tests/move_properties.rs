//! Seam 1 properties for moves: a real Grid over an In-Memory Table Source
//! of records, driven through the frame loop and the shared pump with header
//! drags, column moves and row reorders made behind the Grid's back, Size
//! Overrides and scrolls; and a Grid whose host answers moves late or
//! refuses them while declaring either axis movable or not.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Effect, Grid, Key, Mode, PointerEvent, Viewport,
};
use binnacle_testkit::{Record, RecordTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const ROW_HEADER: f32 = 3.0;

#[derive(Clone, Debug)]
struct Table {
    rows: usize,
    cols: usize,
    col_size: f32,
    gap: f32,
    viewport: Viewport,
}

#[hegel::composite]
fn table(tc: &TestCase) -> Table {
    let cols = tc.draw(hegel::one_of!(
        gs::integers::<usize>().min_value(1).max_value(12),
        gs::integers::<usize>().max_value(12),
    ));
    // Mostly a viewport with a body past both header bands; sometimes any
    // size down to nothing.
    let length = |body: f32, max: f32| {
        hegel::one_of!(
            gs::floats::<f32>().min_value(body).max_value(max),
            gs::floats::<f32>().min_value(body).max_value(max),
            gs::floats::<f32>().min_value(0.0).max_value(max),
        )
    };
    Table {
        rows: tc.draw(gs::integers::<usize>().max_value(6)),
        cols,
        col_size: tc.draw(gs::floats::<f32>().min_value(1.0).max_value(8.0)),
        gap: tc.draw(gs::sampled_from(vec![0.0f32, 1.0])),
        viewport: Viewport::new(
            tc.draw(length(ROW_HEADER + 1.0, 60.0)),
            tc.draw(length(2.0, 8.0)),
        ),
    }
}

#[derive(Clone, Debug)]
enum Input {
    /// Press the header of the `n`th shown column, move to `x` (a fraction
    /// of the viewport width) and release if `release`.
    Drag {
        n: usize,
        x: f32,
        release: bool,
    },
    Release,
    Cancel,
    /// The source moves columns and tells the Grid.
    Move(Vec<Key>, u64),
    /// A bare `Reordered` on `axis`, as after a sort the Grid did not ask for.
    Reordered(Axis),
    SetSize(Key, f32),
    ScrollCols(f32),
}

#[hegel::composite]
fn input(tc: &TestCase, cols: usize) -> Input {
    let key = || gs::integers::<u64>().max_value(cols as u64).map(Key);
    let fraction = || gs::floats::<f32>().min_value(-0.1).max_value(1.1);
    match tc.draw(gs::integers::<u8>().max_value(7)) {
        0 | 1 => Input::Drag {
            n: tc.draw(gs::integers::<usize>().max_value(4)),
            x: tc.draw(fraction()),
            release: tc.draw(gs::weighted_booleans(0.3)),
        },
        2 => Input::Release,
        3 => Input::Cancel,
        4 => Input::Move(
            tc.draw(gs::vecs(key()).max_size(3)),
            tc.draw(gs::integers::<u64>().max_value(cols as u64 + 1)),
        ),
        5 => Input::Reordered(tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))),
        6 => Input::SetSize(
            tc.draw(key()),
            tc.draw(gs::floats::<f32>().min_value(1.0).max_value(20.0)),
        ),
        _ => Input::ScrollCols(tc.draw(gs::floats::<f32>().min_value(-30.0).max_value(30.0))),
    }
}

struct Host {
    table: Table,
    s: Scenario<RecordTable>,
    /// The column Size Overrides set, by Key.
    sizes: Vec<Option<f32>>,
}

impl Host {
    fn new(table: Table) -> Host {
        let rows: Vec<Record> = (0..table.rows)
            .map(|r| vec![Some(r as i64); table.cols])
            .collect();
        let grid = Grid::new(
            AxisConfig::new(table.rows as u64, 1.0).header(1.0),
            AxisConfig::new(table.cols as u64, table.col_size)
                .header(ROW_HEADER)
                .gap(table.gap),
        );
        let sizes = vec![None; table.cols];
        let s = Scenario::new(grid, RecordTable::new(rows, table.cols, []));
        Host { table, s, sizes }
    }

    fn mode(&mut self) -> Mode {
        self.s.frame(self.table.viewport).mode()
    }

    fn input(&mut self, input: Input) {
        let Viewport { width, .. } = self.table.viewport;
        match input {
            Input::Drag { n, x, release } => {
                let snap = self.s.frame(self.table.viewport);
                let Some(col) = snap.cols().get(n) else {
                    return;
                };
                let from = (col.offset.max(ROW_HEADER) + col.end().min(width)) / 2.0;
                let to = x * width;
                self.s.input(|g| {
                    g.pointer(PointerEvent::down(from, 0.5));
                    g.pointer(PointerEvent::moved(to, 0.5));
                    if release {
                        g.pointer(PointerEvent::up(to, 0.5));
                    }
                });
            }
            Input::Release => self.s.input(|g| g.pointer(PointerEvent::up(0.0, 0.0))),
            Input::Cancel => self.s.input(|g| g.action(Action::Cancel)),
            Input::Move(keys, to) => {
                let notice = self.s.source.source.move_items(Axis::Cols, &keys, to);
                let notice = notice.expect("the columns are never sorted here");
                self.s.input(|g| g.notify(Axis::Cols, notice));
            }
            Input::Reordered(axis) => {
                self.s.input(|g| g.notify(axis, ChangeNotice::Reordered));
            }
            Input::SetSize(key, size) => {
                self.s.input(|g| g.set_size(Axis::Cols, key, size));
                if let Some(s) = self.sizes.get_mut(key.0 as usize) {
                    *s = Some(size);
                }
            }
            Input::ScrollCols(dx) => self.s.input(|g| g.action(Action::Scroll { dx, dy: 0.0 })),
        }
        // Answer what the input asked for, including moves.
        self.s.pump();
    }
}

/// Draws a table and steps of inputs, each followed by a frame; `check` sees
/// the Host and the mode before and after the step's last input.
fn runs(tc: &TestCase, mut check: impl FnMut(&mut Host, &Input, Mode, Mode)) {
    let table = tc.draw(table());
    let cols = table.cols;
    let mut host = Host::new(table);
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(8)) {
        let mut inputs = tc.draw(gs::vecs(input(cols)).min_size(1).max_size(4));
        let last = inputs.pop().expect("at least one input");
        for input in inputs {
            host.input(input);
        }
        let before = host.mode();
        host.input(last.clone());
        let after = host.mode();
        check(&mut host, &last, before, after);
    }
}

/// Column Size Overrides follow their Keys wherever moves put them, and the
/// Strip shows the Keys the source has at each Position.
#[hegel::test(test_cases = 2000)]
fn column_widths_follow_their_keys_through_moves(tc: TestCase) {
    runs(&tc, |host, _, _, _| {
        let snap = host.s.frame(host.table.viewport);
        let strip: Vec<_> = snap.cols().to_vec();
        if !strip.is_empty() {
            tc.event("columns shown");
        }
        for item in strip {
            let key = item.key.expect("a pumped frame fills every Key");
            let window = host
                .s
                .source
                .source
                .window(Axis::Cols, item.position..item.position + 1);
            assert_eq!(window, vec![key], "{item:?}");
            let size = host.sizes[key.0 as usize].unwrap_or(host.table.col_size);
            assert_eq!(item.size, size, "{item:?}");
        }
    });
}

/// A Change Notice reordering the columns cancels a header move; one
/// reordering the rows leaves it be.
#[hegel::test(test_cases = 2000)]
fn a_header_move_is_cancelled_only_by_reordering_its_own_axis(tc: TestCase) {
    runs(&tc, |_, last, before, after| {
        let Mode::MovingHeaders { .. } = before else {
            return;
        };
        match last {
            Input::Reordered(Axis::Cols) | Input::Move(..) => {
                tc.event("columns reordered while moving");
                assert_eq!(after, Mode::Idle);
            }
            Input::Reordered(Axis::Rows) | Input::SetSize(..) | Input::ScrollCols(_) => {
                tc.event("moving through other input");
                assert_eq!(after, before);
            }
            _ => {}
        }
    });
}

#[derive(Clone, Debug)]
enum Slow {
    /// Press the header of item `from` on `axis`, move over item `to` (which
    /// may be past the end) and release if `release`.
    Drag {
        axis: Axis,
        from: u64,
        to: u64,
        release: bool,
    },
    Release,
    Cancel,
    SetMovable(Axis, bool),
    /// The host answers the move waiting, if any: moved, or refused.
    Answer {
        refuse: bool,
    },
    /// A click on a cell.
    Click,
}

#[hegel::composite]
fn slow(tc: &TestCase) -> Slow {
    let axis = || gs::sampled_from(vec![Axis::Rows, Axis::Cols]);
    match tc.draw(gs::integers::<u8>().max_value(6)) {
        0 | 1 => Slow::Drag {
            axis: tc.draw(axis()),
            from: tc.draw(gs::integers::<u64>().max_value(SLOW_COUNT - 1)),
            to: tc.draw(gs::integers::<u64>().max_value(SLOW_COUNT + 1)),
            release: tc.draw(gs::weighted_booleans(0.7)),
        },
        2 => Slow::Release,
        3 => Slow::Cancel,
        4 => Slow::SetMovable(tc.draw(axis()), tc.draw(gs::booleans())),
        5 => Slow::Answer {
            refuse: tc.draw(gs::booleans()),
        },
        _ => Slow::Click,
    }
}

/// Items per axis in [`slow_frame`]'s table.
const SLOW_COUNT: u64 = 6;

/// A frame of a 6 by 6 table whose Keys are Positions: rows 1 high under a
/// 1-high band, columns 3 wide after a 2-wide band, so row `p` spans y
/// `1 + p .. 2 + p` and column `p` x `2 + 3p .. 5 + 3p`.
fn slow_frame(grid: &mut Grid) {
    for (axis, range) in grid.begin_frame(Viewport::new(30.0, 10.0)) {
        let keys: Vec<Key> = range.clone().map(Key).collect();
        grid.set_window(axis, range.start, &keys);
    }
}

/// The middle of item `p` on `axis` along it, and the middle of its header
/// across.
fn header_point(axis: Axis, p: u64) -> (f32, f32) {
    match axis {
        Axis::Rows => (1.0, 1.5 + p as f32),
        Axis::Cols => (3.5 + 3.0 * p as f32, 0.5),
    }
}

/// Moves are requested one at a time and only on an axis movable as the
/// drop is made; the moved headers show pending exactly until the host
/// answers; a refusal shows until the next press; and nothing moves early.
#[hegel::test(test_cases = 1000)]
fn moves_are_requested_one_at_a_time_and_only_on_a_movable_axis(tc: TestCase) {
    let mut grid = Grid::new(
        AxisConfig::new(SLOW_COUNT, 1.0).header(1.0),
        AxisConfig::new(SLOW_COUNT, 3.0).header(2.0),
    );
    let mut movable = [false; 2];
    let mut pending: Option<(Axis, Vec<Key>)> = None;
    let mut refused: Option<Axis> = None;
    slow_frame(&mut grid);
    for input in tc.draw(gs::vecs(slow()).min_size(1).max_size(12)) {
        match input {
            Slow::Drag {
                axis,
                from,
                to,
                release,
            } => {
                let (x, y) = header_point(axis, from);
                let (tx, ty) = match axis {
                    Axis::Rows => (x, header_point(axis, to).1),
                    Axis::Cols => (header_point(axis, to).0, y),
                };
                grid.pointer(PointerEvent::down(x, y));
                grid.pointer(PointerEvent::moved(tx, ty));
                refused = None;
                let snap = {
                    slow_frame(&mut grid);
                    grid.snapshot()
                };
                let moving =
                    matches!(snap.mode(), Mode::MovingHeaders { axis: a, .. } if a == axis);
                // A drag onto the item pressed never passes the threshold.
                let dragged = from != to;
                // A header drag starts on any axis, since it may pin; over
                // an item's middle it moves only on a movable axis.
                assert_eq!(moving, dragged && pending.is_none());
                assert_eq!(
                    snap.drop_indicator().is_some(),
                    moving && movable[axis.index()]
                );
                if release {
                    grid.pointer(PointerEvent::up(tx, ty));
                }
            }
            Slow::Release => grid.pointer(PointerEvent::up(0.0, 0.0)),
            Slow::Cancel => grid.action(Action::Cancel),
            Slow::SetMovable(axis, m) => {
                grid.set_movable(axis, m);
                movable[axis.index()] = m;
            }
            Slow::Answer { refuse } => {
                grid.move_answered(refuse.then(|| "no".to_string()));
                if let Some((axis, _)) = pending.take()
                    && refuse
                {
                    refused = Some(axis);
                }
            }
            Slow::Click => {
                grid.pointer(PointerEvent::down(10.0, 4.5));
                grid.pointer(PointerEvent::up(10.0, 4.5));
                refused = None;
            }
        }
        while let Some(stamped) = grid.poll_effect() {
            if let Effect::MoveRequested { axis, keys, .. } = stamped.effect {
                tc.event("a move requested");
                assert!(pending.is_none(), "a second move while {pending:?} waits");
                assert!(movable[axis.index()], "a move on {axis:?}, not movable");
                pending = Some((axis, keys));
            }
        }
        slow_frame(&mut grid);
        let snap = grid.snapshot();
        assert_eq!(
            snap.pending_move().map(|(a, k)| (a, k.to_vec())),
            pending.clone()
        );
        assert_eq!(snap.move_refusal().map(|(a, _)| a), refused);
        for axis in Axis::ALL {
            assert_eq!(snap.movable(axis), movable[axis.index()]);
            for h in snap.headers(axis) {
                let waits = (pending.as_ref())
                    .is_some_and(|(a, keys)| *a == axis && keys.contains(&h.key.unwrap()));
                assert_eq!(h.move_pending, waits, "{h:?}");
                // Nothing moves early, nor late: the host never moves.
                assert_eq!(h.key, Some(Key(h.position)));
            }
        }
    }
}
