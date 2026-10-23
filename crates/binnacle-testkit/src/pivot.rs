//! A pivot over hand-made columns, which come and go as the data would.

use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, Grouping, Key, Located, OpenState, Outline, Position, Support, Value,
    ViewSpec,
};
use binnacle_memory::{PumpSource, Viewed};

use crate::scenario::ScenarioSource;

/// A table whose columns, while the grouping asks for a pivot, are the
/// hand-made pivot columns it was given, each with its outline, in drawn
/// order; otherwise `plain` columns keyed by Position. Its rows are plain,
/// keyed by Position. Changing the pivot columns ([`PivotTable::set_columns`])
/// answers with `Refiltered`, never `Removed`, as a pivot's column axis
/// does, and its columns can't move while it pivots: their order is the
/// data's. The View Spec asked for is applied whole.
pub struct PivotTable {
    rows: u64,
    plain: u64,
    pivot: Vec<(Key, Outline)>,
    applied: ViewSpec,
}

impl PivotTable {
    pub fn new(rows: u64, plain: u64, pivot: Vec<(Key, Outline)>) -> Self {
        PivotTable {
            rows,
            plain,
            pivot,
            applied: ViewSpec::default(),
        }
    }

    /// A pivot column for `field` = `value`, under the column groups keyed
    /// `ancestors`: a group of the innermost level, which can't open.
    pub fn column(key: u64, ancestors: &[u64], field: Key, value: &str) -> (Key, Outline) {
        let up = ancestors.iter().copied().map(Key).collect();
        let value = Value::Text(value.into());
        (
            Key(key),
            Outline::group(up, OpenState::CantOpen, field, value),
        )
    }

    fn pivots(&self) -> bool {
        (self.applied.grouping.as_ref()).is_some_and(Grouping::pivot)
    }

    fn columns(&self) -> Vec<Key> {
        match self.pivots() {
            true => self.pivot.iter().map(|(k, _)| *k).collect(),
            false => (0..self.plain).map(Key).collect(),
        }
    }

    /// The pivot columns change, say as a value leaves the data: the
    /// Change Notice for the columns, if they show.
    pub fn set_columns(&mut self, pivot: Vec<(Key, Outline)>) -> Option<ChangeNotice> {
        self.pivot = pivot;
        let count = self.pivot.len() as u64;
        self.pivots().then_some(ChangeNotice::Refiltered(count))
    }
}

impl ScenarioSource for PivotTable {
    fn window(&mut self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        let keys = match axis {
            Axis::Rows => (0..self.rows).map(Key).collect(),
            Axis::Cols => self.columns(),
        };
        let end = (range.end as usize).min(keys.len());
        keys.get(range.start as usize..end)
            .map_or(Vec::new(), <[Key]>::to_vec)
    }

    fn outlines(&mut self, axis: Axis, keys: &[Key]) -> Option<Vec<Outline>> {
        if axis == Axis::Rows || !self.pivots() {
            return None;
        }
        Some(
            keys.iter()
                .filter_map(|k| self.pivot.iter().find(|(c, _)| c == k))
                .map(|(_, o)| o.clone())
                .collect(),
        )
    }
}

impl PumpSource for PivotTable {
    fn view(&mut self, spec: &ViewSpec) -> Viewed {
        let before = self.columns();
        self.applied = spec.clone();
        let after = self.columns();
        let notices = match before == after {
            true => Vec::new(),
            false => vec![(Axis::Cols, ChangeNotice::Refiltered(after.len() as u64))],
        };
        Viewed {
            applied: spec.clone(),
            totals: Default::default(),
            notices,
        }
    }

    fn support(&mut self, axis: Axis, _key: Key) -> Support {
        Support::default().with_group(axis == Axis::Cols)
    }

    fn movable(&mut self, axis: Axis) -> Option<bool> {
        Some(axis == Axis::Rows || !self.pivots())
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        let keys = match axis {
            Axis::Rows => (0..self.rows).map(Key).collect(),
            Axis::Cols => self.columns(),
        };
        match keys.iter().position(|&k| k == key) {
            Some(p) => Located::Found(p as u64),
            None => Located::Gone { suggest: None },
        }
    }
}
