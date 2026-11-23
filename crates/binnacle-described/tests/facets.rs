//! What a Described Table's description says about its rows beyond their
//! columns: card roles, parents, the Facets that open them, and which
//! listed tables are subject and facet tables.

use binnacle_described::{
    DescribedSource, Endpoint, HostRequest, Listing, Message, Opening, Received, Relation, Schema,
    Scope, Wire,
};
use serde_json::{Value as Json, json};

/// Terminal launches, shaped as flotilla#4438 describes them: an Agent
/// launch opens its agent session, a Tool launch is its own subject under
/// its convoy.
fn launches() -> Json {
    json!({
        "fields": [
            {"name": "name", "type": "string"},
            {"name": "address", "type": "string"},
            {"name": "summary", "type": "string", "roles": ["caption"]},
            {"name": "provider", "type": "string", "roles": ["badge"]},
            {"name": "model", "type": "string", "roles": ["badge"]},
            {"name": "convoy", "type": "string"},
            {"name": "project", "type": "string"},
            {"name": "agent_session", "type": "string"},
            {"name": "session_id", "type": "string"},
            {"name": "host", "type": "string"},
            {"name": "port", "type": "integer"},
            {"name": "phase", "type": "string", "roles": ["state"]},
            {"name": "message", "type": "string"},
        ],
        "primaryKey": ["name"],
        "subject": {"kind": "terminal", "field": "address"},
        "foreignKeys": [
            {"fields": "agent_session", "reference": {"resource": "agentsessions", "fields": "metadata.name"}, "relation": "facet-of"},
            {"fields": "convoy", "reference": {"resource": "convoys", "fields": "metadata.name"}, "relation": "parent"},
            {"fields": "project", "reference": {"resource": "projects", "fields": "metadata.name"}, "relation": "parent"},
            {"fields": "host", "reference": {"resource": "hosts", "fields": "metadata.name"}},
        ],
        "facets": [
            {"recipe": "cleat-session", "label": "Terminal",
             "endpoint": {"session": "session_id", "host": "host", "pool": "nowhere"},
             "reachable": ["Running"], "reason": "message"},
            {"recipe": "command", "label": "Attach",
             "argv": ["flotilla", "attach", "{address}", "--port={port}", "{{x}}"]},
        ],
    })
}

fn source(schema: Json) -> DescribedSource {
    let mut source = DescribedSource::new("terminallaunches", Scope::new("flotilla"));
    assert!(matches!(
        source.poll_request(),
        Some(HostRequest::OpenWatch(_))
    ));
    receive(&mut source, json!({"type": "table", "schema": schema}));
    source
}

fn receive(source: &mut DescribedSource, message: Json) {
    let message = Message::from_json(&message).unwrap();
    assert!(matches!(source.receive(message), Received::Applied(_)));
}

fn rows(source: &mut DescribedSource, rows: Json) {
    receive(source, json!({"type": "rows", "rows": rows}));
}

fn key(source: &DescribedSource, name: &str) -> binnacle_core::Key {
    source.row_key(&[Wire::Text(name.into())]).unwrap()
}

fn text(s: &str) -> Wire {
    Wire::Text(s.into())
}

#[test]
fn caption_and_badge_roles_mark_a_cards_fields_in_order() {
    let schema = Schema::from_json(&launches()).unwrap();
    let names = |role| -> Vec<&str> {
        let fields = schema.role_fields(role);
        fields.map(|f| f.name.as_str()).collect()
    };
    assert_eq!(names("caption"), ["summary"]);
    assert_eq!(names("badge"), ["provider", "model"]);
}

#[test]
fn a_rows_parent_is_its_first_parent_key_that_is_set() {
    let mut source = source(launches());
    rows(
        &mut source,
        json!([
            {"name": "a", "convoy": "c1", "project": "p1"},
            {"name": "b", "convoy": null, "project": "p1"},
            {"name": "c"},
        ]),
    );
    let parent = |name| {
        let related = source.row_parent(key(&source, name))?;
        Some((related.resource()?.to_owned(), related.values[0].clone()))
    };
    assert_eq!(parent("a"), Some(("convoys".into(), text("c1"))));
    assert_eq!(parent("b"), Some(("projects".into(), text("p1"))));
    assert_eq!(parent("c"), None);
}

#[test]
fn a_facet_opens_what_the_row_is_a_facet_of_or_else_its_own_subject() {
    let mut source = source(launches());
    rows(
        &mut source,
        json!([
            {"name": "agent", "address": "t/agent", "agent_session": "s1",
             "session_id": "cleat-7", "host": "kiwi", "port": 22, "phase": "Running"},
            {"name": "tool", "address": "t/tool", "convoy": "c1",
             "session_id": "cleat-8", "host": null, "port": 23, "phase": "Gone", "message": "exited"},
        ]),
    );
    let agent = key(&source, "agent");
    let of = source.row_facet_of(agent).unwrap();
    assert_eq!(of.resource(), Some("agentsessions"));
    assert_eq!(of.values, [&text("s1")]);
    let facets = source.row_facets(agent);
    assert_eq!(facets.len(), 2);
    assert_eq!(facets[0].facet.recipe, "cleat-session");
    assert_eq!(facets[0].facet.label.as_deref(), Some("Terminal"));
    assert_eq!(
        facets[0].of.as_ref().unwrap().resource(),
        Some("agentsessions")
    );
    let endpoint = [("host", &text("kiwi")), ("session", &text("cleat-7"))];
    assert_eq!(facets[0].opening, Opening::Endpoint(endpoint.into()));
    assert!(facets[0].reachable);
    assert_eq!(facets[0].reason, None);
    let argv = ["flotilla", "attach", "t/agent", "--port=22", "{x}"];
    assert_eq!(
        facets[1].opening,
        Opening::Command(argv.map(String::from).into())
    );

    let tool = key(&source, "tool");
    assert_eq!(source.row_facet_of(tool), None);
    assert_eq!(source.row_parent(tool).unwrap().resource(), Some("convoys"));
    let facets = source.row_facets(tool);
    assert_eq!(facets[0].of, None);
    let endpoint = [("session", &text("cleat-8"))];
    assert_eq!(facets[0].opening, Opening::Endpoint(endpoint.into()));
    assert!(!facets[0].reachable);
    assert_eq!(facets[0].reason, Some("exited"));
}

#[test]
fn a_command_is_left_out_where_a_template_names_a_value_the_row_lacks() {
    let mut source = source(launches());
    rows(
        &mut source,
        json!([
            {"name": "no-port", "address": "t/a"},
            {"name": "no-address", "port": 1},
        ]),
    );
    for name in ["no-port", "no-address"] {
        let facets = source.row_facets(key(&source, name));
        let recipes: Vec<&str> = facets.iter().map(|f| f.facet.recipe.as_str()).collect();
        assert_eq!(recipes, ["cleat-session"], "{name}");
        assert!(
            !facets[0].reachable,
            "{name}: no state, so not one of the reachable"
        );
    }
}

#[test]
fn a_facet_reads_its_state_from_the_state_role_unless_it_names_one() {
    let schema = Schema::from_json(&launches()).unwrap();
    assert_eq!(schema.facets[0].state.as_deref(), Some("phase"));
    let mut named = launches();
    named["facets"][0]["state"] = json!("message");
    let schema = Schema::from_json(&named).unwrap();
    assert_eq!(schema.facets[0].state.as_deref(), Some("message"));
}

#[test]
fn a_facet_listing_no_reachable_values_can_always_be_reached() {
    let mut source = source(launches());
    rows(
        &mut source,
        json!([{"name": "a", "address": "t/a", "port": 1, "phase": "Gone"}]),
    );
    let facets = source.row_facets(key(&source, "a"));
    assert!(facets[1].reachable);
}

#[test]
fn what_a_facet_names_that_the_table_lacks_is_left_out() {
    let mut schema = launches();
    schema["facets"].as_array_mut().unwrap().extend([
        json!({"label": "no recipe", "endpoint": {}}),
        json!({"recipe": "command", "argv": ["{nowhere}"]}),
        json!({"recipe": "command", "argv": ["{unclosed"]}),
        json!({"recipe": "command", "argv": [1]}),
        json!({"recipe": "url", "endpoint": {"url": "nowhere"}, "state": "nowhere", "reason": "nowhere"}),
    ]);
    let schema = Schema::from_json(&schema).unwrap();
    let recipes: Vec<&str> = schema.facets.iter().map(|f| f.recipe.as_str()).collect();
    assert_eq!(recipes, ["cleat-session", "command", "url"]);
    let Endpoint::Fields(fields) = &schema.facets[0].endpoint else {
        panic!("an endpoint")
    };
    assert!(!fields.contains_key("pool"));
    let url = &schema.facets[2];
    assert_eq!(url.endpoint, Endpoint::Fields(Default::default()));
    assert_eq!(
        url.state.as_deref(),
        Some("phase"),
        "the state role, in place of a missing field"
    );
    assert_eq!(url.reason, None);
}

#[test]
fn relations_and_facets_survive_a_round_trip_and_an_unknown_relation_is_kept() {
    let mut json = launches();
    json["foreignKeys"][3]["relation"] = json!("sibling");
    let schema = Schema::from_json(&json).unwrap();
    let relations: Vec<Option<&Relation>> = schema
        .foreign_keys
        .iter()
        .map(|k| k.relation.as_ref())
        .collect();
    let sibling = Relation::Other("sibling".into());
    assert_eq!(
        relations,
        [
            Some(&Relation::FacetOf),
            Some(&Relation::Parent),
            Some(&Relation::Parent),
            Some(&sibling)
        ]
    );
    assert_eq!(Schema::from_json(&schema.to_json()).unwrap(), schema);
}

#[test]
fn a_listing_marks_its_subject_and_facet_tables() {
    let listing = Listing::from_json_bytes(
        json!({"kinds": [
            {"kind": "TerminalLaunch", "root": "terminallaunches", "tables": ["terminallaunches"],
             "subjects": ["terminallaunches"], "facets": ["terminallaunches", "unlisted"]},
            {"kind": "Host", "root": "hosts", "tables": ["hosts"]},
            {"root": "nameless", "tables": ["nameless"]},
        ]})
        .to_string()
        .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        listing.tables().collect::<Vec<_>>(),
        ["terminallaunches", "hosts"]
    );
    assert_eq!(
        listing.subject_tables().collect::<Vec<_>>(),
        ["terminallaunches"]
    );
    assert_eq!(
        listing.facet_tables().collect::<Vec<_>>(),
        ["terminallaunches"]
    );
    assert_eq!(Listing::from_json(&listing.to_json()).unwrap(), listing);
}

#[test]
fn a_recorded_listing_reads_with_no_marks() {
    let bytes = include_bytes!("data/listing.json");
    let listing = Listing::from_json_bytes(bytes).unwrap();
    assert!(listing.tables().any(|t| t == "terminalsessions"));
    assert_eq!(listing.subject_tables().count(), 0);
    assert_eq!(listing.facet_tables().count(), 0);
}
