//! A Described Table's Layout: its IDs.

use binnacle_core::Key;
use binnacle_layout::{IdMap, JsonId};
use serde_json::Value as Json;

use crate::source::DescribedSource;
use crate::wire::Wire;

/// A row is named by its primary-key tuple as a JSON array, a column by its
/// field's name as a JSON string. Group Items and Summary Items have no ID
/// here, so a saved Layout leaves them out.
impl IdMap<JsonId, JsonId> for DescribedSource {
    fn row_key(&self, id: &JsonId) -> Option<Key> {
        let Json::Array(items) = id.value() else {
            return None;
        };
        let tuple: Vec<Wire> = items.iter().map(Wire::from_json).collect();
        DescribedSource::row_key(self, &tuple)
    }

    fn col_key(&self, id: &JsonId) -> Option<Key> {
        self.column_key(id.value().as_str()?)
    }

    fn row_id(&self, key: Key) -> Option<JsonId> {
        let wire: Wire = ciborium::from_reader(self.row_identity(key)?).ok()?;
        JsonId::new(wire.to_json())
    }

    fn col_id(&self, key: Key) -> Option<JsonId> {
        Some(JsonId::from(self.column(key)?.name()))
    }
}
