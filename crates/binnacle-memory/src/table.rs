use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;

use binnacle_core::{Axis, ChangeNotice, Key, Located, Position, SortSpec};

use crate::SortKeys;

/// One axis of an In-Memory Table Source: its base Keys, the sorted base
/// order and keep mask it was given, the Remap built from both, and the lazy
/// reverse maps that answer `locate`.
#[derive(Clone, Debug)]
struct AxisOrder {
    len: usize,
    /// The Key of each base item; `None` keys items by Logical Index.
    keys: Option<Vec<Key>>,
    /// Logical Indices in sorted order; `None` is base order.
    sorted: Option<Vec<u32>>,
    /// Which base items the filter keeps, by Logical Index; `None` keeps all.
    keep: Option<Vec<bool>>,
    /// The Remap, Position to Logical Index: the sorted order with items the
    /// filter drops skipped. `None` is base order.
    remap: Option<Vec<u32>>,
    /// Supplied Key to Logical Index, built on the first `locate`.
    logical: Option<HashMap<Key, u32>>,
    /// Logical Index to Position (`GONE` if filtered out), built on the first
    /// `locate` after the Remap changes.
    positions: Option<Vec<u32>>,
    /// Logical Index to its place in the sorted order, built on the first
    /// `Gone` answer after a sort.
    ranks: Option<Vec<u32>>,
}

/// In `AxisOrder::positions`, an item the filter drops.
const GONE: u32 = u32::MAX;

impl AxisOrder {
    fn new(len: usize) -> Self {
        assert!(
            u32::try_from(len).is_ok(),
            "an In-Memory Table Source holds at most u32::MAX items per axis, got {len}"
        );
        AxisOrder {
            len,
            keys: None,
            sorted: None,
            keep: None,
            remap: None,
            logical: None,
            positions: None,
            ranks: None,
        }
    }

    fn key(&self, logical: u32) -> Key {
        match &self.keys {
            None => Key(logical as u64),
            Some(keys) => keys[logical as usize],
        }
    }

    fn set_sorted(&mut self, sorted: Option<Vec<u32>>) {
        self.sorted = sorted;
        self.ranks = None;
        self.rebuild();
    }

    fn set_keep(&mut self, keep: Option<Vec<bool>>) {
        self.keep = keep;
        self.rebuild();
    }

    /// Rebuilds the Remap from the sorted order and keep mask in one pass.
    fn rebuild(&mut self) {
        self.positions = None;
        self.remap = match (&self.sorted, &self.keep) {
            (None, None) => None,
            (sorted, keep) => {
                let kept = |&i: &u32| keep.as_ref().is_none_or(|k| k[i as usize]);
                Some(match sorted {
                    Some(sorted) => sorted.iter().copied().filter(kept).collect(),
                    None => (0..self.len as u32).filter(kept).collect(),
                })
            }
        };
    }

    fn logical(&mut self, key: Key) -> Option<u32> {
        match &self.keys {
            None => u32::try_from(key.0)
                .ok()
                .filter(|&i| (i as usize) < self.len),
            Some(keys) => self
                .logical
                .get_or_insert_with(|| {
                    keys.iter()
                        .enumerate()
                        .map(|(i, &k)| (k, i as u32))
                        .collect()
                })
                .get(&key)
                .copied(),
        }
    }

    fn locate(&mut self, key: Key) -> Located {
        let Some(logical) = self.logical(key) else {
            return Located::Gone { suggest: None };
        };
        let Some(remap) = &self.remap else {
            return Located::Found(logical as u64);
        };
        let len = self.len;
        let positions = self.positions.get_or_insert_with(|| {
            let mut positions = vec![GONE; len];
            for (position, &i) in remap.iter().enumerate() {
                positions[i as usize] = position as u32;
            }
            positions
        });
        match positions[logical as usize] {
            GONE => Located::Gone {
                suggest: self.nearest_kept(logical),
            },
            position => Located::Found(position as u64),
        }
    }

    /// The Position of the kept item nearest to `logical` in sorted order,
    /// the following one on a tie. Needs `positions` built.
    fn nearest_kept(&mut self, logical: u32) -> Option<Position> {
        let positions = self.positions.as_ref()?;
        let rank = match &self.sorted {
            None => logical as usize,
            Some(sorted) => self.ranks.get_or_insert_with(|| {
                let mut ranks = vec![0; sorted.len()];
                for (rank, &i) in sorted.iter().enumerate() {
                    ranks[i as usize] = rank as u32;
                }
                ranks
            })[logical as usize] as usize,
        };
        let at = |rank: usize| {
            let i = self.sorted.as_ref().map_or(rank as u32, |s| s[rank]);
            Some(positions[i as usize]).filter(|&p| p != GONE)
        };
        (1..self.len).find_map(|d| {
            let after = (rank + d < self.len).then(|| at(rank + d)).flatten();
            let before = rank.checked_sub(d).and_then(at);
            after.or(before).map(u64::from)
        })
    }

    fn count(&self) -> u64 {
        self.remap.as_ref().map_or(self.len, Vec::len) as u64
    }

    fn window(&self, range: Range<Position>) -> Vec<Key> {
        let end = range.end.min(self.count());
        let start = range.start.min(end);
        match &self.remap {
            None => (start..end).map(|i| self.key(i as u32)).collect(),
            Some(remap) => remap[start as usize..end as usize]
                .iter()
                .map(|&i| self.key(i))
                .collect(),
        }
    }
}

/// The shared Table Source for hosts whose data lives in memory. It holds no
/// cell data, only the order of each axis, which it computes from keys the host
/// extracts and hands over.
#[derive(Clone, Debug)]
pub struct InMemoryTableSource {
    axes: [AxisOrder; 2],
    sort: SortSpec,
}

impl InMemoryTableSource {
    /// A source over `rows` base rows and `cols` base columns, both in base
    /// order, keyed by Logical Index.
    ///
    /// # Panics
    ///
    /// If either count exceeds `u32::MAX`.
    pub fn new(rows: usize, cols: usize) -> Self {
        InMemoryTableSource {
            axes: [AxisOrder::new(rows), AxisOrder::new(cols)],
            sort: SortSpec::unsorted(),
        }
    }

    /// Keys the items on `axis` with `keys`, one per base item in base order,
    /// in place of their Logical Indices. Keys must be distinct; the
    /// [`crate::KeyInterner`] makes them from the host's own identities.
    ///
    /// # Panics
    ///
    /// If `keys` does not hold one Key per base item.
    pub fn with_keys(mut self, axis: Axis, keys: Vec<Key>) -> Self {
        let order = self.axis_mut(axis);
        assert_eq!(keys.len(), order.len, "one Key per base item");
        order.keys = Some(keys);
        order.logical = None;
        self
    }

    fn axis(&self, axis: Axis) -> &AxisOrder {
        match axis {
            Axis::Rows => &self.axes[0],
            Axis::Cols => &self.axes[1],
        }
    }

    fn axis_mut(&mut self, axis: Axis) -> &mut AxisOrder {
        match axis {
            Axis::Rows => &mut self.axes[0],
            Axis::Cols => &mut self.axes[1],
        }
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.axis(axis).count()
    }

    /// The Keys at the Positions in `range`, clipped to the count.
    pub fn window(&self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.axis(axis).window(range)
    }

    /// Answers `Locate`: the Key's current Position, or `Gone` if no item on
    /// the axis has it. A Key the filter dropped is `Gone` with the Position
    /// of the kept item nearest it in sort order suggested (the following one
    /// on a tie). The reverse maps are built on the first ask after a
    /// change, so hosts that never locate never pay for them.
    pub fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.axis_mut(axis).locate(key)
    }

    /// Sorts the rows by `spec`, with `keys[i]` holding the sort keys of
    /// `spec.columns()[i]` for every base row. Rows compare as tuples in spec
    /// order and ties keep base order. An empty spec restores base order.
    /// Returns the Change Notice the rows need.
    ///
    /// # Panics
    ///
    /// If `keys` does not hold one entry per spec column, or an entry does not
    /// hold one key per base row.
    pub fn apply_sort(&mut self, spec: &SortSpec, keys: Vec<SortKeys<'_>>) -> ChangeNotice {
        let rows = self.axis_mut(Axis::Rows);
        assert_eq!(
            keys.len(),
            spec.columns().len(),
            "one SortKeys per Sort Spec column"
        );
        for column in &keys {
            assert_eq!(column.len(), rows.len, "one sort key per base row");
        }
        if spec.is_empty() {
            rows.set_sorted(None);
            self.sort = SortSpec::unsorted();
            return ChangeNotice::Reordered;
        }
        let mut remap: Vec<u32> = (0..rows.len as u32).collect();
        remap.sort_by(|&a, &b| {
            spec.columns()
                .iter()
                .zip(&keys)
                .map(|(column, keys)| keys.compare(a as usize, b as usize, column.direction))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        });
        rows.set_sorted(Some(remap));
        self.sort = spec.clone();
        ChangeNotice::Reordered
    }

    /// Filters the rows: `keep[i]` says whether base row `i` (by Logical
    /// Index) stays. The rows kept stay in sort order, and the sort is not
    /// redone. Returns the Change Notice the rows need.
    ///
    /// # Panics
    ///
    /// If `keep` does not hold one entry per base row.
    pub fn apply_filter(&mut self, keep: Vec<bool>) -> ChangeNotice {
        let rows = self.axis_mut(Axis::Rows);
        assert_eq!(keep.len(), rows.len, "one keep flag per base row");
        rows.set_keep(Some(keep));
        ChangeNotice::Reset(rows.count())
    }

    /// Removes the filter, keeping every row in sort order.
    pub fn clear_filter(&mut self) -> ChangeNotice {
        let rows = self.axis_mut(Axis::Rows);
        rows.set_keep(None);
        ChangeNotice::Reset(rows.count())
    }

    /// The Sort Spec the rows are in, as last applied.
    pub fn sort_spec(&self) -> &SortSpec {
        &self.sort
    }
}
