//! PROTOTYPE stub of the In-Memory Table Source (binnacle-memory): order machinery
//! only, never the host's data. Keys are Logical Indices (the default).

use crate::grid::{Dir, Key};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Typed sort keys for one column over all base rows, extracted by the host.
pub enum SortKeys {
    I64(Vec<Option<i64>>),
    F64(Vec<Option<f64>>),
    Bytes(Vec<Option<Vec<u8>>>),
}

impl SortKeys {
    fn cmp(&self, a: usize, b: usize) -> Ordering {
        fn nulls_last<T>(x: &Option<T>, y: &Option<T>, f: impl Fn(&T, &T) -> Ordering) -> Ordering {
            match (x, y) {
                (Some(x), Some(y)) => f(x, y),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        match self {
            SortKeys::I64(v) => nulls_last(&v[a], &v[b], |x, y| x.cmp(y)),
            SortKeys::F64(v) => nulls_last(&v[a], &v[b], |x, y| x.total_cmp(y)),
            SortKeys::Bytes(v) => nulls_last(&v[a], &v[b], |x, y| x.cmp(y)),
        }
    }
}

pub struct MemorySource {
    base_len: usize,
    sorted: Vec<u32>,       // sorted base order (all base rows)
    keep: Option<Vec<bool>>, // keep mask over Logical Indices
    remap: Vec<u32>,         // Position -> Logical Index
    reverse: Option<HashMap<Key, u64>>,
}

impl MemorySource {
    pub fn new(base_len: usize) -> Self {
        let sorted: Vec<u32> = (0..base_len as u32).collect();
        MemorySource { base_len, remap: sorted.clone(), sorted, keep: None, reverse: None }
    }

    fn rebuild(&mut self) {
        self.remap = match &self.keep {
            None => self.sorted.clone(),
            Some(k) => self.sorted.iter().copied().filter(|&i| k[i as usize]).collect(),
        };
        self.reverse = None;
    }

    pub fn apply_sort(&mut self, spec: &[(Key, Dir)], keys: Vec<SortKeys>) {
        let mut order: Vec<u32> = (0..self.base_len as u32).collect();
        if !spec.is_empty() {
            order.sort_by(|&a, &b| {
                for ((_, dir), k) in spec.iter().zip(&keys) {
                    let o = k.cmp(a as usize, b as usize);
                    let o = if *dir == Dir::Desc { o.reverse() } else { o };
                    if o != Ordering::Equal {
                        return o;
                    }
                }
                Ordering::Equal // stable: ties keep base order
            });
        }
        self.sorted = order;
        self.rebuild();
    }

    pub fn apply_filter(&mut self, keep: Option<Vec<bool>>) {
        self.keep = keep;
        self.rebuild();
    }

    pub fn count(&self) -> u64 {
        self.remap.len() as u64
    }

    pub fn window(&self, start: u64, len: u64) -> Vec<Key> {
        let s = start as usize;
        let e = (s + len as usize).min(self.remap.len());
        self.remap[s.min(e)..e].iter().map(|&i| i as Key).collect()
    }

    /// Answers `Locate`; the reverse map is built lazily, on first ask.
    pub fn locate(&mut self, key: Key) -> Option<u64> {
        let remap = &self.remap;
        let rev = self
            .reverse
            .get_or_insert_with(|| remap.iter().enumerate().map(|(p, &i)| (i as Key, p as u64)).collect());
        rev.get(&key).copied()
    }
}

/// A source with no order operations: Key == Position. Used for columns, and for
/// the huge virtual table (no data at all).
pub struct IdentitySource {
    pub count: u64,
}

impl IdentitySource {
    pub fn window(&self, start: u64, len: u64) -> Vec<Key> {
        (start..(start + len).min(self.count)).collect()
    }
}
