//! The In-Memory Table Source for `fromRows` in `web/src/rows.ts`. The rows
//! stay in JS: TypeScript extracts each sorted column's keys and stages them
//! here, then sorts. Items are named by Logical Index (`u32`) both ways.

use std::borrow::Cow;

use binnacle_core::{Key, Located};
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

    /// Stages the next sort item's keys as numbers, one per base item of
    /// the axis to sort; `nulls[i] != 0` makes item `i` null.
    pub fn stage_numbers(&mut self, values: &[f64], nulls: &[u8]) {
        let keys = values
            .iter()
            .zip(nulls)
            .map(|(&v, &null)| (null == 0).then_some(v))
            .collect();
        self.staged.push(Staged::Numbers(keys));
    }

    /// Stages the next sort item's keys as UTF-8 strings, which sort by
    /// code point: item `i`'s bytes end at `ends[i]`.
    pub fn stage_strings(&mut self, bytes: Vec<u8>, ends: Vec<u32>, nulls: Vec<u8>) {
        self.staged.push(Staged::Strings { bytes, ends, nulls });
    }

    /// Sorts axis `a` by the Sort Spec of Logical Indices of the other
    /// axis's items and directions (see [`codes::sort_spec`]), one staged
    /// item each, in order. The axis needs a `Reordered` notice.
    pub fn sort(&mut self, a: u8, items: &[u64], directions: &[u8]) {
        let staged = std::mem::take(&mut self.staged);
        let keys = staged.iter().map(Staged::keys).collect();
        self.table
            .apply_sort(axis(a), &codes::sort_spec(items, directions), keys);
    }

    /// Keeps the base items on axis `a` with `keep[i] != 0`; returns the
    /// new count for the `Refiltered` the axis needs.
    pub fn filter(&mut self, a: u8, keep: &[u8]) -> f64 {
        let keep = keep.iter().map(|&k| k != 0).collect();
        self.table.apply_filter(axis(a), keep);
        self.table.count(axis(a)) as f64
    }

    /// Keeps every item on axis `a` again; returns the count for the
    /// `Refiltered`.
    pub fn clear_filter(&mut self, a: u8) -> f64 {
        self.table.clear_filter(axis(a));
        self.table.count(axis(a)) as f64
    }

    /// Base item `item` on axis `a` changed in the data itself and the
    /// filter now keeps it (`keep != 0`) or not: it comes or goes at once.
    /// Returns whether it did, so the axis needs a `Refiltered`.
    pub fn refilter(&mut self, a: u8, item: u32, keep: u8) -> bool {
        (self.table)
            .refilter_item(axis(a), item as usize, keep != 0)
            .is_some()
    }

    /// The user edited base item `item` on axis `a` and the filter now keeps
    /// it or not: an item in view stays, unmatched while the filter drops
    /// it. Returns whether an item out of view came in, so the axis needs a
    /// `Refiltered`.
    pub fn defer(&mut self, a: u8, item: u32, keep: u8) -> bool {
        (self.table)
            .defer_item(axis(a), item as usize, keep != 0)
            .is_some()
    }

    /// For each item on axis `a` with these Logical Indices, 1 if it is
    /// unmatched: in view though the filter drops it.
    pub fn unmatched(&mut self, a: u8, items: &[u32]) -> Vec<u8> {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        (self.table.unmatched(axis(a), &keys).into_iter())
            .map(u8::from)
            .collect()
    }

    /// Whether any item on axis `a` is unmatched.
    pub fn has_unmatched(&self, a: u8) -> bool {
        self.table.has_unmatched(axis(a))
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
    const COLS: u8 = 1;

    fn order(m: &WebMemory) -> Vec<u32> {
        m.window(ROWS, 0.0, m.count(ROWS))
    }

    #[test]
    fn staged_strings_sort_by_code_point_with_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        // "b", null, "a", "é" (two bytes, after every ASCII letter).
        let bytes = b"ba\xc3\xa9".to_vec();
        m.stage_strings(bytes, vec![1, 1, 2, 4], vec![0, 1, 0, 0]);
        m.sort(ROWS, &[0], &[1]);
        assert_eq!(order(&m), vec![3, 0, 2, 1]);
    }

    #[test]
    fn staged_numbers_put_nan_after_numbers_and_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        m.stage_numbers(&[f64::NAN, 2.0, 0.0, 1.0], &[0, 0, 1, 0]);
        m.sort(ROWS, &[0], &[0]);
        assert_eq!(order(&m), vec![3, 1, 0, 2]);
    }

    #[test]
    fn filtering_answers_the_new_count_and_locates_dropped_rows_as_gone() {
        let mut m = WebMemory::new(4, 1);
        assert_eq!(m.filter(ROWS, &[1, 0, 1, 1]), 3.0);
        assert_eq!(m.locate(ROWS, 2), vec![0.0, 1.0]);
        assert_eq!(m.locate(ROWS, 1)[0], 1.0);
        assert_eq!(m.clear_filter(ROWS), 4.0);
    }

    #[test]
    fn the_columns_filter_apart_from_the_rows() {
        let mut m = WebMemory::new(2, 4);
        assert_eq!(m.filter(COLS, &[0, 1, 0, 1]), 2.0);
        assert_eq!(m.window(COLS, 0.0, 4.0), vec![1, 3]);
        assert_eq!(order(&m), vec![0, 1]);
        assert!(!m.defer(COLS, 3, 0));
        assert_eq!(m.unmatched(COLS, &[1, 3]), vec![0, 1]);
        assert!(m.has_unmatched(COLS) && !m.has_unmatched(ROWS));
        assert!(m.refilter(COLS, 0, 1));
        assert_eq!(m.window(COLS, 0.0, 4.0), vec![0, 1, 3]);
        assert_eq!(m.clear_filter(COLS), 4.0);
    }

    #[test]
    fn an_edited_row_stays_unmatched_until_the_rows_are_filtered_again() {
        let mut m = WebMemory::new(3, 1);
        m.filter(ROWS, &[1, 1, 0]);
        assert!(!m.defer(ROWS, 1, 0));
        assert_eq!(order(&m), vec![0, 1]);
        assert_eq!(m.unmatched(ROWS, &[0, 1, 2]), vec![0, 1, 0]);
        assert!(m.has_unmatched(ROWS));
        assert!(m.refilter(ROWS, 1, 0));
        assert_eq!(order(&m), vec![0]);
        assert!(!m.has_unmatched(ROWS));
    }

    #[test]
    fn staged_keys_of_a_row_sort_the_columns() {
        let mut m = WebMemory::new(1, 3);
        m.stage_numbers(&[2.0, 0.0, 1.0], &[0, 1, 0]);
        m.sort(COLS, &[0], &[0]);
        assert_eq!(m.window(COLS, 0.0, 3.0), vec![2, 0, 1]);
        assert_eq!(order(&m), vec![0]);
    }

    #[test]
    fn moved_columns_take_their_new_positions() {
        let mut m = WebMemory::new(1, 3);
        m.move_columns(&[2], 0.0);
        assert_eq!(m.window(1, 0.0, 3.0), vec![2, 0, 1]);
    }
}
