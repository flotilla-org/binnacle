//! Seam 1 properties for `Refiltered`: a real Grid over the In-Memory Table
//! Source, filtered and unfiltered through the frame loop and the shared pump.

use binnacle_core::{
    Axis, AxisConfig, Grid, Key, Modifiers, SelectionUnit, SortColumn, SortSpec, ViewSpec, Viewport,
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
