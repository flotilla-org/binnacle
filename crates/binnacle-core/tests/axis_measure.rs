//! Seam 3: the axis measure. The legacy `druid_table` axis tests
//! (`axis_measure.rs`, disabled with `#[cfg(not)]`) translated to the new
//! measure: Gap only between items, half-open ranges, Size Overrides by Key.

use binnacle_core::{AxisMeasure, ItemAt, Key};

/// The legacy fixture: default size 99, Gap 1, four items, Keys in order.
fn equal_sized() -> AxisMeasure {
    let mut m = AxisMeasure::new(4, 99.0, 1.0);
    m.set_keys(0, &[Key(0), Key(1), Key(2), Key(3)]);
    m
}

fn at(m: &AxisMeasure, offset: f64) -> Option<(u64, bool)> {
    m.locate(0, offset).map(|l| (l.position, l.in_gap))
}

#[test]
fn equal_sized_items() {
    let m = equal_sized();
    // Legacy counted a trailing Gap (400); the Gap now sits only between items.
    assert_eq!(m.extent(), 399.0);
    assert_eq!(m.distance(0, 0), 0.0);
    assert_eq!(m.distance(0, 1), 100.0);
    assert_eq!(at(&m, 0.0), Some((0, false)));
    assert_eq!(at(&m, 1.0), Some((0, false)));
    assert_eq!(at(&m, 100.0), Some((1, false)));
    assert_eq!(at(&m, 350.0), Some((3, false)));
    // Legacy mapped 199 to item 1; it is the Gap after it, the resize handle.
    assert_eq!(at(&m, 199.0), Some((1, true)));
    assert_eq!(at(&m, 200.0), Some((2, false)));
    assert_eq!(at(&m, 201.0), Some((2, false)));
    for p in 0..4 {
        assert_eq!(m.size_at(p), 99.0);
    }
}

#[test]
fn offsets_outside_the_axis_locate_nothing() {
    let m = equal_sized();
    assert_eq!(m.locate(0, -0.5), None);
    assert_eq!(m.locate(0, 399.0), None);
    assert_eq!(m.locate(0, 398.5).map(|l| l.position), Some(3));
}

#[test]
fn locate_reports_the_offset_into_the_item() {
    let m = equal_sized();
    assert_eq!(
        m.locate(1, 150.0),
        Some(ItemAt {
            position: 2,
            within: 50.0,
            in_gap: false
        })
    );
    assert_eq!(
        m.locate(3, -1.0),
        Some(ItemAt {
            position: 2,
            within: 99.0,
            in_gap: true
        })
    );
}

/// Legacy `fixed_axis`: with no Size Overrides the measure costs nothing per
/// item, so a 10^15-item axis answers exactly from a nearby anchor.
#[test]
fn fixed_axis_at_a_quadrillion_items() {
    let n = 1_000_000_000_000_000;
    let m = AxisMeasure::new(n, 99.0, 1.0);
    assert_eq!(m.extent(), 1.0e17 - 1.0);
    assert_eq!(
        m.locate(n - 2, 150.0),
        Some(ItemAt {
            position: n - 1,
            within: 50.0,
            in_gap: false
        })
    );
    assert_eq!(m.locate(n - 1, 99.0), None);
    assert_eq!(m.distance(n / 2, n / 2 + 3), 300.0);
    assert_eq!(m.locate(0, 5.0e16).map(|l| l.position), Some(n / 2));
}

/// Legacy `stored_axis`: Position 2 resized to 49, Position 1's far edge
/// dragged to 109 (size 9).
fn stored() -> AxisMeasure {
    let mut m = equal_sized();
    m.set_size(Key(2), 49.0);
    m.set_size(Key(1), 9.0);
    m
}

#[test]
fn stored_axis() {
    let m = stored();
    let starts: Vec<f64> = (0..4).map(|p| m.distance(0, p)).collect();
    assert_eq!(starts, [0.0, 100.0, 110.0, 160.0]);
    assert_eq!(m.extent(), 259.0);
    assert_eq!(at(&m, 105.0), Some((1, false)));
    assert_eq!(at(&m, 109.0), Some((1, true)));
    assert_eq!(at(&m, 110.0), Some((2, false)));
    assert_eq!(at(&m, 159.0), Some((2, true)));
    assert_eq!(m.smallest_size(), 9.0);
}

#[test]
fn size_overrides_follow_their_key() {
    let mut m = stored();
    m.set_keys(0, &[Key(3), Key(2), Key(1), Key(0)]);
    let sizes: Vec<f32> = (0..4).map(|p| m.size_at(p)).collect();
    assert_eq!(sizes, [99.0, 49.0, 9.0, 99.0]);
}

#[test]
fn replacing_an_override_updates_the_smallest_size() {
    let mut m = stored();
    m.set_size(Key(1), 120.0);
    assert_eq!(m.smallest_size(), 49.0);
    m.set_size(Key(2), 99.0);
    assert_eq!(m.smallest_size(), 99.0);
}

#[test]
fn invalid_sizes_are_ignored() {
    let mut m = equal_sized();
    for bad in [0.0, -3.0, f32::NAN, f32::INFINITY] {
        m.set_size(Key(1), bad);
    }
    assert_eq!(m.size_at(1), 99.0);
    assert_eq!(m.smallest_size(), 99.0);
}

#[test]
fn sizes_outside_the_size_range_are_clamped_into_it() {
    let mut m = AxisMeasure::with_size_range(6, 500.0, 300.0, 2.0, 100.0);
    assert_eq!((m.default_size(), m.gap()), (100.0, 100.0));
    m.set_size(Key(3), 1.0e-20);
    m.set_size(Key(4), 1.0e20);
    m.set_keys_sized(
        0,
        &[Key(0), Key(1), Key(2), Key(3), Key(4)],
        &[0.5, 1.0e9, 50.0],
        0.1,
    );
    let sizes: Vec<f32> = (0..6).map(|p| m.size_at(p)).collect();
    // The hint 0.1 rises to 2; content 0.5 rises to it, 1e9 falls to 100.
    assert_eq!(sizes, [2.0, 100.0, 50.0, 2.0, 100.0, 100.0]);
    assert_eq!(m.smallest_size(), 2.0);
    assert_eq!(AxisMeasure::new(1, 1.0e-3, 0.0).size_at(0), 1.0);
}

/// Gap 0 is allowed and has no border zone: every offset is in exactly one item
/// (the prototype's hit test chose the item above).
#[test]
fn gap_zero_has_no_border_zone() {
    let mut m = AxisMeasure::new(5, 1.0, 0.0);
    m.set_keys(0, &[Key(0), Key(1), Key(2), Key(3), Key(4)]);
    for p in 0..5 {
        assert_eq!(at(&m, p as f64), Some((p, false)));
        assert_eq!(at(&m, p as f64 + 0.5), Some((p, false)));
    }
}

#[test]
fn empty_axis() {
    let m = AxisMeasure::new(0, 10.0, 1.0);
    assert_eq!(m.extent(), 0.0);
    assert_eq!(m.locate(0, 0.0), None);
    assert_eq!(m.distance(0, 0), 0.0);
}

#[test]
fn content_sizes_apply_below_overrides_and_are_raised_to_the_hint() {
    let mut m = AxisMeasure::new(4, 10.0, 0.0);
    m.set_keys_sized(0, &[Key(0), Key(1), Key(2)], &[3.0, 1.0, f32::NAN], 2.0);
    m.set_size(Key(0), 7.0);
    let sizes: Vec<f32> = (0..4).map(|p| m.size_at(p)).collect();
    assert_eq!(sizes, [7.0, 2.0, 10.0, 10.0]);
    assert_eq!(m.smallest_size(), 2.0);
    // Plain Keys forget content sizes but keep the hint.
    m.set_keys(0, &[Key(0), Key(1)]);
    assert_eq!(m.size_at(1), 10.0);
    assert_eq!(m.smallest_size(), 2.0);
}

mod properties {
    use binnacle_core::{AxisMeasure, Key};
    use hegel::generators as gs;
    use hegel::{Generator, TestCase};

    fn size() -> gs::FloatGenerator<f32> {
        gs::floats::<f32>()
            .min_value_exclusive(0.0)
            .allow_infinity(false)
    }

    /// A measure with random sizes, Gap, Window (Keys drawn from a small pool so
    /// overrides hit) and Size Overrides, on an axis of up to `max_count` items.
    #[hegel::composite]
    fn measure(tc: &TestCase, max_count: u64) -> AxisMeasure {
        let count = tc.draw(hegel::one_of!(
            gs::integers::<u64>()
                .min_value(1)
                .max_value(300.min(max_count)),
            gs::integers::<u64>()
                .min_value(max_count.min(999_999_999_999_000))
                .max_value(max_count.min(1_000_000_000_000_000)),
            gs::integers::<u64>().min_value(1).max_value(max_count),
        ));
        let gap = tc.draw(hegel::one_of!(
            gs::just(0.0f32),
            gs::floats::<f32>().min_value(0.0).allow_infinity(false)
        ));
        let (min, max) = tc.draw(
            hegel::tuples!(size(), size()).map(|(a, b)| if a <= b { (a, b) } else { (b, a) }),
        );
        let mut m = AxisMeasure::with_size_range(count, tc.draw(size()), gap, min, max);
        let start = tc.draw(gs::integers::<u64>().max_value(count - 1));
        let start = tc.draw(hegel::one_of!(gs::just(0), gs::just(start)));
        let pool = || gs::integers::<u64>().max_value(60).map(Key);
        let keys = tc.draw(gs::vecs(pool()).max_size(60));
        if tc.draw(gs::booleans()) {
            let sizes = tc.draw(gs::vecs(size()).max_size(60));
            m.set_keys_sized(start, &keys, &sizes, tc.draw(size()));
        } else {
            m.set_keys(start, &keys);
        }
        for (key, s) in tc.draw(gs::vecs(hegel::tuples!(pool(), size())).max_size(20)) {
            m.set_size(key, s);
        }
        m
    }

    /// From any anchor near an item, the item's start (Position to offset) and
    /// offsets inside it and its Gap (offset to Position) agree.
    #[hegel::test(test_cases = 2000)]
    fn offset_to_position_and_position_to_offset_agree(tc: TestCase) {
        let m = tc.draw(measure(u64::MAX));
        let count = m.count();
        let p = tc.draw(gs::integers::<u64>().max_value(count - 1));
        let back = tc.draw(gs::integers::<u64>().max_value(200));
        let ahead = tc.draw(gs::integers::<u64>().max_value(200));
        let from = tc.draw(gs::sampled_from(vec![
            p.saturating_sub(back),
            p.saturating_add(ahead).min(count - 1),
        ]));
        let t = f64::from(tc.draw(gs::floats::<f32>().min_value(0.01).max_value(0.99)));
        let start = m.distance(from, p);
        let size = f64::from(m.size_at(p));
        let gap = f64::from(m.gap());

        // Each item crossed between `from` and `p` costs at most one f64
        // rounding at the magnitude of the distance. Offsets closer than that
        // to an item's edge cannot be told apart, so only clear ones are
        // asserted.
        let crossed = from.abs_diff(p) as f64 + 2.0;
        let tolerance = 4.0 * crossed * f64::EPSILON * (start.abs() + size + gap);
        if size * t > tolerance && size * (1.0 - t) > tolerance {
            let inside = m.locate(from, start + t * size).expect("inside the axis");
            assert_eq!((inside.position, inside.in_gap), (p, false));
            assert!(
                (inside.within - t * size).abs() <= tolerance,
                "{inside:?} for {t} of {size}"
            );
        }

        let in_gap = m.locate(from, start + size + t * gap);
        if gap * (1.0 - t) > tolerance && gap * t > tolerance {
            if p + 1 == count {
                assert_eq!(in_gap, None, "the trailing Gap is outside the axis");
            } else {
                let g = in_gap.expect("Gap inside the axis");
                assert_eq!((g.position, g.in_gap), (p, true));
            }
        }
    }

    /// The extent is every item's size plus a Gap between each pair, found by
    /// walking a small axis item by item.
    #[hegel::test(test_cases = 1000)]
    fn extent_is_items_plus_gaps_between(tc: TestCase) {
        let m = tc.draw(measure(300));
        let walked: f64 = (0..m.count())
            .map(|p| f64::from(m.size_at(p)) + f64::from(m.gap()))
            .sum::<f64>()
            - f64::from(m.gap());
        assert!((m.extent() - walked).abs() <= 1.0e-9 * walked.max(1.0));
    }
}

/// Found by Hegel: an offset just inside the Window's last item, added to that
/// item's start, rounded onto the Window's end and located nothing.
#[test]
fn an_offset_just_inside_the_last_window_item_locates_it() {
    let mut m = AxisMeasure::with_size_range(4, 0.25, 0.0, 0.25, 1.0);
    m.set_keys(1, &[Key(1), Key(2), Key(3)]);
    let just_inside = 0.25 - 6.668321e-17;
    assert_eq!(m.locate(3, just_inside).map(|l| l.position), Some(3));
}
