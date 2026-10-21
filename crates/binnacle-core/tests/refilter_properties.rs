//! Seam 1 properties for `Refiltered`: a real Grid over the In-Memory Table
//! Source, filtered and unfiltered through the frame loop and the shared pump.

use binnacle_core::{
    Action, Axis, AxisConfig, Grid, Key, Modifiers, SelectionUnit, SortColumn, SortSpec, ViewSpec,
    Viewport,
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
/// items still shown between its corners, a corner that left pulled in to
/// the nearest item inside, and it is gone once none is left. (Copy and the
/// drawn range read these Positions, so neither reaches a filtered out or
/// hidden item.)
#[hegel::test(test_cases = 1000)]
fn a_range_covers_the_items_still_shown_between_its_corners(tc: TestCase) {
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
    // Every item shows, so any can be clicked.
    let view = Viewport::new(3.0 + 4.0 * m as f32 + 1.0, n as f32 + 2.0);
    // Sorting the axis descending by the first item across reverses it.
    let sorted = tc.draw(gs::booleans());
    if sorted {
        let by = SortSpec::new(vec![SortColumn::descending(Key(0))]);
        s.input(|g| g.request_view(ViewSpec::default().with_sort(axis, by)));
    }
    s.frame(view);
    // The Keys of the axis in order (Keys are Logical Indices).
    let order: Vec<usize> = match sorted {
        true => (0..len).rev().collect(),
        false => (0..len).collect(),
    };

    // A cell on the axis at `p`, the other axis at `q`.
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

    // Corners as indices into `order`; a Position is the rank among the
    // shown items in `order`.
    let mut corners = Some((anchor.min(head), anchor.max(head)));
    let mut kept = vec![true; len];
    let mut hidden = vec![false; len];
    let keys = || gs::vecs(gs::integers::<u64>().max_value(len as u64 - 1)).max_size(len);
    let steps = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
    for _ in 0..steps {
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
        s.frame(view);
        let shown: Vec<usize> = (0..len)
            .filter(|&i| kept[order[i]] && !hidden[order[i]])
            .collect();
        corners = corners.and_then(|(lo, hi)| {
            let inside: Vec<usize> = shown
                .iter()
                .copied()
                .filter(|k| (lo..=hi).contains(k))
                .collect();
            Some((*inside.first()?, *inside.last()?))
        });
        let rank = |k: usize| shown.iter().position(|&j| j == k).unwrap() as u64;
        let expected = corners.map(|(lo, hi)| vec![(rank(lo), rank(hi) + 1)]);
        let snap = s.grid.snapshot();
        assert_eq!(
            (snap.selection_range()).map(|r| r
                .span(axis)
                .iter()
                .map(|r| (r.start, r.end))
                .collect()),
            expected,
            "after {step:?}: shown {shown:?}"
        );
    }
}
