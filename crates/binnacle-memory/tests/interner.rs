//! Seam 2: the Key Interner maps host identities to Keys without collisions.

use binnacle_core::Key;
use binnacle_memory::KeyInterner;
use hegel::TestCase;
use hegel::generators as gs;

#[test]
fn interning_gives_one_key_per_identity_and_resolves_back() {
    let mut interner = KeyInterner::new();
    let oslo = interner.intern(("Oslo".to_string(), 2024));
    let baku = interner.intern(("Baku".to_string(), 2024));
    assert_ne!(oslo, baku);
    assert_eq!(interner.intern(("Oslo".to_string(), 2024)), oslo);
    assert_eq!(interner.key(&("Baku".to_string(), 2024)), Some(baku));
    assert_eq!(interner.key(&("Pune".to_string(), 2024)), None);
    assert_eq!(interner.identity(oslo), Some(&("Oslo".to_string(), 2024)));
    assert_eq!(interner.identity(Key(99)), None);
}

#[hegel::test(test_cases = 1000)]
fn keys_are_equal_exactly_when_identities_are(tc: TestCase) {
    let identities = tc.draw(gs::vecs(gs::text().max_size(3)).max_size(40));
    let mut interner = KeyInterner::new();
    let keys: Vec<Key> = identities
        .iter()
        .map(|id| interner.intern(id.clone()))
        .collect();
    for (a, key_a) in identities.iter().zip(&keys) {
        assert_eq!(interner.identity(*key_a), Some(a));
        assert_eq!(interner.key(a.as_str()), Some(*key_a));
        for (b, key_b) in identities.iter().zip(&keys) {
            assert_eq!(a == b, key_a == key_b, "{a:?} {b:?}");
        }
    }
}
