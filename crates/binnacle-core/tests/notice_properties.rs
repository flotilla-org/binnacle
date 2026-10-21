//! Seam 1 properties for Change Notices: a real Grid over a keyed Table
//! Source whose items are inserted, removed, reversed and edited between and
//! during user input (clicks, selection drags, header moves, border resizes,
//! focus moves, scrolls), driven through the frame loop and the shared pump.

use binnacle_core::{
    Action, Axis, AxisConfig, ChangeNotice, FocusEnd, FocusStep, Grid, Key, Mode, Modifiers,
    PointerEvent, ScrollAnchor, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{IdentityTable, KeyedTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

const ROW_HEADER: f32 = 3.0;

#[derive(Clone, Debug)]
enum Input {
    Action(Action),
    /// A press at fractions of the viewport, with modifiers.
    Press(f32, f32, Modifiers),
    /// A press on the column header band at a fraction of the width.
    HeaderPress(f32),
    /// A press on the border after the `i`th visible column header.
    BorderPress(usize),
    /// A move to fractions of the viewport.
    Move(f32, f32),
    Release,
    /// The host starts editing the cell at fractions of the counts.
    Edit(f32, f32),
    /// The source inserts `n` items before `at` (a fraction of the count).
    Insert(Axis, f32, u64),
    /// The source removes `n` items from `at` (a fraction of the count).
    Remove(Axis, f32, u64),
    /// The source reverses an axis.
    Reverse(Axis),
    /// The content of a run of items changed.
    Content(Axis, f32, u64),
}

#[hegel::composite]
fn axis(tc: &TestCase) -> Axis {
    tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))
}

#[hegel::composite]
fn fraction(tc: &TestCase) -> f32 {
    tc.draw(gs::floats::<f32>().min_value(0.0).max_value(0.999))
}

#[hegel::composite]
fn input(tc: &TestCase) -> Input {
    let modifiers = gs::sampled_from(vec![Modifiers::NONE, Modifiers::SHIFT, Modifiers::CTRL]);
    let n = || gs::integers::<u64>().min_value(1).max_value(4);
    let by = gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]);
    let move_focus = hegel::tuples!(axis(), by, gs::booleans(), gs::weighted_booleans(0.3)).map(
        |(axis, by, forward, extend)| {
            Input::Action(Action::MoveFocus {
                axis,
                by,
                forward,
                extend,
            })
        },
    );
    tc.draw(hegel::one_of!(
        move_focus,
        hegel::tuples!(fraction(), fraction(), modifiers).map(|(x, y, m)| Input::Press(x, y, m)),
        fraction().map(Input::HeaderPress),
        gs::integers::<usize>().max_value(4).map(Input::BorderPress),
        hegel::tuples!(fraction(), fraction()).map(|(x, y)| Input::Move(x, y)),
        gs::just(Input::Release),
        hegel::tuples!(fraction(), fraction()).map(|(r, c)| Input::Edit(r, c)),
        gs::sampled_from(vec![
            Input::Action(Action::SelectAll),
            Input::Action(Action::Cancel),
        ]),
        hegel::tuples!(axis(), gs::integers::<i64>().min_value(-5).max_value(5))
            .map(|(axis, items)| Input::Action(Action::ScrollBy { axis, items })),
        hegel::tuples!(axis(), fraction(), n()).map(|(a, at, n)| Input::Insert(a, at, n)),
        hegel::tuples!(axis(), fraction(), n()).map(|(a, at, n)| Input::Remove(a, at, n)),
        hegel::tuples!(axis(), fraction(), n()).map(|(a, at, n)| Input::Remove(a, at, n)),
        axis().map(Input::Reverse),
        hegel::tuples!(axis(), fraction(), n()).map(|(a, at, n)| Input::Content(a, at, n)),
    ))
}

/// What a check needs from a Snapshot, owned so the source can be queried
/// beside it.
#[derive(Clone, Debug)]
struct View {
    focus: Option<[FocusEnd; 2]>,
    locating: [bool; 2],
    anchors: [ScrollAnchor; 2],
    counts: [u64; 2],
    /// Each visible column's end, for aiming at borders.
    col_ends: Vec<f32>,
}

struct Host {
    s: Scenario<KeyedTable>,
    viewport: Viewport,
    view: View,
}

impl Host {
    fn new(tc: &TestCase) -> Host {
        let rows = tc.draw(gs::integers::<u64>().max_value(30));
        let cols = tc.draw(gs::integers::<u64>().max_value(6));
        let gap = tc.draw(gs::sampled_from(vec![0.0f32, 1.0]));
        let row_size = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(3.0));
        let col_size = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(6.0));
        let mut grid = Grid::new(
            AxisConfig::new(rows, row_size).header(1.0),
            AxisConfig::new(cols, col_size).header(ROW_HEADER).gap(gap),
        );
        grid.set_border_grab(tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)));
        let width = tc.draw(
            gs::floats::<f32>()
                .min_value(ROW_HEADER + 1.0)
                .max_value(30.0),
        );
        let height = tc.draw(gs::floats::<f32>().min_value(2.0).max_value(12.0));
        let mut host = Host {
            s: Scenario::new(grid, KeyedTable::new(rows, cols)),
            viewport: Viewport::new(width, height),
            view: View {
                focus: None,
                locating: [false; 2],
                anchors: [ScrollAnchor::default(); 2],
                counts: [0; 2],
                col_ends: Vec::new(),
            },
        };
        host.frame();
        host
    }

    fn frame(&mut self) {
        let snap = self.s.frame(self.viewport);
        let focus = snap.focus(Axis::Rows).zip(snap.focus(Axis::Cols));
        self.view = View {
            focus: focus.map(|(r, c)| [r, c]),
            locating: Axis::ALL.map(|a| snap.locating(a)),
            anchors: Axis::ALL.map(|a| snap.scroll_anchor(a)),
            counts: Axis::ALL.map(|a| snap.count(a)),
            col_ends: snap.cols().iter().map(|c| c.end()).collect(),
        };
    }

    fn mode(&mut self) -> Mode {
        self.s.grid.snapshot().mode()
    }

    fn count(&self, axis: Axis) -> u64 {
        self.s.source.axes[axis.index()].len() as u64
    }

    /// Pushes `input`; returns the Change Notice it sent, if any.
    fn input(&mut self, input: Input) -> Option<(Axis, ChangeNotice)> {
        let Viewport { width, height } = self.viewport;
        let notice = |h: &mut Host, axis: Axis, notice: ChangeNotice| {
            h.s.input(|g| g.notify(axis, notice.clone()));
            Some((axis, notice))
        };
        let at = |h: &Host, axis: Axis, f: f32| (f * (h.count(axis) + 1) as f32) as u64;
        match input {
            Input::Action(a) => self.s.input(|g| g.action(a)),
            Input::Press(x, y, m) => self
                .s
                .input(|g| g.pointer(PointerEvent::down(x * width, y * height).modifiers(m))),
            Input::HeaderPress(x) => self.s.input(|g| {
                let x = ROW_HEADER + x * (width - ROW_HEADER);
                g.pointer(PointerEvent::down(x, 0.5))
            }),
            Input::BorderPress(i) => {
                if let Some(&end) = self.view.col_ends.get(i) {
                    self.s
                        .input(|g| g.pointer(PointerEvent::down(end + 0.25, 0.5)));
                }
            }
            Input::Move(x, y) => self
                .s
                .input(|g| g.pointer(PointerEvent::moved(x * width, y * height))),
            Input::Release => self.s.input(|g| g.pointer(PointerEvent::up(0.0, 0.0))),
            Input::Edit(r, c) => {
                let [rows, cols] = &self.s.source.axes;
                let pick =
                    |keys: &[Key], f: f32| keys.get((f * keys.len() as f32) as usize).copied();
                if let (Some(row), Some(col)) = (pick(rows, r), pick(cols, c)) {
                    self.s.input(|g| g.edit_started(row, col));
                }
            }
            Input::Insert(axis, f, n) => {
                let at = at(self, axis, f);
                let n = self.s.source.insert(axis, at, n);
                return notice(self, axis, n);
            }
            Input::Remove(axis, f, n) => {
                let at = at(self, axis, f);
                let n = self.s.source.remove(axis, at, n);
                return notice(self, axis, n);
            }
            Input::Reverse(axis) => {
                let n = self.s.source.reverse(axis);
                return notice(self, axis, n);
            }
            Input::Content(axis, f, n) => {
                let at = at(self, axis, f);
                return notice(self, axis, ChangeNotice::ContentChanged(at..at + n));
            }
        }
        None
    }
}

/// The Mode a Change Notice on `axis` leaves, given the Mode before it: a
/// selection drag is cancelled if items moved (not if only content changed,
/// or nothing was inserted or removed), a resize carries on, and a header
/// move is cancelled if items on its own axis moved. (An edit is checked
/// apart: it ends only if its Keys left.)
fn expected_mode(before: Mode, axis: Axis, notice: &ChangeNotice) -> Mode {
    let moves = match notice {
        ChangeNotice::Inserted { n, .. } | ChangeNotice::Removed { n, .. } => *n > 0,
        ChangeNotice::Reordered | ChangeNotice::Reset(_) | ChangeNotice::Refiltered(_) => true,
        ChangeNotice::ContentChanged(_) => false,
    };
    match before {
        Mode::DraggingSelection if moves => Mode::Idle,
        Mode::MovingHeaders { axis: a, .. } if moves && a == axis => Mode::Idle,
        mode => mode,
    }
}

/// Draws a table, then steps of inputs mixed with Change Notices, each
/// followed by a frame; `check` sees the Host after every frame.
fn runs(tc: &TestCase, check: impl FnMut(&mut Host)) {
    run_checking_notices(tc, |_, _, _, _| {}, check)
}

/// Like [`runs`], also handing `on_notice` the Mode before and after each
/// Change Notice (read from a Snapshot taken either side of it).
fn run_checking_notices(
    tc: &TestCase,
    mut on_notice: impl FnMut(Mode, Mode, (Axis, ChangeNotice), &Host),
    mut check: impl FnMut(&mut Host),
) {
    let mut host = Host::new(tc);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..steps {
        let inputs = tc.draw(gs::vecs(input()).min_size(1).max_size(6));
        for input in inputs {
            if matches!(
                input,
                Input::Insert(..) | Input::Remove(..) | Input::Reverse(_) | Input::Content(..)
            ) {
                let before = host.mode();
                let sent = host.input(input).expect("a notice");
                let after = host.mode();
                on_notice(before, after, sent, &host);
            } else {
                host.input(input);
            }
        }
        host.frame();
        check(&mut host);
    }
}

#[hegel::test(test_cases = 1000)]
fn the_mode_after_a_change_notice_follows_the_rules(tc: TestCase) {
    run_checking_notices(
        &tc,
        |before, after, (axis, notice), host| {
            if let Mode::Editing { row, col } = before {
                let [rows, cols] = &host.s.source.axes;
                let left = !rows.contains(&row) || !cols.contains(&col);
                assert!(
                    after == before || (after == Mode::Idle && left),
                    "{notice:?}: {before:?} to {after:?}"
                );
            } else {
                assert_eq!(after, expected_mode(before, axis, &notice), "{notice:?}");
            }
        },
        // Once the pump has answered, an edit names only Keys still there.
        |host| {
            if let Mode::Editing { row, col } = host.mode() {
                let [rows, cols] = &host.s.source.axes;
                assert!(rows.contains(&row) && cols.contains(&col));
            }
        },
    );
}

#[hegel::test(test_cases = 1000)]
fn focus_and_the_anchor_stay_in_bounds(tc: TestCase) {
    runs(&tc, |host| {
        let v = &host.view;
        for axis in Axis::ALL {
            let i = axis.index();
            assert_eq!(v.counts[i], host.count(axis));
            if let Some(f) = v.focus {
                assert!(f[i].position < v.counts[i], "{axis:?} {f:?}");
            }
            assert!(
                v.anchors[i].position < v.counts[i].max(1),
                "{axis:?} {:?}",
                v.anchors[i]
            );
        }
    });
}

#[hegel::test(test_cases = 1000)]
fn the_focused_position_holds_the_focused_key(tc: TestCase) {
    runs(&tc, |host| {
        let Some(focus) = host.view.focus else { return };
        assert_eq!(host.view.locating, [false; 2]);
        for axis in Axis::ALL {
            let end = focus[axis.index()];
            if let Some(key) = end.key {
                let keys = &host.s.source.axes[axis.index()];
                assert_eq!(keys[end.position as usize], key, "{axis:?} {end:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 1000)]
fn picks_hold_only_keys_the_source_still_has_when_their_rows_were_seen_leaving(tc: TestCase) {
    // Inserts and removals only: a removed row in the Window leaves the
    // picks, one outside it cannot be seen leaving. With a Window covering
    // every row, picks always name live rows.
    let mut host = Host::new(&tc);
    host.viewport = Viewport::new(host.viewport.width, 1000.0);
    host.frame();
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..steps {
        let inputs = tc.draw(gs::vecs(input()).min_size(1).max_size(6));
        for input in inputs {
            host.input(input);
            // Keep the whole axis in the Window between inputs.
            host.frame();
        }
        let picks = host.s.grid.snapshot().picked(Axis::Rows).to_vec();
        let rows: &Vec<Key> = &host.s.source.axes[0];
        for k in picks {
            assert!(rows.contains(&k), "{k:?} picked after it left");
        }
    }
}

/// A range is positional: it keeps the rows between its ends, and rows
/// inserted strictly inside it join it. Modelled as a membership flag per
/// row, moved by each notice.
#[hegel::test(test_cases = 1000)]
fn a_range_keeps_the_rows_between_its_ends_through_inserts_and_removals(tc: TestCase) {
    let mut host = Host::new(&tc);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    for _ in 0..steps {
        // Select something: any inputs, then a shift-click.
        let inputs = tc.draw(gs::vecs(input()).max_size(4));
        for input in inputs {
            host.input(input);
        }
        host.input(Input::Release);
        let (a, b) = (tc.draw(fraction()), tc.draw(fraction()));
        host.input(Input::Press(a, b, Modifiers::SHIFT));
        host.input(Input::Release);
        host.frame();
        let Some(before) = host.s.grid.snapshot().selection_range().cloned() else {
            continue;
        };
        // A range over every row may be whole on the rows (it then grows
        // with inserts at the end and survives emptying) or not: the View
        // cannot tell which. Scenarios cover both.
        let all = 0..host.count(Axis::Rows);
        if before.rows == [all] {
            continue;
        }
        let mut member: Vec<bool> = (0..host.count(Axis::Rows))
            .map(|p| before.covers(Axis::Rows, p))
            .collect();
        let changes = tc.draw(gs::vecs(fraction()).min_size(1).max_size(3));
        for f in changes {
            let n = tc.draw(gs::integers::<u64>().min_value(1).max_value(3));
            let input = if tc.draw(gs::booleans()) {
                Input::Insert(Axis::Rows, f, n)
            } else {
                Input::Remove(Axis::Rows, f, n)
            };
            match host.input(input) {
                Some((_, ChangeNotice::Inserted { at, n })) => {
                    let at = at as usize;
                    let first = member.iter().position(|&m| m);
                    let last = member.iter().rposition(|&m| m);
                    let inside = first.zip(last).is_some_and(|(f, l)| f < at && at <= l);
                    member.splice(at..at, (0..n).map(|_| inside));
                }
                Some((_, ChangeNotice::Removed { at, n })) => {
                    member.drain(at as usize..(at + n) as usize);
                }
                other => panic!("{other:?}"),
            }
        }
        host.frame();
        let after = host.s.grid.snapshot().selection_range().cloned();
        let first = member.iter().position(|&m| m);
        let last = member.iter().rposition(|&m| m);
        match first.zip(last) {
            None => assert_eq!(after, None),
            Some((first, last)) => {
                let after = after.expect("some of the range is left");
                let kept = first as u64..last as u64 + 1;
                assert_eq!(after.rows, [kept]);
                assert_eq!(after.cols, before.cols);
            }
        }
    }
}

/// An item count anywhere in `u64`, often within a few items of the
/// largest.
#[hegel::composite]
fn huge_count(tc: &TestCase) -> u64 {
    tc.draw(hegel::one_of!(
        gs::integers::<u64>().max_value(20).map(|d| u64::MAX - d),
        gs::integers::<u64>(),
        gs::integers::<u64>().max_value(20),
    ))
}

/// Positional notices on axes of up to `u64::MAX` items: an `Inserted`
/// that would take the count past `u64::MAX` gains only the items that fit,
/// a `Removed` is clipped to the count, nothing panics, and focus, the
/// anchor and the range stay below the count.
#[hegel::test(test_cases = 1000)]
fn positional_notices_near_the_largest_count_keep_everything_in_bounds(tc: TestCase) {
    let mut counts = [tc.draw(huge_count()), tc.draw(huge_count())];
    let mut s = Scenario::new(
        Grid::new(
            AxisConfig::new(counts[0], 1.0).header(1.0),
            AxisConfig::new(counts[1], 3.0).header(2.0),
        ),
        IdentityTable {
            rows: IdentitySource::new(counts[0]),
            cols: IdentitySource::new(counts[1]),
        },
    );
    let v = Viewport::new(20.0, 10.0);
    let steps = [FocusStep::Item, FocusStep::Page, FocusStep::Edge];
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(12)) {
        let axis = tc.draw(axis());
        let i = axis.index();
        match tc.draw(gs::integers::<u8>().max_value(3)) {
            0 => {
                let at = tc.draw(hegel::one_of!(huge_count(), gs::just(counts[i])));
                let n = tc.draw(huge_count());
                counts[i] += n.min(u64::MAX - counts[i]);
                s.input(|g| g.notify(axis, ChangeNotice::Inserted { at, n }));
            }
            1 => {
                let at = tc.draw(huge_count());
                let n = tc.draw(huge_count());
                let at = at.min(counts[i]);
                counts[i] -= n.min(counts[i] - at);
                s.input(|g| g.notify(axis, ChangeNotice::Removed { at, n }));
            }
            2 => {
                let (by, forward, extend) = tc.draw(hegel::tuples!(
                    gs::sampled_from(steps.to_vec()),
                    gs::booleans(),
                    gs::booleans()
                ));
                s.input(|g| {
                    g.action(Action::MoveFocus {
                        axis,
                        by,
                        forward,
                        extend,
                    })
                });
            }
            _ => {
                let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
                s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
            }
        }
        s.source.rows = IdentitySource::new(counts[0]);
        s.source.cols = IdentitySource::new(counts[1]);
        let snap = s.frame(v);
        for axis in Axis::ALL {
            let i = axis.index();
            assert_eq!(snap.count(axis), counts[i], "count on {axis:?}");
            if let Some(f) = snap.focus(axis) {
                assert!(f.position < counts[i], "focus on {axis:?} {f:?}");
            }
            let anchor = snap.scroll_anchor(axis);
            assert!(anchor.position < counts[i].max(1), "{axis:?} {anchor:?}");
            if let Some(range) = snap.selection_range() {
                assert!(
                    range.span(axis).iter().all(|r| r.end <= counts[i]),
                    "{axis:?} {range:?}"
                );
            }
        }
    }
}
