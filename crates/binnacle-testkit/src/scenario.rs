use std::fmt::Write;
use std::ops::Range;

use binnacle_core::{Axis, AxisConfig, Grid, Key, Position, Snapshot, Stamped, Viewport};
use binnacle_memory::IdentitySource;

use crate::render::render;

/// What a scenario's host fetches Keys from. Later tickets extend it as the
/// host learns to answer Effects.
pub trait ScenarioSource {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key>;
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

    /// Drains the Effect queue, logging each Effect, and returns them in order.
    pub fn pump(&mut self) -> Vec<Stamped> {
        let mut drained = Vec::new();
        while let Some(stamped) = self.grid.poll_effect() {
            let _ = writeln!(
                self.log,
                "effect (gen {}): {:?}",
                stamped.generation, stamped.effect
            );
            drained.push(stamped);
        }
        drained
    }

    /// Runs one host frame: pump, `begin_frame`, fill each requested Window from
    /// the source, then take the Snapshot.
    pub fn frame(&mut self, viewport: Viewport) -> Snapshot<'_> {
        self.pump();
        for (axis, range) in self.grid.begin_frame(viewport) {
            let _ = writeln!(self.log, "window {axis:?} {range:?}");
            let keys = self.source.window(axis, range.clone());
            self.grid.set_window(axis, range.start, &keys);
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
