//! The In-Memory Table Source for `fromRows` in `web/src/rows.ts`. The rows
//! stay in JS: TypeScript extracts each sorted column's keys and stages them
//! here, then sorts. Items are named by Logical Index (`u32`) both ways.

use std::borrow::Cow;

use binnacle_core::{Axis, Key, Located};
use binnacle_memory::{InMemoryTableSource, SortKeys};
use wasm_bindgen::prelude::*;

use crate::codes::{self, axis, position};

/// One sort column's keys, staged until [`WebMemory::sort`].
enum Staged {
    Numbers(Vec<Option<f64>>),
    /// Every string's UTF-8 bytes end to end; `ends[i]` is where row `i`'s end.
    Strings {
        bytes: Vec<u8>,
        ends: Vec<u32>,
        nulls: Vec<u8>,
    },
}

impl Staged {
    fn keys(&self) -> SortKeys<'_> {
        match self {
            Staged::Numbers(v) => SortKeys::F64(v.clone()),
            Staged::Strings { bytes, ends, nulls } => SortKeys::Bytes(
                ends.iter()
                    .enumerate()
                    .map(|(i, &end)| {
                        let start = if i == 0 { 0 } else { ends[i - 1] as usize };
                        (nulls[i] == 0).then(|| Cow::Borrowed(&bytes[start..end as usize]))
                    })
                    .collect(),
            ),
        }
    }
}

#[wasm_bindgen]
pub struct WebMemory {
    table: InMemoryTableSource,
    staged: Vec<Staged>,
}

#[wasm_bindgen]
impl WebMemory {
    #[wasm_bindgen(constructor)]
    pub fn new(rows: u32, cols: u32) -> WebMemory {
        WebMemory {
            table: InMemoryTableSource::new(rows as usize, cols as usize),
            staged: Vec::new(),
        }
    }

    pub fn count(&self, a: u8) -> f64 {
        self.table.count(axis(a)) as f64
    }

    /// The Logical Indices at Positions `start..end`, clipped to the count.
    pub fn window(&self, a: u8, start: f64, end: f64) -> Vec<u32> {
        self.table
            .window(axis(a), position(start)..position(end))
            .into_iter()
            .map(|k| k.0 as u32)
            .collect()
    }

    /// Where Logical Index `item` sits: `[code, position]` as
    /// [`codes::located`] reads them.
    pub fn locate(&mut self, a: u8, item: u32) -> Vec<f64> {
        match self.table.locate(axis(a), Key(item as u64)) {
            Located::Found(p) => vec![0.0, p as f64],
            Located::Gone { suggest } => vec![1.0, suggest.map_or(f64::NAN, |p| p as f64)],
        }
    }

    /// Stages the next sort column's keys as numbers, one per base row;
    /// `nulls[i] != 0` makes row `i` null.
    pub fn stage_numbers(&mut self, values: &[f64], nulls: &[u8]) {
        let keys = values
            .iter()
            .zip(nulls)
            .map(|(&v, &null)| (null == 0).then_some(v))
            .collect();
        self.staged.push(Staged::Numbers(keys));
    }

    /// Stages the next sort column's keys as UTF-8 strings, which sort by
    /// code point: row `i`'s bytes end at `ends[i]`.
    pub fn stage_strings(&mut self, bytes: Vec<u8>, ends: Vec<u32>, nulls: Vec<u8>) {
        self.staged.push(Staged::Strings { bytes, ends, nulls });
    }

    /// Sorts the rows by the Sort Spec of column Logical Indices and
    /// directions (see [`codes::sort_spec`]), one staged column each, in
    /// order. The rows need a `Reordered` notice.
    pub fn sort(&mut self, cols: &[u64], directions: &[u8]) {
        let staged = std::mem::take(&mut self.staged);
        let keys = staged.iter().map(Staged::keys).collect();
        self.table
            .apply_sort(&codes::sort_spec(cols, directions), keys);
    }

    /// Keeps the base rows with `keep[i] != 0`; returns the new row count
    /// for the `Refiltered` the rows need.
    pub fn filter(&mut self, keep: &[u8]) -> f64 {
        self.table
            .apply_filter(keep.iter().map(|&k| k != 0).collect());
        self.table.count(Axis::Rows) as f64
    }

    /// Keeps every row again; returns the row count for the `Refiltered`.
    pub fn clear_filter(&mut self) -> f64 {
        self.table.clear_filter();
        self.table.count(Axis::Rows) as f64
    }

    /// Base row `row` changed in the data itself and the filter now keeps
    /// it (`keep != 0`) or not: it comes or goes at once. Returns whether it
    /// did, so the rows need a `Refiltered`.
    pub fn refilter_row(&mut self, row: u32, keep: u8) -> bool {
        self.table.refilter_row(row as usize, keep != 0).is_some()
    }

    /// The user edited base row `row` and the filter now keeps it or not: a
    /// row in view stays, unmatched while the filter drops it. Returns
    /// whether a row out of view came in, so the rows need a `Refiltered`.
    pub fn defer_row(&mut self, row: u32, keep: u8) -> bool {
        self.table.defer_row(row as usize, keep != 0).is_some()
    }

    /// For each row with these Logical Indices, 1 if it is unmatched: in
    /// view though the filter drops it.
    pub fn unmatched(&mut self, items: &[u32]) -> Vec<u8> {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        (self.table.unmatched(Axis::Rows, &keys).into_iter())
            .map(u8::from)
            .collect()
    }

    /// Whether any row is unmatched.
    pub fn has_unmatched(&self) -> bool {
        self.table.has_unmatched(Axis::Rows)
    }

    /// Moves the columns with these Logical Indices to before Position `to`.
    /// The columns need a `Reordered` notice.
    pub fn move_columns(&mut self, items: &[u32], to: f64) {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        self.table.move_columns(&keys, position(to));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: u8 = 0;

    fn order(m: &WebMemory) -> Vec<u32> {
        m.window(ROWS, 0.0, m.count(ROWS))
    }

    #[test]
    fn staged_strings_sort_by_code_point_with_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        // "b", null, "a", "é" (two bytes, after every ASCII letter).
        let bytes = b"ba\xc3\xa9".to_vec();
        m.stage_strings(bytes, vec![1, 1, 2, 4], vec![0, 1, 0, 0]);
        m.sort(&[0], &[1]);
        assert_eq!(order(&m), vec![3, 0, 2, 1]);
    }

    #[test]
    fn staged_numbers_put_nan_after_numbers_and_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        m.stage_numbers(&[f64::NAN, 2.0, 0.0, 1.0], &[0, 0, 1, 0]);
        m.sort(&[0], &[0]);
        assert_eq!(order(&m), vec![3, 1, 0, 2]);
    }

    #[test]
    fn filtering_answers_the_new_count_and_locates_dropped_rows_as_gone() {
        let mut m = WebMemory::new(4, 1);
        assert_eq!(m.filter(&[1, 0, 1, 1]), 3.0);
        assert_eq!(m.locate(ROWS, 2), vec![0.0, 1.0]);
        assert_eq!(m.locate(ROWS, 1)[0], 1.0);
        assert_eq!(m.clear_filter(), 4.0);
    }

    #[test]
    fn an_edited_row_stays_unmatched_until_the_rows_are_filtered_again() {
        let mut m = WebMemory::new(3, 1);
        m.filter(&[1, 1, 0]);
        assert!(!m.defer_row(1, 0));
        assert_eq!(order(&m), vec![0, 1]);
        assert_eq!(m.unmatched(&[0, 1, 2]), vec![0, 1, 0]);
        assert!(m.has_unmatched());
        assert!(m.refilter_row(1, 0));
        assert_eq!(order(&m), vec![0]);
        assert!(!m.has_unmatched());
    }

    #[test]
    fn moved_columns_take_their_new_positions() {
        let mut m = WebMemory::new(1, 3);
        m.move_columns(&[2], 0.0);
        assert_eq!(m.window(1, 0.0, 3.0), vec![2, 0, 1]);
    }
}
