//! The Layout: the portable, versioned document of how a table is
//! presented, by the Table Source's IDs. It holds the View Spec, a partial
//! order of the items, the pinned items, Size Overrides and the Header
//! Band thicknesses, each part optional. A source ships one as its default
//! view, a user's saved layout is another, and [`Layout::layered`] lays the
//! second over the first.
//!
//! As JSON it is version 2 of the web's `Layout` (`web/src/layout.ts`),
//! byte for byte: the same fields, and numbers written as JavaScript writes
//! them. It reads and writes as CBOR too. Version 1, the same document
//! without `order`, still reads. `layout.schema.json` beside this crate is
//! its JSON Schema, with string IDs.

mod grid;
mod ids;
mod num;
pub mod view;

use std::fmt;

use schemars::JsonSchema;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub use ids::{IdMap, IdTable};
pub use view::ViewSpec;

/// The version a Layout is written as.
pub const LAYOUT_VERSION: u32 = 2;

/// How a table is presented, by the Table Source's IDs. A part left out is
/// not set: restoring leaves the Grid's own, and layering keeps the
/// default's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Layout")]
#[serde(bound(
    serialize = "RowId: Serialize, ColId: Serialize",
    deserialize = "RowId: Deserialize<'de>, ColId: Deserialize<'de>"
))]
pub struct Layout<RowId, ColId> {
    pub version: Version,
    /// The View Spec: each axis's filter, sort and hidden items, and the
    /// grouping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(transform = absent_not_null)]
    pub view: Option<ViewSpec<RowId, ColId>>,
    /// The items to place first on each axis, in this order; the rest
    /// follow in the Table Source's order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(transform = absent_not_null)]
    pub order: Option<Order<RowId, ColId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(transform = absent_not_null)]
    pub pins: Option<Pins<RowId, ColId>>,
    /// Size Overrides, as `[id, size]` pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(transform = absent_not_null)]
    pub sizes: Option<Sizes<RowId, ColId>>,
    /// The thickness of the Header Band leading each axis: `rows` is the
    /// column header band along the top, `cols` the row header band down
    /// the left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(transform = absent_not_null)]
    pub bands: Option<Bands>,
}

impl<RowId, ColId> Default for Layout<RowId, ColId> {
    fn default() -> Self {
        Layout {
            version: Version,
            view: None,
            order: None,
            pins: None,
            sizes: None,
            bands: None,
        }
    }
}

/// A part's schema as a document holds it: left out when not set, never
/// `null`.
fn absent_not_null(schema: &mut schemars::Schema) {
    let Some(any) = schema.remove("anyOf") else {
        return;
    };
    let set = (any.as_array().into_iter().flatten())
        .find(|s| s.get("type").is_none_or(|t| t != "null"))
        .cloned();
    if let Some(serde_json::Value::Object(set)) = set {
        schema
            .as_object_mut()
            .into_iter()
            .for_each(|o| o.extend(set.clone()));
    }
}

/// A Layout's version: written as [`LAYOUT_VERSION`], read as it or as 1,
/// whose documents are the same less `order`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Version;

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(LAYOUT_VERSION)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Known;
        impl Visitor<'_> for Known {
            type Value = Version;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "Layout version 1 or {LAYOUT_VERSION}")
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Version, E> {
                match v {
                    1 | 2 => Ok(Version),
                    _ => Err(E::custom(format_args!("unknown Layout version {v}"))),
                }
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Version, E> {
                match u64::try_from(v) {
                    Ok(v) => self.visit_u64(v),
                    Err(_) => Err(E::custom(format_args!("unknown Layout version {v}"))),
                }
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Version, E> {
                match v {
                    1.0 | 2.0 => Ok(Version),
                    _ => Err(E::custom(format_args!("unknown Layout version {v}"))),
                }
            }
        }
        deserializer.deserialize_any(Known)
    }
}

impl JsonSchema for Version {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Version".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "The version the document is written as. Version 1, the same document less `order`, also reads.",
            "const": LAYOUT_VERSION,
        })
    }
}

/// Per axis, the items to place first, in this order; an axis left out
/// places none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(bound(
    serialize = "RowId: Serialize, ColId: Serialize",
    deserialize = "RowId: Deserialize<'de>, ColId: Deserialize<'de>"
))]
pub struct Order<RowId, ColId> {
    #[serde(default)]
    pub rows: Vec<RowId>,
    #[serde(default)]
    pub cols: Vec<ColId>,
}

impl<RowId, ColId> Default for Order<RowId, ColId> {
    fn default() -> Self {
        Order {
            rows: Vec::new(),
            cols: Vec::new(),
        }
    }
}

impl<RowId, ColId> Order<RowId, ColId> {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.cols.is_empty()
    }
}

/// Per axis, each edge's pinned items in pin order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(bound(
    serialize = "RowId: Serialize, ColId: Serialize",
    deserialize = "RowId: Deserialize<'de>, ColId: Deserialize<'de>"
))]
pub struct Pins<RowId, ColId> {
    #[serde(default)]
    pub rows: EdgePins<RowId>,
    #[serde(default)]
    pub cols: EdgePins<ColId>,
}

impl<RowId, ColId> Default for Pins<RowId, ColId> {
    fn default() -> Self {
        Pins {
            rows: EdgePins::default(),
            cols: EdgePins::default(),
        }
    }
}

/// The items pinned to each edge of one axis, in pin order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(bound(serialize = "Id: Serialize", deserialize = "Id: Deserialize<'de>"))]
pub struct EdgePins<Id> {
    #[serde(default)]
    pub leading: Vec<Id>,
    #[serde(default)]
    pub trailing: Vec<Id>,
}

impl<Id> Default for EdgePins<Id> {
    fn default() -> Self {
        EdgePins {
            leading: Vec::new(),
            trailing: Vec::new(),
        }
    }
}

/// Per axis, Size Overrides as `[id, size]` pairs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "RowId: Serialize, ColId: Serialize",
    deserialize = "RowId: Deserialize<'de>, ColId: Deserialize<'de>"
))]
pub struct Sizes<RowId, ColId> {
    #[serde(default, with = "num::pairs")]
    pub rows: Vec<(RowId, f64)>,
    #[serde(default, with = "num::pairs")]
    pub cols: Vec<(ColId, f64)>,
}

impl<RowId, ColId> Default for Sizes<RowId, ColId> {
    fn default() -> Self {
        Sizes {
            rows: Vec::new(),
            cols: Vec::new(),
        }
    }
}

impl<RowId: JsonSchema, ColId: JsonSchema> JsonSchema for Sizes<RowId, ColId> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Sizes".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let rows = generator.subschema_for::<Vec<(RowId, f64)>>();
        let cols = generator.subschema_for::<Vec<(ColId, f64)>>();
        schemars::json_schema!({
            "description": "Per axis, Size Overrides as `[id, size]` pairs.",
            "type": "object",
            "properties": { "rows": rows, "cols": cols },
        })
    }
}

/// The thickness of the Header Band leading each axis.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bands {
    #[serde(with = "num")]
    #[schemars(with = "f64")]
    pub rows: f64,
    #[serde(with = "num")]
    #[schemars(with = "f64")]
    pub cols: f64,
}

impl<RowId: PartialEq, ColId: PartialEq> Layout<RowId, ColId> {
    /// `user` laid over `default`: each part `user` sets (the View Spec,
    /// the order, the pins, the bands) replaces the default's whole part,
    /// and Size Overrides merge per ID, `user`'s winning, so one resized
    /// column keeps the default widths of the others. A part `user` leaves
    /// unset keeps the default's.
    pub fn layered(default: Self, user: Self) -> Self {
        let sizes = match (default.sizes, user.sizes) {
            (Some(d), Some(u)) => Some(Sizes {
                rows: merged(d.rows, u.rows),
                cols: merged(d.cols, u.cols),
            }),
            (d, u) => u.or(d),
        };
        Layout {
            version: Version,
            view: user.view.or(default.view),
            order: user.order.or(default.order),
            pins: user.pins.or(default.pins),
            sizes,
            bands: user.bands.or(default.bands),
        }
    }
}

/// `under`'s pairs with `over`'s sizes where they share an ID, then
/// `over`'s other pairs.
fn merged<Id: PartialEq>(under: Vec<(Id, f64)>, over: Vec<(Id, f64)>) -> Vec<(Id, f64)> {
    let mut over: Vec<Option<(Id, f64)>> = over.into_iter().map(Some).collect();
    let mut out: Vec<(Id, f64)> = (under.into_iter())
        .map(|(id, size)| {
            let mine = over
                .iter_mut()
                .find(|p| p.as_ref().is_some_and(|(o, _)| *o == id));
            match mine.and_then(Option::take) {
                Some(pair) => pair,
                None => (id, size),
            }
        })
        .collect();
    out.extend(over.into_iter().flatten());
    out
}

impl<RowId: Serialize, ColId: Serialize> Layout<RowId, ColId> {
    /// The Layout as JSON, as the web writes it.
    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a Layout with serialisable IDs writes as JSON")
    }

    /// The Layout as CBOR.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut out = Vec::new();
        ciborium::into_writer(self, &mut out)
            .expect("a Layout with serialisable IDs writes as CBOR");
        out
    }
}

impl<RowId: for<'de> Deserialize<'de>, ColId: for<'de> Deserialize<'de>> Layout<RowId, ColId> {
    /// Reads a Layout from JSON. A document of an unknown version, or not a
    /// Layout, is an error.
    pub fn from_json(json: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(json)
    }

    /// Reads a Layout from CBOR, as [`Layout::from_json`] does from JSON.
    pub fn from_cbor(cbor: &[u8]) -> Result<Self, ciborium::de::Error<std::io::Error>> {
        ciborium::from_reader(cbor)
    }
}

/// The JSON Schema of a Layout whose IDs are strings, as checked in at
/// `crates/binnacle-layout/layout.schema.json`: pretty-printed, with a
/// final newline.
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Layout<String, String>);
    let mut text = serde_json::to_string_pretty(&schema).expect("a schema writes as JSON");
    text.push('\n');
    text
}
