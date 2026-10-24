//! Seam 1 properties: a real Grid over an identity Table Source, driven through
//! the frame loop with random tables, scroll inputs, Size Overrides and
//! viewports.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::rc::Rc;

use binnacle_core::{
    Action, Axis, AxisConfig, AxisMeasure, Button, CellRange, Cursor, DropTarget, Effect, FocusEnd,
    FocusStep, Grid, HeaderClick, Key, Mode, Modifiers, PointerEvent, PointerKind, Position,
    ScrollAnchor, SelectionUnit, Snapshot, Viewport,
};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{IdentityTable, PumpSource, Scenario, ScenarioSource};
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// Most items a drawn viewport can show along one axis at the smallest item
/// size, which bounds the Keys the identity source materialises per frame.
const MAX_VISIBLE: f32 = 1000.0;

#[hegel::composite]
fn axis_config(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(hegel::one_of!(
        gs::just(0u64),
        gs::integers::<u64>().max_value(50),
        gs::integers::<u64>()
            .min_value(999_999_999_999_000)
            .max_value(1_000_000_000_000_000),
        gs::integers::<u64>(),
    ));
    let header = tc.draw(gs::floats::<f32>());
    let (min, max) = tc.draw(size_range());
    let size = tc.draw(hegel::one_of!(
        gs::sampled_from(vec![min, max]),
        gs::floats::<f32>().min_value(min).max_value(max)
    ));
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(max),
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(2.0f32.min(max)),
    ));
    AxisConfig::new(count, size)
        .min_size(min)
        .max_size(max)
        .header(header)
        .gap(gap)
}

/// An axis's size range: the default, or one whose smallest size is anywhere
/// from 2^-10 to 2^10 and whose largest is up to 2^20 times that, the
/// default's spread.
#[hegel::composite]
fn size_range(tc: &TestCase) -> (f32, f32) {
    let default = (
        AxisConfig::new(0, 1.0).min_size,
        AxisConfig::new(0, 1.0).max_size,
    );
    let custom = hegel::tuples!(
        gs::floats::<f32>()
            .min_value(1.0 / 1024.0)
            .max_value(1024.0),
        gs::floats::<f32>().min_value(1.0).max_value(1_048_576.0)
    )
    .map(|(min, spread)| (min, min * spread));
    tc.draw(hegel::one_of!(gs::just(default), custom))
}

/// A viewport length for an axis: usually one that shows items past the header
/// band, sometimes any length up to that, sometimes hostile. `smallest` is the
/// smallest item size the axis can have.
#[hegel::composite]
fn extent(tc: &TestCase, config: AxisConfig, smallest: f32) -> f32 {
    let header = if config.header.is_finite() {
        config.header.max(0.0)
    } else {
        0.0
    };
    let body = (smallest * MAX_VISIBLE).min(f32::MAX);
    let max = (header + body).min(f32::MAX);
    tc.draw(hegel::one_of!(
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(body)
            .map(move |b| (header + b).min(f32::MAX)),
        gs::floats::<f32>().min_value(0.0).max_value(max),
        gs::floats::<f32>().min_value(-1.0e6).max_value(0.0),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY]),
    ))
}

#[hegel::composite]
fn axis(tc: &TestCase) -> Axis {
    tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]))
}

#[derive(Clone, Debug)]
enum Input {
    Action(Action),
    SetSize(Axis, Key, f32),
    Pointer(Pointer),
    /// A `Reordered` Change Notice that leaves the identity order as it was.
    Notice(Axis),
    /// The identity source now has this many items on the axis; the Grid
    /// hears `Reset`.
    Reset(Axis, u64),
    /// The host calls `set_focus` with a row and a column Key.
    SetFocus(Key, Key),
    /// The host opens its editor on a cell (`edit_started`), or closes it.
    Edit(Option<(Key, Key)>),
}

/// A pointer event whose coordinates are resolved against the previous frame
/// (a [`Target`]), so most land on something.
#[derive(Clone, Copy, Debug)]
struct Pointer {
    event: PointerEvent,
    at: [Coord; 2],
}

#[derive(Clone, Copy, Debug)]
enum Coord {
    /// A fraction of the viewport's length along that axis (may be outside 0..1).
    Fraction(f32),
    /// A fraction of the way through the body, past the header band.
    Body(f32),
    Raw(f32),
    /// A fraction of the way through the header band.
    Header(f32),
    /// On the border after a visible item (in its Gap, else at its end),
    /// picked by index; the middle of the viewport if none shows.
    Border(usize),
}

/// What a pointer event's coordinates resolve against: the previous frame's
/// viewport, its header bands and its item borders, each as `x` then `y`.
#[derive(Clone, Debug, Default)]
struct Target {
    lengths: [f32; 2],
    headers: [f32; 2],
    borders: [Vec<f32>; 2],
}

impl Target {
    fn of(snap: &Snapshot) -> Target {
        let [x, y] = [Axis::Cols, Axis::Rows];
        let header = |a| {
            let h = snap.header(a);
            if h.is_finite() { h } else { 0.0 }
        };
        let borders = |a| {
            let gap = snap.gap(a);
            (snap.strip(a).iter())
                .map(|i| i.end() + gap / 2.0)
                .collect()
        };
        Target {
            lengths: [snap.extent(x), snap.extent(y)],
            headers: [header(x), header(y)],
            borders: [borders(x), borders(y)],
        }
    }
}

impl Pointer {
    /// The event at its coordinates against `target`.
    fn resolve(self, target: &Target) -> PointerEvent {
        let [x, y] = [0, 1].map(|i| {
            let (len, header) = (target.lengths[i], target.headers[i]);
            match self.at[i] {
                Coord::Fraction(f) => f * len,
                Coord::Body(f) => header + f * (len - header),
                Coord::Raw(v) => v,
                Coord::Header(f) => f * header,
                Coord::Border(n) => match &target.borders[i][..] {
                    [] => len / 2.0,
                    borders => borders[n % borders.len()],
                },
            }
        });
        PointerEvent { x, y, ..self.event }
    }
}

#[hegel::composite]
fn coord(tc: &TestCase) -> Coord {
    tc.draw(hegel::one_of!(
        gs::floats::<f32>()
            .min_value(-0.05)
            .max_value(1.05)
            .map(Coord::Fraction),
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(1.0)
            .map(Coord::Body),
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(1.0)
            .map(Coord::Body),
        gs::floats::<f32>().map(Coord::Raw),
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(1.0)
            .map(Coord::Header),
        gs::integers::<usize>().max_value(8).map(Coord::Border),
    ))
}

#[hegel::composite]
fn modifiers(tc: &TestCase) -> Modifiers {
    let (shift, ctrl, alt, meta) = tc.draw(hegel::tuples!(
        gs::weighted_booleans(0.3),
        gs::weighted_booleans(0.2),
        gs::weighted_booleans(0.1),
        gs::weighted_booleans(0.1)
    ));
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
    }
}

#[hegel::composite]
fn pointer(tc: &TestCase) -> Pointer {
    let kind = tc.draw(gs::sampled_from(vec![
        PointerKind::Down,
        PointerKind::Move,
        PointerKind::Move,
        PointerKind::Up,
    ]));
    let button = tc.draw(gs::sampled_from(vec![
        Button::Primary,
        Button::Primary,
        Button::Primary,
        Button::Secondary,
        Button::Middle,
    ]));
    let event = PointerEvent::down(0.0, 0.0)
        .button(button)
        .modifiers(tc.draw(modifiers()))
        .clicks(tc.draw(gs::integers::<u32>().max_value(3)));
    Pointer {
        event: PointerEvent { kind, ..event },
        at: [tc.draw(coord()), tc.draw(coord())],
    }
}

/// A press, a few moves and the release, as one input sequence. Some presses
/// aim at a header border, some at a header, some near a band's edge.
#[hegel::composite]
fn gesture(tc: &TestCase) -> Vec<Pointer> {
    let mut press = tc.draw(pointer());
    if tc.draw(gs::weighted_booleans(0.1)) {
        let (f, g) = (
            tc.draw(gs::floats::<f32>().min_value(0.8).max_value(1.0)),
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
        );
        press.at = if tc.draw(gs::booleans()) {
            [Coord::Header(f), Coord::Header(g)]
        } else {
            [Coord::Header(g), Coord::Header(f)]
        };
    } else if tc.draw(gs::weighted_booleans(0.2)) {
        let (f, g) = (
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
        );
        press.at = if tc.draw(gs::booleans()) {
            [Coord::Body(f), Coord::Header(g)]
        } else {
            [Coord::Header(g), Coord::Body(f)]
        };
    } else if tc.draw(gs::weighted_booleans(0.3)) {
        let (n, f) = (
            tc.draw(gs::integers::<usize>().max_value(8)),
            tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)),
        );
        press.at = if tc.draw(gs::booleans()) {
            [Coord::Border(n), Coord::Header(f)]
        } else {
            [Coord::Header(f), Coord::Border(n)]
        };
    }
    let mut events = vec![Pointer {
        event: PointerEvent {
            kind: PointerKind::Down,
            button: Button::Primary,
            ..press.event
        },
        ..press
    }];
    for at in tc.draw(gs::vecs(hegel::tuples!(coord(), coord())).max_size(4)) {
        events.push(Pointer {
            event: PointerEvent::moved(0.0, 0.0),
            at: [at.0, at.1],
        });
    }
    if tc.draw(gs::booleans()) {
        let at = tc.draw(hegel::tuples!(coord(), coord()));
        events.push(Pointer {
            event: PointerEvent::up(0.0, 0.0),
            at: [at.0, at.1],
        });
    }
    events
}

#[hegel::composite]
fn input(tc: &TestCase) -> Input {
    let distance = || {
        hegel::one_of!(
            gs::floats::<f32>().min_value(-50.0).max_value(50.0),
            gs::floats::<f32>()
        )
    };
    let key =
        || hegel::one_of!(gs::integers::<u64>().max_value(60), gs::integers::<u64>()).map(Key);
    let size = hegel::one_of!(
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .allow_infinity(false),
        gs::floats::<f32>().min_value(0.5).max_value(20.0),
        gs::floats::<f32>().max_value(0.0),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY]),
    );
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
        hegel::tuples!(distance(), distance())
            .map(|(dx, dy)| Input::Action(Action::Scroll { dx, dy })),
        hegel::tuples!(
            axis(),
            hegel::one_of!(
                gs::floats::<f64>().min_value(0.0).max_value(1.0),
                gs::floats::<f64>()
            )
        )
        .map(|(axis, fraction)| Input::Action(Action::ScrollTo { axis, fraction })),
        hegel::tuples!(
            axis(),
            hegel::one_of!(
                gs::integers::<i64>().min_value(-30).max_value(30),
                gs::integers::<i64>()
            )
        )
        .map(|(axis, items)| Input::Action(Action::ScrollBy { axis, items })),
        hegel::tuples!(axis(), key(), size).map(|(a, k, s)| Input::SetSize(a, k, s)),
        pointer().map(Input::Pointer),
        pointer().map(Input::Pointer),
        axis().map(Input::Notice),
        hegel::tuples!(
            axis(),
            hegel::one_of!(gs::integers::<u64>().max_value(40), gs::integers::<u64>())
        )
        .map(|(axis, count)| Input::Reset(axis, count)),
        gs::sampled_from(vec![
            Input::Action(Action::SelectAll),
            Input::Action(Action::SelectColumn),
            Input::Action(Action::Cancel),
            Input::Action(Action::Activate),
            Input::Action(Action::BeginEdit),
            Input::Edit(None),
        ]),
        hegel::tuples!(key(), key()).map(|(r, c)| Input::SetFocus(r, c)),
        hegel::tuples!(key(), key()).map(|(r, c)| Input::Edit(Some((r, c)))),
    ))
}

/// Content sizes a test source answers with on one axis: a table indexed by
/// Key (wrapping) and the minimum-size hint.
#[derive(Clone, Debug)]
struct Content {
    sizes: Vec<f32>,
    hint: f32,
}

#[hegel::composite]
fn content(tc: &TestCase) -> Content {
    let sizes = tc.draw(
        gs::vecs(hegel::one_of!(
            gs::floats::<f32>()
                .min_value_exclusive(0.0)
                .allow_infinity(false),
            gs::floats::<f32>().min_value(0.5).max_value(20.0),
            gs::sampled_from(vec![0.0, -1.0, f32::NAN, f32::INFINITY]),
        ))
        .min_size(1)
        .max_size(8),
    );
    let hint = tc.draw(
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .allow_infinity(false),
    );
    Content { sizes, hint }
}

impl Content {
    /// The content size the source answers for `key`.
    fn raw(&self, key: Key) -> f32 {
        self.sizes[(key.0 % self.sizes.len() as u64) as usize]
    }
}

/// The Windows a test source was asked for, by axis.
type Requests = Rc<RefCell<Vec<(Axis, Range<Position>)>>>;

/// An identity Table Source that also answers with content sizes,
/// remembers the last Window it filled on each axis and records every
/// Window it was asked for.
struct Sized {
    table: IdentityTable,
    content: [Option<Content>; 2],
    windows: [Range<Position>; 2],
    /// Every Window the Grid asked for, by axis, since the test last cleared it.
    requests: Requests,
}

impl ScenarioSource for Sized {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.windows[axis.index()] = range.clone();
        self.requests.borrow_mut().push((axis, range.clone()));
        self.table.window(axis, range)
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        let c = self.content[axis.index()].as_ref()?;
        Some((keys.iter().map(|&k| c.raw(k)).collect(), c.hint))
    }
}

impl PumpSource for Sized {
    fn locate(&mut self, axis: Axis, key: Key) -> binnacle_core::Located {
        self.table.locate(axis, key)
    }
}

/// What the test knows about one axis: its config, its Size Overrides (set
/// directly, or by pointer resizes and read back) and any content sizes. Keys are Positions (identity source), so it knows
/// every item's size.
#[derive(Clone, Debug)]
struct Model {
    config: AxisConfig,
    overrides: BTreeMap<Key, f32>,
    content: Option<Content>,
}

impl Model {
    /// `size` clamped into the axis's size range, or `None` if it is not
    /// positive and finite.
    fn in_range(&self, size: f32) -> Option<f32> {
        (size.is_finite() && size > 0.0)
            .then(|| size.clamp(self.config.min_size, self.config.max_size))
    }

    /// The content size hint as the Grid keeps it.
    fn hint(&self) -> Option<f32> {
        self.in_range(self.content.as_ref()?.hint)
    }

    /// The content size the Grid uses for `key`: raised to the hint and
    /// clamped, or unknown when not positive and finite.
    fn content_size(&self, key: Key) -> Option<f32> {
        let raw = self.content.as_ref()?.raw(key);
        let hint = self.hint().unwrap_or(self.config.min_size);
        self.in_range(raw).map(|s| s.max(hint))
    }

    /// The size of the item at `pos` once its Key is known: Size Override,
    /// else content size, else default.
    fn size_of(&self, pos: Position) -> f32 {
        let key = Key(pos);
        self.overrides
            .get(&key)
            .copied()
            .or_else(|| self.content_size(key))
            .unwrap_or(self.config.default_size)
    }

    fn smallest(&self) -> f32 {
        let hint = self.hint().unwrap_or(f32::INFINITY);
        self.overrides
            .values()
            .fold(self.config.default_size.min(hint), |a, &b| a.min(b))
    }
}

/// An axis with at least one item and a header band of up to
/// [`MAX_VISIBLE`] items of the smallest size: a band dragged to nothing hands
/// its length to the body.
#[hegel::composite]
fn axis_with_items(tc: &TestCase) -> AxisConfig {
    let config = tc.draw(axis_config());
    let header = tc.draw(
        gs::floats::<f32>()
            .min_value(0.0)
            .max_value(config.min_size * MAX_VISIBLE),
    );
    AxisConfig {
        count: config.count.max(1),
        ..config.header(header)
    }
}

/// A viewport extent for `config` (whose header must be finite) leaving a body
/// of up to [`MAX_VISIBLE`] items of size `smallest`.
#[hegel::composite]
fn body_extent(tc: &TestCase, config: AxisConfig, smallest: f32) -> f32 {
    let body = tc.draw(
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .max_value((smallest * MAX_VISIBLE).min(1.0e30)),
    );
    config.header + body
}

/// Which knobs a run draws beyond the table and scroll inputs.
#[derive(Clone, Copy)]
struct Knobs {
    /// Set Size Overrides.
    resize: bool,
    /// Answer some axes with content sizes.
    content: bool,
    /// Draw only tables with items on both axes and viewports with a body,
    /// where pointer events mostly land on cells.
    bodies: bool,
}

const ALL: Knobs = Knobs {
    resize: true,
    content: true,
    bodies: false,
};

const BODIES: Knobs = Knobs {
    bodies: true,
    ..ALL
};

/// What one step of [`frames`] did before its frame.
struct Step<'a> {
    models: &'a [Model; 2],
    /// The Effects the host drained after this step's inputs.
    drained: &'a [Effect],
    /// Whether any input was a focus move.
    moved_focus: bool,
    /// Whether the last input that scrolls was a focus move, so the frame
    /// must show the focused cell.
    reveals_focus: bool,
    /// The Windows the Grid asked for in this step's frame.
    requests: &'a RefCell<Vec<(Axis, Range<Position>)>>,
    /// The End Gap of each axis.
    end_gaps: [f32; 2],
    /// The largest count each axis had at any release in this step: a
    /// release names its drop against the count then, which a later Reset
    /// in the same step may shrink.
    release_counts: [u64; 2],
}

/// Draws a table, then a few steps of random inputs, each followed by a pump
/// and a frame at a random viewport, handing every Snapshot to `check` with
/// what the step did.
fn frames(tc: &TestCase, knobs: Knobs, mut check: impl FnMut(&Snapshot, &Step)) {
    let config = || {
        if knobs.bodies {
            tc.draw(hegel::one_of!(axis_with_items(), everyday_axis()))
        } else {
            tc.draw(axis_config())
        }
    };
    let (rows, cols) = (config(), config());
    let viewport_for = |models: &[Model; 2]| {
        let [r, c] = models;
        let along = |m: &Model| {
            if knobs.bodies {
                tc.draw(body_extent(m.config, m.smallest()))
            } else {
                tc.draw(extent(m.config, m.smallest()))
            }
        };
        let w = along(c);
        Viewport::new(w, along(r))
    };
    let mut contents = [None, None];
    if knobs.content {
        for c in &mut contents {
            *c = tc.draw(gs::optional(content()));
        }
    }
    let [rc, cc] = contents.clone();
    let mut models = [(rows, rc), (cols, cc)].map(|(config, content)| Model {
        config,
        overrides: BTreeMap::new(),
        content,
    });
    let table = IdentityTable {
        rows: IdentitySource::new(rows.count),
        cols: IdentitySource::new(cols.count),
    };
    let requests = Rc::new(RefCell::new(Vec::new()));
    let source = Sized {
        table,
        content: contents,
        windows: [0..0, 0..0],
        requests: requests.clone(),
    };
    let mut grid = Grid::new(rows, cols);
    // Moves, but no pins: these properties hold for unpinned axes.
    grid.set_movable(Axis::Cols, true);
    for axis in Axis::ALL {
        grid.set_pin_zone(axis, 0.0);
    }
    configure(tc, &mut grid);
    let end_gaps = Axis::ALL.map(|axis| {
        let gap = tc.draw(hegel::one_of!(
            gs::just(0.0f32),
            gs::just(1.0f32),
            gs::floats::<f32>().min_value(0.0).max_value(8.0),
        ));
        grid.set_end_gap(axis, gap);
        gap
    });
    let mut s = Scenario::new(grid, source);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    let mut target = Target::default();
    // Every Key a pointer resize has been seen on, per axis.
    let mut resized: [BTreeSet<Key>; 2] = Default::default();
    // Whether a selection drag is in progress. Pointer inputs come last in a
    // step, so before them it is the last frame's Mode, less any drag a
    // Cancel or notice earlier in the step ended.
    let mut dragging = false;
    // Sometimes a frame before any input, so pointer events have a viewport
    // to land in from the start.
    if tc.draw(gs::booleans()) {
        target = Target::of(&s.frame(viewport_for(&models)));
        s.pump();
    }
    for _ in 0..steps {
        let (mut moved_focus, mut reveals_focus) = (false, false);
        let mut release_counts = [0; 2];
        let mut inputs = tc.draw(gs::vecs(input()).max_size(4));
        if tc.draw(gs::weighted_booleans(0.5)) {
            inputs.extend(tc.draw(gesture()).into_iter().map(Input::Pointer));
        }
        for input in inputs {
            match &input {
                Input::Action(Action::MoveFocus { .. }) => {
                    moved_focus = true;
                    reveals_focus = true;
                }
                // Cancel may restore an earlier focus, even an unplaced one.
                Input::Action(Action::Cancel) => {
                    moved_focus = true;
                    reveals_focus = false;
                    dragging = false;
                }
                Input::Action(_) => reveals_focus = false,
                // A click focuses what is under the pointer without scrolling,
                // and drops a reveal a focus move left pending.
                Input::Pointer(_) => {
                    moved_focus = true;
                    reveals_focus = false;
                }
                // A notice cancels a selection drag, restoring the focus from
                // before its press as Cancel does: unplaced, or not revealed.
                Input::Notice(_) => {
                    moved_focus = true;
                    if std::mem::take(&mut dragging) {
                        reveals_focus = false;
                    }
                }
                // A Reset also may move focus to a suggestion or unplace it,
                // and a move made before it may not have placed focus at all.
                Input::Reset(..) => {
                    moved_focus = true;
                    reveals_focus = false;
                    dragging = false;
                }
                // `set_focus` reveals once placed, unless an axis is empty.
                Input::SetFocus(..) => {
                    moved_focus = true;
                    reveals_focus = false;
                }
                Input::SetSize(..) | Input::Edit(_) => {}
            }
            match input {
                Input::Action(action) => s.input(|g| g.action(action)),
                Input::Pointer(p) => {
                    if p.event.kind == PointerKind::Up {
                        release_counts = Axis::ALL
                            .map(|a| release_counts[a.index()].max(models[a.index()].config.count));
                    }
                    s.input(|g| g.pointer(p.resolve(&target)))
                }
                Input::SetSize(axis, key, size) if knobs.resize => {
                    s.input(|g| g.set_size(axis, key, size));
                    let model = &mut models[axis.index()];
                    if let Some(size) = model.in_range(size) {
                        model.overrides.insert(key, size);
                    }
                }
                Input::SetSize(..) => {}
                Input::Notice(axis) => {
                    s.input(|g| g.notify(axis, binnacle_core::ChangeNotice::Reordered))
                }
                Input::Reset(axis, count) => {
                    let identity = IdentitySource::new(count);
                    match axis {
                        Axis::Rows => s.source.table.rows = identity,
                        Axis::Cols => s.source.table.cols = identity,
                    }
                    let model = &mut models[axis.index()];
                    model.config.count = count;
                    s.input(|g| g.notify(axis, binnacle_core::ChangeNotice::Reset(count)))
                }
                Input::SetFocus(row, col) => s.input(|g| g.set_focus(row, col)),
                Input::Edit(Some((row, col))) => s.input(|g| g.edit_started(row, col)),
                Input::Edit(None) => s.input(|g| g.edit_ended(binnacle_core::EditEnd::Commit)),
            }
        }
        let drained: Vec<Effect> = s.effects();
        // A band resize moves where the body starts: viewports follow it.
        for axis in Axis::ALL {
            models[axis.index()].config.header = s.grid.header(axis);
        }
        // A pointer resize changes Size Overrides of Keys in the Window (or of
        // one seen resizing earlier): read those back. The model takes the
        // Grid's word here, so only
        // `dragging_a_header_border_resizes_its_item_by_the_pointer_travel`
        // catches a wrong pointer resize.
        for axis in Axis::ALL {
            let i = axis.index();
            let model = &mut models[i];
            let window = s.source.windows[i].clone().map(Key);
            let keys: BTreeSet<Key> = (model.overrides.keys().copied())
                .chain(window)
                .chain(resized[i].iter().copied())
                .collect();
            for key in keys {
                match s.grid.size_override(axis, key) {
                    Some(size) => model.overrides.insert(key, size),
                    None => model.overrides.remove(&key),
                };
            }
        }
        let next = viewport_for(&models);
        requests.borrow_mut().clear();
        let step = Step {
            models: &models,
            drained: &drained,
            moved_focus,
            reveals_focus,
            requests: &requests,
            end_gaps,
            release_counts,
        };
        let snap = s.frame(next);
        if let Mode::Resizing { axis, key, .. } = snap.mode() {
            resized[axis.index()].insert(key);
        }
        target = Target::of(&snap);
        dragging = snap.mode() == Mode::DraggingSelection;
        check(&snap, &step);
    }
}

/// Draws the Grid's selection and pointer settings.
fn configure(tc: &TestCase, grid: &mut Grid) {
    let unit = tc.draw(gs::sampled_from(vec![
        SelectionUnit::Cells,
        SelectionUnit::Rows,
    ]));
    grid.set_selection_unit(unit);
    let clicks = vec![HeaderClick::Sort, HeaderClick::Select, HeaderClick::None];
    for axis in Axis::ALL {
        if tc.draw(gs::booleans()) {
            grid.set_header_click(axis, tc.draw(gs::sampled_from(clicks.clone())));
        }
    }
    let threshold = tc.draw(hegel::one_of!(
        gs::just(1.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(5.0),
        gs::floats::<f32>()
    ));
    grid.set_drag_threshold(threshold);
    let grab = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(3.0),
        gs::floats::<f32>()
    ));
    grid.set_border_grab(grab);
}

/// The size the Grid gives `pos`: as drawn in the Strip if it is there, else
/// the model's. A Position the end clamp has just moved the anchor back to may
/// have no Key yet, and the Strip then shows it at the default size.
fn seen_size(snap: &Snapshot, axis: Axis, model: &Model, pos: Position) -> f32 {
    snap.strip(axis)
        .iter()
        .find(|i| i.position == pos)
        .map_or_else(|| model.size_of(pos), |i| i.size)
}

/// The body is the part of the viewport past the header band, where items show.
fn body(snap: &Snapshot, axis: Axis) -> (f32, f32) {
    (snap.header(axis), snap.extent(axis))
}

fn next_f32(x: f32) -> f32 {
    f32::from_bits(x.to_bits() + 1)
}

#[hegel::test(test_cases = 2000)]
fn strips_are_ordered_separated_by_the_gap_and_cover_the_viewport(tc: TestCase) {
    frames(&tc, ALL, |snap, step| {
        let models = step.models;
        for axis in Axis::ALL {
            let model = &models[axis.index()];
            let gap = snap.gap(axis);
            let strip = snap.strip(axis);
            let (start, end) = body(snap, axis);
            let count = snap.count(axis);
            let anchor = snap.scroll_anchor(axis);
            // AxisConfig promises coverage only for sizes of at least one f32
            // step at the viewport's far edge.
            let resolvable = model.smallest() >= next_f32(end) - end;
            let anchor_size = seen_size(snap, axis, model, anchor.position);
            // An anchor in the Gap after its item starts the Strip at the next.
            let in_gap = count > 0 && anchor.offset >= anchor_size;
            // What is left of the anchor's item past the header can be too thin
            // to register in f32; then the Strip starts at the next item.
            let sliver = !in_gap
                && (f64::from(start) - f64::from(anchor.offset)) as f32 + anchor_size <= start;
            let Some(first) = strip.first() else {
                // The item after the anchor's Gap starts at the header plus what
                // remains of the Gap; after a sliver, the Strip steps from the
                // rounded sliver end, up to one f32 step later.
                let next_start = (f64::from(start) + f64::from(anchor_size) + f64::from(gap)
                    - f64::from(anchor.offset)) as f32;
                let rounding = if sliver { next_f32(end) - end } else { 0.0 };
                assert!(
                    count == 0
                        || start >= end
                        || !resolvable
                        || ((in_gap || sliver)
                            && (anchor.position + 1 == count || next_start + rounding >= end)),
                    "{axis:?}: empty Strip with content and room"
                );
                continue;
            };
            let expected = anchor.position + u64::from(in_gap);
            assert!(
                first.position == expected || (sliver && first.position == expected + 1),
                "{axis:?}: Strip starts at {first:?}, not the Scroll Anchor {anchor:?}"
            );
            // After the anchor's Gap the next item starts within the Gap; after
            // an undrawable sliver, within the Gap plus that sliver, which is
            // under one f32 step at the body's start.
            let latest = if first.position == anchor.position {
                start
            } else if in_gap {
                start + gap
            } else {
                start + gap + (next_f32(start) - start)
            };
            assert!(
                first.offset <= latest,
                "{axis:?}: Strip starts after the body"
            );
            for pair in strip.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                assert_eq!(
                    b.position,
                    a.position + 1,
                    "{axis:?}: Positions not consecutive"
                );
                assert_eq!(
                    b.offset,
                    a.end() + gap,
                    "{axis:?}: items not separated by exactly the Gap"
                );
            }
            for item in strip {
                // An unfilled Position has no Key to size it by.
                let expected = match item.key {
                    Some(_) => model.size_of(item.position),
                    None => model.config.default_size,
                };
                assert_eq!(item.size, expected, "{axis:?}: {item:?} has the wrong size");
                assert!(item.position < count, "{axis:?}: {item:?} past the count");
                assert!(
                    item.offset < end && item.end() > start,
                    "{axis:?}: {item:?} not visible"
                );
            }
            let last = strip.last().unwrap();
            assert!(
                !resolvable || last.end() + gap >= end || last.position == count - 1,
                "{axis:?}: Strip stops short at {last:?} with content remaining"
            );
        }
    });
}

/// Content sizes are left out: their minimum-size hint arrives with the Window
/// it bounds, so the frame that first brings it may show loading items.
#[hegel::test(test_cases = 1000)]
fn every_strip_item_carries_the_key_the_source_gave_for_its_position(tc: TestCase) {
    frames(
        &tc,
        Knobs {
            resize: true,
            content: false,
            bodies: false,
        },
        |snap, _| {
            for axis in Axis::ALL {
                for item in snap.strip(axis) {
                    assert_eq!(item.key, Some(Key(item.position)), "{axis:?}");
                }
            }
        },
    );
}

#[hegel::test(test_cases = 2000)]
fn the_scroll_anchor_and_thumb_stay_in_bounds(tc: TestCase) {
    frames(&tc, ALL, |snap, step| {
        let models = step.models;
        for axis in Axis::ALL {
            let model = &models[axis.index()];
            let anchor = snap.scroll_anchor(axis);
            let count = snap.count(axis);
            assert!(
                anchor.position < count.max(1),
                "{axis:?}: {anchor:?} past the count {count}"
            );
            // Summed in f64: the f32 sum may round down onto the offset.
            let stride = f64::from(seen_size(snap, axis, model, anchor.position))
                + f64::from(snap.gap(axis));
            assert!(
                anchor.offset >= 0.0 && f64::from(anchor.offset) < stride,
                "{axis:?}: {anchor:?} outside its item and Gap"
            );
            let thumb = snap.thumb(axis);
            assert!(
                (0.0..=1.0).contains(&thumb.fraction) && (0.0..=1.0).contains(&thumb.size),
                "{axis:?}: {thumb:?}"
            );
        }
    });
}

/// Once scrolled off the start, the view never shows empty space after the
/// last item, past the End Gap. Without Size Overrides or content sizes every size is known in
/// advance (sizes the Grid has not seen are estimates), so the clamp is exact up
/// to f32 stepping across the Strip.
#[hegel::test(test_cases = 2000)]
fn scrolling_stops_where_the_last_item_ends_the_viewport(tc: TestCase) {
    frames(
        &tc,
        Knobs {
            resize: false,
            content: false,
            bodies: false,
        },
        |snap, step| {
            let models = step.models;
            for axis in Axis::ALL {
                // The last item stops the End Gap short of the viewport's end.
                let (start, extent) = body(snap, axis);
                let end_gap = step.end_gaps[axis.index()];
                let end = (extent - end_gap).max(start.min(extent));
                let count = snap.count(axis);
                let anchor = snap.scroll_anchor(axis);
                let resolvable = models[axis.index()].smallest() >= next_f32(end) - end;
                let Some(last) = snap.strip(axis).last() else {
                    continue;
                };
                if anchor.position == 0 && anchor.offset == 0.0
                    || last.position + 1 < count
                    || !resolvable
                    || start >= end
                {
                    continue;
                }
                // Slack for f32 stepping across the Strip, at the largest magnitude
                // it passes through (the first offset sits an anchor offset
                // before the header), and for the clamp itself, which is found in
                // f64 within the last item's stride.
                let strip = snap.strip(axis);
                let steps = strip.len() as f32 + 2.0;
                let reach = end.max(strip[0].offset.abs());
                let stride = last.size + snap.gap(axis);
                let slack = steps * (next_f32(reach) - reach)
                    + (4.0 * f64::EPSILON * f64::from(stride)) as f32;
                assert!(
                    last.end() >= end - slack,
                    "{axis:?}: {anchor:?} leaves {} empty after {last:?}",
                    end - last.end()
                );
            }
        },
    );
}

#[hegel::test(test_cases = 2000)]
fn scroll_to_then_reading_the_thumb_round_trips(tc: TestCase) {
    let rows = tc.draw(axis_config());
    let cols = tc.draw(axis_config());
    let axis = tc.draw(axis());
    let fraction = tc.draw(hegel::one_of!(
        gs::sampled_from(vec![0.0, 0.5, 1.0]),
        gs::floats::<f64>().min_value(0.0).max_value(1.0),
        gs::floats::<f64>()
    ));
    let (w, h) = tc.draw(hegel::tuples!(
        extent(cols, cols.default_size),
        extent(rows, rows.default_size)
    ));
    let end_gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(8.0),
    ));
    check_scroll_to_round_trip([rows, cols], axis, fraction, Viewport::new(w, h), end_gap);
}

/// #246: a body of one f32 step at 0.5 is still a body, so the single
/// column, which does not fit it, scrolls. Measured in f32, the body
/// rounded to nothing and the thumb looked as though it could not move.
#[test]
fn scroll_to_round_trips_over_a_body_one_f32_step_long() {
    let rows = AxisConfig::new(0, 1.0).max_size(1048576.0);
    let cols = AxisConfig::new(1, 1.0)
        .max_size(1048576.0)
        .header(0.5000002);
    let viewport = Viewport::new(1.5000002, 0.0);
    check_scroll_to_round_trip([rows, cols], Axis::Cols, 0.5, viewport, 1.0);
}

/// `ScrollTo` on `axis` of a fresh Grid over `configs` and an identity
/// source, with `end_gap` on that axis, reads back `fraction` from the thumb.
fn check_scroll_to_round_trip(
    configs: [AxisConfig; 2],
    axis: Axis,
    fraction: f64,
    viewport: Viewport,
    end_gap: f32,
) {
    let config = configs[axis.index()];
    // Fractions outside 0..=1 are clamped; NaN is ignored, leaving the fresh
    // Grid at the start.
    let expected = if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    };
    let mut s = Scenario::identity(configs[0], configs[1]);
    s.input(|g| g.set_end_gap(axis, end_gap));
    s.frame(viewport);
    s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    let snap = s.frame(viewport);
    let thumb = snap.thumb(axis).fraction;
    // The End Gap shortens the body the last item may end in.
    let body = f64::from(snap.extent(axis)) - f64::from(snap.header(axis)) - f64::from(end_gap);
    let scrollable = scrollable(snap.count(axis), config, body);
    if scrollable <= 0.0 {
        assert_eq!(thumb, 0.0);
        return;
    }
    if expected == 0.0 || expected == 1.0 {
        assert_eq!(
            thumb, expected,
            "ScrollTo {fraction} must reach the end exactly"
        );
        return;
    }
    // The anchor's offset is f32, rounded toward the start and kept below the
    // sum of item size and Gap, so it lands within two f32 steps of the
    // stride; the rest is f64 rounding.
    let stride = (f64::from(config.default_size) + f64::from(config.gap)).min(f64::from(f32::MAX));
    let stride = stride as f32;
    let step = f64::from(next_f32(stride)) - f64::from(stride);
    let tolerance = (2.0 * step + scrollable * 1.0e-12) / scrollable;
    assert!(
        (thumb - expected).abs() <= tolerance.max(1.0e-12),
        "ScrollTo {fraction} read back {thumb}"
    );
}

/// How far an axis of default-sized items can scroll for a body length: until
/// the last item ends the body, or with no body until it starts it.
fn scrollable(count: u64, config: AxisConfig, body: f64) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let stride = f64::from(config.default_size) + f64::from(config.gap);
    let last_start = count.saturating_sub(1) as f64 * stride;
    if body <= 0.0 {
        return last_start;
    }
    (last_start + f64::from(config.default_size) - body).max(0.0)
}

#[hegel::test(test_cases = 1000)]
fn a_default_size_that_is_not_positive_and_finite_is_rejected(tc: TestCase) {
    let bad = tc.draw(hegel::one_of!(
        gs::floats::<f32>().max_value(0.0),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY]),
    ));
    let axis = tc.draw(axis());
    let (rows, cols) = match axis {
        Axis::Rows => (AxisConfig::new(1, bad), AxisConfig::new(1, 1.0)),
        Axis::Cols => (AxisConfig::new(1, 1.0), AxisConfig::new(1, bad)),
    };
    let built = std::panic::catch_unwind(|| Grid::new(rows, cols));
    assert!(
        built.is_err(),
        "Grid accepted default size {bad} on {axis:?}"
    );
}

#[hegel::test(test_cases = 1000)]
fn a_gap_that_is_not_finite_and_non_negative_is_rejected(tc: TestCase) {
    let bad = tc.draw(hegel::one_of!(
        gs::floats::<f32>().max_value(-f32::MIN_POSITIVE),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY]),
    ));
    let axis = tc.draw(axis());
    let (rows, cols) = match axis {
        Axis::Rows => (AxisConfig::new(1, 1.0).gap(bad), AxisConfig::new(1, 1.0)),
        Axis::Cols => (AxisConfig::new(1, 1.0), AxisConfig::new(1, 1.0).gap(bad)),
    };
    let built = std::panic::catch_unwind(|| Grid::new(rows, cols));
    assert!(built.is_err(), "Grid accepted Gap {bad} on {axis:?}");
}

#[hegel::test(test_cases = 2000)]
fn focus_is_unplaced_or_below_the_count_on_both_axes(tc: TestCase) {
    frames(&tc, ALL, |snap, _| {
        let ends = Axis::ALL.map(|axis| snap.focus(axis));
        assert_eq!(
            ends[0].is_some(),
            ends[1].is_some(),
            "focus placed on one axis only: {ends:?}"
        );
        for axis in Axis::ALL {
            if let Some(end) = snap.focus(axis) {
                assert!(
                    end.position < snap.count(axis),
                    "{axis:?}: {end:?} past the count {}",
                    snap.count(axis)
                );
                // The identity source's Key for a Position is the Position.
                assert!(
                    end.key.is_none_or(|k| k == Key(end.position)),
                    "{axis:?}: {end:?}"
                );
            }
        }
    });
}

/// A focus move not overridden by a later scroll shows the focused cell: its
/// item on each axis is in the Strip with its Key, and lies wholly inside the
/// body when it fits there.
#[hegel::test(test_cases = 2000)]
fn after_a_focus_move_the_focused_cell_lies_within_the_viewport(tc: TestCase) {
    frames(&tc, ALL, |snap, step| {
        if !step.reveals_focus {
            return;
        }
        let placed = Axis::ALL.iter().all(|&a| snap.count(a) > 0);
        assert_eq!(snap.focus(Axis::Rows).is_some(), placed);
        for axis in Axis::ALL {
            let Some(end) = snap.focus(axis) else {
                return;
            };
            let (start, stop) = body(snap, axis);
            let model = &step.models[axis.index()];
            let strip = snap.strip(axis);
            // Strip offsets step in f32, each step rounding by up to one f32
            // step at the far edge; items thinner than that drift can be laid
            // out past the item the anchor shows.
            let drift = (strip.len() as f32 + 2.0) * (next_f32(stop) - stop);
            // The drawn size ranges spread at most 2^20, so one f32 step of
            // the anchor's stride is always below the smallest size and the
            // anchor's rounding never skips an item; a host spreading its range
            // past 2^23 loses that.
            let resolvable = model.smallest() >= drift;
            if start >= stop || !resolvable {
                continue;
            }
            let item = strip.iter().find(|i| i.position == end.position);
            let Some(item) = item else {
                panic!("{axis:?}: focused {end:?} not in the Strip {strip:?}");
            };
            assert_eq!(item.key, Some(Key(end.position)), "{axis:?}: {item:?}");
            assert_eq!(end.key, item.key, "{axis:?}: focus Key not completed");
            if item.size <= stop - start {
                // Slack for f32 stepping across the Strip, and for the anchor
                // offset, rounded toward the start by up to one f32 step of
                // the anchor item's stride. The anchor may sit in an item far
                // larger than the focused one, so it is that stride that counts.
                let anchor = snap.scroll_anchor(axis).position;
                let anchor_size = seen_size(snap, axis, model, anchor);
                let stride = (anchor_size + snap.gap(axis)).min(f32::MAX);
                let slack = drift + (next_f32(stride) - stride);
                assert!(
                    item.offset >= start - slack && item.end() <= stop + slack,
                    "{axis:?}: {item:?} not inside the body {start}..{stop}"
                );
            } else {
                assert_eq!(
                    item.offset, start,
                    "{axis:?}: {item:?} longer than the body does not start it"
                );
            }
        }
        if snap.focus(Axis::Rows).is_some() {
            assert!(snap.focus_rect().is_some() || !resolvable_body(snap, step));
        }
    });
}

/// #159: revealing a row past a Gap of 2^18 anchors in the item before it,
/// where an f32 offset steps by 1/8. The row may sit one such step off, no
/// more: the anchor's offset is bounded by the item's stride in f64, not by
/// that stride rounded to f32.
#[test]
fn revealing_focus_across_a_huge_gap_misplaces_the_row_by_one_f32_step_at_most() {
    let range = |c: AxisConfig| c.min_size(1.0).max_size(1048576.0);
    let rows = range(AxisConfig::new(2, 1.0)).gap(262144.06);
    let cols = range(AxisConfig::new(1, 1.0));
    // Row 0 is raised to the hint; row 1's size is unknown, so default.
    let content = Content {
        sizes: vec![1.0, 0.0],
        hint: 786433.0,
    };
    let source = Sized {
        table: IdentityTable {
            rows: IdentitySource::new(2),
            cols: IdentitySource::new(1),
        },
        content: [Some(content), None],
        windows: [0..0, 0..0],
        requests: Rc::new(RefCell::new(Vec::new())),
    };
    let mut s = Scenario::new(Grid::new(rows, cols), source);
    for forward in [false, true] {
        s.input(|g| {
            g.action(Action::MoveFocus {
                axis: Axis::Rows,
                by: FocusStep::Item,
                forward,
                extend: false,
            })
        });
    }
    let v = Viewport::new(1.0, 1.03125);
    let snap = s.frame(v);
    let item = snap
        .strip(Axis::Rows)
        .iter()
        .find(|i| i.position == 1)
        .copied();
    let item = item.expect("the focused row is not in the Strip");
    let stride = 786433.0f32 + 262144.06;
    let step = next_f32(stride) - stride;
    assert!(
        item.offset >= -step && item.end() <= v.height + step,
        "{item:?} not inside the body 0..{}",
        v.height
    );
}

/// Whether both axes have a body and sizes f32 offsets place precisely, so a
/// Strip can show the focused cell (see the property above).
fn resolvable_body(snap: &Snapshot, step: &Step) -> bool {
    Axis::ALL.iter().all(|&axis| {
        let (start, stop) = body(snap, axis);
        let model = &step.models[axis.index()];
        let drift = (snap.strip(axis).len() as f32 + 2.0) * (next_f32(stop) - stop);
        start < stop && model.smallest() >= drift
    })
}

/// Each drain holds at most one `FocusChanged`, naming the cell focused when
/// drained; and once a frame shows the focused cell's Keys, the host has heard
/// of them by the next drain unless focus moved again.
#[hegel::test(test_cases = 2000)]
fn focus_changed_comes_once_per_drain_and_names_the_focused_cell(tc: TestCase) {
    let mut shown: Option<(Key, Key)> = None;
    let mut heard: Option<(Key, Key)> = None;
    frames(&tc, ALL, |snap, step| {
        let changes: Vec<(Key, Key)> = step
            .drained
            .iter()
            .filter_map(|e| match *e {
                Effect::FocusChanged { row, col } => Some((row, col)),
                _ => None,
            })
            .collect();
        assert!(changes.len() <= 1, "{changes:?} in one drain");
        let focused = snap
            .focus(Axis::Rows)
            .zip(snap.focus(Axis::Cols))
            .map(|(r, c)| (Key(r.position), Key(c.position)));
        if let Some(&change) = changes.first() {
            assert_eq!(Some(change), focused, "FocusChanged names another cell");
            assert_ne!(Some(change), heard, "the host already heard of this cell");
            heard = Some(change);
        }
        if !step.moved_focus && shown.is_some() {
            assert_eq!(heard, shown, "the host never heard of the focused cell");
        }
        let keys = snap.focus(Axis::Rows).zip(snap.focus(Axis::Cols));
        shown = keys.and_then(|(r, c)| r.key.zip(c.key));
    });
}

/// Focus moves land where the spec says, on a fresh Grid where nothing else
/// scrolls: the first move takes the first visible cell (the Scroll Anchor,
/// at the start) or goes to its edge; an item move steps one, clamped; an
/// edge move goes to the first or last item; a page move goes at least one
/// item that way, to the item a body's length on (no nearer than a body
/// less one item, no further than a body's length of the smallest items),
/// or to the edge.
#[hegel::test(test_cases = 2000)]
fn focus_moves_land_one_item_a_page_or_an_edge_away(tc: TestCase) {
    let rows = tc.draw(axis_config());
    let cols = tc.draw(axis_config());
    let (w, h) = tc.draw(hegel::tuples!(
        extent(cols, cols.default_size),
        extent(rows, rows.default_size)
    ));
    let viewport = Viewport::new(w, h);
    let mut s = Scenario::identity(rows, cols);
    let mut snap_body = [0.0f32; 2];
    {
        let snap = s.frame(viewport);
        for axis in Axis::ALL {
            snap_body[axis.index()] = snap.extent(axis) - snap.header(axis);
        }
    }
    let counts = [rows.count, cols.count];
    let mut model: Option<[u64; 2]> = None;
    let moves = tc.draw(
        gs::vecs(hegel::tuples!(
            axis(),
            gs::sampled_from(vec![FocusStep::Item, FocusStep::Page, FocusStep::Edge]),
            gs::booleans()
        ))
        .min_size(1)
        .max_size(8),
    );
    for (axis, by, forward) in moves {
        s.input(|g| {
            g.action(Action::MoveFocus {
                axis,
                by,
                forward,
                extend: false,
            })
        });
        let snap = s.frame(viewport);
        let got = snap
            .focus(Axis::Rows)
            .zip(snap.focus(Axis::Cols))
            .map(|(r, c)| [r.position, c.position]);
        if counts.contains(&0) {
            assert_eq!(got, None, "focus placed in an empty table");
            continue;
        }
        let i = axis.index();
        let last = counts[i] - 1;
        let Some(before) = model else {
            let mut expected = [0, 0];
            if by == FocusStep::Edge && forward {
                expected[i] = last;
            }
            assert_eq!(got, Some(expected), "first {by:?} move on {axis:?}");
            model = got;
            continue;
        };
        let got = got.expect("focus became unplaced");
        assert_eq!(got[1 - i], before[1 - i], "the other axis moved");
        let from = before[i];
        match by {
            FocusStep::Item if forward => assert_eq!(got[i], (from + 1).min(last)),
            FocusStep::Item => assert_eq!(got[i], from.saturating_sub(1)),
            FocusStep::Edge => assert_eq!(got[i], if forward { last } else { 0 }),
            FocusStep::Page => {
                let config = [rows, cols][i];
                let stride = f64::from(config.default_size) + f64::from(config.gap);
                let body = f64::from(snap_body[i].max(0.0));
                let most = (body / stride).ceil() as u64 + 1;
                let moved = got[i].abs_diff(from);
                let edge = if forward { last } else { 0 };
                // The landing item holds the point a body's length on, so
                // it starts less than one stride (its size and Gap) before
                // that point, unless the edge came first.
                let far_enough = moved as f64 * stride >= body - stride || got[i] == edge;
                assert!(
                    (if forward {
                        got[i] >= from
                    } else {
                        got[i] <= from
                    }) && (moved >= 1 || from == edge)
                        && (moved <= most.max(1))
                        && far_enough,
                    "page {forward} from {from} landed on {} (body {body}, stride {stride})",
                    got[i],
                );
            }
        }
        model = Some(got);
    }
}

/// The midpoint of the visible part of `item` along `axis`, if that part is
/// wide enough that f32 placement of the Strip cannot move it onto a
/// neighbour: the Strip steps in f32 from the anchor, each step rounding by up
/// to one f32 step at the far edge, while hit testing measures in f64.
fn clickable(snap: &Snapshot, axis: Axis, item: &binnacle_core::StripItem) -> Option<f32> {
    let (start, stop) = body(snap, axis);
    let lo = item.offset.max(start);
    let hi = item.end().min(stop);
    let reach = stop.max(snap.strip(axis)[0].offset.abs());
    let drift = (snap.strip(axis).len() as f32 + 4.0) * (next_f32(reach) - reach);
    (hi - lo > 4.0 * drift && hi > lo).then(|| lo + (hi - lo) / 2.0)
}

/// Hit testing agrees with the Strips: a click in the middle of the visible
/// part of any cell focuses exactly that cell, whatever the sizes, Gaps and
/// scroll (the prototype's hit test was off by one with row Gap 0).
#[hegel::test(test_cases = 2000)]
fn a_click_inside_a_visible_cell_focuses_that_cell(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_a_body()));
    let viewport = Viewport::new(cols.1, rows.1);
    let (rows, cols) = (rows.0, cols.0);
    let mut s = Scenario::identity(rows, cols);
    s.frame(viewport);
    for axis in Axis::ALL {
        let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
        s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    }
    let picked = {
        let snap = s.frame(viewport);
        let (r, c) = (snap.rows(), snap.cols());
        if r.is_empty() || c.is_empty() {
            return;
        }
        let row = r[tc.draw(gs::integers::<usize>().max_value(r.len() - 1))];
        let col = c[tc.draw(gs::integers::<usize>().max_value(c.len() - 1))];
        let at = clickable(&snap, Axis::Cols, &col).zip(clickable(&snap, Axis::Rows, &row));
        at.map(|xy| (xy, [row.position, col.position]))
    };
    let Some(((x, y), cell)) = picked else {
        return;
    };
    s.input(|g| g.pointer(PointerEvent::down(x, y)));
    let snap = s.frame(viewport);
    let focus = Axis::ALL.map(|a| snap.focus(a).map(|f| f.position));
    assert_eq!(
        focus,
        cell.map(Some),
        "click at {x},{y} missed the cell it is inside"
    );
}

/// An axis with items, a finite header band and a viewport extent leaving a
/// body past it, of up to [`MAX_VISIBLE`] items of the axis's smallest size:
/// the viewport outlasts gestures that shrink an item to that size.
#[hegel::composite]
fn axis_with_a_body(tc: &TestCase) -> (AxisConfig, f32) {
    let config = tc.draw(axis_with_items());
    (config, tc.draw(body_extent(config, config.min_size)))
}

/// A press that lands in a Gap between cells, in the header corner, or
/// outside the items leaves focus and selection alone (a Gap is part of
/// layout and hit testing but belongs to no cell).
#[hegel::test(test_cases = 2000)]
fn a_click_on_no_cell_changes_nothing(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_a_body()));
    let viewport = Viewport::new(cols.1, rows.1);
    let mut s = Scenario::identity(rows.0, cols.0);
    s.frame(viewport);
    s.input(|g| {
        g.action(Action::MoveFocus {
            axis: Axis::Rows,
            by: FocusStep::Item,
            forward: true,
            extend: false,
        })
    });
    let (before, point) = {
        let snap = s.frame(viewport);
        let seen = (Axis::ALL.map(|a| snap.focus(a)), seen_selection(&snap));
        // A point in a Gap after a visible item, or beyond the last one.
        let axis = tc.draw(axis());
        let strip = snap.strip(axis);
        let (start, stop) = body(&snap, axis);
        // How far f32 Strip ends can sit from the true item ends.
        let drift = || {
            let reach = stop.max(strip[0].offset.abs());
            (strip.len() as f32 + 4.0) * (next_f32(reach) - reach)
        };
        let mut misses: Vec<f32> = strip
            .iter()
            .filter_map(|i| {
                let (lo, hi) = (i.end().max(start), (i.end() + snap.gap(axis)).min(stop));
                (hi - lo > 4.0 * drift()).then(|| lo + (hi - lo) / 2.0)
            })
            .collect();
        misses.push(-1.0);
        misses.push(stop + 1.0);
        if let Some(last) = strip
            .last()
            .filter(|l| l.position + 1 == snap.count(axis) && stop - l.end() > 4.0 * drift())
        {
            misses.push(last.end() + (stop - last.end()) / 2.0);
        }
        let v = misses[tc.draw(gs::integers::<usize>().max_value(misses.len() - 1))];
        let other = tc.draw(coord());
        let across = Pointer {
            event: PointerEvent::down(0.0, 0.0),
            at: [other, other],
        }
        .resolve(&Target::of(&snap));
        let point = match axis {
            Axis::Rows => (across.x, v),
            Axis::Cols => (v, across.y),
        };
        (seen, point)
    };
    let modifiers = tc.draw(modifiers());
    s.input(|g| {
        g.pointer(PointerEvent::down(point.0, point.1).modifiers(modifiers));
        g.pointer(PointerEvent::up(point.0, point.1).modifiers(modifiers));
    });
    let snap = s.frame(viewport);
    let after = (Axis::ALL.map(|a| snap.focus(a)), seen_selection(&snap));
    assert_eq!(after, before, "press at {point:?} acted");
}

/// What the host can see of the selection: the range and each axis's picks.
fn seen_selection(snap: &Snapshot) -> (Option<CellRange>, [Vec<Key>; 2]) {
    let picks = Axis::ALL.map(|a| snap.picked(a).to_vec());
    (snap.selection_range().cloned(), picks)
}

/// Picks are sorted and distinct, the range is not empty and inside the
/// counts, and `DraggingSelection` always captures the pointer.
#[hegel::test(test_cases = 2000)]
fn the_selection_is_well_formed(tc: TestCase) {
    frames(&tc, BODIES, |snap, _| {
        let (range, picks) = seen_selection(snap);
        for picks in &picks {
            assert!(picks.windows(2).all(|w| w[0] < w[1]), "{picks:?}");
        }
        if let Some(r) = &range {
            for axis in Axis::ALL {
                // Runs sorted, apart and not empty, inside the count.
                let span = r.span(axis);
                assert!(!span.is_empty(), "{axis:?}: {r:?}");
                assert!(
                    span.iter()
                        .all(|s| s.start < s.end && s.end <= snap.count(axis)),
                    "{axis:?}: {r:?}"
                );
                assert!(
                    span.windows(2).all(|w| w[0].end < w[1].start),
                    "{axis:?}: {r:?}"
                );
            }
        }
        if snap.mode() == Mode::DraggingSelection {
            tc.event("dragging");
            assert!(snap.capturing());
        }
    });
}

/// Every visible cell's `selected` flag says whether it is in the range or in
/// a picked row or column, and the fill rects cover exactly the selected
/// cells.
#[hegel::test(test_cases = 2000)]
fn cell_state_and_fill_rects_agree_with_the_range_and_picks(tc: TestCase) {
    frames(&tc, BODIES, |snap, _| {
        let (range, [row_picks, col_picks]) = seen_selection(snap);
        if !row_picks.is_empty() {
            tc.event("row picks");
        }
        if !col_picks.is_empty() {
            tc.event("column picks");
        }
        if range.is_some() {
            tc.event("range");
        }
        let rects = snap.selection_rects();
        for row in snap.rows() {
            for col in snap.cols() {
                let in_range = range
                    .as_ref()
                    .is_some_and(|r| r.contains(row.position, col.position));
                let picked = row.key.is_some_and(|k| row_picks.contains(&k))
                    || col.key.is_some_and(|k| col_picks.contains(&k));
                let selected = snap.cell_state(row, col).selected;
                assert_eq!(
                    selected,
                    in_range || picked,
                    "cell {},{}",
                    row.position,
                    col.position
                );
                let (Some(x), Some(y)) = (
                    clickable(snap, Axis::Cols, col),
                    clickable(snap, Axis::Rows, row),
                ) else {
                    continue;
                };
                let filled = rects
                    .iter()
                    .any(|f| x >= f.x && x < f.x + f.width && y >= f.y && y < f.y + f.height);
                assert_eq!(
                    filled, selected,
                    "cell {},{} at {x},{y} in {rects:?}",
                    row.position, col.position
                );
            }
        }
    });
}

/// Each drain holds at most one `SelectionChanged`, and any change the host
/// can see between frames was announced in the drain before the later one.
#[hegel::test(test_cases = 2000)]
fn selection_changed_comes_once_per_drain_and_for_every_change(tc: TestCase) {
    let mut last: Option<(Option<CellRange>, [Vec<Key>; 2])> = None;
    frames(&tc, BODIES, |snap, step| {
        let announced = step
            .drained
            .iter()
            .filter(|e| **e == Effect::SelectionChanged)
            .count();
        assert!(announced <= 1, "{:?}", step.drained);
        let seen = seen_selection(snap);
        if let Some(before) = last.as_ref().filter(|b| **b != seen) {
            assert_eq!(announced, 1, "{before:?} became {seen:?} unannounced");
        }
        last = Some(seen);
    });
}

/// `Cancel` during a press, drag or resize restores focus, range, picks,
/// Size Overrides and band thicknesses from before the press and ends the gesture; the release
/// that follows changes nothing.
#[hegel::test(test_cases = 2000)]
fn cancel_restores_the_state_from_before_the_gesture(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_a_body()));
    let viewport = Viewport::new(cols.1, rows.1);
    let mut grid = Grid::new(rows.0, cols.0);
    configure(&tc, &mut grid);
    let table = IdentityTable {
        rows: IdentitySource::new(rows.0.count),
        cols: IdentitySource::new(cols.0.count),
    };
    let mut s = Scenario::new(grid, table);
    let mut target = Target::of(&s.frame(viewport));
    // Focus, selection and the Size Overrides of `shown` Keys.
    let seen = |s: &mut Scenario<IdentityTable>, shown: &[Range<Position>; 2]| {
        let snap = s.frame(viewport);
        let focus = Axis::ALL.map(|a| snap.focus(a));
        let selection = seen_selection(&snap);
        let headers = Axis::ALL.map(|a| snap.header(a));
        let sizes: Vec<_> = Axis::ALL
            .into_iter()
            .flat_map(|a| shown[a.index()].clone().map(move |p| (a, Key(p))))
            .map(|(a, k)| s.grid.size_override(a, k))
            .collect();
        (focus, selection, (headers, sizes))
    };
    for input in tc.draw(gs::vecs(input()).max_size(6)) {
        match input {
            Input::Action(action) => s.input(|g| g.action(action)),
            Input::Pointer(p) => s.input(|g| g.pointer(p.resolve(&target))),
            Input::Notice(axis) => {
                s.input(|g| g.notify(axis, binnacle_core::ChangeNotice::Reordered))
            }
            Input::SetFocus(row, col) => s.input(|g| g.set_focus(row, col)),
            Input::Edit(Some((row, col))) => s.input(|g| g.edit_started(row, col)),
            Input::Edit(None) => s.input(|g| g.edit_ended(binnacle_core::EditEnd::Commit)),
            // The resize below needs the drawn counts.
            Input::SetSize(..) | Input::Reset(..) => {}
        }
    }
    // Release any press the setup left held, so the gesture starts fresh.
    s.input(|g| g.pointer(PointerEvent::up(-1.0, -1.0)));
    // The Keys a pointer can resize: those shown and one either side.
    let shown = {
        let snap = s.frame(viewport);
        target = Target::of(&snap);
        Axis::ALL.map(|a| match (snap.strip(a).first(), snap.strip(a).last()) {
            (Some(first), Some(last)) => {
                first.position.saturating_sub(1)..last.position.saturating_add(2)
            }
            _ => 0..0,
        })
    };
    let before = seen(&mut s, &shown);
    let mut gesture = tc.draw(gesture());
    gesture.retain(|p| p.event.kind != PointerKind::Up);
    for p in gesture {
        s.input(|g| g.pointer(p.resolve(&target)));
    }
    match s.frame(viewport).mode() {
        Mode::Resizing { .. } => tc.event("resizing"),
        Mode::ResizingBand { .. } => tc.event("resizing a band"),
        _ => {}
    }
    s.input(|g| g.action(Action::Cancel));
    let (focus, selection, sizes) = seen(&mut s, &shown);
    assert_eq!(
        (&selection, &sizes),
        (&before.1, &before.2),
        "Cancel did not restore"
    );
    // A restored focus end saved without its Key may take it from the Window,
    // as the next `set_window` would; the identity source's Key there is the
    // Position.
    for (after, saved) in focus.iter().zip(&before.0) {
        let completed = saved.map(|e| FocusEnd {
            key: e.key.or(Some(Key(e.position))),
            ..e
        });
        assert!(
            after == saved || *after == completed,
            "Cancel did not restore focus: {after:?} for {saved:?}"
        );
    }
    let before = (focus, selection, sizes);
    let snap = s.frame(viewport);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
    let release = tc.draw(hegel::tuples!(coord(), coord()));
    let up = Pointer {
        event: PointerEvent::up(0.0, 0.0),
        at: [release.0, release.1],
    };
    s.input(|g| g.pointer(up.resolve(&target)));
    assert_eq!(seen(&mut s, &shown), before, "the release acted");
}

/// The part of `lo..hi` along `axis` inside the body, as its midpoint, if it
/// is wide enough that f32 placement of the Strip cannot move it (see
/// [`clickable`]).
fn inside_body(snap: &Snapshot, axis: Axis, lo: f32, hi: f32) -> Option<f32> {
    let (start, stop) = body(snap, axis);
    let strip = snap.strip(axis);
    let (lo, hi) = (lo.max(start), hi.min(stop));
    let reach = stop.max(strip.first()?.offset.abs());
    let drift = (strip.len() as f32 + 4.0) * (next_f32(reach) - reach);
    (hi - lo > 4.0 * drift && hi > lo).then(|| lo + (hi - lo) / 2.0)
}

/// A point at `v` along `axis`, halfway across the header band the other
/// axis leads (where that axis's headers are drawn).
fn in_header_band(snap: &Snapshot, axis: Axis, v: f32) -> (f32, f32) {
    let band = snap.header(axis.other()) / 2.0;
    match axis {
        Axis::Rows => (band, v),
        Axis::Cols => (v, band),
    }
}

/// A gesture under one viewport can drag the header band to nothing, handing
/// its length to the body, and resize an item to the smallest size, which
/// the Window then assumes for every item. A drawn viewport still holds at
/// most twice [`MAX_VISIBLE`] items of that size. A huge header once let a
/// band drag show the whole of a 1e15-item axis, and a frame then
/// materialised a Key per item (#255). The seed keeps the check
/// deterministic.
#[hegel::test(seed = Some(255), test_cases = 500)]
fn a_gesture_cannot_open_more_than_a_bounded_body(tc: TestCase) {
    let (config, viewport) = tc.draw(axis_with_a_body());
    let items = f64::from(viewport) / f64::from(config.min_size);
    assert!(
        items <= 2.0 * f64::from(MAX_VISIBLE) + 1.0,
        "{config:?} in {viewport}: {items} items of the smallest size"
    );
}

/// An axis with a body, usually of everyday sizes so several borders show,
/// sometimes any.
#[hegel::composite]
fn axis_with_borders(tc: &TestCase) -> (AxisConfig, f32) {
    if tc.draw(gs::weighted_booleans(0.3)) {
        return tc.draw(axis_with_a_body());
    }
    let config = tc.draw(everyday_axis());
    let body = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(200.0));
    (config, config.header + body)
}

/// An axis of everyday sizes: a few to fifty items or very many, sizes from
/// one unit (the default smallest size) to 20, a Gap of 0, 1 or up to 2, a
/// header band of at least half a unit.
#[hegel::composite]
fn everyday_axis(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(hegel::one_of!(
        gs::integers::<u64>().min_value(1).max_value(50),
        gs::integers::<u64>().min_value(1)
    ));
    let size = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(20.0));
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::just(1.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(2.0)
    ));
    let header = tc.draw(gs::floats::<f32>().min_value(0.5).max_value(5.0));
    AxisConfig::new(count, size).header(header).gap(gap)
}

/// A press on a header border (the Gap after an item, widened by the border
/// grab) resizes that item; a press on a header away from its borders does
/// not. While resizing, the item's size is its start size plus the pointer's
/// travel along the axis, clamped into the axis's size range. A release reports the new size if it changed; `Cancel` restores
/// the start size and reports nothing.
#[hegel::test(test_cases = 2000)]
fn dragging_a_header_border_resizes_its_item_by_the_pointer_travel(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_borders()));
    let viewport = Viewport::new(cols.1, rows.1);
    let grab = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(3.0),
        gs::floats::<f32>().min_value(0.0).max_value(3.0),
        gs::floats::<f32>().min_value(0.0).allow_infinity(false)
    ));
    let mut s = Scenario::identity(rows.0, cols.0);
    s.input(|g| g.set_border_grab(grab));
    s.frame(viewport);
    for axis in Axis::ALL {
        let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
        s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    }
    let axis = tc.draw(axis());
    let (item, interior, border, last) = {
        let snap = s.frame(viewport);
        tc.assume(snap.header(axis.other()) > 0.0);
        // Mostly items whose border shows, sometimes any visible item.
        let (start, stop) = body(&snap, axis);
        let ending: Vec<_> = (snap.strip(axis).iter())
            .filter(|i| i.end() >= start && i.end() < stop)
            .copied()
            .collect();
        let strip = if ending.is_empty() || tc.draw(gs::weighted_booleans(0.2)) {
            snap.strip(axis).to_vec()
        } else {
            ending
        };
        tc.assume(!strip.is_empty());
        let item = strip[tc.draw(gs::integers::<usize>().max_value(strip.len() - 1))];
        let gap = snap.gap(axis);
        let edge = grab.min(item.size);
        let interior = inside_body(&snap, axis, item.offset + grab, item.end() - grab);
        // The Gap after the item, or the grab's reach inside its end.
        let zones: Vec<f32> = [
            inside_body(&snap, axis, item.end(), item.end() + gap),
            inside_body(&snap, axis, item.end() - edge, item.end()),
        ]
        .into_iter()
        .flatten()
        .collect();
        // Within a grab of the band's end, the band's edge wins.
        let band_end = snap.header(axis) + grab;
        let zones: Vec<f32> = zones.into_iter().filter(|&v| v >= band_end).collect();
        let interior = interior.filter(|&v| v >= band_end);
        let border = (!zones.is_empty())
            .then(|| zones[tc.draw(gs::integers::<usize>().max_value(zones.len() - 1))]);
        let last = item.position + 1 == snap.count(axis);
        let point = |v: Option<f32>| v.map(|v| in_header_band(&snap, axis, v));
        (item, point(interior), point(border), last)
    };
    // More than a grab inside both edges is the header itself.
    if let Some((x, y)) = interior {
        tc.event("interior press");
        s.input(|g| g.pointer(PointerEvent::down(x, y)));
        let mode = s.frame(viewport).mode();
        assert_eq!(mode, Mode::Idle, "a press inside {item:?} resized");
        s.input(|g| g.pointer(PointerEvent::up(x, y)));
        s.frame(viewport);
    }
    let Some((x0, y0)) = border else {
        return;
    };
    tc.event("border press");
    if last {
        tc.event("last item");
    }
    s.pump();
    let key = item.key.expect("identity source fills every Position");
    s.input(|g| g.pointer(PointerEvent::down(x0, y0)));
    let snap = s.frame(viewport);
    assert_eq!(
        snap.mode(),
        Mode::Resizing {
            axis,
            key,
            start_size: item.size
        },
        "press at {x0},{y0} on the border of {item:?}"
    );
    assert!(snap.capturing());
    let start = item.size;
    let mut size = start;
    for travel in tc.draw(
        gs::vecs(hegel::one_of!(
            gs::floats::<f32>().min_value(-50.0).max_value(50.0),
            gs::floats::<f32>().allow_nan(false).allow_infinity(false)
        ))
        .min_size(1)
        .max_size(4),
    ) {
        let (x, y) = match axis {
            Axis::Rows => (x0, y0 + travel),
            Axis::Cols => (x0 + travel, y0),
        };
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let moved = match axis {
            Axis::Rows => f64::from(y) - f64::from(y0),
            Axis::Cols => f64::from(x) - f64::from(x0),
        };
        let config = [rows.0, cols.0][axis.index()];
        size = (f64::from(start) + moved)
            .clamp(f64::from(config.min_size), f64::from(config.max_size)) as f32;
        s.input(|g| g.pointer(PointerEvent::moved(x, y)));
        let expected = (size != start).then_some(size);
        assert_eq!(s.grid.size_override(axis, key), expected, "after {travel}");
        check_resize_feedback(&tc, &s.frame(viewport), axis, key);
    }
    if tc.draw(gs::booleans()) {
        s.input(|g| g.pointer(PointerEvent::up(x0, y0)));
        let drained: Vec<Effect> = s.effects();
        tc.event(if size != start {
            "resized"
        } else {
            "unchanged"
        });
        let reported = (size != start).then_some(Effect::SizeChanged { axis, key, size });
        assert_eq!(drained, Vec::from_iter(reported));
        assert_eq!(
            s.grid.size_override(axis, key),
            (size != start).then_some(size)
        );
    } else {
        s.input(|g| g.action(Action::Cancel));
        assert_eq!(s.pump(), vec![]);
        assert_eq!(s.grid.size_override(axis, key), None);
    }
    let snap = s.frame(viewport);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
}

/// A press on a Header Band's edge (in the other band, the band's last Gap's
/// length widened by the border grab either side) resizes the band; a press
/// deeper in the band does not, and a band of thickness 0 has no edge. While
/// resizing, the thickness is the start thickness plus the pointer's travel
/// along the axis, clamped into the axis's size range. A release reports the
/// new thickness if it changed; `Cancel` restores the start thickness and
/// reports nothing.
#[hegel::test(test_cases = 2000)]
fn dragging_a_band_edge_resizes_the_band_by_the_pointer_travel(tc: TestCase) {
    // Everyday axes: a band shrunk from a huge thickness would leave a body
    // of millions of items to fetch each frame.
    let [rows, cols] = [0, 1].map(|_| {
        let max = tc.draw(hegel::one_of!(
            gs::just(AxisMeasure::DEFAULT_MAX_SIZE),
            gs::floats::<f32>()
                .min_value(1.0)
                .max_value(AxisMeasure::DEFAULT_MAX_SIZE)
        ));
        let config = tc.draw(everyday_axis()).max_size(max);
        let body = tc.draw(gs::floats::<f32>().min_value(1.0).max_value(200.0));
        (config, config.header + body)
    });
    let viewport = Viewport::new(cols.1, rows.1);
    let grab = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(3.0),
        gs::floats::<f32>().min_value(0.0).allow_infinity(false)
    ));
    let axis = tc.draw(axis());
    let mut s = Scenario::identity(rows.0, cols.0);
    s.input(|g| g.set_border_grab(grab));
    if tc.draw(gs::weighted_booleans(0.1)) {
        s.input(|g| g.set_header(axis, 0.0));
    }
    let snap = s.frame(viewport);
    let (h, gap, extent) = (snap.header(axis), snap.gap(axis), snap.extent(axis));
    let across_band = snap.header(axis.other());
    tc.assume(across_band > 0.0);
    // Where the row-number band's edge reaches into the corner, it wins.
    let other_edge = {
        let o = axis.other();
        let ho = snap.header(o);
        (ho > 0.0).then(|| (ho - snap.gap(o) - grab).max(0.0)..(ho + grab))
    };
    let across = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)) * across_band;
    tc.assume(across < across_band);
    if axis == Axis::Rows {
        tc.assume(!other_edge.is_some_and(|e| e.contains(&across)));
    }
    let point = |v: f32| match axis {
        Axis::Rows => (across, v),
        Axis::Cols => (v, across),
    };
    let (lo, hi) = ((h - gap - grab).max(0.0), (h + grab).min(extent));
    let resizing_band = |mode| matches!(mode, Mode::ResizingBand { axis: a, .. } if a == axis);
    // Deeper in the band than its edge reaches.
    if lo > 0.0 {
        let v = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0)) * lo;
        if v < lo {
            tc.event("press inside the band");
            let (x, y) = point(v);
            s.input(|g| g.pointer(PointerEvent::down(x, y)));
            assert!(
                !resizing_band(s.frame(viewport).mode()),
                "press at {v} of {h}"
            );
            s.input(|g| g.pointer(PointerEvent::up(x, y)));
            s.frame(viewport);
            s.pump();
        }
    }
    let f = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0));
    let v0 = lo + f * (hi - lo);
    tc.assume(v0 >= lo && v0 < hi);
    let (x0, y0) = point(v0);
    s.input(|g| g.pointer(PointerEvent::down(x0, y0)));
    let snap = s.frame(viewport);
    if h == 0.0 {
        tc.event("no band");
        assert!(
            !resizing_band(snap.mode()),
            "a 0-thickness band was grabbed"
        );
        return;
    }
    tc.event("edge press");
    assert_eq!(
        snap.mode(),
        Mode::ResizingBand {
            axis,
            start_thickness: h
        },
        "press at {x0},{y0} on the edge of the band {h} thick"
    );
    check_band_feedback(&snap, axis);
    let config = [rows.0, cols.0][axis.index()];
    let mut thickness = h;
    for travel in tc.draw(
        gs::vecs(hegel::one_of!(
            gs::floats::<f32>().min_value(-50.0).max_value(50.0),
            gs::floats::<f32>().allow_nan(false).allow_infinity(false)
        ))
        .min_size(1)
        .max_size(4),
    ) {
        let (x, y) = match axis {
            Axis::Rows => (x0, y0 + travel),
            Axis::Cols => (x0 + travel, y0),
        };
        if !(x.is_finite() && y.is_finite()) {
            continue;
        }
        let moved = match axis {
            Axis::Rows => f64::from(y) - f64::from(y0),
            Axis::Cols => f64::from(x) - f64::from(x0),
        };
        thickness = if moved == 0.0 {
            h
        } else {
            (f64::from(h) + moved).clamp(f64::from(config.min_size), f64::from(config.max_size))
                as f32
        };
        s.input(|g| g.pointer(PointerEvent::moved(x, y)));
        let snap = s.frame(viewport);
        assert_eq!(snap.header(axis), thickness, "after {travel}");
        check_band_feedback(&snap, axis);
    }
    if tc.draw(gs::booleans()) {
        s.input(|g| g.pointer(PointerEvent::up(x0, y0)));
        tc.event(if thickness != h {
            "resized"
        } else {
            "unchanged"
        });
        let reported = (thickness != h).then_some(Effect::BandSizeChanged { axis, thickness });
        assert_eq!(s.effects(), Vec::from_iter(reported));
        assert_eq!(s.frame(viewport).header(axis), thickness);
    } else {
        s.input(|g| g.action(Action::Cancel));
        assert_eq!(s.pump(), vec![]);
        assert_eq!(s.frame(viewport).header(axis), h);
    }
    let snap = s.frame(viewport);
    assert_eq!(snap.mode(), Mode::Idle);
    assert!(!snap.capturing());
}

/// Mode feedback while resizing the band leading `axis`: capturing, the
/// axis's resize cursor, and the guide at the band's end.
fn check_band_feedback(snap: &Snapshot, axis: Axis) {
    assert!(snap.capturing());
    let cursor = match axis {
        Axis::Rows => Cursor::ResizeRow,
        Axis::Cols => Cursor::ResizeCol,
    };
    assert_eq!(snap.cursor(), cursor);
    let h = snap.header(axis);
    let guide = snap.resize_guide().expect("no guide for a band resize");
    assert_eq!(
        (guide.axis, guide.key, guide.offset, guide.size),
        (axis, None, h, h)
    );
}

/// A resize in progress captures the pointer, shows the resize cursor for its
/// axis, and puts its guide line at the resized item's trailing edge wherever
/// the Strip shows that item, or at the end of the band being resized.
#[hegel::test(test_cases = 2000)]
fn a_resize_in_progress_captures_and_guides(tc: TestCase) {
    frames(&tc, BODIES, |snap, _| match snap.mode() {
        Mode::Resizing { axis, key, .. } => {
            tc.event("resizing");
            check_resize_feedback(&tc, snap, axis, key);
        }
        Mode::ResizingBand { axis, .. } => {
            tc.event("resizing a band");
            check_band_feedback(snap, axis);
        }
        _ => assert_eq!(snap.resize_guide(), None),
    });
}

/// Mode feedback while resizing the item with `key` on `axis`: capturing,
/// the axis's resize cursor, and the guide at the item's end in the Strip.
fn check_resize_feedback(tc: &TestCase, snap: &Snapshot, axis: Axis, key: Key) {
    assert!(snap.capturing());
    let cursor = match axis {
        Axis::Rows => Cursor::ResizeRow,
        Axis::Cols => Cursor::ResizeCol,
    };
    assert_eq!(snap.cursor(), cursor);
    let strip = snap.strip(axis);
    let Some(item) = strip.iter().find(|i| i.key == Some(key)) else {
        return;
    };
    tc.event("resized item shown");
    let guide = snap.resize_guide().expect("no guide for a shown item");
    assert_eq!(
        (guide.axis, guide.key, guide.size),
        (axis, Some(key), item.size)
    );
    // The Strip steps in f32 from the anchor; the guide is placed in f64.
    let (_, stop) = body(snap, axis);
    let reach = stop.max(strip[0].offset.abs()).max(item.end().abs());
    let drift = (strip.len() as f32 + 4.0) * (next_f32(reach) - reach);
    assert!(
        (guide.offset - item.end()).abs() <= drift,
        "{guide:?} is not at the end of {item:?}"
    );
}

/// Every `MoveRequested` names distinct columns and a drop that changes their
/// order. The identity table never moves anything, so a Key is its Position.
#[hegel::test(test_cases = 2000)]
fn move_requests_name_distinct_columns_and_a_drop_that_moves_them(tc: TestCase) {
    frames(&tc, BODIES, |_, step| {
        for effect in step.drained {
            let Effect::MoveRequested { axis, keys, to } = effect else {
                continue;
            };
            tc.event("move requested");
            assert_eq!(*axis, Axis::Cols);
            check_moved_keys(keys);
            let (first, last) = (keys[0].0, keys[keys.len() - 1].0);
            let block = last - first + 1 == keys.len() as u64;
            assert!(*to <= step.release_counts[Axis::Cols.index()]);
            assert!(
                !(block && (first..=last + 1).contains(to)),
                "{keys:?} to {to} moves nothing"
            );
        }
    });
}

/// A header move in progress moves column headers only, captures, shows the
/// move cursor and a drop indicator in the middle of the Gap before the drop
/// Position.
#[hegel::test(test_cases = 2000)]
fn a_header_move_in_progress_shows_its_drop(tc: TestCase) {
    frames(&tc, BODIES, |snap, _| {
        let Mode::MovingHeaders { axis, key, target } = snap.mode() else {
            assert!(snap.moving_keys().is_empty());
            assert_eq!(snap.drop_indicator(), None);
            assert_eq!(snap.pin_zones(), vec![]);
            return;
        };
        tc.event("moving");
        assert!(snap.capturing());
        assert_eq!(snap.cursor(), Cursor::Move);
        let keys = snap.moving_keys();
        check_moved_keys(keys);
        assert!(keys.contains(&key));
        // Nothing is pinned and the pin zones are off, so nothing pins.
        assert_eq!(snap.pin_zones(), vec![]);
        let to = match target {
            // Only the columns are movable.
            DropTarget::Stay => {
                assert_eq!(axis, Axis::Rows);
                assert_eq!(snap.drop_indicator(), None);
                return;
            }
            DropTarget::Move { to, unpin: false } => to,
            other => panic!("nothing pins, yet {other:?}"),
        };
        assert_eq!(axis, Axis::Cols);
        assert!(to <= snap.count(axis));
        let drop = snap
            .drop_indicator()
            .expect("no drop indicator while moving");
        assert_eq!((drop.axis, drop.target), (axis, target));
        let strip = snap.strip(axis);
        let half = snap.gap(axis) / 2.0;
        let at = match strip.iter().find(|i| i.position == to) {
            Some(item) => item.offset - half,
            None => match strip.last() {
                Some(last) if last.position + 1 == to => last.end() + half,
                _ => return,
            },
        };
        tc.event("drop shown");
        // The Strip steps in f32 from the anchor; the indicator is placed in
        // f64.
        let (_, stop) = body(snap, axis);
        let reach = stop.max(strip[0].offset.abs()).max(at.abs());
        let drift = (strip.len() as f32 + 4.0) * (next_f32(reach) - reach);
        assert!(
            (drop.offset - at).abs() <= drift,
            "{drop:?} is not at {at} in {strip:?}"
        );
    });
}

/// Moved Keys are distinct and, in the identity table, in Position order.
fn check_moved_keys(keys: &[Key]) {
    assert!(!keys.is_empty());
    assert!(keys.windows(2).all(|w| w[0].0 < w[1].0), "{keys:?}");
}

/// Dragging a column header and dropping it over another column's first half
/// (or Gap-free start) asks to move it before that column, over its second
/// half or the Gap after it, after it; a drop beside itself asks nothing.
/// The pointer may be in the header band or the body; what a header click
/// does makes no difference.
#[hegel::test(test_cases = 2000)]
fn dropping_a_dragged_column_header_requests_the_move_the_pointer_shows(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_borders()));
    let viewport = Viewport::new(cols.1, rows.1);
    let mut s = Scenario::identity(rows.0, cols.0);
    let click = tc.draw(gs::sampled_from(vec![
        HeaderClick::Sort,
        HeaderClick::Select,
        HeaderClick::None,
    ]));
    let threshold = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(5.0));
    s.input(|g| {
        g.set_header_click(Axis::Cols, click);
        g.set_drag_threshold(threshold);
        g.set_movable(Axis::Cols, true);
        g.set_pin_zone(Axis::Cols, 0.0);
    });
    s.frame(viewport);
    let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
    s.input(|g| {
        g.action(Action::ScrollTo {
            axis: Axis::Cols,
            fraction,
        })
    });
    let snap = s.frame(viewport);
    let strip = snap.cols();
    // Column headers need a band to be pressed in.
    if strip.is_empty() || snap.rows().is_empty() || snap.header(Axis::Rows) <= 0.0 {
        return;
    }
    let pick = |tc: &TestCase| strip[tc.draw(gs::integers::<usize>().max_value(strip.len() - 1))];
    let (grabbed, over) = (pick(&tc), pick(&tc));
    let Some(from) = clickable(&snap, Axis::Cols, &grabbed) else {
        return;
    };
    // Somewhere over `over` or the Gap after it, away from its middle and its
    // edges by more than f32 placement can move them.
    let (start, stop) = body(&snap, Axis::Cols);
    let gap = snap.gap(Axis::Cols);
    let reach = stop.max(strip[0].offset.abs());
    let drift = (strip.len() as f32 + 4.0) * (next_f32(reach) - reach);
    let f = tc.draw(gs::floats::<f32>().min_value(0.0).max_value(1.0));
    let x = over.offset + f * (over.size + gap);
    let mid = over.offset + over.size / 2.0;
    let edges = [over.offset, mid, over.end() + gap];
    if edges.iter().any(|&e| (x - e).abs() <= 4.0 * drift)
        || !(start..stop).contains(&x)
        || (x - from).abs() < threshold
    {
        return;
    }
    let to = over.position + u64::from(x >= mid);
    let y = if tc.draw(gs::booleans()) {
        snap.header(Axis::Rows) / 2.0
    } else {
        let row = snap.rows()[0];
        (row.offset.max(snap.header(Axis::Rows)) + row.end().min(viewport.height)) / 2.0
    };
    let key = grabbed.key.expect("a shown column has a Key");
    let (px, py) = in_header_band(&snap, Axis::Cols, from);
    s.input(|g| {
        g.pointer(PointerEvent::down(px, py));
        g.pointer(PointerEvent::moved(x, y));
    });
    let snap = s.frame(viewport);
    assert_eq!(
        snap.mode(),
        Mode::MovingHeaders {
            axis: Axis::Cols,
            key,
            target: DropTarget::Move { to, unpin: false }
        },
        "dragged from {from} to {x} over {over:?}"
    );
    assert_eq!(snap.moving_keys(), &[key]);
    s.input(|g| g.pointer(PointerEvent::up(x, y)));
    let drained: Vec<Effect> = s.effects();
    let p = grabbed.position;
    if to == p || to == p + 1 {
        tc.event("in place");
        assert_eq!(drained, vec![]);
    } else {
        tc.event("moved");
        assert_eq!(
            drained,
            vec![Effect::MoveRequested {
                axis: Axis::Cols,
                keys: vec![key],
                to
            }]
        );
    }
    assert_eq!(s.frame(viewport).mode(), Mode::Idle);
}

/// Each Window the Grid asks for is proportional to the body over the axis's
/// smallest stride (`min_size` plus the Gap), never to the count or to how
/// small a size was asked for. Exactly: the Window bound steps from the anchor,
/// at most one stride before the header, until it passes the viewport's end,
/// so it spans at most `ceil((body + stride) / step)` items, plus the overscan
/// (2 each side). `step` is the stride less what f32 rounding can lose per
/// item: half an f32 step at the far edge for each addition (the size, then
/// the Gap). At the end of the axis the Window reaches back
/// `ceil(body / stride) + 1` items plus overscan, which is within the same
/// bound.
#[hegel::test(test_cases = 2000)]
fn window_requests_are_bounded_by_the_body_over_the_min_size(tc: TestCase) {
    frames(&tc, ALL, |snap, step| {
        for (axis, range) in step.requests.borrow().iter() {
            let config = step.models[axis.index()].config;
            let (extent, gap) = (snap.extent(*axis), snap.gap(*axis));
            let body = f64::from((extent - snap.header(*axis)).max(0.0));
            let stride = config.min_size + gap;
            let far = (extent + stride).min(f32::MAX);
            let additions = if gap > 0.0 { 2.0 } else { 1.0 };
            let shortfall = additions * f64::from(next_f32(far) - far) / 2.0;
            let item_step = f64::from(stride) - shortfall;
            if item_step <= 0.0 {
                continue;
            }
            let bound = ((body + f64::from(stride)) / item_step).ceil() + 4.0;
            let len = (range.end - range.start) as f64;
            assert!(
                len <= bound,
                "{axis:?}: asked for {range:?} ({len}) with a body of {body}, \
                 min size {} and Gap {gap}: bound {bound}",
                config.min_size
            );
        }
    });
}

/// Every size the Grid lays out lies in the axis's size range, and the Gap is
/// at most its largest size, whatever sizes the inputs and source give.
#[hegel::test(test_cases = 2000)]
fn every_strip_item_size_lies_in_the_size_range(tc: TestCase) {
    frames(&tc, ALL, |snap, step| {
        for axis in Axis::ALL {
            let config = step.models[axis.index()].config;
            let range = config.min_size..=config.max_size;
            for item in snap.strip(axis) {
                assert!(
                    range.contains(&item.size),
                    "{axis:?}: {item:?} outside {range:?}"
                );
            }
            assert!(snap.gap(axis) <= config.max_size, "{axis:?}");
        }
    });
}

/// A default size or Gap outside the size range is clamped into it: every
/// item takes the clamped default and the Gap is capped at the largest size.
#[hegel::test(test_cases = 1000)]
fn a_default_size_or_gap_outside_the_size_range_is_clamped(tc: TestCase) {
    let (min, max) = tc.draw(size_range());
    let any = || {
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .allow_infinity(false)
    };
    let default = tc.draw(any());
    let gap = tc.draw(hegel::one_of!(gs::just(0.0f32), any()));
    let axis = tc.draw(axis());
    let config = AxisConfig::new(5, default)
        .min_size(min)
        .max_size(max)
        .gap(gap);
    let plain = AxisConfig::new(1, 1.0);
    let (rows, cols) = match axis {
        Axis::Rows => (config, plain),
        Axis::Cols => (plain, config),
    };
    let mut s = Scenario::identity(rows, cols);
    // Enough room for the first item at any size in the range.
    let snap = s.frame(Viewport::new(max, max));
    let first = snap.strip(axis)[0];
    assert_eq!(first.size, default.clamp(min, max), "default {default}");
    assert_eq!(snap.gap(axis), gap.min(max), "Gap {gap}");
}

/// A size range whose smallest size is not positive and finite, or whose
/// largest is not finite or below the smallest, is rejected.
#[hegel::test(test_cases = 1000)]
fn a_size_range_that_is_not_positive_finite_and_ordered_is_rejected(tc: TestCase) {
    let (min, max) = tc.draw(hegel::one_of!(
        hegel::tuples!(
            hegel::one_of!(
                gs::floats::<f32>().max_value(0.0),
                gs::sampled_from(vec![f32::NAN, f32::INFINITY])
            ),
            gs::floats::<f32>()
        ),
        hegel::tuples!(
            gs::floats::<f32>().min_value(1.0).max_value(1.0e6),
            gs::floats::<f32>().max_value(0.999)
        ),
        hegel::tuples!(
            gs::floats::<f32>().min_value(1.0).max_value(1.0e6),
            gs::sampled_from(vec![f32::NAN, f32::INFINITY])
        )
    ));
    let config = AxisConfig::new(1, 1.0).min_size(min).max_size(max);
    let built = std::panic::catch_unwind(|| Grid::new(config, AxisConfig::new(1, 1.0)));
    assert!(built.is_err(), "Grid accepted the size range {min}..={max}");
}

/// A `Reset` on either axis keeps every Size Override, on both axes: a Key
/// that comes back keeps its size.
#[hegel::test(test_cases = 1000)]
fn a_reset_keeps_size_overrides_on_both_axes(tc: TestCase) {
    let count = || hegel::one_of!(gs::integers::<u64>().max_value(30), gs::integers::<u64>());
    let mut grid = Grid::new(
        AxisConfig::new(tc.draw(count()), 1.0),
        AxisConfig::new(tc.draw(count()), 1.0),
    );
    let set = hegel::tuples!(
        axis(),
        gs::integers::<u64>().max_value(40).map(Key),
        // Inside the default size range, where a Size Override is kept as set.
        gs::floats::<f32>()
            .min_value(AxisMeasure::DEFAULT_MIN_SIZE)
            .max_value(AxisMeasure::DEFAULT_MAX_SIZE)
    );
    let mut model: BTreeMap<(usize, Key), f32> = BTreeMap::new();
    for (axis, key, size) in tc.draw(gs::vecs(set).max_size(8)) {
        grid.set_size(axis, key, size);
        model.insert((axis.index(), key), size);
    }
    let reset = tc.draw(axis());
    grid.notify(reset, binnacle_core::ChangeNotice::Reset(tc.draw(count())));
    for (&(i, key), &size) in &model {
        let axis = Axis::ALL[i];
        assert_eq!(
            grid.size_override(axis, key),
            Some(size),
            "{axis:?} {key:?}"
        );
    }
}

/// `set_gap` keeps the Scroll Anchor's item where it is: the anchor, and so
/// the item's Strip offset, stays the same. Only two things move it back:
/// an anchor in the Gap past where the new Gap ends is pulled inside its
/// item's new stride, and the end clamp pulls the view back to show the last
/// item. The other axis is untouched. A changed Gap is layout damage.
#[hegel::test(test_cases = 2000)]
fn set_gap_keeps_the_scroll_anchors_item_at_its_offset(tc: TestCase) {
    let [rows, cols] = [0, 1].map(|_| tc.draw(axis_with_borders()));
    let viewport = Viewport::new(cols.1, rows.1);
    let mut s = Scenario::identity(rows.0, cols.0);
    s.frame(viewport);
    for axis in Axis::ALL {
        let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
        s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    }
    let axis = tc.draw(axis());
    let (before, other, first, old_gap) = {
        let snap = s.frame(viewport);
        let first = snap.strip(axis).first().copied();
        (
            snap.scroll_anchor(axis),
            snap.scroll_anchor(axis.other()),
            first,
            snap.gap(axis),
        )
    };
    let config = [rows.0, cols.0][axis.index()];
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::just(1.0f32),
        gs::floats::<f32>().min_value(0.0).max_value(3.0),
        gs::floats::<f32>().min_value(0.0).allow_infinity(false)
    ));
    s.input(|g| g.set_gap(axis, gap));
    let snap = s.frame(viewport);
    let new_gap = gap.min(config.max_size);
    assert_eq!(snap.gap(axis), new_gap);
    assert_eq!(snap.damage().layout, new_gap != old_gap);
    assert_eq!(snap.scroll_anchor(axis.other()), other);
    let after = snap.scroll_anchor(axis);
    if after == before {
        tc.event("anchor kept");
        // An anchor inside its item puts that item first in the Strip.
        if let Some(first) = first.filter(|f| f.position == before.position) {
            let now = snap.strip(axis).first().copied();
            assert_eq!(
                now.map(|i| (i.position, i.offset)),
                Some((first.position, first.offset))
            );
        }
        return;
    }
    assert!(
        (after.position, after.offset) < (before.position, before.offset),
        "{before:?} moved forward to {after:?}"
    );
    // Every item has the default size: nothing set others.
    let size = config.default_size.clamp(config.min_size, config.max_size);
    let past_the_gap = before.offset >= size + new_gap;
    // The end clamp binds: the thumb (from the measure, in f64) is at the
    // end of its travel, or the whole axis fits and nothing scrolls. The
    // Strip cannot say, as f32 placement past a huge header may drop items.
    let thumb = snap.thumb(axis);
    let at_end = thumb.fraction == 1.0 || (thumb.size >= 1.0 && after == ScrollAnchor::default());
    tc.event(if past_the_gap {
        "pulled inside the stride"
    } else {
        "end clamp"
    });
    assert!(
        at_end || (past_the_gap && after.position == before.position),
        "{before:?} moved to {after:?} with the Gap {old_gap} -> {new_gap}"
    );
}

/// A table for the reveal properties: whole-number sizes and Gaps, so f32
/// layout is exact, a viewport with a body, the axis under test, where it is
/// scrolled to (a thumb fraction, then whole units past that item's start),
/// and a Position on it to scroll to.
#[hegel::composite]
fn reveal_case(tc: &TestCase) -> (AxisConfig, Viewport, Axis, (f64, u32), Position) {
    let count = tc.draw(hegel::one_of!(
        gs::integers::<u64>().min_value(1).max_value(50),
        gs::integers::<u64>()
            .min_value(1)
            .max_value(1_000_000_000_000_000),
    ));
    let whole = |lo: u32, hi: u32| {
        gs::integers::<u32>()
            .min_value(lo)
            .max_value(hi)
            .map(|n| n as f32)
    };
    let header = tc.draw(whole(0, 40));
    let config = AxisConfig::new(count, tc.draw(whole(1, 50)))
        .header(header)
        .gap(tc.draw(whole(0, 3)));
    // A body at least one unit long on the axis under test.
    let length = header + tc.draw(whole(1, 2000));
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let v = match axis {
        Axis::Rows => Viewport::new(40.0, length),
        Axis::Cols => Viewport::new(length, 40.0),
    };
    let fraction = tc.draw(gs::floats::<f64>().min_value(0.0).max_value(1.0));
    let units = tc.draw(gs::integers::<u32>().max_value(100));
    let p = tc.draw(gs::integers::<u64>().max_value(count - 1));
    (config, v, axis, (fraction, units), p)
}

/// The scenario a `reveal_case` describes, scrolled and framed; the other
/// axis is small. The scroll lands on a whole unit: a fraction can leave the
/// anchor a sub-f32-step past an item start, which the f32 Strip cannot show.
fn revealing(
    config: AxisConfig,
    v: Viewport,
    axis: Axis,
    (fraction, units): (f64, u32),
) -> Scenario<IdentityTable> {
    let other = AxisConfig::new(3, 10.0);
    let mut s = match axis {
        Axis::Rows => Scenario::identity(config, other),
        Axis::Cols => Scenario::identity(other, config),
    };
    s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    let at = s.frame(v).scroll_anchor(axis).position;
    s.input(|g| g.scroll_to_item(axis, at));
    s.frame(v);
    let (dx, dy) = match axis {
        Axis::Rows => (0.0, units as f32),
        Axis::Cols => (units as f32, 0.0),
    };
    s.input(|g| g.action(Action::Scroll { dx, dy }));
    s.frame(v);
    s
}

/// The item at `p` on `axis`, and the item whose layout a failed lookup reports.
fn item_at(snap: &Snapshot, axis: Axis, p: Position) -> binnacle_core::StripItem {
    match snap.strip(axis).iter().find(|i| i.position == p) {
        Some(item) => *item,
        None => panic!("{p} not in the Strip {:?}", snap.strip(axis)),
    }
}

/// `reveal` scrolls the least distance that shows the item: wholly inside the
/// body when it fits there (else starting the body), and not at all when it
/// is already wholly shown.
#[hegel::test(test_cases = 1000)]
fn reveal_shows_the_item_with_the_least_scroll(tc: TestCase) {
    let (config, v, axis, scroll, p) = tc.draw(reveal_case());
    let mut s = revealing(config, v, axis, scroll);
    let before = s.frame(v);
    let (start, stop) = body(&before, axis);
    let anchor = before.scroll_anchor(axis);
    let shown = before
        .strip(axis)
        .iter()
        .any(|i| i.position == p && i.offset >= start && i.end() <= stop);
    s.input(|g| g.reveal(axis, p));
    let after = s.frame(v);
    if shown {
        assert_eq!(
            after.scroll_anchor(axis),
            anchor,
            "already shown: no scroll"
        );
    }
    let item = item_at(&after, axis, p);
    if item.size <= stop - start {
        assert!(
            item.offset >= start && item.end() <= stop,
            "{item:?} not inside {start}..{stop}"
        );
    } else {
        assert_eq!(
            item.offset, start,
            "{item:?} longer than the body does not start it"
        );
    }
}

/// `reveal` never moves focus, placed or not.
#[hegel::test(test_cases = 500)]
fn reveal_leaves_focus(tc: TestCase) {
    let (config, v, axis, scroll, p) = tc.draw(reveal_case());
    let mut s = revealing(config, v, axis, scroll);
    if tc.draw(gs::booleans()) {
        s.input(|g| g.set_focus(Key(0), Key(0)));
        s.frame(v);
    }
    let focus = Axis::ALL.map(|a| s.frame(v).focus(a));
    s.input(|g| g.reveal(axis, p));
    assert_eq!(Axis::ALL.map(|a| s.frame(v).focus(a)), focus);
}

/// `scroll_to_item` starts the body with the item, unless that would scroll
/// past the end, where the last item ends the body instead.
#[hegel::test(test_cases = 1000)]
fn scroll_to_item_starts_the_body_with_the_item_or_reaches_the_end(tc: TestCase) {
    let (config, v, axis, scroll, p) = tc.draw(reveal_case());
    let mut s = revealing(config, v, axis, scroll);
    s.input(|g| g.scroll_to_item(axis, p));
    let after = s.frame(v);
    let (start, stop) = body(&after, axis);
    let strip = after.strip(axis);
    let item = strip.iter().find(|i| i.position == p);
    let last = strip.last().filter(|i| i.position == after.count(axis) - 1);
    match (item, last) {
        (Some(item), _) if item.offset == start => {}
        (_, Some(last)) => assert!(
            last.end() <= stop && strip.first().is_some_and(|f| f.position <= p),
            "neither starts the body with {p} nor reaches the end: {strip:?}"
        ),
        _ => panic!("{p} does not start the body {start}..{stop}: {strip:?}"),
    }
}

/// Scrolled to the end however it gets there, the last scrolling item ends
/// the End Gap short of the viewport's end, or with a trailing pinned
/// Strip, where the scrolling Strip ends, that Strip ending the End Gap
/// short instead. Sizes are whole quarters, so every sum is exact.
#[hegel::test(test_cases = 1000)]
fn scrolled_to_the_end_the_last_item_ends_the_end_gap_short(tc: TestCase) {
    use binnacle_core::Edge;
    let quarters = |lo, hi| tc.draw(gs::integers::<u16>().min_value(lo).max_value(hi)) as f32 / 4.0;
    let axis = tc.draw(axis());
    let count = tc.draw(gs::integers::<u64>().min_value(1).max_value(40));
    let (size, gap, header) = (quarters(4, 24), quarters(0, 8), quarters(0, 12));
    let end_gap = quarters(0, 12);
    let config = AxisConfig::new(count, size).header(header).gap(gap);
    let other = AxisConfig::new(3, 2.0);
    let extent = quarters(0, 160);
    let viewport = match axis {
        Axis::Rows => Viewport::new(10.0, extent),
        Axis::Cols => Viewport::new(extent, 10.0),
    };
    let mut s = match axis {
        Axis::Rows => Scenario::identity(config, other),
        Axis::Cols => Scenario::identity(other, config),
    };
    let pinned = (count > 1 && tc.draw(gs::booleans()))
        .then(|| tc.draw(gs::integers::<u64>().max_value(count - 1)));
    s.input(|g| {
        g.set_end_gap(axis, end_gap);
        if let Some(key) = pinned {
            g.pin(axis, Edge::Trailing, Key(key));
        }
    });
    let last_scrolling = if pinned == Some(count - 1) {
        count - 2
    } else {
        count - 1
    };
    s.frame(viewport);
    let ways = if pinned.is_some() { 3 } else { 4 };
    let way = tc.draw(gs::integers::<u8>().max_value(ways - 1));
    s.input(|g| match way {
        0 => g.action(Action::Scroll {
            dx: 1.0e6,
            dy: 1.0e6,
        }),
        1 => g.action(Action::ScrollTo {
            axis,
            fraction: 1.0,
        }),
        2 => g.reveal(axis, last_scrolling),
        _ => {
            g.set_focus(Key(0), Key(0));
            g.action(Action::MoveFocus {
                axis,
                by: FocusStep::Edge,
                forward: true,
                extend: false,
            });
        }
    });
    s.frame(viewport);
    let snap = s.frame(viewport);
    let far = (extent - end_gap).max(header.min(extent));
    let trailing = snap.pinned(axis, Edge::Trailing);
    let end = match trailing.last() {
        Some(last) => {
            assert_eq!(last.end(), far, "the trailing pinned Strip's end");
            snap.scrolling(axis).end
        }
        None => far,
    };
    let scrolling = count - u64::from(pinned.is_some());
    let length = scrolling as f32 * size + (scrolling - 1) as f32 * gap;
    let start = snap.scrolling(axis).start;
    // A reveal shows the start of an item longer than the body.
    let reveals = way >= 2;
    if end <= start || length <= end - start || reveals && size >= end - start {
        return;
    }
    let last = snap.strip(axis).last().copied();
    assert_eq!(
        last.map(|it| it.end()),
        Some(end),
        "the last scrolling item's end ({:?} pinned)",
        trailing.first().map(|it| it.position)
    );
}
