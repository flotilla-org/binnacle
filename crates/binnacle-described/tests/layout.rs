//! A Described Table's default presentation and its IDs: the generic
//! default Layout, a default view loaded by field name, a user's Layout
//! over both, and Nested columns that stay in reach of filters and find.

mod support;

use std::collections::BTreeMap;

use binnacle_core::{Axis, Condition, Edge, FilterSpec, Key, SortDirection, TextQuery, ViewSpec};
use binnacle_described::{
    DescribedSource, Field, FieldType, Message, RowsMessage, Schema, Scope, TableMessage, Wire,
    deterministic_cbor,
};
use binnacle_layout::{EdgePins, IdMap, JsonId, Layout, Pins, Sizes};
use binnacle_memory::PumpSource;
use binnacle_testkit::Scenario;
use hegel::TestCase;
use hegel::generators as gs;
use support::{Table, column_down, replay, scenario};

type Doc = Layout<JsonId, JsonId>;

fn id(name: &str) -> JsonId {
    JsonId::from(name)
}

fn ids(names: &[&str]) -> Vec<JsonId> {
    names.iter().map(|n| id(n)).collect()
}

fn hosts() -> Scenario<Table> {
    scenario(replay("rows-hosts.ndjson"))
}

fn crews() -> Scenario<Table> {
    scenario(replay("crews.ndjson"))
}

const NESTED: [&str; 5] = [
    "metadata.labels",
    "metadata.annotations",
    "status.conditions",
    "spec",
    "status",
];

fn restore(s: &mut Scenario<Table>, layout: &Doc) {
    layout.restore(&mut s.grid, &s.source.0);
    s.frame(support::VIEWPORT);
}

fn names(s: &Scenario<Table>, keys: impl IntoIterator<Item = Key>) -> Vec<String> {
    (keys.into_iter())
        .map(|k| s.source.0.column(k).unwrap().name().to_owned())
        .collect()
}

fn shown_columns(s: &mut Scenario<Table>) -> Vec<String> {
    let snapshot = s.frame(support::VIEWPORT);
    let keys: Vec<Key> = snapshot.drawn(Axis::Cols).filter_map(|i| i.key).collect();
    names(s, keys)
}

fn hidden_columns(s: &mut Scenario<Table>) -> Vec<String> {
    let keys = s.frame(support::VIEWPORT).hidden(Axis::Cols).to_vec();
    names(s, keys)
}

fn pinned(s: &mut Scenario<Table>) -> Vec<String> {
    let keys: Vec<Key> = s
        .frame(support::VIEWPORT)
        .pins(Axis::Cols, Edge::Leading)
        .collect();
    names(s, keys)
}

fn view_doc() -> Doc {
    let json = include_bytes!("fixtures/hosts-view.json");
    Doc::from_json(json).unwrap()
}

#[test]
fn the_default_layout_hides_the_nested_columns_and_pins_key_then_state() {
    let mut s = hosts();
    let layout = s.source.0.default_layout();
    restore(&mut s, &layout);
    let mut hidden = hidden_columns(&mut s);
    hidden.sort();
    let mut want = NESTED.to_vec();
    want.sort();
    assert_eq!(hidden, want, "the chooser offers them");
    assert_eq!(pinned(&mut s), ["metadata.name", "status.phase"]);
    let shown = shown_columns(&mut s);
    assert!(NESTED.iter().all(|n| !shown.contains(&n.to_string())));
}

#[test]
fn the_default_layout_lists_a_field_with_both_roles_once() {
    let mut source = DescribedSource::new("t", Scope::new("n"));
    let mut both = Field::new("a", FieldType::String);
    both.roles = vec!["state".into(), "key".into()];
    let mut state = Field::new("b", FieldType::String);
    state.roles = vec!["state".into()];
    let mut key = Field::new("c", FieldType::String);
    key.roles = vec!["key".into()];
    source.receive(Message::Table(TableMessage {
        schema: Schema {
            fields: vec![state, both, key, Field::new("d", FieldType::Array)],
            primary_key: vec!["c".into()],
            foreign_keys: Vec::new(),
        },
        incarnation: None,
        table: None,
    }));
    let layout = source.default_layout();
    assert_eq!(layout.pins.unwrap().cols.leading, ids(&["a", "c", "b"]));
    assert_eq!(layout.view.unwrap().cols.hidden, ids(&["d"]));
}

#[test]
fn a_quick_filter_on_text_only_in_a_hidden_nested_column_keeps_that_row() {
    let mut s = crews();
    let layout = s.source.0.default_layout();
    restore(&mut s, &layout);
    assert!(hidden_columns(&mut s).contains(&"spec".to_owned()));
    s.input(|g| g.set_quick_filter(Axis::Rows, Some(TextQuery::new("rust"))));
    assert_eq!(column_down(&mut s, "name"), ["alpha"]);
    let applied = s.frame(support::VIEWPORT).applied_view().clone();
    let quick = applied.axis(Axis::Rows).filter.quick.clone().unwrap();
    assert_eq!(
        quick.searches,
        binnacle_core::Searched::Shown,
        "reported as asked"
    );
}

#[test]
fn a_quick_filter_asked_before_the_columns_came_still_searches_the_nested_ones() {
    let mut source = DescribedSource::new("crews", Scope::new("n"));
    let spec = ViewSpec::default().with_filter(
        Axis::Rows,
        FilterSpec::default().with_quick(Some(TextQuery::new("rust"))),
    );
    source.view(&spec);
    for message in support::messages("crews.ndjson") {
        source.receive(message);
    }
    let window = source.window(Axis::Rows, 0..source.count(Axis::Rows));
    let alpha = source.row_key(&[Wire::Text("alpha".into())]).unwrap();
    assert_eq!(window.keys, [alpha]);
}

#[test]
fn a_text_condition_on_a_hidden_nested_column_filters_the_rows() {
    let mut s = crews();
    let layout = s.source.0.default_layout();
    restore(&mut s, &layout);
    let spec = s.source.0.column_key("spec").unwrap();
    let condition = Condition::text(spec, TextQuery::new("rust"));
    s.input(|g| g.add_condition(Axis::Rows, condition));
    assert_eq!(column_down(&mut s, "name"), ["alpha"]);
}

#[test]
fn a_default_view_loaded_by_field_name_sets_the_order_hidden_columns_and_sort() {
    let mut s = hosts();
    let layout = s.source.0.presentation(Some(&view_doc()), None);
    restore(&mut s, &layout);
    // The view sets no pins, so the generic ones stay and lead the order.
    let arranged = s.grid.arranged(Axis::Cols).to_vec();
    assert_eq!(
        names(&s, arranged),
        ["metadata.creationTimestamp", "metadata.name"]
    );
    let shown = shown_columns(&mut s);
    assert_eq!(
        shown[..3],
        [
            "metadata.name",
            "status.phase",
            "metadata.creationTimestamp"
        ]
    );
    let mut hidden = hidden_columns(&mut s);
    hidden.sort();
    let mut want = NESTED.to_vec();
    want.push("metadata.resourceVersion");
    want.sort();
    assert_eq!(hidden, want);
    let snapshot = s.frame(support::VIEWPORT);
    let sort = snapshot
        .applied_view()
        .axis(Axis::Rows)
        .sort
        .columns()
        .to_vec();
    let created = s.source.0.column_key("metadata.creationTimestamp").unwrap();
    assert_eq!(sort.len(), 1);
    assert_eq!(sort[0].key, created);
    assert_eq!(sort[0].direction, SortDirection::Descending);
    assert_eq!(pinned(&mut s), ["metadata.name", "status.phase"]);
}

#[test]
fn a_user_layout_with_only_sizes_keeps_the_views_parts_and_wins_per_column() {
    let s = hosts();
    let user = Doc {
        sizes: Some(Sizes {
            rows: Vec::new(),
            cols: vec![(id("metadata.name"), 31.0)],
        }),
        ..Doc::default()
    };
    let mut view = view_doc();
    view.sizes = Some(Sizes {
        rows: Vec::new(),
        cols: vec![(id("metadata.name"), 12.0), (id("spec"), 14.0)],
    });
    let layout = s.source.0.presentation(Some(&view), Some(&user));
    assert_eq!(layout.view, view.view);
    assert_eq!(layout.order, view.order);
    assert_eq!(
        layout.sizes.unwrap().cols,
        [(id("metadata.name"), 31.0), (id("spec"), 14.0)]
    );
    assert_eq!(layout.pins, s.source.0.default_layout().pins);
}

#[test]
fn a_user_layout_with_only_pins_changes_only_the_pins() {
    let mut s = hosts();
    let user = Doc {
        pins: Some(Pins {
            cols: EdgePins {
                leading: ids(&["metadata.namespace"]),
                ..EdgePins::default()
            },
            ..Pins::default()
        }),
        ..Doc::default()
    };
    let layout = s.source.0.presentation(Some(&view_doc()), Some(&user));
    restore(&mut s, &layout);
    assert_eq!(pinned(&mut s), ["metadata.namespace"]);
    let mut hidden = hidden_columns(&mut s);
    hidden.sort();
    assert_eq!(hidden.len(), 6);
    assert_eq!(shown_columns(&mut s)[0], "metadata.namespace");
    let layout_view = s.source.0.presentation(Some(&view_doc()), None).view;
    assert_eq!(
        s.source.0.presentation(Some(&view_doc()), Some(&user)).view,
        layout_view
    );
}

#[test]
fn a_default_view_naming_unknown_columns_loads_and_keeps_them_on_save() {
    let json = br#"{"version":2,"view":{"rows":{"sort":[{"id":"nosuch","direction":"ascending"},{"id":"metadata.name","direction":"descending"}]},"cols":{"hidden":["nosuch","spec"]}},"order":{"cols":["nosuch","metadata.status","metadata.name"]}}"#;
    let view = Doc::from_json(json).unwrap();
    let mut s = hosts();
    let layout = s.source.0.presentation(Some(&view), None);
    restore(&mut s, &layout);
    assert_eq!(hidden_columns(&mut s), ["spec"]);
    let snapshot = s.frame(support::VIEWPORT);
    assert_eq!(
        snapshot
            .applied_view()
            .axis(Axis::Rows)
            .sort
            .columns()
            .len(),
        1
    );
    assert_eq!(shown_columns(&mut s)[0], "metadata.name");
    let saved = Doc::read(&mut s.grid, &s.source.0).keeping(&layout, &s.source.0);
    let view = saved.view.unwrap();
    assert!(view.cols.hidden.contains(&id("nosuch")));
    assert!(view.rows.sort.iter().any(|c| c.id == id("nosuch")));
    assert!(saved.order.unwrap().cols.unwrap().contains(&id("nosuch")));
}

/// A table with a primary key of `kinds`, one part each, the rows' tuples.
fn keyed(parts: &[bool], tuples: &[Vec<Wire>]) -> DescribedSource {
    let mut source = DescribedSource::new("t", Scope::new("n"));
    let name = |i: usize| format!("k{i}");
    let fields = (parts.iter().enumerate())
        .map(|(i, &int)| {
            let kind = if int {
                FieldType::Integer
            } else {
                FieldType::String
            };
            Field::new(name(i), kind)
        })
        .chain([Field::new("value", FieldType::String)])
        .collect();
    source.receive(Message::Table(TableMessage {
        schema: Schema {
            fields,
            primary_key: (0..parts.len()).map(name).collect(),
            foreign_keys: Vec::new(),
        },
        incarnation: None,
        table: None,
    }));
    let rows = (tuples.iter())
        .map(|t| {
            let mut row: BTreeMap<String, Wire> = (t.iter().enumerate())
                .map(|(i, w)| (name(i), w.clone()))
                .collect();
            row.insert("value".into(), Wire::Text("v".into()));
            row
        })
        .collect();
    source.receive(Message::Rows(RowsMessage {
        rows,
        cursor: None,
        partial: false,
    }));
    source
}

#[hegel::test(test_cases = 300)]
fn row_and_column_ids_round_trip_and_unknown_ids_name_nothing(tc: TestCase) {
    let parts = tc.draw(gs::vecs(gs::booleans()).min_size(1).max_size(3));
    let tuples = tc.draw(
        gs::vecs(
            hegel::generators::vecs(gs::integers::<u64>())
                .min_size(parts.len())
                .max_size(parts.len()),
        )
        .max_size(6),
    );
    let tuples: Vec<Vec<Wire>> = (tuples.iter())
        .map(|raw| {
            (parts.iter().zip(raw))
                .map(|(&int, &n)| match (int, n % 3) {
                    (true, 0) => Wire::Int(i128::from(u64::MAX - n % 5)),
                    (true, _) => Wire::Int(i128::from(n as i64)),
                    (false, _) => Wire::Text(format!("t{}", n % 7)),
                })
                .collect()
        })
        .collect();
    let source = keyed(&parts, &tuples);
    for tuple in &tuples {
        let json = JsonId::new(serde_json::Value::Array(
            tuple.iter().map(Wire::to_json).collect(),
        ))
        .unwrap();
        let key = IdMap::row_key(&source, &json).unwrap();
        assert_eq!(IdMap::row_id(&source, key), Some(json));
        let cbor = deterministic_cbor(&Wire::Array(tuple.clone()));
        assert_eq!(source.row_identity(key), Some(&cbor[..]));
    }
    for column in source.columns() {
        let name = id(column.name());
        let key = source.col_key(&name).unwrap();
        assert_eq!(key, column.key());
        assert_eq!(source.col_id(key), Some(name));
    }
    let lacking = JsonId::from("nosuch");
    assert_eq!(source.col_key(&lacking), None);
    let stranger = JsonId::new(serde_json::json!(["no", "such", "row", 1, 2, 3, 4])).unwrap();
    assert_eq!(IdMap::row_key(&source, &stranger), None);
    assert_eq!(
        source.col_key(&JsonId::new(serde_json::json!(3)).unwrap()),
        None
    );
    assert_eq!(source.col_id(Key(u64::MAX >> 1)), None);
    assert_eq!(IdMap::row_id(&source, Key(u64::MAX >> 1)), None);
}
