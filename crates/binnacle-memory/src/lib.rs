//! binnacle-memory: Table Sources for hosts whose data lives in memory.

mod interner;
mod memory_source;
mod pump;
mod sort_keys;
mod table;
mod transposed;

use std::ops::Range;

use binnacle_core::{Key, Located, Position};

pub use interner::KeyInterner;
pub use memory_source::{MemorySource, TextMatcher, Unsortable, query_error};
pub use pump::{FindCount, FindStep, Moved, PumpSource, Viewed, answer_move, forget_retired, pump};
pub use sort_keys::{SortKeys, compare_f64};
pub use table::{InMemoryTableSource, MovePolicy};
pub use transposed::{
    Transposed, transpose_find, transpose_match, transpose_spec, transpose_totals,
};

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

    /// Answers `Locate`: a Key is at the Position it names, if that is below
    /// the count.
    pub fn locate(&self, key: Key) -> Located {
        if key.0 < self.count {
            Located::Found(key.0)
        } else {
            Located::Gone { suggest: None }
        }
    }
}
