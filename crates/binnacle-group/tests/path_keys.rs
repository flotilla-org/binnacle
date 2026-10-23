//! Path Keys through the Grid's public API over the In-Memory Table Source
//! under the grouping layer, pumped: the Size Overrides, pins and sort
//! terms on Summary Items and pivot columns survive any change to the data
//! (slicers coming and going) and go exactly when the levels change so
//! that their paths no longer fit, never to come back.

use std::collections::HashSet;

use binnacle_core::{
    AggregateValue, Axis, AxisConfig, Comparison, Condition, Edge, Grid, Grouping, Key, Level,
    Placement, Scalar, SortColumn, ViewSpec,
};
use binnacle_group::{GroupedMemory, Path, SUMMARY_KEY_BIT};
use binnacle_memory::{MemorySource, pump};
use hegel::TestCase;
use hegel::generators as gs;

type Record = [Option<i64>; 3];

#[hegel::composite]
fn record(tc: &TestCase) -> Record {
    let value = || gs::optional(gs::integers::<i64>().min_value(0).max_value(3));
    [tc.draw(value()), tc.draw(value()), tc.draw(value())]
}

/// Which fields group which axis, as indices into the record.
#[derive(Clone, Debug, PartialEq)]
struct Levels {
    rows: Vec<usize>,
    cols: Vec<usize>,
}

impl Levels {
    fn grouping(&self) -> Option<Grouping> {
        if self.rows.is_empty() && self.cols.is_empty() {
            return None;
        }
        let level = |&f: &usize| Level::field(Key(f as u64));
        let mut grouping = Grouping::default();
        grouping.row_levels = self.rows.iter().map(level).collect();
        grouping.col_levels = self.cols.iter().map(level).collect();
        grouping.values = vec![AggregateValue::count()];
        grouping.cols.grand_total = Placement::After;
        Some(grouping)
    }

    /// Whether a group's path fits the levels of `axis`.
    fn fit(&self, axis: Axis, path: &Path) -> bool {
        let fields = match axis {
            Axis::Rows => &self.rows,
            Axis::Cols => &self.cols,
        };
        !path.is_empty()
            && path.len() <= fields.len()
            && (path.iter().zip(fields)).all(|((f, _), &l)| f.0 == l as u64)
    }
}

#[hegel::composite]
fn levels(tc: &TestCase) -> Levels {
    let order = tc.draw(gs::permutations(vec![0usize, 1, 2]));
    let rows = tc.draw(gs::integers::<usize>().max_value(2));
    let cols = tc.draw(gs::integers::<usize>().max_value(3 - rows));
    Levels {
        rows: order[..rows].to_vec(),
        cols: order[rows..rows + cols].to_vec(),
    }
}

/// What happens next: the data seen through another slicer (or none), or
/// other levels.
#[derive(Clone, Debug)]
enum Step {
    Slice(Option<(usize, i64)>),
    Regroup(Levels),
}

#[hegel::composite]
fn step(tc: &TestCase) -> Step {
    match tc.draw(gs::booleans()) {
        true => Step::Slice(tc.draw(gs::optional(hegel::tuples!(
            gs::integers::<usize>().max_value(2),
            gs::integers::<i64>().max_value(3)
        )))),
        false => Step::Regroup(tc.draw(levels())),
    }
}

/// Every item of `axis` that is a group (a Summary Item or a pivot
/// column), by Key with its path.
fn groups(source: &mut GroupedMemory<Record>, rows: &[Record], axis: Axis) -> Vec<(Key, Path)> {
    if !source.grouped(axis) {
        return Vec::new();
    }
    let n = source.count(axis);
    let window = source.window(rows, axis, 0..n);
    (window.keys.into_iter())
        .filter(|k| k.0 & SUMMARY_KEY_BIT != 0)
        .filter_map(|k| Some((k, source.path(k)?.clone())))
        .filter(|(_, p)| !p.is_empty())
        .collect()
}

#[hegel::test(test_cases = 150)]
fn state_on_path_keys_survives_the_data_and_goes_exactly_when_their_paths_stop_fitting(
    tc: TestCase,
) {
    let rows = tc.draw(gs::vecs(record()).min_size(1).max_size(30));
    let source = (0..3).fold(MemorySource::<Record>::new(rows.len(), 3), |s, c| {
        s.i64_column(Key(c as u64), move |r: &Record| r[c])
    });
    let mut source = GroupedMemory::new(source);
    let mut grid = Grid::new(AxisConfig::new(0, 1.0), AxisConfig::new(3, 5.0));
    let mut now = tc.draw(levels());
    let mut spec = ViewSpec::default().with_grouping(now.grouping());
    grid.request_view(spec.clone());
    pump(&mut grid, &mut source.over(&rows), |_, _| {});

    // A width and a pin on every group of either axis, and the rows sorted
    // by the first pivot column.
    let mut held: Vec<(Axis, Key, Path)> = Vec::new();
    for axis in Axis::ALL {
        for (key, path) in groups(&mut source, &rows, axis) {
            grid.set_size(axis, key, 3.0);
            grid.pin(axis, Edge::Leading, key);
            held.push((axis, key, path));
        }
    }
    let sorted = held
        .iter()
        .find(|(a, ..)| *a == Axis::Cols)
        .map(|(_, k, _)| *k);
    if let Some(key) = sorted {
        grid.cycle_sort(Axis::Rows, key, false);
        spec.rows.sort = vec![SortColumn::ascending(key)].into();
    }
    pump(&mut grid, &mut source.over(&rows), |_, _| {});
    let mut alive: HashSet<Key> = held.iter().map(|(_, k, _)| *k).collect();

    for _ in 0..tc.draw(gs::integers::<usize>().min_value(1).max_value(5)) {
        match tc.draw(step()) {
            Step::Slice(slicer) => {
                let slicers = (slicer.into_iter())
                    .map(|(c, t)| {
                        Condition::compare(
                            Key(c as u64),
                            Comparison::AtLeast(Scalar::Number(t as f64)),
                        )
                    })
                    .collect();
                if let Some(g) = spec.grouping.as_mut() {
                    g.slicers = slicers;
                }
            }
            Step::Regroup(levels) => {
                now = levels;
                let slicers = (spec.grouping.as_ref()).map_or(Vec::new(), |g| g.slicers.clone());
                spec.grouping = now.grouping().map(|mut g| {
                    g.slicers = slicers;
                    g
                });
            }
        }
        grid.request_view(spec.clone());
        pump(&mut grid, &mut source.over(&rows), |_, _| {});
        alive.retain(|k| {
            let (axis, _, path) = held.iter().find(|(_, h, _)| h == k).unwrap();
            now.fit(*axis, path)
        });
        for (axis, key, path) in &held {
            let kept = alive.contains(key);
            let sized = grid.size_override(*axis, *key).is_some();
            assert_eq!(sized, kept, "{axis:?} {path:?} under {now:?}");
            let pinned = grid.pins(*axis, Edge::Leading).any(|k| k == *key);
            assert_eq!(pinned, kept, "pin on {path:?} under {now:?}");
        }
        if let Some(key) = sorted {
            let named = grid.held_keys(Axis::Cols).contains(&key);
            assert_eq!(named, alive.contains(&key), "the sort term under {now:?}");
        }
        // The user's next View Spec keeps the sort term while the Grid
        // does, applied or not.
        spec.rows.sort = match sorted.filter(|k| alive.contains(k)) {
            Some(key) => vec![SortColumn::ascending(key)].into(),
            None => Default::default(),
        };
    }
}
