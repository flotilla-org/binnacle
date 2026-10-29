//! The host's mapping between its IDs and the Grid's Keys.

use std::collections::HashMap;
use std::hash::Hash;

use binnacle_core::Key;

/// How a host names a table's items: its own IDs for the rows and the
/// columns, and the Keys the Grid holds for them, both ways. An ID or Key
/// the host does not know is `None`. A host that interns (as the web's Key
/// Interner does) knows every ID, so the Grid holds even those its Table
/// Source lacks for now.
pub trait IdMap<RowId, ColId> {
    fn row_key(&self, id: &RowId) -> Option<Key>;
    fn col_key(&self, id: &ColId) -> Option<Key>;
    fn row_id(&self, key: Key) -> Option<RowId>;
    fn col_id(&self, key: Key) -> Option<ColId>;
}

/// An [`IdMap`] that knows the IDs it was given, and no others.
#[derive(Clone, Debug)]
pub struct IdTable<RowId, ColId> {
    rows: Names<RowId>,
    cols: Names<ColId>,
}

#[derive(Clone, Debug)]
struct Names<Id> {
    keys: HashMap<Id, Key>,
    ids: HashMap<Key, Id>,
}

impl<Id: Clone + Eq + Hash> Names<Id> {
    fn new(pairs: impl IntoIterator<Item = (Id, Key)>) -> Self {
        let mut names = Names {
            keys: HashMap::new(),
            ids: HashMap::new(),
        };
        for (id, key) in pairs {
            names.keys.insert(id.clone(), key);
            names.ids.insert(key, id);
        }
        names
    }
}

impl<RowId: Clone + Eq + Hash, ColId: Clone + Eq + Hash> IdTable<RowId, ColId> {
    /// A table naming the rows and columns by these pairs, each ID and Key
    /// once per axis.
    pub fn new(
        rows: impl IntoIterator<Item = (RowId, Key)>,
        cols: impl IntoIterator<Item = (ColId, Key)>,
    ) -> Self {
        IdTable {
            rows: Names::new(rows),
            cols: Names::new(cols),
        }
    }
}

impl<RowId: Clone + Eq + Hash, ColId: Clone + Eq + Hash> IdMap<RowId, ColId>
    for IdTable<RowId, ColId>
{
    fn row_key(&self, id: &RowId) -> Option<Key> {
        self.rows.keys.get(id).copied()
    }

    fn col_key(&self, id: &ColId) -> Option<Key> {
        self.cols.keys.get(id).copied()
    }

    fn row_id(&self, key: Key) -> Option<RowId> {
        self.rows.ids.get(&key).cloned()
    }

    fn col_id(&self, key: Key) -> Option<ColId> {
        self.cols.ids.get(&key).cloned()
    }
}
