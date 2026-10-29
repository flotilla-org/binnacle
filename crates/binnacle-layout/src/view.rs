//! The View Spec by the Table Source's IDs, in the shape `ViewSpec` has in
//! `web/src/source.ts`, and its conversion to and from the core's Keys.

use binnacle_core as core;
use binnacle_core::{Key, SortDirection};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::IdMap;

/// The declared view a Table Source should present, by IDs: one part per
/// axis, and the whole-table grouping (`null` for a plain table). A part
/// left out of a document reads as empty. The open groups name Group Items
/// by their IDs on the axis, as [`crate::Layout`] says.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    serialize = "RowId: Serialize, ColId: Serialize",
    deserialize = "RowId: Deserialize<'de>, ColId: Deserialize<'de>"
))]
pub struct ViewSpec<RowId, ColId> {
    #[serde(default)]
    pub rows: AxisView<ColId, RowId>,
    #[serde(default)]
    pub cols: AxisView<RowId, ColId>,
    #[serde(default)]
    pub grouping: Option<Grouping<ColId, RowId, ColId>>,
}

impl<RowId, ColId> Default for ViewSpec<RowId, ColId> {
    fn default() -> Self {
        ViewSpec {
            rows: AxisView::default(),
            cols: AxisView::default(),
            grouping: None,
        }
    }
}

/// One axis's part of a View Spec: its filter and sort, naming items of the
/// other axis (`CrossId`), and its own items (`Id`) hidden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(bound(
    serialize = "CrossId: Serialize, Id: Serialize",
    deserialize = "CrossId: Deserialize<'de>, Id: Deserialize<'de>"
))]
pub struct AxisView<CrossId, Id> {
    #[serde(default)]
    pub filter: FilterSpec<CrossId>,
    #[serde(default)]
    pub sort: Vec<SortColumn<CrossId>>,
    #[serde(default)]
    pub hidden: Vec<Id>,
}

impl<CrossId, Id> Default for AxisView<CrossId, Id> {
    fn default() -> Self {
        AxisView {
            filter: FilterSpec::default(),
            sort: Vec::new(),
            hidden: Vec::new(),
        }
    }
}

/// One item of the other axis to sort by, and which way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SortColumn<Id> {
    pub id: Id,
    pub direction: Direction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Ascending,
    Descending,
}

/// Which items of an axis to keep: the quick filter (`null` for none) and
/// the conditions, every one of which must keep an item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(bound(serialize = "Id: Serialize", deserialize = "Id: Deserialize<'de>"))]
pub struct FilterSpec<Id> {
    #[serde(default)]
    pub quick: Option<TextQuery<Id>>,
    #[serde(default)]
    pub conditions: Vec<Condition<Id>>,
}

impl<Id> Default for FilterSpec<Id> {
    fn default() -> Self {
        FilterSpec {
            quick: None,
            conditions: Vec::new(),
        }
    }
}

/// Text to look for in the items of the other axis it searches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TextQuery<Id> {
    pub text: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub searches: Searched<Id>,
}

/// The items a Text Query searches: `"shown"`, or those listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Searched<Id> {
    Shown(Shown),
    Items(Vec<Id>),
}

/// The word `"shown"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Shown {
    #[serde(rename = "shown")]
    Shown,
}

/// A Text condition's query: a Text Query without items to search, as it
/// searches its own.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct TextMatch {
    pub text: String,
    pub case_sensitive: bool,
    pub regex: bool,
}

/// One condition, read in item `id` of the other axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Condition<Id> {
    pub id: Id,
    pub kind: ConditionKind,
    pub not: bool,
}

/// What a condition asks of its item's values. A OneOf's values are text,
/// or `null` for a blank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type")]
pub enum ConditionKind {
    Text { query: TextMatch },
    OneOf { values: Vec<Option<String>> },
    Compare { comparison: Comparison },
    Native { text: String },
}

/// A comparison with a scalar, or between two, both included.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Comparison {
    Equal { value: Scalar },
    NotEqual { value: Scalar },
    Less { value: Scalar },
    AtMost { value: Scalar },
    Greater { value: Scalar },
    AtLeast { value: Scalar },
    Between { low: Scalar, high: Scalar },
}

/// A number, or a date in milliseconds since the Unix epoch (UTC).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Scalar {
    Number {
        #[serde(with = "crate::num")]
        #[schemars(with = "f64")]
        value: f64,
    },
    Date {
        #[serde(with = "crate::num")]
        #[schemars(with = "f64")]
        value: f64,
    },
}

/// The whole-table grouping, naming base fields (`FieldId`, the column IDs
/// of a records table).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct Grouping<FieldId, RowId, ColId> {
    pub row_levels: Vec<Level<FieldId>>,
    pub col_levels: Vec<Level<FieldId>>,
    pub values: Vec<AggregateValue<FieldId>>,
    pub slicers: Vec<Condition<FieldId>>,
    pub row_axis: GroupedAxis<RowId>,
    pub col_axis: GroupedAxis<ColId>,
}

/// One level of an axis's grouping: a field with its Subtotal's place, or
/// `{ "values": true }` for the values pseudo-level.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum Level<FieldId> {
    Field { field: FieldId, subtotal: Placement },
    Values { values: True },
}

/// The value `true`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, JsonSchema)]
#[schemars(transform = only_true)]
pub struct True;

fn only_true(schema: &mut schemars::Schema) {
    *schema = schemars::json_schema!({ "const": true });
}

impl Serialize for True {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for True {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match bool::deserialize(deserializer)? {
            true => Ok(True),
            false => Err(serde::de::Error::custom("expected true")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    Before,
    After,
    Off,
}

/// An aggregate over a base field, or over nothing (`null`) for a count,
/// with the parameters a declared aggregate takes, if any.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AggregateValue<FieldId> {
    pub field: Option<FieldId>,
    pub aggregate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<String>>,
}

/// How one grouped axis is laid out, and which groups are open.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct GroupedAxis<Id> {
    pub grand_total: Placement,
    pub form: Form,
    pub expand: Expand<Id>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Form {
    Outline,
    Header,
}

/// The groups open: the first `depth` levels, except the Group Items
/// `except` names, which are the other way.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Expand<Id> {
    pub depth: Depth,
    pub except: Vec<Id>,
}

/// How many levels are open: a number, or `"all"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Depth {
    Levels(u32),
    All(All),
}

/// The word `"all"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum All {
    #[serde(rename = "all")]
    All,
}

impl<RowId: Clone, ColId: Clone> ViewSpec<RowId, ColId> {
    /// The View Spec by Keys, `ids` naming each item; whatever names an ID
    /// it does not know is left out (a sort term, a condition, a hidden
    /// item, a searched item, a level, a value, a slicer, an open group).
    pub fn keys(&self, ids: &impl IdMap<RowId, ColId>) -> core::ViewSpec {
        let row = |id: &RowId| ids.row_key(id);
        let col = |id: &ColId| ids.col_key(id);
        let mut spec = core::ViewSpec {
            rows: axis_keys(&self.rows, col, row),
            cols: axis_keys(&self.cols, row, col),
            grouping: None,
        };
        spec.grouping = self.grouping.as_ref().map(|g| {
            let mut grouping = core::Grouping::default();
            let levels = |levels: &[Level<ColId>]| {
                (levels.iter())
                    .filter_map(|l| match l {
                        Level::Field { field, subtotal } => Some(core::Level::Field {
                            field: col(field)?,
                            subtotal: (*subtotal).into(),
                        }),
                        Level::Values { .. } => Some(core::Level::Values),
                    })
                    .collect()
            };
            grouping.row_levels = levels(&g.row_levels);
            grouping.col_levels = levels(&g.col_levels);
            grouping.values = (g.values.iter())
                .filter_map(|v| {
                    let field = match &v.field {
                        Some(f) => Some(col(f)?),
                        None => None,
                    };
                    let params = v.params.clone().unwrap_or_default();
                    Some(core::AggregateValue::new(field, v.aggregate.clone()).with_params(params))
                })
                .collect();
            grouping.slicers = (g.slicers.iter())
                .filter_map(|c| condition_key(c, col))
                .collect();
            grouped_keys(&mut grouping.rows, &g.row_axis, row);
            grouped_keys(&mut grouping.cols, &g.col_axis, col);
            grouping
        });
        spec
    }

    /// The View Spec `spec` by IDs, `ids` naming each Key; whatever names a
    /// Key it does not know is left out, as [`ViewSpec::keys`] leaves out
    /// IDs, and so is a condition of a kind this document does not hold.
    pub fn from_keys(spec: &core::ViewSpec, ids: &impl IdMap<RowId, ColId>) -> Self {
        let row = |k: Key| ids.row_id(k);
        let col = |k: Key| ids.col_id(k);
        ViewSpec {
            rows: axis_ids(&spec.rows, col, row),
            cols: axis_ids(&spec.cols, row, col),
            grouping: spec.grouping.as_ref().map(|g| {
                let levels = |levels: &[core::Level]| {
                    (levels.iter())
                        .filter_map(|l| match l {
                            core::Level::Field { field, subtotal } => Some(Level::Field {
                                field: col(*field)?,
                                subtotal: (*subtotal).into(),
                            }),
                            core::Level::Values => Some(Level::Values { values: True }),
                            _ => None,
                        })
                        .collect()
                };
                Grouping {
                    row_levels: levels(&g.row_levels),
                    col_levels: levels(&g.col_levels),
                    values: (g.values.iter())
                        .filter_map(|v| {
                            let field = match v.field {
                                Some(f) => Some(col(f)?),
                                None => None,
                            };
                            let params = (!v.params.is_empty()).then(|| v.params.clone());
                            Some(AggregateValue {
                                field,
                                aggregate: v.aggregate.clone(),
                                params,
                            })
                        })
                        .collect(),
                    slicers: (g.slicers.iter())
                        .filter_map(|c| condition_id(c, col))
                        .collect(),
                    row_axis: grouped_ids(&g.rows, row),
                    col_axis: grouped_ids(&g.cols, col),
                }
            }),
        }
    }
}

fn axis_keys<CrossId, Id>(
    part: &AxisView<CrossId, Id>,
    cross: impl Fn(&CrossId) -> Option<Key> + Copy,
    own: impl Fn(&Id) -> Option<Key>,
) -> core::AxisView {
    let sort = (part.sort.iter())
        .filter_map(|c| {
            Some(core::SortColumn {
                key: cross(&c.id)?,
                direction: c.direction.into(),
            })
        })
        .collect();
    let quick = part.filter.quick.as_ref().map(|q| {
        let searches = match &q.searches {
            Searched::Shown(_) => core::Searched::Shown,
            Searched::Items(items) => {
                core::Searched::Keys(items.iter().filter_map(cross).collect())
            }
        };
        core::TextQuery::new(q.text.clone())
            .case_sensitive(q.case_sensitive)
            .regex(q.regex)
            .searching(searches)
    });
    let conditions: Vec<core::Condition> = (part.filter.conditions.iter())
        .filter_map(|c| condition_key(c, cross))
        .collect();
    core::AxisView {
        filter: core::FilterSpec::default()
            .with_quick(quick)
            .with_conditions(conditions.into()),
        sort: core::SortSpec::new(sort),
        hidden: part.hidden.iter().filter_map(own).collect(),
    }
}

fn axis_ids<CrossId, Id>(
    part: &core::AxisView,
    cross: impl Fn(Key) -> Option<CrossId> + Copy,
    own: impl Fn(Key) -> Option<Id>,
) -> AxisView<CrossId, Id> {
    let quick = part.filter.quick.as_ref().map(|q| TextQuery {
        text: q.text.clone(),
        case_sensitive: q.case_sensitive,
        regex: q.regex,
        searches: match &q.searches {
            core::Searched::Shown => Searched::Shown(Shown::Shown),
            core::Searched::Keys(keys) => {
                Searched::Items(keys.iter().filter_map(|&k| cross(k)).collect())
            }
        },
    });
    AxisView {
        filter: FilterSpec {
            quick,
            conditions: (part.filter.conditions.conditions().iter())
                .filter_map(|c| condition_id(c, cross))
                .collect(),
        },
        sort: (part.sort.columns().iter())
            .filter_map(|c| {
                Some(SortColumn {
                    id: cross(c.key)?,
                    direction: c.direction.into(),
                })
            })
            .collect(),
        hidden: part.hidden.iter().filter_map(|&k| own(k)).collect(),
    }
}

fn condition_key<Id>(
    c: &Condition<Id>,
    key: impl Fn(&Id) -> Option<Key>,
) -> Option<core::Condition> {
    let k = key(&c.id)?;
    let condition = match &c.kind {
        ConditionKind::Text { query } => core::Condition::text(
            k,
            core::TextQuery::new(query.text.clone())
                .case_sensitive(query.case_sensitive)
                .regex(query.regex),
        ),
        ConditionKind::OneOf { values } => core::Condition::one_of(
            k,
            values
                .iter()
                .map(|v| core::Value::of(v.as_deref()))
                .collect(),
        ),
        ConditionKind::Compare { comparison } => core::Condition::compare(k, (*comparison).into()),
        ConditionKind::Native { text } => core::Condition::native(k, text.clone()),
    };
    Some(condition.not(c.not))
}

fn condition_id<Id>(c: &core::Condition, id: impl Fn(Key) -> Option<Id>) -> Option<Condition<Id>> {
    let kind = match &c.kind {
        core::ConditionKind::Text(q) => ConditionKind::Text {
            query: TextMatch {
                text: q.text.clone(),
                case_sensitive: q.case_sensitive,
                regex: q.regex,
            },
        },
        core::ConditionKind::OneOf(values) => ConditionKind::OneOf {
            values: (values.iter())
                .map(|v| match v {
                    core::Value::Text(t) => Some(t.clone()),
                    core::Value::Blank => None,
                })
                .collect(),
        },
        core::ConditionKind::Compare(c) => ConditionKind::Compare {
            comparison: (*c).into(),
        },
        core::ConditionKind::Native(text) => ConditionKind::Native { text: text.clone() },
        _ => return None,
    };
    Some(Condition {
        id: id(c.key)?,
        kind,
        not: c.not,
    })
}

fn grouped_keys<Id>(
    axis: &mut core::GroupedAxis,
    from: &GroupedAxis<Id>,
    key: impl Fn(&Id) -> Option<Key>,
) {
    axis.grand_total = from.grand_total.into();
    axis.form = match from.form {
        Form::Outline => core::Form::Outline,
        Form::Header => core::Form::Header,
    };
    axis.expand = core::Expand {
        depth: match from.expand.depth {
            Depth::Levels(n) => core::Depth::Levels(n),
            Depth::All(_) => core::Depth::All,
        },
        except: from.expand.except.iter().filter_map(key).collect(),
    };
}

fn grouped_ids<Id>(axis: &core::GroupedAxis, id: impl Fn(Key) -> Option<Id>) -> GroupedAxis<Id> {
    GroupedAxis {
        grand_total: axis.grand_total.into(),
        form: match axis.form {
            core::Form::Outline => Form::Outline,
            core::Form::Header => Form::Header,
        },
        expand: Expand {
            depth: match axis.expand.depth {
                core::Depth::Levels(n) => Depth::Levels(n),
                core::Depth::All => Depth::All(All::All),
            },
            except: axis.expand.except.iter().filter_map(|&k| id(k)).collect(),
        },
    }
}

impl From<Direction> for SortDirection {
    fn from(d: Direction) -> Self {
        match d {
            Direction::Ascending => SortDirection::Ascending,
            Direction::Descending => SortDirection::Descending,
        }
    }
}

impl From<SortDirection> for Direction {
    fn from(d: SortDirection) -> Self {
        match d {
            SortDirection::Ascending => Direction::Ascending,
            SortDirection::Descending => Direction::Descending,
        }
    }
}

impl From<Placement> for core::Placement {
    fn from(p: Placement) -> Self {
        match p {
            Placement::Before => core::Placement::Before,
            Placement::After => core::Placement::After,
            Placement::Off => core::Placement::Off,
        }
    }
}

impl From<core::Placement> for Placement {
    fn from(p: core::Placement) -> Self {
        match p {
            core::Placement::Before => Placement::Before,
            core::Placement::After => Placement::After,
            core::Placement::Off => Placement::Off,
        }
    }
}

impl From<Scalar> for core::Scalar {
    /// A date's milliseconds are truncated to a whole number.
    fn from(s: Scalar) -> Self {
        match s {
            Scalar::Number { value } => core::Scalar::Number(value),
            Scalar::Date { value } => core::Scalar::Date(value as i64),
        }
    }
}

impl From<core::Scalar> for Scalar {
    fn from(s: core::Scalar) -> Self {
        match s {
            core::Scalar::Number(value) => Scalar::Number { value },
            core::Scalar::Date(ms) => Scalar::Date { value: ms as f64 },
        }
    }
}

impl From<Comparison> for core::Comparison {
    fn from(c: Comparison) -> Self {
        use core::Comparison as C;
        match c {
            Comparison::Equal { value } => C::Equal(value.into()),
            Comparison::NotEqual { value } => C::NotEqual(value.into()),
            Comparison::Less { value } => C::Less(value.into()),
            Comparison::AtMost { value } => C::AtMost(value.into()),
            Comparison::Greater { value } => C::Greater(value.into()),
            Comparison::AtLeast { value } => C::AtLeast(value.into()),
            Comparison::Between { low, high } => C::Between(low.into(), high.into()),
        }
    }
}

impl From<core::Comparison> for Comparison {
    fn from(c: core::Comparison) -> Self {
        use core::Comparison as C;
        match c {
            C::Equal(s) => Comparison::Equal { value: s.into() },
            C::NotEqual(s) => Comparison::NotEqual { value: s.into() },
            C::Less(s) => Comparison::Less { value: s.into() },
            C::AtMost(s) => Comparison::AtMost { value: s.into() },
            C::Greater(s) => Comparison::Greater { value: s.into() },
            C::AtLeast(s) => Comparison::AtLeast { value: s.into() },
            C::Between(low, high) => Comparison::Between {
                low: low.into(),
                high: high.into(),
            },
        }
    }
}
