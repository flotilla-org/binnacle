//! Seam 1 properties for column moves: a real Grid over an In-Memory Table
//! Source of records, driven through the frame loop and the shared pump with
//! header drags, column moves and row reorders made behind the Grid's back,
//! Size Overrides and scrolls.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, Grid, Key, Mode, PointerEvent, Viewport,
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
                let notice = self.s.source.source.move_columns(&keys, to);
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
