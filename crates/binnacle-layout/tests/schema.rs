//! The checked-in JSON Schema is the one the types generate, and it
//! accepts what the crate reads and refuses what it doesn't.

use std::path::Path;

use binnacle_layout::{JsonId, Layout};

fn schema() -> serde_json::Value {
    serde_json::from_str(&binnacle_layout::schema_json()).unwrap()
}

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

/// Documents the crate reads, each valid against the schema.
const READ: [&str; 5] = [
    include_str!("fixtures/layout.json"),
    r#"{"version":1,"pins":{"rows":{"leading":[],"trailing":[]},"cols":{"leading":["a"],"trailing":[]}}}"#,
    r#"{"version":2}"#,
    r#"{"version":2,"order":{"rows":[["ns","a"],["ns",2]]},"pins":{"rows":{"leading":[["ns","a"]],"trailing":[]},"cols":{"leading":[],"trailing":[]}}}"#,
    r#"{"version":2,"view":{"rows":{"sort":[{"id":"a","direction":"ascending"}]}},"sizes":{"rows":[[["k",1],30]],"cols":[]}}"#,
];

/// Documents it refuses, each invalid: misspelt fields at any depth, an
/// unknown version, and IDs of no ID's shape.
const REFUSED: [&str; 7] = [
    r#"{"version":2,"view":{"rows":{"sorts":[{"id":"a","direction":"ascending"}]}},"oder":{"cols":["a"]}}"#,
    r#"{"version":2,"oder":{"cols":["a"]}}"#,
    r#"{"version":2,"view":{"rows":{"sorts":[]}}}"#,
    r#"{"version":2,"bands":{"rows":1,"cols":2,"corner":3}}"#,
    r#"{"version":3}"#,
    r#"{"version":2,"order":{"cols":[{"a":1}]}}"#,
    r#"{"version":2,"order":{"cols":[[[1]]]}}"#,
];

#[test]
fn the_schema_accepts_what_the_crate_reads_and_refuses_misspelt_fields() {
    let validator = jsonschema::validator_for(&schema()).expect("the schema compiles");
    for doc in READ {
        assert!(
            Layout::<JsonId, JsonId>::from_json(doc.as_bytes()).is_ok(),
            "reads {doc}"
        );
        let errors: Vec<String> = (validator.iter_errors(&serde_json::from_str(doc).unwrap()))
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{doc}: {errors:?}");
    }
    for doc in REFUSED {
        assert!(
            Layout::<JsonId, JsonId>::from_json(doc.as_bytes()).is_err(),
            "refuses {doc}"
        );
        assert!(
            !validator.is_valid(&serde_json::from_str(doc).unwrap()),
            "invalid {doc}"
        );
    }
}

#[test]
fn descriptions_keep_no_line_wrapped_from_a_doc_comment() {
    fn check(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => map.iter().for_each(|(key, v)| match v {
                serde_json::Value::String(text) if key == "description" => {
                    assert!(!text.replace("\n\n", "").contains('\n'), "{text:?}")
                }
                _ => check(v),
            }),
            serde_json::Value::Array(items) => items.iter().for_each(check),
            _ => {}
        }
    }
    check(&schema());
}
