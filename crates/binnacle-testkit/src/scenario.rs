use std::cell::RefCell;
use std::fmt::Write;
use std::ops::Range;

use binnacle_core::{
    Axis, AxisConfig, ChangeNotice, Grid, Key, Located, Position, Snapshot, SortSpec, Stamped,
    Viewport,
};
use binnacle_memory::{IdentitySource, MemorySource, PumpSource};

use crate::render::render;

/// What a scenario's host fetches Keys from. Later tickets extend it as the
/// host learns to answer Effects.
pub trait ScenarioSource {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key>;

    /// Content sizes for the Keys just fetched on `axis`, with the smallest
    /// content size the axis can have. `None` (the default) sends Keys only.
    fn sizes(&mut self, _axis: Axis, _keys: &[Key]) -> Option<(Vec<f32>, f32)> {
        None
    }

    /// Answers `SortRequested` (see [`PumpSource::sort`]). The default
    /// cannot sort and rejects every request, keeping base order.
    fn sort(&mut self, _spec: &SortSpec) -> Result<ChangeNotice, SortSpec> {
        Err(SortSpec::unsorted())
    }

    /// Answers `Locate`. The default knows no Keys and answers `Gone`.
    fn locate(&mut self, _axis: Axis, _key: Key) -> Located {
        Located::Gone { suggest: None }
    }

    /// Answers `MoveRequested` (see [`PumpSource::move_items`]). The default
    /// moves nothing.
    fn move_items(&mut self, _axis: Axis, _keys: &[Key], _to: Position) -> Option<ChangeNotice> {
        None
    }
}

/// A table whose Keys are their Positions on both axes.
pub struct IdentityTable {
    pub rows: IdentitySource,
    pub cols: IdentitySource,
}

impl ScenarioSource for IdentityTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        match axis {
            Axis::Rows => self.rows.window(range),
            Axis::Cols => self.cols.window(range),
        }
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let count = match axis {
            Axis::Rows => self.rows.count(),
            Axis::Cols => self.cols.count(),
        };
        if key.0 < count {
            Located::Found(key.0)
        } else {
            Located::Gone { suggest: None }
        }
    }
}

/// A table holding the Key at every Position on each axis, so items can be
/// inserted and removed anywhere. New items get Keys never used before.
pub struct KeyedTable {
    pub axes: [Vec<Key>; 2],
    next: u64,
}

impl KeyedTable {
    /// `rows` rows keyed 0.. and `cols` columns keyed 0.. (the axes' Keys
    /// are independent).
    pub fn new(rows: u64, cols: u64) -> Self {
        KeyedTable {
            axes: [(0..rows).map(Key).collect(), (0..cols).map(Key).collect()],
            next: rows.max(cols),
        }
    }

    fn keys(&mut self, axis: Axis) -> &mut Vec<Key> {
        &mut self.axes[at_index(axis)]
    }

    /// Inserts `n` new items before Position `at` (clamped to the count);
    /// returns the Change Notice the Grid needs.
    pub fn insert(&mut self, axis: Axis, at: Position, n: u64) -> ChangeNotice {
        let at = at.min(self.axes[at_index(axis)].len() as u64);
        let fresh: Vec<Key> = (self.next..self.next + n).map(Key).collect();
        self.next += n;
        let keys = self.keys(axis);
        keys.splice(at as usize..at as usize, fresh);
        ChangeNotice::Inserted { at, n }
    }

    /// Removes the `n` items from Position `at` (both clipped to the count);
    /// returns the Change Notice the Grid needs.
    pub fn remove(&mut self, axis: Axis, at: Position, n: u64) -> ChangeNotice {
        let keys = self.keys(axis);
        let len = keys.len() as u64;
        let at = at.min(len);
        let n = n.min(len - at);
        keys.drain(at as usize..(at + n) as usize);
        ChangeNotice::Removed { at, n }
    }
}

impl KeyedTable {
    /// Reverses the order of `axis`; returns the Change Notice the Grid
    /// needs.
    pub fn reverse(&mut self, axis: Axis) -> ChangeNotice {
        self.keys(axis).reverse();
        ChangeNotice::Reordered
    }
}

impl ScenarioSource for KeyedTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        let keys = &self.axes[at_index(axis)];
        let end = (range.end as usize).min(keys.len());
        let start = (range.start as usize).min(end);
        keys[start..end].to_vec()
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        match self.axes[at_index(axis)].iter().position(|&k| k == key) {
            Some(p) => Located::Found(p as u64),
            None => Located::Gone { suggest: None },
        }
    }
}

fn at_index(axis: Axis) -> usize {
    match axis {
        Axis::Rows => 0,
        Axis::Cols => 1,
    }
}

/// A row of a [`RecordTable`]: one integer per column, `None` for null.
pub type Record = Vec<Option<i64>>;

/// A table of integer records behind an In-Memory Table Source, keyed by
/// Logical Index on both axes. Every column sorts unless left out; columns
/// move.
pub struct RecordTable {
    pub rows: Vec<Record>,
    pub source: MemorySource<Record>,
}

impl RecordTable {
    /// A table of `rows`, each `cols` wide, sortable by the columns in
    /// `sortable`.
    pub fn new(rows: Vec<Record>, cols: usize, sortable: impl IntoIterator<Item = u64>) -> Self {
        let mut source = MemorySource::<Record>::new(rows.len(), cols);
        for c in sortable {
            source = source.i64_column(Key(c), move |r: &Record| r[c as usize]);
        }
        RecordTable { rows, source }
    }

    /// Keeps only the records for which `keep` is true; the Grid needs the
    /// returned Change Notice.
    pub fn filter(&mut self, keep: impl Fn(&Record) -> bool + 'static) -> ChangeNotice {
        self.source.filter(&self.rows, keep)
    }

    /// Appends `records` to the base data; the Grid needs the returned
    /// Change Notice.
    pub fn append(&mut self, records: Vec<Record>) -> ChangeNotice {
        self.rows.extend(records);
        self.source.appended(&self.rows, &[])
    }

    /// Sets base row `row`'s value in column `col`; the Grid needs the
    /// returned Change Notice, if any.
    pub fn edit(&mut self, row: usize, col: usize, value: Option<i64>) -> Option<ChangeNotice> {
        self.rows[row][col] = value;
        self.source.edited(&self.rows, row, Key(col as u64))
    }

    /// Replaces the base data with `rows`; the Grid needs the returned
    /// Change Notice.
    pub fn replace(&mut self, rows: Vec<Record>) -> ChangeNotice {
        self.rows = rows;
        self.source.replaced(&self.rows, None)
    }
}

impl ScenarioSource for RecordTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.source.window(axis, range)
    }

    fn sort(&mut self, spec: &SortSpec) -> Result<ChangeNotice, SortSpec> {
        self.source
            .sort(&self.rows, spec)
            .map_err(|_| self.source.sort_spec().clone())
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.locate(axis, key)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        (axis == Axis::Cols).then(|| self.source.move_columns(keys, to))
    }
}

/// A scenario's source seen by the shared pump, logging each answer.
struct Answering<'a, S> {
    source: &'a mut S,
    log: &'a RefCell<String>,
}

impl<S: ScenarioSource> PumpSource for Answering<'_, S> {
    fn sort(&mut self, spec: &SortSpec) -> Result<ChangeNotice, SortSpec> {
        let answer = self.source.sort(spec);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  sort {:?}: {answer:?}",
            spec.columns()
        );
        answer
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let answer = self.source.locate(axis, key);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  locate {axis:?} {key:?}: {answer:?}"
        );
        answer
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        let answer = self.source.move_items(axis, keys, to);
        let _ = writeln!(
            self.log.borrow_mut(),
            "  move {axis:?} {keys:?} to {to}: {answer:?}"
        );
        answer
    }
}

/// A host in miniature: a Grid, its Table Source and a log of what passed
/// between them since the last step.
pub struct Scenario<S> {
    pub grid: Grid,
    pub source: S,
    log: String,
    steps: usize,
}

impl Scenario<IdentityTable> {
    /// A Grid over an identity Table Source with the configured counts.
    pub fn identity(rows: AxisConfig, cols: AxisConfig) -> Self {
        let source = IdentityTable {
            rows: IdentitySource::new(rows.count),
            cols: IdentitySource::new(cols.count),
        };
        Scenario::new(Grid::new(rows, cols), source)
    }
}

impl<S: ScenarioSource> Scenario<S> {
    pub fn new(grid: Grid, source: S) -> Self {
        Scenario {
            grid,
            source,
            log: String::new(),
            steps: 0,
        }
    }

    /// Pushes input into the Grid. Input calls return nothing; their Effects are
    /// drained by the next [`Scenario::pump`].
    pub fn input(&mut self, f: impl FnOnce(&mut Grid)) {
        f(&mut self.grid);
    }

    /// Drains the Effect queue through the shared pump, which answers sort
    /// requests and `Locate` from the source; logs each Effect and answer and
    /// returns the Effects in order.
    pub fn pump(&mut self) -> Vec<Stamped> {
        let mut drained = Vec::new();
        let answers = RefCell::new(String::new());
        let mut source = Answering {
            source: &mut self.source,
            log: &answers,
        };
        let log = &mut self.log;
        binnacle_memory::pump(&mut self.grid, &mut source, |_, stamped| {
            let _ = writeln!(
                log,
                "effect (gen {}): {:?}",
                stamped.generation, stamped.effect
            );
            // The pump answers an Effect before handing it on.
            log.push_str(&answers.take());
            drained.push(stamped);
        });
        drained
    }

    /// Runs one host frame: pump, `begin_frame`, fill each requested Window from
    /// the source, then take the Snapshot.
    pub fn frame(&mut self, viewport: Viewport) -> Snapshot<'_> {
        self.pump();
        for (axis, range) in self.grid.begin_frame(viewport) {
            let _ = writeln!(self.log, "window {axis:?} {range:?}");
            let keys = self.source.window(axis, range.clone());
            match self.source.sizes(axis, &keys) {
                Some((sizes, min_size)) => {
                    self.grid
                        .set_window_sized(axis, range.start, &keys, &sizes, min_size)
                }
                None => self.grid.set_window(axis, range.start, &keys),
            }
        }
        self.grid.snapshot()
    }

    /// Runs a frame and returns a numbered, labelled text rendering of it with
    /// everything logged since the previous step.
    pub fn step(&mut self, label: &str, viewport: Viewport) -> String {
        self.steps += 1;
        let header = format!("--- {}. {label}\n", self.steps);
        let snapshot = self.frame(viewport);
        let picture = render(&snapshot);
        let log = std::mem::take(&mut self.log);
        format!("{header}{log}{picture}")
    }
}
