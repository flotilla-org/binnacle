use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use binnacle_core::{Axis, ChangeNotice, Grid, Key, Located, Position, SortSpec, Stamped};

use crate::{InMemoryTableSource, PumpSource, SortKeys};

/// A Sort Spec named a column with no sort-key closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsortable {
    pub column: Key,
}

impl fmt::Display for Unsortable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "column {:?} has no sort keys", self.column)
    }
}

impl std::error::Error for Unsortable {}

type Extract<R, T> = Box<dyn Fn(&R) -> Option<T>>;
type Keep<R> = Box<dyn Fn(&R) -> bool>;
type ExtractBytes<R> = Box<dyn for<'r> Fn(&'r R) -> Option<Cow<'r, [u8]>>>;

enum Column<R> {
    I64(Extract<R, i64>),
    F64(Extract<R, f64>),
    Bytes(ExtractBytes<R>),
}

impl<R> Column<R> {
    fn extract<'r>(&self, rows: &'r [R]) -> SortKeys<'r> {
        match self {
            Column::I64(f) => SortKeys::I64(rows.iter().map(f).collect()),
            Column::F64(f) => SortKeys::F64(rows.iter().map(f).collect()),
            Column::Bytes(f) => SortKeys::Bytes(rows.iter().map(f).collect()),
        }
    }
}

/// An In-Memory Table Source for rows of type `R` in a host's slice, with a
/// closure per sortable column extracting its sort key from a row. It never
/// holds the rows: each sort borrows them.
pub struct MemorySource<R> {
    table: InMemoryTableSource,
    columns: HashMap<Key, Column<R>>,
    /// The filter in force, kept to place rows the base data gains.
    keep: Option<Keep<R>>,
}

impl<R> MemorySource<R> {
    /// A source over `rows` base rows and `cols` base columns, none of them
    /// sortable until given a closure.
    pub fn new(rows: usize, cols: usize) -> Self {
        MemorySource {
            table: InMemoryTableSource::new(rows, cols),
            columns: HashMap::new(),
            keep: None,
        }
    }

    /// See [`InMemoryTableSource::with_keys`].
    pub fn with_keys(mut self, axis: Axis, keys: Vec<Key>) -> Self {
        self.table = self.table.with_keys(axis, keys);
        self
    }

    /// Makes the column with Key `column` sort by an integer; `None` is null.
    pub fn i64_column(mut self, column: Key, f: impl Fn(&R) -> Option<i64> + 'static) -> Self {
        self.columns.insert(column, Column::I64(Box::new(f)));
        self
    }

    /// Makes the column with Key `column` sort by a float; `None` is null.
    pub fn f64_column(mut self, column: Key, f: impl Fn(&R) -> Option<f64> + 'static) -> Self {
        self.columns.insert(column, Column::F64(Box::new(f)));
        self
    }

    /// Makes the column with Key `column` sort by a byte string, borrowed from
    /// the row or computed; `None` is null.
    pub fn bytes_column(
        mut self,
        column: Key,
        f: impl for<'r> Fn(&'r R) -> Option<Cow<'r, [u8]>> + 'static,
    ) -> Self {
        self.columns.insert(column, Column::Bytes(Box::new(f)));
        self
    }

    /// Sorts by `spec`, extracting each spec column's keys from `rows`, and
    /// returns the Change Notice the rows need. If a spec column has no
    /// closure, nothing changes.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn sort(&mut self, rows: &[R], spec: &SortSpec) -> Result<ChangeNotice, Unsortable> {
        let columns = spec
            .columns()
            .iter()
            .map(|c| self.columns.get(&c.key).ok_or(Unsortable { column: c.key }))
            .collect::<Result<Vec<_>, _>>()?;
        let keys = columns.into_iter().map(|c| c.extract(rows)).collect();
        Ok(self.table.apply_sort(spec, keys))
    }

    /// Keeps only the rows for which `keep` is true, in sort order, and
    /// returns the Change Notice the rows need. The sort is not redone. The
    /// filter stays in force for rows the base data gains or changes.
    ///
    /// # Panics
    ///
    /// If `rows` is not as long as the base row count.
    pub fn filter(&mut self, rows: &[R], keep: impl Fn(&R) -> bool + 'static) -> ChangeNotice {
        let notice = self.table.apply_filter(rows.iter().map(&keep).collect());
        self.keep = Some(Box::new(keep));
        notice
    }

    /// See [`InMemoryTableSource::clear_filter`].
    pub fn clear_filter(&mut self) -> ChangeNotice {
        self.keep = None;
        self.table.clear_filter()
    }

    /// Rows were appended to the host's base data, now `rows`, keyed by
    /// `keys` if the rows have host Keys (empty otherwise). They take their
    /// place in the sort in force and the filter in force drops those it
    /// rejects. See [`InMemoryTableSource::append_rows`] for the notice.
    ///
    /// # Panics
    ///
    /// If `rows` is shorter than before, or as [`InMemoryTableSource::append_rows`].
    pub fn appended(&mut self, rows: &[R], keys: &[Key]) -> ChangeNotice {
        let old = self.table.base_rows();
        let n = rows
            .len()
            .checked_sub(old)
            .expect("rows were appended, not removed");
        let keep: Vec<bool> = self
            .keep
            .as_ref()
            .map_or_else(Vec::new, |keep| rows[old..].iter().map(keep).collect());
        let sort_keys = self.sort_keys(rows);
        self.table.append_rows(n, keys, sort_keys, &keep)
    }

    /// The value in `column` of base row `row` (a Logical Index into `rows`)
    /// changed. Returns `ContentChanged` for the row's Position, `Reordered`
    /// if the rows are sorted by that column, `Reset` if the filter now keeps
    /// or drops the row where it did not before, or `None` if the row is
    /// filtered out either way.
    ///
    /// # Panics
    ///
    /// If `row` is not a row of `rows`, or `rows` is not as long as the base
    /// row count.
    pub fn edited(&mut self, rows: &[R], row: usize, column: Key) -> Option<ChangeNotice> {
        if let Some(keep) = &self.keep
            && keep(&rows[row]) != self.table.row_position(row).is_some()
        {
            return Some(self.reorder(rows));
        }
        if self.sort_spec().find(column).is_some() {
            self.resort(rows);
            return Some(ChangeNotice::Reordered);
        }
        let p = self.table.row_position(row)?;
        Some(ChangeNotice::ContentChanged(p..p + 1))
    }

    /// The host's base data changed in some other way (rows inserted or
    /// removed anywhere, or replaced): it is now `rows`, keyed by `keys` (by
    /// Logical Index if `None`). The sort and filter in force are applied
    /// again and the rows need the `Reset` returned.
    ///
    /// # Panics
    ///
    /// As [`InMemoryTableSource::replace_rows`].
    pub fn replaced(&mut self, rows: &[R], keys: Option<Vec<Key>>) -> ChangeNotice {
        let keep: Vec<bool> = self
            .keep
            .as_ref()
            .map_or_else(Vec::new, |keep| rows.iter().map(keep).collect());
        let sort_keys = self.sort_keys(rows);
        self.table.replace_rows(rows.len(), keys, sort_keys, keep)
    }

    /// The sort keys of `rows` for the Sort Spec in force.
    fn sort_keys<'r>(&self, rows: &'r [R]) -> Vec<SortKeys<'r>> {
        // Every spec column had a closure when it was applied.
        self.sort_spec()
            .columns()
            .iter()
            .map(|c| self.columns[&c.key].extract(rows))
            .collect()
    }

    /// Sorts `rows` again by the Sort Spec in force.
    fn resort(&mut self, rows: &[R]) {
        let spec = self.sort_spec().clone();
        let _ = self.sort(rows, &spec);
    }

    /// Applies the sort and filter in force to `rows` again; returns the
    /// `Reset` the rows need.
    fn reorder(&mut self, rows: &[R]) -> ChangeNotice {
        if !self.sort_spec().is_empty() {
            self.resort(rows);
        }
        match &self.keep {
            Some(keep) => self.table.apply_filter(rows.iter().map(keep).collect()),
            None => ChangeNotice::Reset(self.count(Axis::Rows)),
        }
    }

    /// The Sort Spec the rows are in, as last applied.
    pub fn sort_spec(&self) -> &SortSpec {
        self.table.sort_spec()
    }

    /// Runs [`crate::pump`] with this source over the host's `rows`: sort
    /// requests are answered by sorting them (or rejected if a column has no
    /// closure), `Locate` by the Remap, column moves by moving the columns.
    /// `seen` gets every Effect drained.
    pub fn pump(&mut self, rows: &[R], grid: &mut Grid, seen: impl FnMut(&mut Grid, Stamped)) {
        crate::pump(grid, &mut self.over(rows), seen);
    }

    /// This source with the host's `rows`, as [`crate::pump`] needs it: the
    /// answers [`MemorySource::pump`] gives, for hosts that drive the pump
    /// themselves.
    pub fn over<'a>(&'a mut self, rows: &'a [R]) -> impl PumpSource + 'a {
        Over { source: self, rows }
    }

    /// See [`InMemoryTableSource::move_columns`]. Sorting names columns by
    /// Key, so it is unaffected by the column order.
    pub fn move_columns(&mut self, keys: &[Key], to: Position) -> ChangeNotice {
        self.table.move_columns(keys, to)
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.table.count(axis)
    }

    pub fn window(&self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.table.window(axis, range)
    }

    pub fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.table.locate(axis, key)
    }
}

/// A [`MemorySource`] with the rows it sorts, as the pump needs it.
struct Over<'a, R> {
    source: &'a mut MemorySource<R>,
    rows: &'a [R],
}

impl<R> PumpSource for Over<'_, R> {
    fn sort(&mut self, spec: &SortSpec) -> Result<ChangeNotice, SortSpec> {
        self.source
            .sort(self.rows, spec)
            .map_err(|_| self.source.sort_spec().clone())
    }

    fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.source.locate(axis, key)
    }

    fn move_items(&mut self, axis: Axis, keys: &[Key], to: Position) -> Option<ChangeNotice> {
        (axis == Axis::Cols).then(|| self.source.move_columns(keys, to))
    }
}
