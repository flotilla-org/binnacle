//! Seam 1 properties: a real Grid over an identity Table Source, driven through
//! the frame loop with random tables and viewports.

use binnacle_core::{Axis, AxisConfig, Grid, Key, Snapshot, Viewport};
use binnacle_testkit::Scenario;
use hegel::generators as gs;
use hegel::{Generator, TestCase};

/// Most items a drawn viewport can show along one axis, which bounds the Keys
/// the identity source materialises per frame.
const MAX_VISIBLE: f32 = 1000.0;

#[hegel::composite]
fn axis_config(tc: &TestCase) -> AxisConfig {
    let count = tc.draw(hegel::one_of!(
        gs::just(0u64),
        gs::integers::<u64>().max_value(50),
        gs::integers::<u64>(),
    ));
    let header = tc.draw(gs::floats::<f32>());
    let any_size = gs::floats::<f32>()
        .min_value_exclusive(0.0)
        .allow_infinity(false);
    // Sizes near the f32 spacing at the header, where an offset may not advance.
    let scale = if header.is_finite() {
        header.abs().max(1.0)
    } else {
        1.0
    };
    let tiny_size = gs::floats::<f32>()
        .min_value(1.0e-9)
        .max_value(1.0e-5)
        .map(move |f| f * scale);
    let size = tc.draw(hegel::one_of!(any_size, tiny_size));
    AxisConfig::new(count, size).header(header)
}

/// A viewport length for an axis: usually one that shows items past the header
/// band, sometimes any length up to that, sometimes hostile.
#[hegel::composite]
fn extent(tc: &TestCase, config: AxisConfig) -> f32 {
    let header = if config.header.is_finite() {
        config.header.max(0.0)
    } else {
        0.0
    };
    let body = (config.default_size * MAX_VISIBLE).min(f32::MAX);
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

/// Draws a table and a sequence of viewports, running a frame for each and
/// handing every Snapshot to `check` with the configs for both axes.
fn frames(tc: &TestCase, mut check: impl FnMut(&Snapshot, [AxisConfig; 2])) {
    let rows = tc.draw(axis_config());
    let cols = tc.draw(axis_config());
    let viewports = tc.draw(
        gs::vecs(hegel::tuples!(extent(cols), extent(rows)).map(|(w, h)| Viewport::new(w, h)))
            .min_size(1)
            .max_size(6),
    );
    let mut s = Scenario::identity(rows, cols);
    for viewport in viewports {
        check(&s.frame(viewport), [rows, cols]);
    }
}

fn axis_index(axis: Axis) -> usize {
    match axis {
        Axis::Rows => 0,
        Axis::Cols => 1,
    }
}

/// The body is the part of the viewport past the header band, where items show.
fn body(snap: &Snapshot, axis: Axis) -> (f32, f32) {
    (snap.header(axis), snap.extent(axis))
}

#[hegel::test(test_cases = 2000)]
fn strips_are_ordered_gapless_and_cover_the_viewport(tc: TestCase) {
    frames(&tc, |snap, configs| {
        for axis in Axis::ALL {
            let size = configs[axis_index(axis)].default_size;
            let strip = snap.strip(axis);
            let (start, end) = body(snap, axis);
            let count = snap.count(axis);
            // AxisConfig promises coverage only for sizes of at least one f32
            // step at the viewport's far edge.
            let resolvable = size >= f32::from_bits(end.to_bits() + 1) - end;
            let Some(first) = strip.first() else {
                assert!(
                    count == 0 || start >= end || !resolvable,
                    "{axis:?}: empty Strip with content and room"
                );
                continue;
            };
            assert_eq!(
                first.position, 0,
                "{axis:?}: Strip starts at the Scroll Anchor"
            );
            assert!(
                first.offset <= start,
                "{axis:?}: Strip starts after the body"
            );
            for pair in strip.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                assert_eq!(
                    b.position,
                    a.position + 1,
                    "{axis:?}: Positions not consecutive"
                );
                assert_eq!(b.offset, a.end(), "{axis:?}: items overlap or leave a gap");
            }
            for item in strip {
                assert!(item.size > 0.0, "{axis:?}: empty item {item:?}");
                assert!(item.position < count, "{axis:?}: {item:?} past the count");
                assert!(
                    item.offset < end && item.end() > start,
                    "{axis:?}: {item:?} not visible"
                );
            }
            let last = strip.last().unwrap();
            assert!(
                !resolvable || last.end() >= end || last.position == count - 1,
                "{axis:?}: Strip stops short at {last:?} with content remaining"
            );
        }
    });
}

#[hegel::test(test_cases = 1000)]
fn every_strip_item_carries_the_key_the_source_gave_for_its_position(tc: TestCase) {
    frames(&tc, |snap, _| {
        for axis in Axis::ALL {
            for item in snap.strip(axis) {
                assert_eq!(item.key, Some(Key(item.position)), "{axis:?}");
            }
        }
    });
}

#[hegel::test(test_cases = 1000)]
fn a_default_size_that_is_not_positive_and_finite_is_rejected(tc: TestCase) {
    let bad = tc.draw(hegel::one_of!(
        gs::floats::<f32>().max_value(0.0),
        gs::sampled_from(vec![f32::NAN, f32::INFINITY]),
    ));
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
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
