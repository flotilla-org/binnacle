//! The In-Memory Table Source for `fromRows` in `web/src/rows.ts`. The rows
//! stay in JS: TypeScript extracts each sorted column's keys and stages them
//! here, then sorts. Items are named by Logical Index (`u32`) both ways.
//!
//! Grouped rows go through the grouping layer, answered from the rows in
//! view: TypeScript stages each level's values as text, sorts the rows by
//! the levels' fields first, then groups ([`WebMemory::group`]).

use std::borrow::Cow;

use binnacle_core::{Axis, Grouping, ItemKind, Key, Level, Located, Value, ViewSpec};
use binnacle_group::{
    AggregateBackend, Answer, Batch, Figure, GroupingLayer, MemoryAggregates, settle,
};
use binnacle_memory::{InMemoryTableSource, MovePolicy, SortKeys};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::codes::{self, axis, position};
use crate::grid::to_js;
use crate::grouping::{WireItemKind, WireOpenState};

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

/// Strings staged as for [`WebMemory::stage_strings`], as Values.
fn values(bytes: &[u8], ends: &[u32], nulls: &[u8]) -> Vec<Value> {
    (ends.iter().enumerate())
        .map(|(i, &end)| {
            let start = if i == 0 { 0 } else { ends[i - 1] as usize };
            let text = String::from_utf8_lossy(&bytes[start..end as usize]);
            Value::of((nulls[i] == 0).then_some(&*text))
        })
        .collect()
}

/// The grouping layer's backend: the rows in view, grouped once per stamp.
struct Backend<'a> {
    table: &'a InMemoryTableSource,
    levels: &'a [Vec<Value>],
    aggregates: &'a mut Option<MemoryAggregates>,
}

impl AggregateBackend<Figure> for Backend<'_> {
    fn answer(&mut self, batch: &Batch) -> Vec<Answer<Figure>> {
        if self.aggregates.as_ref().map(|a| a.stamp()) != Some(batch.stamp) {
            let keys = self.table.window(Axis::Rows, 0..self.table.count(Axis::Rows));
            let levels = self.levels;
            *self.aggregates = Some(MemoryAggregates::new(batch, &keys, |level, i| {
                let item = keys[i].0 as usize;
                levels[level].get(item).cloned().unwrap_or(Value::Blank)
            }));
        }
        self.aggregates
            .as_mut()
            .map_or(Vec::new(), |a| a.answer(batch))
    }
}

/// One item of a grouped Window: its layer Key, its Logical Index if a
/// Leaf, its kind, open state and depth, the path of its group (a Leaf's
/// parent's) as each level's column index and text (`null` for a blank),
/// and a Group Item's count.
#[derive(Serialize)]
struct GroupedItem {
    key: u64,
    item: Option<u32>,
    kind: WireItemKind,
    open: WireOpenState,
    depth: u32,
    path: Vec<(u32, Option<String>)>,
    count: Option<f64>,
}

#[wasm_bindgen]
pub struct WebMemory {
    table: InMemoryTableSource,
    staged: Vec<Staged>,
    layer: GroupingLayer<Figure>,
    aggregates: Option<MemoryAggregates>,
    /// Each level's value of every row, by Logical Index.
    levels: Vec<Vec<Value>>,
    staged_levels: Vec<Vec<Value>>,
}

#[wasm_bindgen]
impl WebMemory {
    #[wasm_bindgen(constructor)]
    pub fn new(rows: u32, cols: u32) -> WebMemory {
        WebMemory {
            table: InMemoryTableSource::new(rows as usize, cols as usize),
            staged: Vec::new(),
            layer: GroupingLayer::new(),
            aggregates: None,
            levels: Vec::new(),
            staged_levels: Vec::new(),
        }
    }

    /// The items on axis `a`: Group Items included while the rows are
    /// grouped.
    pub fn count(&self, a: u8) -> f64 {
        match self.layer.count(axis(a)) {
            Some(n) => n as f64,
            None => self.table.count(axis(a)) as f64,
        }
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
        let located = match self.layer.grouped(axis(a)) {
            true => self.locate_grouped(Key(item as u64)),
            false => self.table.locate(axis(a), Key(item as u64)),
        };
        match located {
            Located::Found(p) => vec![0.0, p as f64],
            Located::Gone { suggest } => vec![1.0, suggest.map_or(f64::NAN, |p| p as f64)],
        }
    }

    /// Where the Summary Item with the layer's Key `key` sits, as
    /// [`WebMemory::locate`] answers.
    pub fn locate_summary(&mut self, key: u64) -> Vec<f64> {
        match self.locate_grouped(Key(key)) {
            Located::Found(p) => vec![0.0, p as f64],
            Located::Gone { .. } => vec![1.0, f64::NAN],
        }
    }

    /// Stages the next level's values as UTF-8 strings, one per row, as
    /// [`WebMemory::stage_strings`] takes them.
    pub fn stage_level(&mut self, bytes: Vec<u8>, ends: Vec<u32>, nulls: Vec<u8>) {
        self.staged_levels.push(values(&bytes, &ends, &nulls));
    }

    /// Groups the rows by the columns `fields` (indices), outermost first,
    /// one staged level each, over the rows in view as they are sorted and
    /// filtered now. Returns the new count for the `Refiltered`.
    pub fn group(&mut self, fields: &[u32]) -> f64 {
        self.levels = std::mem::take(&mut self.staged_levels);
        let mut grouping = Grouping::default();
        grouping.row_levels = (fields.iter())
            .map(|&f| Level::field(Key(f as u64)))
            .collect();
        grouping.values = Grouping::rows_by(Key(0)).values;
        self.layer
            .request_view(ViewSpec::default().with_grouping(Some(grouping)));
        self.settle();
        self.count(0)
    }

    /// Ungroups the rows; returns the count for the `Refiltered`.
    pub fn ungroup(&mut self) -> f64 {
        self.levels.clear();
        self.layer.request_view(ViewSpec::default());
        self.settle();
        self.count(0)
    }

    /// Groups the rows again after they were sorted, filtered or hidden;
    /// returns the count for the `Refiltered`.
    pub fn regroup(&mut self) -> f64 {
        if self.layer.grouped(Axis::Rows) {
            self.layer.refresh();
            self.settle();
        }
        self.count(0)
    }

    /// Whether the rows are grouped.
    pub fn grouped(&self) -> bool {
        self.layer.grouped(Axis::Rows)
    }

    /// The grouped rows' items at Positions `start..end`, clipped to the
    /// count: an array of objects, each Leaf by its Logical Index and every
    /// item with its outline and its group's path.
    pub fn grouped_window(&mut self, start: f64, end: f64) -> JsValue {
        let range = position(start)..position(end);
        let want = range.end.min(self.count(0) as u64).saturating_sub(range.start) as usize;
        let mut items = self.layer.window(Axis::Rows, range.clone());
        while items.len() < want {
            self.settle();
            let more = self.layer.window(Axis::Rows, range.clone());
            if more.len() <= items.len() {
                break;
            }
            items = more;
        }
        let path = |key: Key| {
            let summary = self.layer.summary(key);
            let path = summary.map_or(&[][..], |s| &s.path[..]);
            (path.iter())
                .map(|(field, value)| {
                    let text = match value {
                        Value::Text(t) => Some(t.clone()),
                        Value::Blank => None,
                    };
                    (field.0 as u32, text)
                })
                .collect::<Vec<_>>()
        };
        let items: Vec<GroupedItem> = (items.into_iter())
            .map(|(key, outline)| {
                let leaf = outline.kind == ItemKind::Leaf;
                let count = (self.layer.summary(key)).and_then(|s| match s.aggregates {
                    [Figure::Count(n)] => Some(*n as f64),
                    _ => None,
                });
                GroupedItem {
                    key: key.0,
                    item: leaf.then_some(key.0 as u32),
                    kind: (&outline).into(),
                    open: (&outline).into(),
                    depth: outline.depth,
                    path: match leaf {
                        true => outline.parent().map_or_else(Vec::new, path),
                        false => path(key),
                    },
                    count,
                }
            })
            .collect();
        to_js(&items)
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
    /// item each, in order. Returns whether the items shown moved: the axis
    /// then needs a `Reordered` notice.
    pub fn sort(&mut self, a: u8, items: &[u64], directions: &[u8]) -> bool {
        let staged = std::mem::take(&mut self.staged);
        let keys = staged.iter().map(Staged::keys).collect();
        let spec = codes::sort_spec(items, directions);
        self.table.resort(axis(a), &spec, keys).is_some()
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

    /// The data changed and the filter now keeps each base item on axis `a`
    /// as `keep` says: an item whose flag changed comes or goes at once, the
    /// rest stay as they are. Returns whether any came or went, so the axis
    /// needs a `Refiltered`.
    pub fn refilter_all(&mut self, a: u8, keep: &[u8]) -> bool {
        let keep: Vec<bool> = keep.iter().map(|&k| k != 0).collect();
        self.table.refilter_items(axis(a), &keep).is_some()
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

    /// Hides the base items on axis `a` with these Logical Indices and shows
    /// every other, apart from the filter (indices past the last are
    /// skipped). Returns whether any came or went, so the axis needs a
    /// `Refiltered`.
    pub fn hide(&mut self, a: u8, items: &[u32]) -> bool {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        self.table.apply_hidden(axis(a), &keys).is_some()
    }

    /// Moves the items on axis `a` with these Logical Indices to before
    /// Position `to`, read in the filtered order. On a sorted axis the
    /// order shown is baked and the axis's sort cleared, unless moves on a
    /// sorted axis are refused: then nothing moves and this is `false`.
    /// The axis needs a `Reordered` notice if it moved.
    pub fn move_items(&mut self, a: u8, items: &[u32], to: f64) -> bool {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        self.table
            .move_items(axis(a), &keys, position(to))
            .is_some()
    }

    /// Whether the items on axis `a` can move now.
    pub fn movable(&self, a: u8) -> bool {
        self.table.movable(axis(a))
    }

    /// Whether moves on a sorted axis are refused rather than baked.
    pub fn refuse_sorted_moves(&mut self, refuse: bool) {
        let policy = if refuse {
            MovePolicy::Refuse
        } else {
            MovePolicy::Bake
        };
        self.table.set_move_policy(policy);
    }
}

impl WebMemory {
    fn settle(&mut self) {
        let mut backend = Backend {
            table: &self.table,
            levels: &self.levels,
            aggregates: &mut self.aggregates,
        };
        settle(&mut self.layer, &mut backend);
        while self.layer.poll_viewed().is_some() {}
    }

    fn locate_grouped(&mut self, key: Key) -> Located {
        loop {
            if let Some(located) = self.layer.locate(Axis::Rows, key) {
                return located;
            }
            self.settle();
        }
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
    fn hidden_items_leave_apart_from_the_filter_until_shown() {
        let mut m = WebMemory::new(5, 3);
        assert!(m.hide(ROWS, &[1, 3, 9]));
        assert_eq!(order(&m), vec![0, 2, 4]);
        m.filter(ROWS, &[1, 1, 0, 1, 1]);
        assert_eq!(order(&m), vec![0, 4]);
        m.clear_filter(ROWS);
        assert_eq!(order(&m), vec![0, 2, 4]);
        assert!(!m.hide(ROWS, &[3, 1]), "the same items, so nothing moves");
        assert!(m.hide(COLS, &[0]));
        assert_eq!(m.window(COLS, 0.0, m.count(COLS)), vec![1, 2]);
        assert!(m.hide(ROWS, &[]));
        assert_eq!(order(&m), vec![0, 1, 2, 3, 4]);
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
    fn moved_items_take_their_new_positions_on_either_axis() {
        let mut m = WebMemory::new(3, 3);
        assert!(m.move_items(COLS, &[2], 0.0));
        assert_eq!(m.window(COLS, 0.0, 3.0), vec![2, 0, 1]);
        assert!(m.move_items(ROWS, &[0], 3.0));
        assert_eq!(order(&m), vec![1, 2, 0]);
    }

    #[test]
    fn grouped_rows_count_their_group_items_and_leaves_keep_their_place() {
        let mut m = WebMemory::new(5, 2);
        // Column 1 is "b", "a", "b", null, "a", sorted by it first.
        m.stage_strings(b"babab".to_vec(), vec![1, 2, 3, 3, 4], vec![0, 0, 0, 1, 0]);
        m.sort(ROWS, &[1], &[0]);
        assert_eq!(order(&m), vec![1, 4, 0, 2, 3]);
        m.stage_level(b"babab".to_vec(), vec![1, 2, 3, 3, 4], vec![0, 0, 0, 1, 0]);
        assert_eq!(m.group(&[1]), 8.0, "three groups and five Leaves");
        assert!(m.grouped());
        // a: 1, 4; b: 0, 2; blank: 3.
        assert_eq!(m.locate(ROWS, 4), vec![0.0, 2.0]);
        assert_eq!(m.locate(ROWS, 3), vec![0.0, 7.0]);
        m.filter(ROWS, &[1, 1, 0, 1, 1]);
        assert_eq!(m.regroup(), 7.0);
        assert_eq!(m.locate(ROWS, 2)[0], 1.0, "filtered out");
        assert_eq!(m.ungroup(), 4.0);
        assert!(!m.grouped());
        assert_eq!(m.locate(ROWS, 3), vec![0.0, 3.0]);
    }

    #[test]
    fn a_sorted_axis_refuses_moves_if_asked_to() {
        let mut m = WebMemory::new(3, 1);
        m.refuse_sorted_moves(true);
        m.stage_numbers(&[2.0, 0.0, 1.0], &[0, 0, 0]);
        m.sort(ROWS, &[0], &[0]);
        assert!(!m.movable(ROWS));
        assert!(!m.move_items(ROWS, &[0], 0.0));
        assert_eq!(order(&m), vec![1, 2, 0]);
        assert!(m.movable(COLS));
    }
}
