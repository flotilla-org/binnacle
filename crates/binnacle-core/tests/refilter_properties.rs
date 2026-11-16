//! Seam 1 properties for `Refiltered`: a real Grid over the In-Memory Table
//! Source, filtered and unfiltered through the frame loop and the shared pump.

use binnacle_core::{
    Action, Axis, AxisConfig, Edge, Grid, Key, Modifiers, SelectionUnit, SortColumn, SortSpec,
    ViewSpec, Viewport,
};
use binnacle_testkit::{RecordTable, Scenario};
use hegel::TestCase;
use hegel::generators as gs;

/// A filter that keeps some rows and then lets them all back restores focus
/// and every Size Override, and keeps exactly the picks of the rows the
/// filter kept.
#[hegel::test(test_cases = 300)]
fn a_filter_round_trip_restores_focus_sizes_and_the_surviving_picks(tc: TestCase) {
    let n = tc.draw(gs::integers::<u64>().min_value(1).max_value(30));
    // Column 0 holds a value to sort by, column 1 the record's own index.
    let rows = (0..n)
        .map(|i| {
            let value = tc.draw(gs::integers::<i64>().min_value(0).max_value(5));
            vec![Some(value), Some(i as i64)]
        })
        .collect();
    let mut grid = Grid::new(
        AxisConfig::new(n, 1.0).header(1.0),
        AxisConfig::new(2, 4.0).header(3.0),
    );
    if tc.draw(gs::booleans()) {
        grid.set_selection_unit(SelectionUnit::Rows);
    }
    let mut s = Scenario::new(grid, RecordTable::new(rows, 2, [0]));
    if tc.draw(gs::booleans()) {
        let table = &mut s.source;
        let notices = table
            .source
            .view(
                &table.rows,
                &ViewSpec::default().with_sort(
                    Axis::Rows,
                    SortSpec::new(vec![SortColumn::descending(Key(0))]),
                ),
            )
            .expect("column 0 sorts");
        s.input(|g| notices.into_iter().for_each(|(axis, n)| g.notify(axis, n)));
    }
    // Every row shows, so any can be picked by a click.
    let all = Viewport::new(20.0, n as f32 + 1.0);
    s.frame(all);
    let positions: Vec<u64> = tc.draw(gs::vecs(gs::integers::<u64>().max_value(n - 1)).max_size(6));
    for &p in &positions {
        s.click(4.0, p as f32 + 1.5, Modifiers::CTRL);
        s.frame(all);
    }
    let focus = Key(tc.draw(gs::integers::<u64>().max_value(n - 1)));
    s.input(|g| g.set_focus(focus, Key(1)));
    let sizes: Vec<(u64, f32)> = tc.draw(gs::vecs(hegel::tuples!(
        gs::integers::<u64>().max_value(n - 1),
        gs::floats::<f32>().min_value(0.5).max_value(8.0)
    )));
    s.input(|g| {
        for &(key, size) in &sizes {
            g.set_size(Axis::Rows, Key(key), size);
        }
    });
    // Any viewport while filtered: what is kept must not depend on what shows.
    let height = tc.draw(gs::integers::<u32>().min_value(2).max_value(12)) as f32;
    let view = Viewport::new(20.0, height);
    s.frame(view);
    let picked = s.grid.snapshot().picked(Axis::Rows).to_vec();
    let overrides: Vec<Option<f32>> = (0..n)
        .map(|k| s.grid.size_override(Axis::Rows, Key(k)))
        .collect();

    let mut keep: Vec<bool> = tc.draw(
        gs::vecs(gs::booleans())
            .min_size(n as usize)
            .max_size(n as usize),
    );
    keep[focus.0 as usize] = true;
    let kept = keep.clone();
    let notice = s
        .source
        .filter(move |r| kept[r[1].expect("an index") as usize]);
    s.input(|g| g.notify(Axis::Rows, notice));
    s.frame(view);
    let surviving: Vec<Key> = picked
        .iter()
        .copied()
        .filter(|k| keep[k.0 as usize])
        .collect();
    assert_eq!(
        s.grid.snapshot().picked(Axis::Rows),
        surviving,
        "picks once filtered"
    );

    let notice = s.source.source.clear_filter(&s.source.rows);
    s.input(|g| g.notify(Axis::Rows, notice));
    s.frame(view);
    let snap = s.grid.snapshot();
    assert_eq!(snap.focus(Axis::Rows).and_then(|f| f.key), Some(focus));
    assert_eq!(
        snap.picked(Axis::Rows),
        surviving,
        "picks after the round trip"
    );
    for k in 0..n {
        assert_eq!(
            s.grid.size_override(Axis::Rows, Key(k)),
            overrides[k as usize],
            "Size Override of record {k}"
        );
    }
}

/// What a step after the range is made does to its axis.
#[derive(Clone, Debug)]
enum Refilter {
    /// Keeps the records flagged (rows only).
    Filter(Vec<bool>),
    Hide(Vec<u64>),
    Show(Vec<u64>),
}

/// A range made on either axis, then filtered, hidden and shown around in
/// any order: once every answer is in, it covers on that axis exactly the
/// items still shown drawn between its corners, a corner that left pulled in
/// to the nearest item drawn inside, and it is gone once none is left. Up to
/// 3 items are pinned first, so a range can straddle a pin drawn apart from
/// its Position (#193). (Copy and the drawn range read these Positions, so
/// neither reaches a filtered out or hidden item.)
#[hegel::test(test_cases = 1000)]
fn a_range_covers_the_items_still_shown_drawn_between_its_corners(tc: TestCase) {
    range_through_refilters(tc, false);
}

/// The same on a slow host, which may draw before the `Locate` answers come
/// or answer without drawing, so a corner pulled in has no Key yet at the
/// next step, and on a viewport that may be smaller and scrolled once the
/// range is made, so the item a corner is pulled in to may be off screen
/// (#377).
#[hegel::test(test_cases = 1000)]
fn a_range_covers_the_items_still_shown_with_windows_before_answers(tc: TestCase) {
    range_through_refilters(tc, true);
}

fn range_through_refilters(tc: TestCase, slow: bool) {
    let n = tc.draw(gs::integers::<usize>().min_value(1).max_value(12));
    let m = tc.draw(gs::integers::<usize>().min_value(1).max_value(6));
    let axis = tc.draw(gs::sampled_from(vec![Axis::Rows, Axis::Cols]));
    let len = match axis {
        Axis::Rows => n,
        Axis::Cols => m,
    };
    // Column 0 holds the record's own index, for the filter.
    let rows = (0..n)
        .map(|r| (0..m).map(|c| Some((10 * r + c) as i64)).collect())
        .collect();
    let mut grid = Grid::new(
        AxisConfig::new(n as u64, 1.0).header(1.0),
        AxisConfig::new(m as u64, 4.0).header(3.0),
    );
    for a in Axis::ALL {
        grid.set_hide_support(a, true);
    }
    let mut s = Scenario::new(grid, RecordTable::new(rows, m, 0..m as u64));
    // Every item shows, so any can be clicked, and the pinned Strips meet
    // the scrolling one with no space between.
    let view = Viewport::new(3.0 + 4.0 * m as f32, n as f32 + 1.0);
    // Sorting the axis descending by the first item across reverses it.
    let sorted = tc.draw(gs::booleans());
    if sorted {
        let by = SortSpec::new(vec![SortColumn::descending(Key(0))]);
        s.input(|g| g.request_view(ViewSpec::default().with_sort(axis, by)));
    }
    // The Keys of the axis by Position (Keys are Logical Indices).
    let order: Vec<usize> = match sorted {
        true => (0..len).rev().collect(),
        false => (0..len).collect(),
    };
    let pins = tc.draw(
        gs::vecs(hegel::tuples!(
            gs::integers::<usize>().max_value(len - 1),
            gs::booleans()
        ))
        .max_size(3),
    );
    let mut pinned: [Vec<usize>; 2] = [Vec::new(), Vec::new()];
    for (k, leading) in pins {
        let edge = if leading {
            Edge::Leading
        } else {
            Edge::Trailing
        };
        s.input(|g| g.pin(axis, edge, Key(k as u64)));
        // Pinning a Key again moves it to the end of its edge's list.
        pinned.iter_mut().for_each(|p| p.retain(|&j| j != k));
        pinned[edge as usize].push(k);
    }
    s.frame(view);
    // Every Key in drawn order, shown or not: the leading pins, the rest by
    // Position, then the trailing pins.
    let drawn: Vec<usize> = (pinned[0].iter().copied())
        .chain(
            order
                .iter()
                .copied()
                .filter(|k| !pinned.iter().flatten().any(|j| j == k)),
        )
        .chain(pinned[1].iter().copied())
        .collect();

    // A cell on the axis at drawn index `p`, the other axis at `q`.
    let at = |p: usize, q: usize| match axis {
        Axis::Rows => (3.0 + 4.0 * q as f32 + 2.0, 1.0 + p as f32 + 0.5),
        Axis::Cols => (3.0 + 4.0 * p as f32 + 2.0, 1.0 + q as f32 + 0.5),
    };
    let other = match axis {
        Axis::Rows => m,
        Axis::Cols => n,
    };
    let anchor = tc.draw(gs::integers::<usize>().max_value(len - 1));
    let head = tc.draw(gs::integers::<usize>().max_value(len - 1));
    let (q1, q2) = (
        tc.draw(gs::integers::<usize>().max_value(other - 1)),
        tc.draw(gs::integers::<usize>().max_value(other - 1)),
    );
    let (x, y) = at(anchor, q1);
    s.click(x, y, Modifiers::NONE);
    let (x, y) = at(head, q2);
    s.click(x, y, Modifiers::SHIFT);
    // Hiding columns asks for the whole View Spec, which reorders the rows:
    // only a range whole on the rows survives that. A range of one cell
    // needs an axis whole to be a range at all.
    if axis == Axis::Cols || (anchor == head && q1 == q2) || tc.draw(gs::booleans()) {
        s.input(|g| {
            g.action(match axis {
                Axis::Rows => Action::SelectRow,
                Axis::Cols => Action::SelectColumn,
            })
        });
    }
    s.frame(view);
    assert!(s.grid.snapshot().selection_range().is_some());
    let full = !(slow && tc.draw(gs::booleans()));
    let view = if !full {
        let smaller = match axis {
            Axis::Rows => Viewport::new(
                view.width,
                tc.draw(
                    gs::integers::<u32>()
                        .min_value(2)
                        .max_value((n as u32).max(2)),
                ) as f32,
            ),
            Axis::Cols => Viewport::new(
                tc.draw(
                    gs::integers::<u32>()
                        .min_value(4)
                        .max_value(3 + 4 * m as u32),
                ) as f32,
                view.height,
            ),
        };
        let by = tc.draw(gs::integers::<u32>().max_value(4 * len as u32)) as f32;
        let (dx, dy) = match axis {
            Axis::Rows => (0.0, by),
            Axis::Cols => (by, 0.0),
        };
        s.input(|g| g.action(Action::Scroll { dx, dy }));
        s.frame(smaller);
        smaller
    } else {
        view
    };

    // Corners as indices into `drawn`.
    let mut corners = Some((anchor.min(head), anchor.max(head)));
    // Whether each corner has a Key. One pulled in to an item no Window has
    // given yet has none, and drops the range at the next step (#377). Off
    // a full viewport the test can't always tell. A corner pulled in to a
    // scrolling item becomes `No` if the step is answered without a frame
    // (`drained`), else `Yes` on a full viewport and `Maybe` off one. A
    // frame, or one drawn before a View Spec's answers (`pace` 1), turns
    // `No` into `Yes` or `Maybe` the same way, and a corner drawn in the
    // Strip is `Yes`. At a step, `No` drops the range; `Maybe`, a step
    // that may send no `Refiltered`, or an unsure step not yet drawn
    // (`unseen`) only lets the range be dropped (`unsure`).
    let mut keyed = [Keyed::Yes; 2];
    // Whether a step not yet drawn may have dropped the range.
    let mut unseen = false;
    let is_pin = |d: usize| d < pinned[0].len() || d >= len - pinned[1].len();
    let mut kept = vec![true; len];
    let mut hidden = vec![false; len];
    let keys = || gs::vecs(gs::integers::<u64>().max_value(len as u64 - 1)).max_size(len);
    let most = if slow { 6 } else { 4 };
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(most));
    for done in 1..=steps {
        let was: Vec<bool> = (0..len).map(|k| kept[k] && !hidden[k]).collect();
        let step = if axis == Axis::Rows && tc.draw(gs::booleans()) {
            Refilter::Filter(tc.draw(gs::vecs(gs::booleans()).min_size(len).max_size(len)))
        } else if tc.draw(gs::booleans()) {
            Refilter::Hide(tc.draw(keys()))
        } else {
            Refilter::Show(tc.draw(keys()))
        };
        match &step {
            Refilter::Filter(keep) => {
                kept = keep.clone();
                let keep = keep.clone();
                let notice = s.source.filter(move |r| keep[r[0].unwrap() as usize / 10]);
                s.input(|g| g.notify(Axis::Rows, notice));
            }
            Refilter::Hide(keys) => {
                keys.iter().for_each(|&k| hidden[k as usize] = true);
                let keys: Vec<Key> = keys.iter().copied().map(Key).collect();
                s.input(|g| g.hide(axis, &keys).unwrap());
            }
            Refilter::Show(keys) => {
                keys.iter().for_each(|&k| hidden[k as usize] = false);
                let keys: Vec<Key> = keys.iter().copied().map(Key).collect();
                s.input(|g| g.show(axis, &keys));
            }
        }
        let is_shown = |k: usize| kept[k] && !hidden[k];
        // A slow host may answer without drawing, so the next step's notice
        // comes before any Window gives a pulled-in corner its Key, or may
        // draw before the answers come.
        let pace = if slow {
            tc.draw(gs::integers::<u8>().max_value(2))
        } else {
            2
        };
        let drained = pace == 0 && done < steps;
        // A step that shows the same items may send no `Refiltered`.
        let changed = (0..len).any(|k| is_shown(k) != was[k]);
        // Hiding and showing ask for a View Spec the pump answers, so a
        // frame drawn before the answers brings a Window first.
        if pace == 1 && !matches!(step, Refilter::Filter(_)) {
            for k in keyed.iter_mut().filter(|k| **k == Keyed::No) {
                *k = if full { Keyed::Yes } else { Keyed::Maybe };
            }
        }
        let mut unsure = unseen || keyed.contains(&Keyed::Maybe);
        if keyed.contains(&Keyed::No) {
            match changed {
                true => corners = None,
                false => unsure = true,
            }
        }
        let before = corners;
        corners = corners.and_then(|(lo, hi)| {
            let inside: Vec<usize> = (lo..=hi).filter(|&d| is_shown(drawn[d])).collect();
            Some((*inside.first()?, *inside.last()?))
        });
        if let (Some((lo, hi)), Some((was_lo, was_hi))) = (corners, before) {
            for (n, (now, was)) in [(lo, was_lo), (hi, was_hi)].into_iter().enumerate() {
                if now != was && !is_pin(now) {
                    keyed[n] = match (drained, full) {
                        (true, _) => Keyed::No,
                        (false, true) => Keyed::Yes,
                        (false, false) => Keyed::Maybe,
                    };
                }
            }
        }
        if drained {
            s.effects();
            unseen = unsure;
            continue;
        }
        unseen = false;
        if pace == 1 {
            s.step_waiting("the Locates unanswered", view);
        }
        s.frame(view);
        // A Position is the rank among the shown Keys in `order`.
        let shown: Vec<usize> = order.iter().copied().filter(|&k| is_shown(k)).collect();
        let position = |k: usize| shown.iter().position(|&j| j == k).unwrap() as u64;
        let expected = corners.map(|(lo, hi)| {
            let mut covered: Vec<u64> = (lo..=hi)
                .filter(|&d| is_shown(drawn[d]))
                .map(|d| position(drawn[d]))
                .collect();
            covered.sort();
            covered
        });
        let snap = s.grid.snapshot();
        let covered = (snap.selection_range())
            .map(|r| r.span(axis).iter().cloned().flatten().collect::<Vec<u64>>());
        if unsure && covered.is_none() {
            tc.event("dropped with a corner maybe keyless");
            corners = None;
            continue;
        }
        assert_eq!(covered, expected, "after {step:?}: shown {shown:?}");
        // The frame's Window may reach a keyless corner past what shows; a
        // corner drawn in the Strip has its Key from it.
        for k in keyed.iter_mut().filter(|k| **k == Keyed::No) {
            *k = if full { Keyed::Yes } else { Keyed::Maybe };
        }
        if let Some((lo, hi)) = corners {
            for (n, d) in [lo, hi].into_iter().enumerate() {
                let p = position(drawn[d]);
                let strip = snap.strip(axis).iter();
                if strip
                    .into_iter()
                    .any(|i| i.position == p && i.key.is_some())
                {
                    keyed[n] = Keyed::Yes;
                }
            }
        }
    }
}

/// Whether a range corner has a Key, as the model knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Keyed {
    Yes,
    Maybe,
    No,
}

/// Eleven rows 1 tall, the last pinned trailing, all shown, with a range
/// from row 0 to row 7.
fn eleven_rows_ranged() -> (Scenario<RecordTable>, Viewport) {
    let rows = (0..11).map(|r| vec![Some(r)]).collect();
    let grid = Grid::new(
        AxisConfig::new(11, 1.0).header(1.0),
        AxisConfig::new(1, 4.0).header(3.0),
    );
    let mut s = Scenario::new(grid, RecordTable::new(rows, 1, [0]));
    let view = Viewport::new(7.0, 12.0);
    s.input(|g| g.pin(Axis::Rows, Edge::Trailing, Key(10)));
    s.frame(view);
    s.click(5.0, 1.5, Modifiers::NONE);
    s.click(5.0, 8.5, Modifiers::SHIFT);
    s.frame(view);
    (s, view)
}

/// Keeps the rows in `keep` and returns the rows the range covers once
/// every answer is in.
fn filter_to(s: &mut Scenario<RecordTable>, view: Viewport, keep: &[i64]) -> Option<Vec<u64>> {
    let keep = keep.to_vec();
    let notice = s.source.filter(move |r| keep.contains(&r[0].unwrap()));
    s.input(|g| g.notify(Axis::Rows, notice));
    let snap = s.frame(view);
    (snap.selection_range()).map(|r| r.span(Axis::Rows).iter().cloned().flatten().collect())
}

/// Found by Hegel (#317). The second filter pulls the range's first corner
/// (row 3, gone) in to row 4 at Position 0. The view keeps row 7 on top,
/// at Position 3, but every row fits, so the end clamp draws them all, and
/// the frame's Window gives the corner its Key. The third filter keeps
/// only row 7, the range's other corner, so the range covers it.
#[test]
fn a_range_survives_a_second_refilter_after_a_corner_was_pulled_in() {
    let (mut s, view) = eleven_rows_ranged();
    assert_eq!(filter_to(&mut s, view, &[3, 7]), Some(vec![0, 1]));
    assert_eq!(
        filter_to(&mut s, view, &[4, 5, 6, 7, 10]),
        Some(vec![0, 1, 2, 3])
    );
    let snap = s.frame(view);
    let drawn: Vec<u64> = snap.drawn(Axis::Rows).map(|i| i.position).collect();
    assert_eq!(drawn, [0, 1, 2, 3, 4], "rows 4 to 7, then row 10 pinned");
    assert_eq!(filter_to(&mut s, view, &[7]), Some(vec![0]));
}

/// #377: twelve rows 1 tall, all shown, with a range from row 0 to row 7.
/// Filtering to row 7 pulls the corner on row 0 in to row 7, and the host
/// answers without drawing, so no Window gives that corner a Key. Filtering
/// to row 6 then drops the range: placed from where row 0 would be, it would
/// cover row 6, which it never covered.
#[test]
fn a_corner_still_keyless_at_the_next_refilter_drops_the_range() {
    let rows = (0..12).map(|r| vec![Some(r)]).collect();
    let grid = Grid::new(
        AxisConfig::new(12, 1.0).header(1.0),
        AxisConfig::new(1, 4.0).header(3.0),
    );
    let mut s = Scenario::new(grid, RecordTable::new(rows, 1, [0]));
    let view = Viewport::new(7.0, 13.0);
    s.frame(view);
    s.click(5.0, 1.5, Modifiers::NONE);
    s.click(5.0, 8.5, Modifiers::SHIFT);
    s.frame(view);
    let notice = s.source.filter(|r| r[0] == Some(7));
    s.input(|g| g.notify(Axis::Rows, notice));
    s.effects();
    assert_eq!(filter_to(&mut s, view, &[6]), None);
}

/// #377 with frames between, on thirty rows, six shown, a range from row 0
/// to row 20 scrolled to show row 20. Filtering out rows 0 to 2 pulls the
/// corner on row 0 in to row 3, which no Window holds. Letting every row
/// back then drops the range rather than cover rows 0 to 2 again.
#[test]
fn a_corner_keyless_off_screen_drops_the_range_at_the_next_refilter() {
    let rows = (0..30).map(|r| vec![Some(r)]).collect();
    let grid = Grid::new(
        AxisConfig::new(30, 1.0).header(1.0),
        AxisConfig::new(1, 4.0).header(3.0),
    );
    let mut s = Scenario::new(grid, RecordTable::new(rows, 1, [0]));
    let view = Viewport::new(7.0, 7.0);
    s.frame(view);
    s.click(5.0, 1.5, Modifiers::NONE);
    s.input(|g| g.action(Action::Scroll { dx: 0.0, dy: 15.0 }));
    s.frame(view);
    s.click(5.0, 6.5, Modifiers::SHIFT);
    s.frame(view);
    // Rows 3 to 20 are now Positions 0 to 17.
    let from_3: Vec<i64> = (3..30).collect();
    assert_eq!(filter_to(&mut s, view, &from_3), Some((0..18).collect()));
    assert!(
        s.frame(view).strip(Axis::Rows)[0].position > 2,
        "row 3 is off screen"
    );
    let every: Vec<i64> = (0..30).collect();
    assert_eq!(filter_to(&mut s, view, &every), None);
}
