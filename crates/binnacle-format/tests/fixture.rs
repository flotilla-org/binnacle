//! The shared fixture formats to its expected text, which vitest checks the
//! web's formatter against too. `BLESS=1` rewrites the file; read the diff
//! before committing it.

use binnacle_format::fixture::{Table, VALUES_EXPECTED};

#[test]
fn the_shared_fixture_formats_to_its_expected_text() {
    let text = Table::values().expected_text();
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/values.expected.txt"),
            &text,
        )
        .unwrap();
        return;
    }
    assert_eq!(text, VALUES_EXPECTED);
}
