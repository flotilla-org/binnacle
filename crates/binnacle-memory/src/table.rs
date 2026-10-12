use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;

use binnacle_core::{Axis, ChangeNotice, Key, Located, Position, SortSpec};

use crate::SortKeys;

/// One axis of an In-Memory Table Source: its base Keys, the Remap and the
/// lazy reverse maps that answer `locate`.
#[derive(Clone, Debug)]
struct AxisOrder {
    len: usize,
    /// The Key of each base item; `None` keys items by Logical Index.
    keys: Option<Vec<Key>>,
    /// The Remap, Position to Logical Index; `None` is base order.
    remap: Option<Vec<u32>>,
    /// Supplied Key to Logical Index, built on the first `locate`.
    logical: Option<HashMap<Key, u32>>,
    /// Logical Index to Position, built on the first `locate` after a sort.
    positions: Option<Vec<u32>>,
}

impl AxisOrder {
    fn new(len: usize) -> Self {
        assert!(
            u32::try_from(len).is_ok(),
            "an In-Memory Table Source holds at most u32::MAX items per axis, got {len}"
        );
        AxisOrder {
            len,
            keys: None,
            remap: None,
            logical: None,
            positions: None,
        }
    }

    fn key(&self, logical: u32) -> Key {
        match &self.keys {
            None => Key(logical as u64),
            Some(keys) => keys[logical as usize],
        }
    }

    fn set_remap(&mut self, remap: Option<Vec<u32>>) {
        self.remap = remap;
        self.positions = None;
    }

    /// The Logical Index of the item with `key`, if any.
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
        let position = match &self.remap {
            None => logical,
            Some(remap) => self.positions.get_or_insert_with(|| {
                let mut positions = vec![0; remap.len()];
                for (position, &i) in remap.iter().enumerate() {
                    positions[i as usize] = position as u32;
                }
                positions
            })[logical as usize],
        };
        Located::Found(position as u64)
    }

    fn count(&self) -> u64 {
        self.len as u64
    }

    /// Moves the items with `keys`, in that order, to just before the item at
    /// Position `to` in the current order.
    fn move_items(&mut self, keys: &[Key], to: Position) {
        let mut moving = vec![false; self.len];
        let mut moved = Vec::with_capacity(keys.len());
        for &key in keys {
            if let Some(i) = self.logical(key)
                && !std::mem::replace(&mut moving[i as usize], true)
            {
                moved.push(i);
            }
        }
        if moved.is_empty() {
            return;
        }
        let order: Vec<u32> = match &self.remap {
            None => (0..self.len as u32).collect(),
            Some(remap) => remap.clone(),
        };
        let to = (to.min(self.count())) as usize;
        let stays = |i: &&u32| !moving[**i as usize];
        let remap = order[..to]
            .iter()
            .filter(stays)
            .chain(&moved)
            .chain(order[to..].iter().filter(stays))
            .copied()
            .collect();
        self.set_remap(Some(remap));
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
    /// the axis has it. The reverse maps are built on the first ask after a
    /// change, so hosts that never locate never pay for them.
    pub fn locate(&mut self, axis: Axis, key: Key) -> Located {
        self.axis_mut(axis).locate(key)
    }

    /// Moves the columns with `keys`, in the order given, to just before the
    /// column at Position `to` (the count for the end), Positions as they
    /// were before the move: the answer to `MoveRequested` on the columns.
    /// Keys no column has, and repeats, are skipped. Row order is untouched.
    /// Returns the Change Notice the columns need.
    pub fn move_columns(&mut self, keys: &[Key], to: Position) -> ChangeNotice {
        self.axis_mut(Axis::Cols).move_items(keys, to);
        ChangeNotice::Reordered
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
            rows.set_remap(None);
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
        rows.set_remap(Some(remap));
        self.sort = spec.clone();
        ChangeNotice::Reordered
    }

    /// The Sort Spec the rows are in, as last applied.
    pub fn sort_spec(&self) -> &SortSpec {
        &self.sort
    }
}
