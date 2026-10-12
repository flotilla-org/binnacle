//! binnacle-memory: Table Sources for hosts whose data lives in memory.

use std::ops::Range;

use binnacle_core::{Key, Position};

/// A Table Source axis with no data behind it: the Key at each Position is the
/// Position itself. It serves huge virtual tables and fixed column sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentitySource {
    count: u64,
}

impl IdentitySource {
    pub fn new(count: u64) -> Self {
        IdentitySource { count }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    /// The Keys for `range`, clipped to the count.
    pub fn window(&self, range: Range<Position>) -> Vec<Key> {
        (range.start..range.end.min(self.count)).map(Key).collect()
    }
}
