use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
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
    /// Unmatched items, by Logical Index: the filter drops them but they stay
    /// until it is applied again.
    unmatched: HashSet<u32>,
    /// The Remap, Position to Logical Index: the sorted order with items the
    /// filter drops skipped, unless unmatched. `None` is base order.
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
            unmatched: HashSet::new(),
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
        self.unmatched.clear();
        self.rebuild();
    }

    /// The filter now keeps item `logical` or not. With `defer`, an item in
    /// view stays, unmatched while the filter drops it. Returns whether the
    /// item came into view or left it; `false` with no filter in force.
    fn rekeep(&mut self, logical: usize, keep: bool, defer: bool) -> bool {
        let shown = self.position(logical).is_some();
        let Some(flags) = &mut self.keep else {
            return false;
        };
        flags[logical] = keep;
        let i = logical as u32;
        if defer && shown && !keep {
            self.unmatched.insert(i);
        } else {
            self.unmatched.remove(&i);
        }
        let moved = shown != (keep || self.unmatched.contains(&i));
        if moved {
            self.rebuild();
        }
        moved
    }
    /// Rebuilds the Remap from the sorted order and keep mask in one pass.
    fn rebuild(&mut self) {
        self.positions = None;
        self.remap = match (&self.sorted, &self.keep) {
            (None, None) => None,
            (sorted, keep) => {
                let unmatched = &self.unmatched;
                let kept = |&i: &u32| {
                    keep.as_ref().is_none_or(|k| k[i as usize]) || unmatched.contains(&i)
                };
                Some(match sorted {
                    Some(sorted) => sorted.iter().copied().filter(kept).collect(),
                    None => (0..self.len as u32).filter(kept).collect(),
                })
            }
        };
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
        match self.position(logical as usize) {
            Some(position) => Located::Found(position),
            None => Located::Gone {
                suggest: self.nearest_kept(logical),
            },
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

    /// Adds `n` base items after the last, keyed by `keys` if the axis has
    /// host Keys. `place` merges the new Logical Indices into the sorted
    /// order, if there is one; `keep` holds their flags while a filter is in
    /// force.
    fn append(
        &mut self,
        n: usize,
        keys: &[Key],
        place: impl FnOnce(&[u32], Vec<u32>) -> Vec<u32>,
        keep: &[bool],
    ) {
        let len = self.len + n;
        assert!(
            u32::try_from(len).is_ok(),
            "an In-Memory Table Source holds at most u32::MAX items per axis, got {len}"
        );
        match &self.keep {
            Some(_) => assert_eq!(keep.len(), n, "one keep flag per appended row"),
            None => assert!(keep.is_empty(), "no filter is in force"),
        }
        match &mut self.keys {
            Some(own) => {
                assert_eq!(keys.len(), n, "one Key per appended item");
                own.extend_from_slice(keys);
                if let Some(logical) = &mut self.logical {
                    logical.extend(keys.iter().zip(self.len as u32..).map(|(&k, i)| (k, i)));
                }
            }
            None => assert!(keys.is_empty(), "the items have no host Keys"),
        }
        let new = (self.len as u32..len as u32).collect();
        self.len = len;
        if let Some(sorted) = &mut self.sorted {
            *sorted = place(sorted, new);
            self.ranks = None;
        }
        if let Some(own) = &mut self.keep {
            own.extend_from_slice(keep);
        }
        self.rebuild();
    }

    /// The Position of the item with Logical Index `logical`, if it is kept.
    fn position(&mut self, logical: usize) -> Option<Position> {
        if logical >= self.len {
            return None;
        }
        let Some(remap) = &self.remap else {
            return Some(logical as u64);
        };
        let len = self.len;
        let positions = self.positions.get_or_insert_with(|| {
            let mut positions = vec![GONE; len];
            for (position, &i) in remap.iter().enumerate() {
                positions[i as usize] = position as u32;
            }
            positions
        });
        Some(positions[logical])
            .filter(|&p| p != GONE)
            .map(u64::from)
    }

    /// Moves the items with `keys`, in that order, to just before the item at
    /// Position `to` in the order before any filter (columns, the only axis
    /// moved, are never filtered, so that is the current order).
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
        let order: Vec<u32> = match &self.sorted {
            None => (0..self.len as u32).collect(),
            Some(sorted) => sorted.clone(),
        };
        let to = (to.min(self.len as u64)) as usize;
        let stays = |i: &&u32| !moving[**i as usize];
        let order = order[..to]
            .iter()
            .filter(stays)
            .chain(&moved)
            .chain(order[to..].iter().filter(stays))
            .copied()
            .collect();
        self.set_sorted(Some(order));
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

/// Compares base rows `a` and `b` by `spec`, as a tuple in spec order.
fn compare(spec: &SortSpec, keys: &[SortKeys<'_>], a: u32, b: u32) -> Ordering {
    spec.columns()
        .iter()
        .zip(keys)
        .map(|(column, keys)| keys.compare(a as usize, b as usize, column.direction))
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// Checks that `keys` holds one entry per `spec` column, each with one key
/// per base row of `len`.
fn check_sort_keys(spec: &SortSpec, keys: &[SortKeys<'_>], len: usize) {
    assert_eq!(
        keys.len(),
        spec.columns().len(),
        "one SortKeys per Sort Spec column"
    );
    for column in keys {
        assert_eq!(column.len(), len, "one sort key per base row");
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
        &self.axes[axis.index()]
    }

    fn axis_mut(&mut self, axis: Axis) -> &mut AxisOrder {
        &mut self.axes[axis.index()]
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.axis(axis).count()
    }

    /// The Keys at the Positions in `range`, clipped to the count.
    pub fn window(&self, axis: Axis, range: Range<Position>) -> Vec<Key> {
        self.axis(axis).window(range)
    }

    /// The Logical Indices at the Positions in `range`, clipped to the count:
    /// a part of the Remap.
    pub fn logical_window(&self, axis: Axis, range: Range<Position>) -> Vec<usize> {
        let order = self.axis(axis);
        let end = range.end.min(order.count());
        let start = range.start.min(end);
        match &order.remap {
            None => (start as usize..end as usize).collect(),
            Some(remap) => remap[start as usize..end as usize]
                .iter()
                .map(|&i| i as usize)
                .collect(),
        }
    }

    /// The Logical Index of the item with `key` on `axis`, whether or not
    /// the filter keeps it.
    pub fn logical_index(&mut self, axis: Axis, key: Key) -> Option<usize> {
        self.axis_mut(axis).logical(key).map(|i| i as usize)
    }

    /// Answers `Locate`: the Key's current Position, or `Gone` if no item on
    /// the axis has it. A Key the filter dropped is `Gone` with the Position
    /// of the kept item nearest it in sort order suggested (the following one
    /// on a tie). The reverse maps are built on the first ask after a
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
        check_sort_keys(spec, &keys, rows.len);
        if spec.is_empty() {
            rows.set_sorted(None);
            self.sort = SortSpec::unsorted();
            return ChangeNotice::Reordered;
        }
        let mut remap: Vec<u32> = (0..rows.len as u32).collect();
        remap.sort_by(|&a, &b| compare(spec, &keys, a, b));
        rows.set_sorted(Some(remap));
        self.sort = spec.clone();
        ChangeNotice::Reordered
    }

    /// Filters the rows: `keep[i]` says whether base row `i` (by Logical
    /// Index) stays. The rows kept stay in sort order, and the sort is not
    /// redone, and no row stays unmatched. Returns the `Refiltered` the rows
    /// need.
    ///
    /// # Panics
    ///
    /// If `keep` does not hold one entry per base row.
    pub fn apply_filter(&mut self, keep: Vec<bool>) -> ChangeNotice {
        let rows = self.axis_mut(Axis::Rows);
        assert_eq!(keep.len(), rows.len, "one keep flag per base row");
        rows.set_keep(Some(keep));
        ChangeNotice::Refiltered(rows.count())
    }

    /// Removes the filter, keeping every row in sort order. Returns the
    /// `Refiltered` the rows need.
    pub fn clear_filter(&mut self) -> ChangeNotice {
        let rows = self.axis_mut(Axis::Rows);
        rows.set_keep(None);
        ChangeNotice::Refiltered(rows.count())
    }

    /// The filter in force now keeps base row `row` (a Logical Index) or
    /// not, because the data changed: the row comes into view or leaves it
    /// at once, and is no longer unmatched. Returns the `Refiltered` the rows
    /// need if it came or went; nothing while no filter is in force.
    ///
    /// # Panics
    ///
    /// If there is no such row.
    pub fn refilter_row(&mut self, row: usize, keep: bool) -> Option<ChangeNotice> {
        let rows = self.axis_mut(Axis::Rows);
        rows.rekeep(row, keep, false)
            .then(|| ChangeNotice::Refiltered(rows.count()))
    }

    /// The filter in force now keeps base row `row` or not, because the user
    /// edited it: a row in view stays, unmatched while the filter drops it,
    /// until the filter is applied again ([`InMemoryTableSource::apply_filter`]
    /// or [`InMemoryTableSource::clear_filter`]). A row out of view is
    /// treated as by [`InMemoryTableSource::refilter_row`].
    ///
    /// # Panics
    ///
    /// If there is no such row.
    pub fn defer_row(&mut self, row: usize, keep: bool) -> Option<ChangeNotice> {
        let rows = self.axis_mut(Axis::Rows);
        rows.rekeep(row, keep, true)
            .then(|| ChangeNotice::Refiltered(rows.count()))
    }

    /// Whether each item with `keys` on `axis` is unmatched: in view though
    /// the filter in force drops it. Keys the axis lacks are not.
    pub fn unmatched(&mut self, axis: Axis, keys: &[Key]) -> Vec<bool> {
        let order = self.axis_mut(axis);
        (keys.iter())
            .map(|&k| {
                order
                    .logical(k)
                    .is_some_and(|i| order.unmatched.contains(&i))
            })
            .collect()
    }

    /// Whether any item on `axis` is unmatched.
    pub fn has_unmatched(&self, axis: Axis) -> bool {
        !self.axis(axis).unmatched.is_empty()
    }

    /// The number of base rows, kept or not.
    pub fn base_rows(&self) -> usize {
        self.axis(Axis::Rows).len
    }

    /// The Position of base row `row` (a Logical Index), or `None` if the
    /// filter drops it or there is no such row.
    pub fn row_position(&mut self, row: usize) -> Option<Position> {
        self.axis_mut(Axis::Rows).position(row)
    }

    /// `n` rows were appended to the base data, keyed by `keys` if the rows
    /// have host Keys ([`InMemoryTableSource::with_keys`]; empty otherwise).
    /// They take their place in the sort in force, after rows they tie with,
    /// and the filter in force drops those it rejects: `sort_keys` holds the
    /// sort keys of every base row, new ones included, as for
    /// [`InMemoryTableSource::apply_sort`] with the applied Sort Spec (empty
    /// when unsorted), and `keep` the new rows' keep flags while a filter is
    /// in force (empty otherwise). Returns `Inserted` if the rows kept land
    /// together (`n` 0 if none is kept), else `Reset`.
    ///
    /// # Panics
    ///
    /// If `keys`, `sort_keys` or `keep` does not hold what is described.
    pub fn append_rows(
        &mut self,
        n: usize,
        keys: &[Key],
        sort_keys: Vec<SortKeys<'_>>,
        keep: &[bool],
    ) -> ChangeNotice {
        let spec = &self.sort;
        let rows = &mut self.axes[0];
        let old = rows.len;
        check_sort_keys(spec, &sort_keys, old + n);
        let before = rows.count();
        rows.append(
            n,
            keys,
            |sorted, mut new| {
                new.sort_by(|&a, &b| compare(spec, &sort_keys, a, b));
                merge(sorted, &new, |&a, &b| compare(spec, &sort_keys, a, b))
            },
            keep,
        );
        let mut placed: Vec<Position> = (old..old + n)
            .filter_map(|logical| rows.position(logical))
            .collect();
        placed.sort_unstable();
        let at = placed.first().copied().unwrap_or(before);
        let kept = placed.len() as u64;
        if placed.last().is_none_or(|&last| last + 1 - at == kept) {
            ChangeNotice::Inserted { at, n: kept }
        } else {
            ChangeNotice::Reset(rows.count())
        }
    }

    /// The base rows changed in some other way: there are now `len`, keyed
    /// by `keys` (or by Logical Index if `None`). The sort and filter in
    /// force are applied to them again: `sort_keys` holds every row's sort
    /// keys for the applied Sort Spec (empty when unsorted) and `keep` every
    /// row's keep flag while a filter is in force (empty otherwise). Returns
    /// the `Reset` the rows need.
    ///
    /// # Panics
    ///
    /// As [`InMemoryTableSource::new`] and [`InMemoryTableSource::with_keys`],
    /// or if `sort_keys` or `keep` does not hold what is described.
    pub fn replace_rows(
        &mut self,
        len: usize,
        keys: Option<Vec<Key>>,
        sort_keys: Vec<SortKeys<'_>>,
        keep: Vec<bool>,
    ) -> ChangeNotice {
        let mut rows = AxisOrder::new(len);
        if let Some(keys) = keys {
            assert_eq!(keys.len(), len, "one Key per base item");
            rows.keys = Some(keys);
        }
        let filtered = self.axes[0].keep.is_some();
        self.axes[0] = rows;
        let spec = self.sort.clone();
        self.apply_sort(&spec, sort_keys);
        if filtered {
            self.apply_filter(keep);
        } else {
            assert!(keep.is_empty(), "no filter is in force");
        }
        ChangeNotice::Reset(self.count(Axis::Rows))
    }

    /// The Sort Spec the rows are in, as last applied.
    pub fn sort_spec(&self) -> &SortSpec {
        &self.sort
    }
}

/// Merges `new` into `sorted`, both ordered by `compare`; an item of `new`
/// goes after the items of `sorted` it ties with.
fn merge(sorted: &[u32], new: &[u32], compare: impl Fn(&u32, &u32) -> Ordering) -> Vec<u32> {
    let mut merged = Vec::with_capacity(sorted.len() + new.len());
    let mut rest = sorted;
    for n in new {
        let before = rest.partition_point(|s| compare(n, s).is_ge());
        merged.extend_from_slice(&rest[..before]);
        merged.push(*n);
        rest = &rest[before..];
    }
    merged.extend_from_slice(rest);
    merged
}
