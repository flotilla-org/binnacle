//! What a Described Table's rows open (its Facets), and the relations its
//! foreign keys mark (`parent`, `facet-of`).

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json};

use crate::message::Field;
use crate::wire::Wire;

/// What a foreign key says about the row it refers to, beyond naming it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Relation {
    /// The row it refers to is this row's parent, in the tree a surface
    /// shows (an agent session's convoy, a convoy's project).
    Parent,
    /// This row is a way to open the row it refers to (a terminal launch
    /// opens its agent session).
    FacetOf,
    /// A relation this crate does not know, by its name.
    Other(String),
}

impl Relation {
    pub fn from_name(name: &str) -> Relation {
        match name {
            "parent" => Relation::Parent,
            "facet-of" => Relation::FacetOf,
            other => Relation::Other(other.to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Relation::Parent => "parent",
            Relation::FacetOf => "facet-of",
            Relation::Other(name) => name,
        }
    }
}

/// Where a Facet's recipe gets its fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// Each of the recipe's fields, by the name of the table's field that
    /// holds it.
    Fields(BTreeMap<String, String>),
    /// A command line, each argument a template: `{name}` is the row's
    /// value of field `name`, `{address}` its subject address, and `{{` and
    /// `}}` are braces.
    Argv(Vec<String>),
}

/// One way a Described Table's rows can be opened, from the description's
/// `facets`: a recipe (`cleat-session`, `agent-session`, `jackstay`, `url`,
/// `command`; open) and where its fields come from. A row's Facet opens the
/// row its `facet-of` key refers to, or else the row's own subject.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facet {
    pub recipe: String,
    /// The name a menu shows for it, if the description gives one.
    pub label: Option<String>,
    pub endpoint: Endpoint,
    /// The field whose value says whether the Facet can be reached: the
    /// Facet's `state`, else the table's first field with the `state` role.
    pub state: Option<String>,
    /// The values of `state` that mean it can be reached. `None`: it can
    /// always be reached.
    pub reachable: Option<Vec<String>>,
    /// The field saying why it can't be reached, if any.
    pub reason: Option<String>,
}

impl Facet {
    /// Reads one of a schema's `facets`. One without a recipe, or whose
    /// `argv` is not a list of templates naming the table's fields, is
    /// none; an `endpoint` entry, `state` or `reason` naming a field the
    /// table lacks is left out.
    pub(crate) fn from_json(json: &Json, fields: &[Field]) -> Option<Facet> {
        let object = json.as_object()?;
        let has = |name: &str| fields.iter().any(|f| f.name == name);
        let text = |name: &str| object.get(name).and_then(Json::as_str).map(String::from);
        let field = |name: &str| text(name).filter(|f| has(f));
        let recipe = text("recipe")?;
        let endpoint = match object.get("argv") {
            Some(argv) => {
                let argv = (argv.as_array()?.iter())
                    .map(|a| a.as_str().map(String::from))
                    .collect::<Option<Vec<String>>>()?;
                let names = |name: &str| name == ADDRESS || has(name);
                if !argv
                    .iter()
                    .all(|a| template(a).is_some_and(|mut t| t.all(names)))
                {
                    return None;
                }
                Endpoint::Argv(argv)
            }
            None => Endpoint::Fields(
                (object.get("endpoint").and_then(Json::as_object).into_iter())
                    .flatten()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str().filter(|f| has(f))?.into())))
                    .collect(),
            ),
        };
        let state = field("state").or_else(|| {
            let stated = fields.iter().find(|f| f.has_role("state"));
            stated.map(|f| f.name.clone())
        });
        let reachable = (object.get("reachable").and_then(Json::as_array)).map(|values| {
            (values.iter())
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });
        Some(Facet {
            recipe,
            label: text("label"),
            endpoint,
            state,
            reachable,
            reason: field("reason"),
        })
    }

    pub(crate) fn to_json(&self) -> Json {
        let mut object = Map::new();
        object.insert("recipe".into(), Json::String(self.recipe.clone()));
        if let Some(label) = &self.label {
            object.insert("label".into(), Json::String(label.clone()));
        }
        match &self.endpoint {
            Endpoint::Fields(fields) => {
                let fields = (fields.iter())
                    .map(|(k, v)| (k.clone(), Json::String(v.clone())))
                    .collect();
                object.insert("endpoint".into(), Json::Object(fields));
            }
            Endpoint::Argv(argv) => {
                let argv = argv.iter().cloned().map(Json::String).collect();
                object.insert("argv".into(), Json::Array(argv));
            }
        }
        if let Some(state) = &self.state {
            object.insert("state".into(), Json::String(state.clone()));
        }
        if let Some(reachable) = &self.reachable {
            let reachable = reachable.iter().cloned().map(Json::String).collect();
            object.insert("reachable".into(), Json::Array(reachable));
        }
        if let Some(reason) = &self.reason {
            object.insert("reason".into(), Json::String(reason.clone()));
        }
        Json::Object(object)
    }
}

/// The placeholder for a row's subject address in a command template.
pub(crate) const ADDRESS: &str = "address";

/// One piece of a template: text as it stands, or a placeholder's name.
#[derive(Debug, PartialEq, Eq)]
enum Piece<'a> {
    Text(String),
    Name(&'a str),
}

/// The pieces of `template`, `None` if a brace is unmatched or a
/// placeholder empty.
fn pieces(template: &str) -> Option<Vec<Piece<'_>>> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = template;
    while let Some(at) = rest.find(['{', '}']) {
        text.push_str(&rest[..at]);
        let brace = &rest[at..at + 1];
        if rest[at + 1..].starts_with(brace) {
            text.push_str(brace);
            rest = &rest[at + 2..];
            continue;
        }
        if brace == "}" {
            return None;
        }
        let end = rest[at + 1..].find(['{', '}'])? + at + 1;
        let name = &rest[at + 1..end];
        if &rest[end..end + 1] == "{" || name.is_empty() {
            return None;
        }
        if !text.is_empty() {
            out.push(Piece::Text(std::mem::take(&mut text)));
        }
        out.push(Piece::Name(name));
        rest = &rest[end + 1..];
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Piece::Text(text));
    }
    Some(out)
}

/// The placeholder names in `template`, `None` if it is malformed.
fn template(template: &str) -> Option<impl Iterator<Item = &str>> {
    let pieces = pieces(template)?;
    Some(pieces.into_iter().filter_map(|p| match p {
        Piece::Name(name) => Some(name),
        Piece::Text(_) => None,
    }))
}

/// `template` with each placeholder replaced by what `value` gives for its
/// name: `None` if it is malformed or `value` gives none for one.
pub(crate) fn expand<'a>(
    template: &'a str,
    value: impl Fn(&'a str) -> Option<String>,
) -> Option<String> {
    let mut out = String::new();
    for piece in pieces(template)? {
        match piece {
            Piece::Text(text) => out.push_str(&text),
            Piece::Name(name) => out.push_str(&value(name)?),
        }
    }
    Some(out)
}

/// A value as a template or a state comparison reads it: text as it is,
/// a number or a boolean as Rust writes it (a float `1.0` as `1`, so a
/// `reachable` value for it reads `1`); `None` for a null, bytes, a list or
/// a map.
pub(crate) fn text(value: &Wire) -> Option<String> {
    match value {
        Wire::Text(s) => Some(s.clone()),
        Wire::Int(i) => Some(i.to_string()),
        Wire::Float(x) => Some(x.to_string()),
        Wire::Bool(b) => Some(b.to_string()),
        Wire::Null | Wire::Bytes(_) | Wire::Array(_) | Wire::Map(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(t: &str) -> Option<Vec<&str>> {
        template(t).map(Iterator::collect)
    }

    #[test]
    fn templates_name_their_placeholders_and_escape_doubled_braces() {
        assert_eq!(names("{address}"), Some(vec!["address"]));
        assert_eq!(
            names("--at={spec.host}:{port}"),
            Some(vec!["spec.host", "port"])
        );
        assert_eq!(names("{{literal}}"), Some(vec![]));
        assert_eq!(names("plain"), Some(vec![]));
        for bad in ["{", "}", "{}", "{a", "a}", "{a{b}", "x}y"] {
            assert_eq!(names(bad), None, "{bad}");
        }
    }

    #[test]
    fn expanding_fills_each_placeholder_or_gives_none() {
        let value = |name: &str| (name != "gone").then(|| format!("<{name}>"));
        assert_eq!(expand("a{x}b{{c}}", value), Some("a<x>b{c}".into()));
        assert_eq!(expand("{gone}", value), None);
        assert_eq!(expand("{", value), None);
    }
}
