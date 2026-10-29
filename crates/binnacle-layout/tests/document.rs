//! The Layout document: lossless through JSON and CBOR, every part
//! optional; layering part by part; and converting the View Spec to Keys
//! skips the IDs the host does not know while the document keeps them.

mod support;

use binnacle_core::{Key, Searched as KeySearched};
use binnacle_layout::view::{Searched, ViewSpec};
use binnacle_layout::{IdTable, Layout, Sizes};
use hegel::TestCase;

#[hegel::test(test_cases = 300)]
fn a_layout_round_trips_through_json_and_cbor(tc: TestCase) {
    let layout = support::layout(&tc);
    let json = layout.to_json();
    assert_eq!(Layout::<String, String>::from_json(&json).unwrap(), layout);
    let cbor = layout.to_cbor();
    assert_eq!(Layout::<String, String>::from_cbor(&cbor).unwrap(), layout);
}

#[test]
fn absent_parts_are_left_out_and_numbers_write_as_javascript_does() {
    let layout: Layout<String, String> = Layout {
        bands: Some(binnacle_layout::Bands {
            rows: 24.0,
            cols: -0.0,
        }),
        sizes: Some(Sizes {
            rows: vec![],
            cols: vec![("a".into(), 150.0), ("b".into(), 77.5)],
        }),
        ..Layout::default()
    };
    assert_eq!(
        String::from_utf8(layout.to_json()).unwrap(),
        r#"{"version":2,"sizes":{"rows":[],"cols":[["a",150],["b",77.5]]},"bands":{"rows":24,"cols":0}}"#
    );
}

#[test]
fn a_version_1_layout_reads_and_another_version_does_not() {
    let v1 = br#"{"version":1,"pins":{"rows":{"leading":[],"trailing":[]},"cols":{"leading":["a"],"trailing":[]}}}"#;
    let layout = Layout::<String, String>::from_json(v1).unwrap();
    assert_eq!(layout.pins.unwrap().cols.leading, ["a"]);
    for version in ["0", "3", "\"2\""] {
        let doc = format!(r#"{{"version":{version}}}"#);
        assert!(
            Layout::<String, String>::from_json(doc.as_bytes()).is_err(),
            "version {version}"
        );
    }
}

/// The fixture `web/test/layout.test.ts` reads too.
const FIXTURE: &str = include_str!("fixtures/layout.json");

#[test]
fn the_shared_fixture_reads_and_writes_back_byte_for_byte() {
    let layout = Layout::<String, String>::from_json(FIXTURE.as_bytes()).unwrap();
    assert_eq!(
        String::from_utf8(layout.to_json()).unwrap(),
        FIXTURE.trim_end()
    );
}

/// Two size lists with an ID at most once each.
fn distinct_sizes(tc: &TestCase) -> Vec<(String, f64)> {
    let mut sizes = support::sizes(tc);
    let mut seen = Vec::new();
    sizes.retain(|(id, _)| {
        let fresh = !seen.contains(id);
        seen.push(id.clone());
        fresh
    });
    sizes
}

fn size_of(sizes: &[(String, f64)], id: &str) -> Option<f64> {
    sizes.iter().find(|(i, _)| i == id).map(|&(_, s)| s)
}

#[hegel::test(test_cases = 300)]
fn a_part_the_user_sets_replaces_the_defaults_whole_and_an_unset_one_keeps_it(tc: TestCase) {
    let default = support::layout(&tc);
    let user = support::layout(&tc);
    let layered = Layout::layered(default.clone(), user.clone());
    assert_eq!(layered.view, user.view.clone().or(default.view.clone()));
    assert_eq!(layered.order, user.order.clone().or(default.order.clone()));
    assert_eq!(layered.pins, user.pins.clone().or(default.pins.clone()));
    assert_eq!(layered.bands, user.bands.or(default.bands));
    if user.sizes.is_none() || default.sizes.is_none() {
        assert_eq!(layered.sizes, user.sizes.or(default.sizes));
    }
}

#[hegel::test(test_cases = 300)]
fn sizes_merge_per_id_with_the_users_winning(tc: TestCase) {
    let (under, over) = (distinct_sizes(&tc), distinct_sizes(&tc));
    let sized = |rows: Vec<(String, f64)>| Layout {
        sizes: Some(Sizes { rows, cols: vec![] }),
        ..Layout::default()
    };
    let layered = Layout::<String, String>::layered(sized(under.clone()), sized(over.clone()));
    let merged = layered.sizes.unwrap().rows;
    let mut ids: Vec<&String> = under.iter().chain(&over).map(|(id, _)| id).collect();
    ids.dedup();
    for id in ids {
        let want = size_of(&over, id).or(size_of(&under, id));
        assert_eq!(size_of(&merged, id), want, "{id:?}");
        assert_eq!(
            merged.iter().filter(|(i, _)| i == id).count(),
            1,
            "{id:?} once"
        );
    }
    assert_eq!(merged.len(), {
        let mut all: Vec<&String> = under.iter().chain(&over).map(|(id, _)| id).collect();
        all.sort();
        all.dedup();
        all.len()
    });
    // The default's order holds, the user's new IDs after it.
    let firsts: Vec<&String> = merged.iter().take(under.len()).map(|(id, _)| id).collect();
    assert_eq!(firsts, under.iter().map(|(id, _)| id).collect::<Vec<_>>());
}

/// A table knowing `a` and `b` on both axes, and nothing else.
fn known() -> IdTable<String, String> {
    let named = || [("a".to_owned(), Key(10)), ("b".to_owned(), Key(11))];
    IdTable::new(named(), named())
}

#[hegel::test(test_cases = 300)]
fn converting_to_keys_skips_unknown_ids_and_the_document_keeps_them(tc: TestCase) {
    let view = support::view(&tc);
    let before = view.clone();
    let ids = known();
    let spec = view.keys(&ids);
    let is_known = |k: &Key| *k == Key(10) || *k == Key(11);
    let count = |list: &[String]| list.iter().filter(|id| *id == "a" || *id == "b").count();
    for (part, keyed) in [(&view.rows, &spec.rows), (&view.cols, &spec.cols)] {
        let sort: Vec<String> = part.sort.iter().map(|c| c.id.clone()).collect();
        assert_eq!(keyed.sort.columns().len(), count(&sort));
        assert!(keyed.sort.columns().iter().all(|c| is_known(&c.key)));
        let conditions: Vec<String> = part
            .filter
            .conditions
            .iter()
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(
            keyed.filter.conditions.conditions().len(),
            count(&conditions)
        );
        assert_eq!(keyed.hidden.len(), count(&part.hidden));
        assert!(keyed.hidden.iter().all(is_known));
        if let (Some(q), Some(k)) = (&part.filter.quick, &keyed.filter.quick)
            && let (Searched::Items(items), KeySearched::Keys(keys)) = (&q.searches, &k.searches)
        {
            assert_eq!(keys.len(), count(items));
        }
    }
    if let (Some(g), Some(k)) = (&view.grouping, &spec.grouping) {
        let slicers: Vec<String> = g.slicers.iter().map(|c| c.id.clone()).collect();
        assert_eq!(k.slicers.len(), count(&slicers));
        assert_eq!(k.rows.expand.except.len(), count(&g.row_axis.expand.except));
    }
    // The document is as it was, unknown IDs and all, and so is what it writes.
    assert_eq!(view, before);
    let doc = Layout::<String, String> {
        view: Some(view),
        ..Layout::default()
    };
    let again = Layout::<String, String>::from_json(&doc.to_json()).unwrap();
    assert_eq!(again.view.as_ref(), Some(&before));
    // Back from Keys, the known parts read as the host names them.
    let known_only = ViewSpec::from_keys(&spec, &ids);
    assert_eq!(known_only.keys(&ids), spec);
}
