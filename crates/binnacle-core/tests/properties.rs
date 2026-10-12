//! Seam 1 properties: a real Grid over an identity Table Source, driven through
//! the frame loop with random tables, scroll inputs, Size Overrides and
//! viewports.

use std::collections::BTreeMap;
use std::ops::Range;

use binnacle_core::{Action, Axis, AxisConfig, Grid, Key, Position, Snapshot, Viewport};
use binnacle_memory::IdentitySource;
use binnacle_testkit::{IdentityTable, Scenario, ScenarioSource};
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
    let size = tc.draw(size_near(header));
    let gap = tc.draw(hegel::one_of!(
        gs::just(0.0f32),
        gs::floats::<f32>().min_value(0.0).allow_infinity(false),
        gs::floats::<f32>().min_value(0.0).max_value(2.0),
    ));
    AxisConfig::new(count, size).header(header).gap(gap)
}

/// Any positive finite size, or one near the f32 spacing at the header, where
/// an offset may not advance.
#[hegel::composite]
fn size_near(tc: &TestCase, header: f32) -> f32 {
    let any_size = gs::floats::<f32>()
        .min_value_exclusive(0.0)
        .allow_infinity(false);
    let scale = if header.is_finite() {
        header.abs().max(1.0)
    } else {
        1.0
    };
    let tiny_size = gs::floats::<f32>()
        .min_value(1.0e-9)
        .max_value(1.0e-5)
        .map(move |f| f * scale);
    tc.draw(hegel::one_of!(any_size, tiny_size))
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
}

#[hegel::composite]
fn input(tc: &TestCase) -> Input {
    let distance = || {
        hegel::one_of!(
            gs::floats::<f32>().min_value(-50.0).max_value(50.0),
            gs::floats::<f32>()
        )
    };
    let key = hegel::one_of!(gs::integers::<u64>().max_value(60), gs::integers::<u64>()).map(Key);
    let size = hegel::one_of!(
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .allow_infinity(false),
        gs::floats::<f32>().min_value(0.5).max_value(20.0),
        gs::floats::<f32>().max_value(0.0),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY]),
    );
    tc.draw(hegel::one_of!(
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
        hegel::tuples!(axis(), key, size).map(|(a, k, s)| Input::SetSize(a, k, s)),
    ))
}

fn index(axis: Axis) -> usize {
    match axis {
        Axis::Rows => 0,
        Axis::Cols => 1,
    }
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
    /// The content size the Grid should use for `key`: raised to the hint, or
    /// unknown when not positive and finite.
    fn size_of(&self, key: Key) -> Option<f32> {
        let s = self.sizes[(key.0 % self.sizes.len() as u64) as usize];
        (s.is_finite() && s > 0.0).then(|| s.max(self.hint))
    }
}

/// An identity Table Source that also answers with content sizes.
struct Sized {
    table: IdentityTable,
    content: [Option<Content>; 2],
}

impl ScenarioSource for Sized {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.table.window(axis, range)
    }

    fn sizes(&mut self, axis: Axis, keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        let c = self.content[index(axis)].as_ref()?;
        let raw = keys
            .iter()
            .map(|k| c.sizes[(k.0 % c.sizes.len() as u64) as usize]);
        Some((raw.collect(), c.hint))
    }
}

/// What the test knows about one axis: its config, the Size Overrides it set
/// and any content sizes. Keys are Positions (identity source), so it knows
/// every item's size.
#[derive(Clone, Debug)]
struct Model {
    config: AxisConfig,
    overrides: BTreeMap<Key, f32>,
    content: Option<Content>,
}

impl Model {
    /// The size of the item at `pos` once its Key is known: Size Override,
    /// else content size, else default.
    fn size_of(&self, pos: Position) -> f32 {
        let key = Key(pos);
        self.overrides
            .get(&key)
            .copied()
            .or_else(|| self.content.as_ref()?.size_of(key))
            .unwrap_or(self.config.default_size)
    }

    fn smallest(&self) -> f32 {
        let hint = self.content.as_ref().map_or(f32::INFINITY, |c| c.hint);
        self.overrides
            .values()
            .fold(self.config.default_size.min(hint), |a, &b| a.min(b))
    }
}

/// Which knobs a run draws beyond the table and scroll inputs.
#[derive(Clone, Copy)]
struct Knobs {
    /// Set Size Overrides.
    resize: bool,
    /// Answer some axes with content sizes.
    content: bool,
}

const ALL: Knobs = Knobs {
    resize: true,
    content: true,
};

/// Draws a table, then a few steps of random inputs each followed by a frame
/// at a random viewport, handing every Snapshot to `check` with the models of
/// both axes.
fn frames(tc: &TestCase, knobs: Knobs, mut check: impl FnMut(&Snapshot, &[Model; 2])) {
    let rows = tc.draw(axis_config());
    let cols = tc.draw(axis_config());
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
    let source = Sized {
        table,
        content: contents,
    };
    let mut s = Scenario::new(Grid::new(rows, cols), source);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    for _ in 0..steps {
        for input in tc.draw(gs::vecs(input()).max_size(4)) {
            match input {
                Input::Action(action) => s.input(|g| g.action(action)),
                Input::SetSize(axis, key, size) if knobs.resize => {
                    s.input(|g| g.set_size(axis, key, size));
                    if size.is_finite() && size > 0.0 {
                        models[index(axis)].overrides.insert(key, size);
                    }
                }
                Input::SetSize(..) => {}
            }
        }
        let [r, c] = &models;
        let (w, h) = tc.draw(hegel::tuples!(
            extent(c.config, c.smallest()),
            extent(r.config, r.smallest())
        ));
        check(&s.frame(Viewport::new(w, h)), &models);
    }
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
    frames(&tc, ALL, |snap, models| {
        for axis in Axis::ALL {
            let model = &models[index(axis)];
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
            let Some(first) = strip.first() else {
                // The item after the anchor's Gap starts at the header plus what
                // remains of the Gap.
                let next_start = (f64::from(start) + f64::from(anchor_size) + f64::from(gap)
                    - f64::from(anchor.offset)) as f32;
                assert!(
                    count == 0
                        || start >= end
                        || !resolvable
                        || (in_gap && (anchor.position + 1 == count || next_start >= end)),
                    "{axis:?}: empty Strip with content and room"
                );
                continue;
            };
            // What is left of the anchor's item past the header can be too thin
            // to register in f32; then the Strip starts at the next item.
            let sliver =
                (f64::from(start) - f64::from(anchor.offset)) as f32 + anchor_size <= start;
            let expected = anchor.position + u64::from(in_gap);
            assert!(
                first.position == expected || (sliver && first.position == expected + 1),
                "{axis:?}: Strip starts at {first:?}, not the Scroll Anchor {anchor:?}"
            );
            assert!(
                first.offset <= if in_gap { start + gap } else { start },
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
    frames(&tc, ALL, |snap, models| {
        for axis in Axis::ALL {
            let model = &models[index(axis)];
            let anchor = snap.scroll_anchor(axis);
            let count = snap.count(axis);
            assert!(
                anchor.position < count.max(1),
                "{axis:?}: {anchor:?} past the count {count}"
            );
            assert!(
                anchor.offset >= 0.0
                    && anchor.offset
                        < seen_size(snap, axis, model, anchor.position) + snap.gap(axis),
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
/// last item. Without Size Overrides or content sizes every size is known in
/// advance (sizes the Grid has not seen are estimates), so the clamp is exact up
/// to f32 stepping across the Strip.
#[hegel::test(test_cases = 2000)]
fn scrolling_stops_where_the_last_item_ends_the_viewport(tc: TestCase) {
    frames(
        &tc,
        Knobs {
            resize: false,
            content: false,
        },
        |snap, models| {
            for axis in Axis::ALL {
                let (start, end) = body(snap, axis);
                let count = snap.count(axis);
                let anchor = snap.scroll_anchor(axis);
                let resolvable = models[index(axis)].smallest() >= next_f32(end) - end;
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
                // Slack for f32 stepping across the Strip, and for the clamp itself,
                // which is found in f64 within the last item's stride.
                let steps = snap.strip(axis).len() as f32 + 2.0;
                let stride = last.size + snap.gap(axis);
                let slack =
                    steps * (next_f32(end) - end) + (4.0 * f64::EPSILON * f64::from(stride)) as f32;
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
    let config = [rows, cols][index(axis)];
    let fraction = tc.draw(hegel::one_of!(
        gs::sampled_from(vec![0.0, 0.5, 1.0]),
        gs::floats::<f64>().min_value(0.0).max_value(1.0),
        gs::floats::<f64>()
    ));
    // Fractions outside 0..=1 are clamped; NaN is ignored, leaving the fresh
    // Grid at the start.
    let expected = if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    };
    let (w, h) = tc.draw(hegel::tuples!(
        extent(cols, cols.default_size),
        extent(rows, rows.default_size)
    ));
    let viewport = Viewport::new(w, h);
    let mut s = Scenario::identity(rows, cols);
    s.frame(viewport);
    s.input(|g| g.action(Action::ScrollTo { axis, fraction }));
    let snap = s.frame(viewport);
    let thumb = snap.thumb(axis).fraction;
    let scrollable = scrollable(
        snap.count(axis),
        config,
        snap.extent(axis) - snap.header(axis),
    );
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
    // f32 sum of item size and Gap (itself rounded), so it lands within two f32
    // steps of the stride; the rest is f64 rounding.
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
fn scrollable(count: u64, config: AxisConfig, body: f32) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let stride = f64::from(config.default_size) + f64::from(config.gap);
    let last_start = count.saturating_sub(1) as f64 * stride;
    if body <= 0.0 {
        return last_start;
    }
    (last_start + f64::from(config.default_size) - f64::from(body)).max(0.0)
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
