//! The tables a server lists (`GET /tables`), by kind, with which of them
//! name a subject and which hold Facets.

use serde_json::{Value as Json, json};

use crate::message::DecodeError;

/// The tables a server lists, as `GET /tables` answers with
/// `application/json`: `{"kinds": [...]}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub kinds: Vec<ListedKind>,
}

/// One kind a server lists: its tables, and which of them are subject
/// tables (their descriptions name a subject) and which facet tables
/// (their descriptions have `facets`). A table may be both.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListedKind {
    pub kind: String,
    /// The kind's root table, if the listing names one.
    pub root: Option<String>,
    pub tables: Vec<String>,
    /// Those of `tables` that are subject tables.
    pub subjects: Vec<String>,
    /// Those of `tables` that are facet tables.
    pub facets: Vec<String>,
}

impl Listing {
    /// Reads a listing from its JSON object. A kind without a name is left
    /// out, and a subject or facet table it does not list among its
    /// `tables` is none.
    pub fn from_json(json: &Json) -> Result<Listing, DecodeError> {
        let object = json
            .as_object()
            .ok_or_else(|| DecodeError::schema("a listing is not an object"))?;
        let texts = |json: &Json, name: &str| -> Vec<String> {
            (json.get(name).and_then(Json::as_array).into_iter())
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        };
        let kinds = (object.get("kinds").and_then(Json::as_array).into_iter())
            .flatten()
            .filter_map(|kind| {
                let tables = texts(kind, "tables");
                let listed = |name: &str| {
                    let mut names = texts(kind, name);
                    names.retain(|t| tables.contains(t));
                    names
                };
                Some(ListedKind {
                    kind: kind.get("kind")?.as_str()?.to_owned(),
                    root: kind.get("root").and_then(Json::as_str).map(String::from),
                    subjects: listed("subjects"),
                    facets: listed("facets"),
                    tables,
                })
            })
            .collect();
        Ok(Listing { kinds })
    }

    /// Reads a listing from JSON bytes.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Listing, DecodeError> {
        let json: Json = serde_json::from_slice(bytes).map_err(DecodeError::json)?;
        Listing::from_json(&json)
    }

    /// The listing as JSON, as a server answers: a kind's `subjects` and
    /// `facets` only where it has some.
    pub fn to_json(&self) -> Json {
        let kinds = (self.kinds.iter())
            .map(|k| {
                let mut kind = json!({"kind": k.kind, "tables": k.tables});
                if let Some(root) = &k.root {
                    kind["root"] = json!(root);
                }
                if !k.subjects.is_empty() {
                    kind["subjects"] = json!(k.subjects);
                }
                if !k.facets.is_empty() {
                    kind["facets"] = json!(k.facets);
                }
                kind
            })
            .collect::<Vec<Json>>();
        json!({ "kinds": kinds })
    }

    /// Every table listed, kind by kind.
    pub fn tables(&self) -> impl Iterator<Item = &str> {
        self.kinds
            .iter()
            .flat_map(|k| k.tables.iter().map(String::as_str))
    }

    /// The subject tables, kind by kind.
    pub fn subject_tables(&self) -> impl Iterator<Item = &str> {
        self.kinds
            .iter()
            .flat_map(|k| k.subjects.iter().map(String::as_str))
    }

    /// The facet tables, kind by kind.
    pub fn facet_tables(&self) -> impl Iterator<Item = &str> {
        self.kinds
            .iter()
            .flat_map(|k| k.facets.iter().map(String::as_str))
    }
}
