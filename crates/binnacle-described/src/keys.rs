//! Keys from identities' bytes, the same in every source that sees them.

use std::collections::HashMap;

use binnacle_core::Key;

/// The highest bit, which the Grouping Layer keeps for Summary Item Keys.
const SUMMARY_BIT: u64 = 1 << 63;

/// A Key Interner over identities as bytes (a primary-key tuple's
/// deterministic CBOR, a field's name). A Key is a hash of its identity, so
/// every source gives the same identity the same Key, and an identity that
/// leaves and comes back keeps it; two identities whose hashes collide take
/// the next free Key, in order of first sight. Keys never have the
/// highest bit set, which Summary Items' have.
#[derive(Clone, Debug, Default)]
pub(crate) struct StableKeys {
    keys: HashMap<Vec<u8>, Key>,
    identities: HashMap<Key, Vec<u8>>,
}

impl StableKeys {
    pub fn intern(&mut self, identity: &[u8]) -> Key {
        if let Some(&key) = self.keys.get(identity) {
            return key;
        }
        let mut key = hash(identity);
        while self.identities.contains_key(&Key(key)) {
            key = (key + 1) & !SUMMARY_BIT;
        }
        self.keys.insert(identity.to_vec(), Key(key));
        self.identities.insert(Key(key), identity.to_vec());
        Key(key)
    }

    pub fn key(&self, identity: &[u8]) -> Option<Key> {
        self.keys.get(identity).copied()
    }

    pub fn identity(&self, key: Key) -> Option<&[u8]> {
        self.identities.get(&key).map(Vec::as_slice)
    }
}

/// FNV-1a, 64 bits, less the highest: fixed, so the same on every run and
/// platform.
fn hash(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash & !SUMMARY_BIT
}
