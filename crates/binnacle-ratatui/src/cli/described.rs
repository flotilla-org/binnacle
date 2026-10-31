//! [`CliTable`] for a Described Table.

use std::ops::Range;

use binnacle_core::{Axis, Grid, Key, Position, Totals, ViewSpec};
use binnacle_described::DescribedSource;
use binnacle_format::{Role, Value};
use binnacle_group::Window;

use super::CliTable;

impl CliTable for DescribedSource {
    fn count(&self, axis: Axis) -> u64 {
        DescribedSource::count(self, axis)
    }

    fn window(&mut self, axis: Axis, range: Range<Position>) -> Window {
        DescribedSource::window(self, axis, range)
    }

    fn declare(&mut self, grid: &mut Grid) {
        DescribedSource::declare(self, grid)
    }

    fn columns(&self) -> Vec<Key> {
        DescribedSource::columns(self)
            .iter()
            .map(|c| c.key())
            .collect()
    }

    fn header(&self, col: Key) -> String {
        self.column(col)
            .map_or_else(String::new, |c| c.header().to_owned())
    }

    fn field(&self, col: Key) -> String {
        self.column(col)
            .map_or_else(String::new, |c| c.name().to_owned())
    }

    fn value(&self, row: Key, col: Key) -> Value {
        self.cell(row, col).cloned().unwrap_or(Value::Null)
    }

    fn unit(&self, col: Key) -> Option<String> {
        self.column(col)?.unit().map(String::from)
    }

    fn role(&self, col: Key) -> Role {
        self.column(col).map(|c| c.role()).unwrap_or_default()
    }

    fn view_changed(&mut self) -> Option<(ViewSpec, Totals)> {
        self.poll_view()
    }
}
