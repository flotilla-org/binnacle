//! Seam 1 properties for activating, editing, typing and `set_focus`: a real
//! Grid over an identity Table Source, driven with random input sequences
//! that mix the host's edit handshake with pointer input, Actions and Change
//! Notices.

use binnacle_core::{
    Action, Axis, AxisConfig, Button, ChangeNotice, EditEnd, Effect, FocusStep, Grid, Key, Mode,
    Modifiers, PointerEvent, PointerKind, SelectionUnit, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{IdentityTable, Scenario};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// An axis of up to 40 items (the Keys [`key`] mostly draws) with everyday
/// sizes, Gap and header band.
#[hegel::composite]
fn axis_config(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(40));
    let size = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(20.0));
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(2.0)
    ));
    let header = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(5.0));
    AxisConfig::new(count, size).header(header).gap(gap)
}

#[derive(Clone, Debug)]
enum Input {
    Action(Action),
    /// A pointer event at fractions of the viewport (may be outside 0..1).
    Pointer(PointerEvent),
    EditStarted(Key, Key),
    EditEnded(EditEnd),
    SetFocus(Key, Key),
    Reordered(Axis),
    /// The identity source now has this many items on the axis.
    Reset(Axis, u64),
}

fn axis() -> impl Generator<Axis> {
    gs::sampled_from(vec![Axis::Rows, Axis::Cols])
}

/// Mostly Keys an axis of up to 40 items has, sometimes any.
fn key() -> impl Generator<Key> {
    hegel::one_of!(gs::integers::<u64>().max_value(45), gs::integers::<u64>()).map(Key)
}

#[hegel::composite]
fn pointer(tc: &TestCase) -> PointerEvent {
    let kind = tc.draw(gs::sampled_from(vec![
        PointerKind::Down,
        PointerKind::Down,
        PointerKind::Move,
        PointerKind::Up,
    ]));
    let at = || {
        hegel::one_of!(
            gs::floats::<f32>().min_value(-0.2).max_value(1.2),
            gs::sampled_from(vec![f32::NAN, f32::INFINITY])
        )
    };
    let (x, y) = (tc.draw(at()), tc.draw(at()));
    let modifiers = tc.draw(gs::sampled_from(vec![
        Modifiers::NONE,
        Modifiers::NONE,
        Modifiers::SHIFT,
        Modifiers::CTRL,
    ]));
    let event = PointerEvent::down(x, y)
        .button(tc.draw(gs::sampled_from(vec![
            Button::Primary,
            Button::Primary,
            Button::Secondary,
        ])))
        .modifiers(modifiers)
        .clicks(tc.draw(gs::integers::<u32>().min_value(1).max_value(3)));
    PointerEvent { kind, ..event }
}

#[hegel::composite]
fn input(tc: &TestCase) -> Input {
    let focus_step = gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]);
    tc.draw(hegel::one_of!(
        hegel::tuples!(axis(), focus_step, gs::booleans(), gs::booleans()).map(
            |(axis, by, forward, extend)| Input::Action(Action::MoveFocus {
                axis,
                by,
                forward,
                extend
            })
        ),
        gs::sampled_from(vec![
            Input::Action(Action::Activate),
            Input::Action(Action::BeginEdit),
            Input::Action(Action::TypeText("x".into())),
            Input::Action(Action::Cancel),
            Input::Action(Action::SelectAll),
            Input::EditEnded(EditEnd::Commit),
            Input::EditEnded(EditEnd::Cancel),
        ]),
        gs::integers::<i64>()
            .min_value(-10)
            .max_value(10)
            .map(|items| Input::Action(Action::ScrollBy {
                axis: Axis::Rows,
                items
            })),
        pointer().map(Input::Pointer),
        hegel::tuples!(key(), key()).map(|(r, c)| Input::EditStarted(r, c)),
        hegel::tuples!(key(), key()).map(|(r, c)| Input::EditStarted(r, c)),
        hegel::tuples!(key(), key()).map(|(r, c)| Input::SetFocus(r, c)),
        axis().map(Input::Reordered),
        hegel::tuples!(
            axis(),
            hegel::one_of!(
                gs::integers::<u64>().max_value(40),
                gs::integers::<u64>().max_value(3)
            )
        )
        .map(|(axis, count)| Input::Reset(axis, count)),
    ))
}

/// What the host believes about the edit, kept from the inputs alone.
#[derive(Clone, Debug, Default)]
struct EditModel {
    /// The edited cell, if the Grid should be editing.
    cell: Option<[Key; 2]>,
    /// Axes reset since the edit started whose `Locate` answers are still to
    /// come in this step's pump.
    checking: [bool; 2],
    /// The `EditCancelled` the Grid should send, in order.
    cancelled: Vec<Effect>,
}

impl EditModel {
    fn cancel(&mut self) {
        if let Some([row, col]) = self.cell.take() {
            self.cancelled.push(Effect::EditCancelled { row, col });
        }
        self.checking = [false; 2];
    }

    /// The pump answered every `Locate`: an edited Key a reset axis no
    /// longer has (the identity source has Keys below its count) has left.
    fn answered(&mut self, counts: [u64; 2]) {
        if let Some(cell) = self.cell
            && (0..2).any(|i| self.checking[i] && cell[i].0 >= counts[i])
        {
            self.cancel();
        }
        self.checking = [false; 2];
    }
}

/// What one step did, for the checks.
struct Step<'a> {
    drained: &'a [Effect],
    model: &'a EditModel,
    /// The last `set_focus` in the step if nothing that moves focus came
    /// after it, with the focused Positions just before it (the Scroll
    /// Anchor's while unplaced) when nothing earlier in the step moved them.
    set_focus: Option<([Key; 2], [Option<u64>; 2])>,
}

/// Draws a table and a viewport with room for a few cells, then a few steps
/// of random inputs, each followed by a pump and a frame, handing every
/// Snapshot to `check` with what the step did.
fn steps(tc: &TestCase, mut check: impl FnMut(&binnacle_core::Snapshot, &Step)) {
    let (rows, cols) = (tc.draw(axis_config()), tc.draw(axis_config()));
    // A body on each axis, so pointer events land on cells.
    let along = |c: AxisConfig| {
        tc.draw(
            gs::floats::<f32>()
                .min_value(c.header + 1.0)
                .max_value(c.header + 100.0),
        )
    };
    let viewport = Viewport::new(along(cols), along(rows));
    let table = IdentityTable {
        rows: IdentitySource::new(rows.count),
        cols: IdentitySource::new(cols.count),
    };
    let mut grid = Grid::new(rows, cols);
    grid.set_selection_unit(tc.draw(gs::sampled_from(vec![
        SelectionUnit::Cells,
        SelectionUnit::Rows,
    ])));
    let mut s = Scenario::new(grid, table);
    let mut counts = [rows.count, cols.count];
    let mut model = EditModel::default();
    let mut focused = {
        let snap = s.frame(viewport);
        Axis::ALL.map(|a| snap.focus(a).map(|f| f.position))
    };
    let mut anchors = Axis::ALL.map(|a| s.grid.snapshot().scroll_anchor(a).position);
    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(6)) {
        model.cancelled.clear();
        let mut set_focus = None;
        // Whether anything in the step so far may have moved focus or the
        // Scroll Anchors since the last Snapshot.
        let mut moved = false;
        for input in tc.draw(gs::vecs(input()).max_size(5)) {
            // Only the edit handshake and what merely emits leave a
            // `set_focus` standing; anything else may move focus or scroll.
            let leaves = matches!(
                input,
                Input::EditStarted(..)
                    | Input::EditEnded(_)
                    | Input::Action(Action::Activate | Action::BeginEdit | Action::TypeText(_))
            );
            if !leaves {
                set_focus = None;
            }
            match input {
                Input::Action(action) => {
                    if action == Action::Cancel {
                        model.cancel();
                    }
                    s.input(|g| g.action(action));
                }
                Input::Pointer(p) => {
                    let p = PointerEvent {
                        x: p.x * viewport.width,
                        y: p.y * viewport.height,
                        ..p
                    };
                    let press = p.kind == PointerKind::Down && p.button == Button::Primary;
                    if press && p.x.is_finite() && p.y.is_finite() {
                        model.cancel();
                    }
                    s.input(|g| g.pointer(p));
                }
                Input::EditStarted(row, col) => {
                    if model.cell != Some([row, col]) {
                        model.cell = Some([row, col]);
                        model.checking = [false; 2];
                    }
                    s.input(|g| g.edit_started(row, col));
                }
                Input::EditEnded(end) => {
                    model.cell = None;
                    model.checking = [false; 2];
                    s.input(|g| g.edit_ended(end));
                }
                Input::SetFocus(row, col) => {
                    let before = Axis::ALL.map(|a| {
                        let i = a.index();
                        let p = focused[i].unwrap_or(anchors[i]);
                        (!moved).then_some(p)
                    });
                    if !counts.contains(&0) {
                        set_focus = Some(([row, col], before));
                    }
                    s.input(|g| g.set_focus(row, col));
                }
                Input::Reordered(axis) => {
                    s.input(|g| g.notify(axis, ChangeNotice::Reordered));
                }
                Input::Reset(axis, count) => {
                    counts[axis.index()] = count;
                    let identity = IdentitySource::new(count);
                    match axis {
                        Axis::Rows => s.source.rows = identity,
                        Axis::Cols => s.source.cols = identity,
                    }
                    if count == 0 {
                        model.cancel();
                    } else if model.cell.is_some() {
                        model.checking[axis.index()] = true;
                    }
                    s.input(|g| g.notify(axis, ChangeNotice::Reset(count)));
                }
            }
            moved |= !leaves;
        }
        let drained: Vec<Effect> = s.effects();
        model.answered(counts);
        let snap = s.frame(viewport);
        focused = Axis::ALL.map(|a| snap.focus(a).map(|f| f.position));
        anchors = Axis::ALL.map(|a| snap.scroll_anchor(a).position);
        check(
            &snap,
            &Step {
                drained: &drained,
                model: &model,
                set_focus,
            },
        );
    }
}

/// The Grid is editing exactly the cell the host started editing, until the
/// host ends the edit, a `Cancel` or a primary press ends it, or a Change
/// Notice takes away one of its Keys; it sends `EditCancelled` exactly when
/// it ends the edit itself.
#[hegel::test(test_cases = 1000)]
fn an_edit_lasts_until_the_host_cancel_a_press_or_a_departed_key_ends_it(tc: TestCase) {
    steps(&tc, |snap, step| {
        let expected = step
            .model
            .cell
            .map_or(Mode::Idle, |[row, col]| Mode::Editing { row, col });
        match snap.mode() {
            Mode::Editing { .. } | Mode::Idle => assert_eq!(snap.mode(), expected),
            // A gesture in progress: the press that started it ended any edit.
            _ => assert_eq!(expected, Mode::Idle, "{:?}", snap.mode()),
        }
        let cancelled: Vec<&Effect> = (step.drained.iter())
            .filter(|e| matches!(e, Effect::EditCancelled { .. }))
            .collect();
        let expected: Vec<&Effect> = step.model.cancelled.iter().collect();
        assert_eq!(cancelled, expected);
    });
}

/// While editing, the editor rect is the edited cell's rect whenever both
/// its Keys are in the Strips, and `cell_state` marks that cell alone.
#[hegel::test(test_cases = 1000)]
fn the_editor_rect_and_cell_state_show_the_edited_cell(tc: TestCase) {
    steps(&tc, |snap, _| {
        let edited = match snap.mode() {
            Mode::Editing { row, col } => Some((row, col)),
            _ => None,
        };
        let find = |axis, key| snap.strip(axis).iter().find(|i| i.key == Some(key));
        let shown = edited.and_then(|(r, c)| find(Axis::Rows, r).zip(find(Axis::Cols, c)));
        match (shown, snap.editor_rect()) {
            (Some((r, c)), Some(rect)) => {
                assert_eq!(
                    (rect.x, rect.y, rect.width, rect.height),
                    (c.offset, r.offset, c.size, r.size)
                )
            }
            (None, None) => {}
            other => panic!("editor rect disagrees with the Strips: {other:?}"),
        }
        for r in snap.rows() {
            for c in snap.cols() {
                let mine =
                    edited.is_some_and(|(row, col)| r.key == Some(row) && c.key == Some(col));
                assert_eq!(snap.cell_state(r, c).editing, mine, "{r:?} {c:?}");
            }
        }
    });
}

/// After a `set_focus` that nothing later overrode, each focused end is the
/// named Key at its Position if the axis has it (the identity source has
/// Keys below the count), else where it was before; once both are present,
/// the cell shows.
#[hegel::test(test_cases = 1000)]
fn set_focus_lands_on_the_keys_the_source_has_and_shows_them(tc: TestCase) {
    steps(&tc, |snap, step| {
        let Some((keys, before)) = step.set_focus else {
            return;
        };
        let mut present = true;
        for axis in Axis::ALL {
            let i = axis.index();
            let end = snap.focus(axis).expect("set_focus places focus");
            assert!(!snap.locating(axis), "{axis:?} still locating");
            if keys[i].0 < snap.count(axis) {
                assert_eq!(end.position, keys[i].0, "{axis:?}");
            } else {
                present = false;
                if let Some(before) = before[i] {
                    assert_eq!(end.position, before, "{axis:?}: a departed Key moved focus");
                }
            }
        }
        if present {
            let rect = snap.focus_rect().expect("the cell is revealed");
            for (axis, lo, len) in [
                (Axis::Rows, rect.y, rect.height),
                (Axis::Cols, rect.x, rect.width),
            ] {
                let (start, end) = (snap.header(axis), snap.extent(axis));
                // The Strip steps offsets in f32: across N items it drifts
                // up to N + 2 f32 steps at the far edge (the reveal slack
                // of #16).
                let steps = snap.strip(axis).len() as f32 + 2.0;
                let slack = steps * (f32::from_bits(end.to_bits() + 1) - end);
                if len <= end - start {
                    assert!(lo >= start && lo + len <= end + slack, "{axis:?}: {rect:?}");
                }
            }
        }
    });
}

/// `Activate` and `BeginEdit` name the focused cell, and send nothing while
/// focus is unplaced; `TypeText` passes its text on as it is.
#[hegel::test(test_cases = 1000)]
fn activate_and_begin_edit_name_the_focused_cell(tc: TestCase) {
    let action = tc.draw(gs::sampled_from(vec![Action::Activate, Action::BeginEdit]));
    let text = tc.draw(gs::text().max_size(4));
    let (rows, cols) = (tc.draw(axis_config()), tc.draw(axis_config()));
    let mut s = Scenario::identity(rows, cols);
    let viewport = Viewport::new(cols.header + 50.0, rows.header + 50.0);
    s.frame(viewport);
    let cell = tc.draw(gs::optional(hegel::tuples!(
        gs::integers::<u64>().max_value(rows.count - 1),
        gs::integers::<u64>().max_value(cols.count - 1)
    )));
    if let Some((r, c)) = cell {
        s.input(|g| g.set_focus(Key(r), Key(c)));
    }
    s.frame(viewport);
    s.pump();
    s.input(|g| {
        g.action(action.clone());
        g.action(Action::TypeText(text.clone()));
    });
    let drained: Vec<Effect> = s.effects();
    let mut expected = Vec::new();
    if let Some((r, c)) = cell {
        let (row, col) = (Key(r), Key(c));
        expected.push(match action {
            Action::Activate => Effect::Activated {
                row,
                col,
                drill_down: None,
            },
            _ => Effect::EditRequested { row, col },
        });
    }
    expected.push(Effect::TextTyped { text });
    assert_eq!(drained, expected);
}
