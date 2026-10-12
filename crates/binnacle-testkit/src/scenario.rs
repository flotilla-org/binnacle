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

/// A row of a [`RecordTable`]: one integer per column, `None` for null.
pub type Record = Vec<Option<i64>>;

/// A table of integer records behind an In-Memory Table Source, keyed by
/// Logical Index on both axes. Every column sorts unless left out.
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
    pub fn filter(&mut self, keep: impl Fn(&Record) -> bool) -> ChangeNotice {
        self.source.filter(&self.rows, keep)
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
