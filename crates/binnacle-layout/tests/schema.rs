//! The checked-in JSON Schema is the one the types generate.

use std::path::Path;

#[test]
fn layout_schema_json_is_the_generated_schema() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("layout.schema.json");
    let generated = binnacle_layout::schema_json();
    if std::env::var_os("UPDATE_LAYOUT_SCHEMA").is_some() {
        std::fs::write(&path, &generated).expect("layout.schema.json writes");
    }
    let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        checked_in == generated,
        "crates/binnacle-layout/layout.schema.json is stale. Regenerate it with\n\n    \
         UPDATE_LAYOUT_SCHEMA=1 cargo test -p binnacle-layout --test schema\n\nand commit the result."
    );
}
