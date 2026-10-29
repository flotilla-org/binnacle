//! The values example's table, shared with its snapshot test: the shared
//! fixture of Typed Values, one Value Renderer a column, each column as wide
//! as its widest cell.

use binnacle_core::{Axis, AxisConfig, Grid, Key, Viewport};
use binnacle_format::Value;
use binnacle_format::fixture::Table;
use binnacle_ratatui::{CellRenderer, Content, ValueRenderer, fit_width};

pub struct Values {
    pub table: Table,
}

impl Values {
    pub fn new() -> Values {
        Values {
            table: Table::values(),
        }
    }

    /// A Grid over the fixture, its columns fitted to their cells.
    pub fn grid(&self) -> Grid {
        let (rows, cols) = (
            self.table.rows.len() as u64,
            self.table.columns.len() as u64,
        );
        let mut grid = Grid::new(
            AxisConfig::new(rows, 1.0).header(1.0),
            AxisConfig::new(cols, 10.0).header(3.0).gap(1.0),
        );
        let row_keys: Vec<Key> = (0..rows).map(Key).collect();
        for col in 0..cols {
            let width = fit_width(self, &row_keys, Key(col));
            grid.set_size(Axis::Cols, Key(col), width.max(4) as f32);
        }
        grid
    }

    /// Column `col`'s Value Renderer, its clock starting at the fixture's
    /// own time and its dates in the fixture's zone.
    pub fn renderer(&self, col: Key) -> ValueRenderer<impl Fn(Key, Key) -> Value + '_> {
        let c = col.0 as usize;
        let column = &self.table.columns[c];
        let renderer = ValueRenderer::new(self.table.now, move |row: Key, _| {
            self.table.rows[row.0 as usize][c].clone()
        })
        .role(column.role)
        .options(column.options.clone())
        .zone(self.table.zone);
        match &column.unit {
            Some(unit) => renderer.unit(unit.clone()),
            None => renderer,
        }
    }

    /// Fills the Grid's Windows: rows and columns are keyed by Position.
    pub fn fill(&self, grid: &mut Grid, viewport: Viewport) {
        for (axis, range) in grid.begin_frame(viewport) {
            let keys: Vec<Key> = range.clone().map(Key).collect();
            grid.set_window(axis, range.start, &keys);
        }
    }
}

impl Content for Values {
    /// The stable text copy and find read: dates absolute.
    fn cell(&self, row: Key, col: Key) -> String {
        self.renderer(col).text(row, col)
    }

    fn column_header(&self, col: Key) -> String {
        let column = &self.table.columns[col.0 as usize];
        match &column.unit {
            Some(unit) => format!("{} ({unit})", column.name),
            None => column.name.clone(),
        }
    }

    fn column_renderer(&self, col: Key) -> Box<dyn CellRenderer + '_> {
        Box::new(self.renderer(col))
    }
}
