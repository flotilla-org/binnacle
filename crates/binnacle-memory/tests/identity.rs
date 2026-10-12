//! Seam 2: the identity Table Source answers Windows with Key == Position.

use binnacle_core::Key;
use binnacle_memory::IdentitySource;
use hegel::TestCase;
use hegel::generators as gs;

/// Most Keys a drawn Window can span, which bounds what `window` materialises.
const MAX_WINDOW: u64 = 10_000;

#[hegel::test(test_cases = 1000)]
fn window_gives_each_position_as_its_key_clipped_to_the_count(tc: TestCase) {
    let count = tc.draw(gs::integers::<u64>());
    let start = tc.draw(gs::integers::<u64>());
    let len = tc.draw(gs::integers::<u64>().max_value(MAX_WINDOW));
    let end = start.saturating_add(len);
    let keys = IdentitySource::new(count).window(start..end);
    let expected_len = end.min(count).saturating_sub(start);
    assert_eq!(keys.len() as u64, expected_len);
    for (i, key) in keys.iter().enumerate() {
        assert_eq!(*key, Key(start + i as u64));
    }
}
