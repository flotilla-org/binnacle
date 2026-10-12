use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

use binnacle_core::Key;

/// Maps the host's own identities (strings, composite tuples) to Keys without
/// collisions: Keys are handed out in order of first sight, so two identities
/// share a Key only if they are equal.
#[derive(Clone, Debug)]
pub struct KeyInterner<T> {
    identities: Vec<T>,
    keys: HashMap<T, Key>,
}

impl<T: Clone + Eq + Hash> KeyInterner<T> {
    pub fn new() -> Self {
        KeyInterner {
            identities: Vec::new(),
            keys: HashMap::new(),
        }
    }

    /// The Key for `identity`, issuing the next one if it is new.
    pub fn intern(&mut self, identity: T) -> Key {
        if let Some(&key) = self.keys.get(&identity) {
            return key;
        }
        let key = Key(self.identities.len() as u64);
        self.identities.push(identity.clone());
        self.keys.insert(identity, key);
        key
    }

    /// The Key already issued for `identity`, if any.
    pub fn key<Q>(&self, identity: &Q) -> Option<Key>
    where
        T: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.keys.get(identity).copied()
    }

    /// The identity a Key was issued for.
    pub fn identity(&self, key: Key) -> Option<&T> {
        usize::try_from(key.0)
            .ok()
            .and_then(|i| self.identities.get(i))
    }
}

impl<T: Clone + Eq + Hash> Default for KeyInterner<T> {
    fn default() -> Self {
        KeyInterner::new()
    }
}
