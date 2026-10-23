//! The `grouping` part of a View Spec and the outline of a Window item, as
//! plain objects for `serde-wasm-bindgen`, in the shapes `web/src/source.ts`
//! names with IDs: Keys as `u64` (a `bigint` in JS), names as strings.

use binnacle_core::{
    Aggregate, AggregateValue, Depth, Expand, Form, GroupOf, GroupedAxis, Grouping, ItemKind, Key,
    Level, OpenState, Outline, Placement, SetPosition, Value,
};
use serde::{Deserialize, Serialize};

use crate::effect::WireCondition;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WirePlacement {
    Before,
    After,
    Off,
}

impl From<Placement> for WirePlacement {
    fn from(p: Placement) -> Self {
        match p {
            Placement::Before => WirePlacement::Before,
            Placement::After => WirePlacement::After,
            Placement::Off => WirePlacement::Off,
        }
    }
}

impl From<WirePlacement> for Placement {
    fn from(p: WirePlacement) -> Self {
        match p {
            WirePlacement::Before => Placement::Before,
            WirePlacement::After => Placement::After,
            WirePlacement::Off => Placement::Off,
        }
    }
}

/// A level: a field with its Subtotal's placement, or `{ values: true }`
/// for the values pseudo-level. `Other` is a level this bridge does not
/// carry yet; it never comes back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireLevel {
    Field {
        field: u64,
        subtotal: WirePlacement,
    },
    Values {
        values: bool,
    },
    #[serde(skip_deserializing)]
    Other {},
}

impl From<&Level> for WireLevel {
    fn from(level: &Level) -> Self {
        match level {
            Level::Field { field, subtotal } => WireLevel::Field {
                field: field.0,
                subtotal: (*subtotal).into(),
            },
            Level::Values => WireLevel::Values { values: true },
            _ => WireLevel::Other {},
        }
    }
}

impl WireLevel {
    fn into_core(self) -> Option<Level> {
        match self {
            WireLevel::Field { field, subtotal } => Some(Level::Field {
                field: Key(field),
                subtotal: subtotal.into(),
            }),
            WireLevel::Values { .. } => Some(Level::Values),
            WireLevel::Other {} => None,
        }
    }
}

/// A value: an aggregate over a field, or over nothing (`null`) for a count,
/// with the parameters a declared aggregate takes (none if omitted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireAggregateValue {
    pub field: Option<u64>,
    pub aggregate: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
}

impl From<&AggregateValue> for WireAggregateValue {
    fn from(v: &AggregateValue) -> Self {
        WireAggregateValue {
            field: v.field.map(|k| k.0),
            aggregate: v.aggregate.clone(),
            params: v.params.clone(),
        }
    }
}

impl From<WireAggregateValue> for AggregateValue {
    fn from(v: WireAggregateValue) -> Self {
        AggregateValue::new(v.field.map(Key), v.aggregate).with_params(v.params)
    }
}

/// An aggregate the Table Source offers over a field: its name and the
/// names of the parameters it takes (none if omitted).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireAggregate {
    pub name: String,
    #[serde(default)]
    pub params: Vec<String>,
}

impl From<&Aggregate> for WireAggregate {
    fn from(a: &Aggregate) -> Self {
        WireAggregate {
            name: a.name.clone(),
            params: a.params.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireForm {
    Outline,
    Header,
}

impl From<Form> for WireForm {
    fn from(form: Form) -> Self {
        match form {
            Form::Outline => WireForm::Outline,
            Form::Header => WireForm::Header,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireAll {
    All,
}

/// How many levels are open by default: a number, or `"all"`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireDepth {
    Levels(u32),
    All(WireAll),
}

impl From<Depth> for WireDepth {
    fn from(depth: Depth) -> Self {
        match depth {
            Depth::Levels(n) => WireDepth::Levels(n),
            Depth::All => WireDepth::All(WireAll::All),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireExpand {
    pub depth: WireDepth,
    pub except: Vec<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireGroupedAxis {
    pub grand_total: WirePlacement,
    pub form: WireForm,
    pub expand: WireExpand,
}

impl From<&GroupedAxis> for WireGroupedAxis {
    fn from(a: &GroupedAxis) -> Self {
        WireGroupedAxis {
            grand_total: a.grand_total.into(),
            form: a.form.into(),
            expand: WireExpand {
                depth: a.expand.depth.into(),
                except: a.expand.except.iter().map(|k| k.0).collect(),
            },
        }
    }
}

impl WireGroupedAxis {
    fn into_core(self, mut axis: GroupedAxis) -> GroupedAxis {
        axis.grand_total = self.grand_total.into();
        axis.form = match self.form {
            WireForm::Outline => Form::Outline,
            WireForm::Header => Form::Header,
        };
        axis.expand = Expand {
            depth: match self.expand.depth {
                WireDepth::Levels(n) => Depth::Levels(n),
                WireDepth::All(_) => Depth::All,
            },
            except: self.expand.except.into_iter().map(Key).collect(),
        };
        axis
    }
}

/// The `grouping` part of a View Spec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireGrouping {
    pub row_levels: Vec<WireLevel>,
    pub col_levels: Vec<WireLevel>,
    pub values: Vec<WireAggregateValue>,
    pub slicers: Vec<WireCondition>,
    pub row_axis: WireGroupedAxis,
    pub col_axis: WireGroupedAxis,
}

impl From<&Grouping> for WireGrouping {
    fn from(g: &Grouping) -> Self {
        let levels = |levels: &[Level]| levels.iter().map(WireLevel::from).collect();
        WireGrouping {
            row_levels: levels(&g.row_levels),
            col_levels: levels(&g.col_levels),
            values: g.values.iter().map(WireAggregateValue::from).collect(),
            slicers: g.slicers.iter().map(WireCondition::from).collect(),
            row_axis: (&g.rows).into(),
            col_axis: (&g.cols).into(),
        }
    }
}

impl From<WireGrouping> for Grouping {
    fn from(w: WireGrouping) -> Self {
        let levels = |levels: Vec<WireLevel>| {
            (levels.into_iter())
                .filter_map(WireLevel::into_core)
                .collect()
        };
        let mut g = Grouping::default();
        g.row_levels = levels(w.row_levels);
        g.col_levels = levels(w.col_levels);
        g.values = w.values.into_iter().map(AggregateValue::from).collect();
        g.slicers = (w.slicers.into_iter())
            .filter_map(WireCondition::into_core)
            .collect();
        g.rows = w.row_axis.into_core(g.rows);
        g.cols = w.col_axis.into_core(g.cols);
        g
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WireItemKind {
    Leaf,
    Group,
    Subtotal,
    GrandTotal,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WireOpenState {
    Open,
    Closed,
    CantOpen,
}

/// The group a Group Item or Subtotal belongs to; `value` is `null` for a
/// blank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireGroupOf {
    pub field: u64,
    pub value: Option<String>,
}

/// An item's outline, both ways: in with a Window, out in the Snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireOutline {
    pub kind: WireItemKind,
    pub depth: u32,
    pub open: WireOpenState,
    pub ancestors: Vec<u64>,
    #[serde(default)]
    pub group: Option<WireGroupOf>,
    /// On a Group Item, how many items are shown under it.
    #[serde(default)]
    pub span: f64,
    /// The group of each ancestor, outermost first.
    #[serde(default)]
    pub path: Vec<WireGroupOf>,
    #[serde(default)]
    pub set: Option<WireSet>,
}

impl From<&GroupOf> for WireGroupOf {
    fn from(g: &GroupOf) -> Self {
        WireGroupOf {
            field: g.field.0,
            value: match &g.value {
                Value::Text(t) => Some(t.clone()),
                Value::Blank => None,
            },
        }
    }
}

impl From<WireGroupOf> for GroupOf {
    fn from(g: WireGroupOf) -> Self {
        GroupOf {
            field: Key(g.field),
            value: Value::of(g.value.as_deref()),
        }
    }
}

/// An item's place among its siblings: its index (0 first) and the set's
/// size, as numbers.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireSet {
    pub index: f64,
    pub size: f64,
}

impl From<&Outline> for WireItemKind {
    fn from(o: &Outline) -> Self {
        match o.kind {
            ItemKind::Leaf => WireItemKind::Leaf,
            ItemKind::Group => WireItemKind::Group,
            ItemKind::Subtotal => WireItemKind::Subtotal,
            ItemKind::GrandTotal => WireItemKind::GrandTotal,
        }
    }
}

impl From<&Outline> for WireOpenState {
    fn from(o: &Outline) -> Self {
        match o.open {
            OpenState::Open => WireOpenState::Open,
            OpenState::Closed => WireOpenState::Closed,
            OpenState::CantOpen => WireOpenState::CantOpen,
        }
    }
}

impl From<&Outline> for WireOutline {
    fn from(o: &Outline) -> Self {
        WireOutline {
            kind: o.into(),
            depth: o.depth,
            open: o.into(),
            ancestors: o.ancestors.iter().map(|k| k.0).collect(),
            group: o.group.as_ref().map(WireGroupOf::from),
            span: o.span as f64,
            path: o.path.iter().map(WireGroupOf::from).collect(),
            set: o.set.map(|s| WireSet {
                index: s.index as f64,
                size: s.size as f64,
            }),
        }
    }
}

impl From<WireOutline> for Outline {
    fn from(w: WireOutline) -> Self {
        let ancestors: Vec<Key> = w.ancestors.into_iter().map(Key).collect();
        let open = match w.open {
            WireOpenState::Open => OpenState::Open,
            WireOpenState::Closed => OpenState::Closed,
            WireOpenState::CantOpen => OpenState::CantOpen,
        };
        let mut outline = match w.group {
            Some(g) => Outline::group(ancestors, open, Key(g.field), Value::of(g.value.as_deref())),
            None => Outline::leaf(ancestors),
        }
        .with_span(w.span as u64)
        .with_path(w.path.into_iter().map(GroupOf::from).collect());
        outline.kind = match w.kind {
            WireItemKind::Leaf => ItemKind::Leaf,
            WireItemKind::Group => ItemKind::Group,
            WireItemKind::Subtotal => ItemKind::Subtotal,
            WireItemKind::GrandTotal => ItemKind::GrandTotal,
        };
        outline.open = open;
        outline.depth = w.depth;
        outline.set = w.set.map(|s| SetPosition {
            index: s.index as u64,
            size: s.size as u64,
        });
        outline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binnacle_core::{Axis, Condition, TextQuery};

    #[test]
    fn a_grouping_comes_back_as_it_went_out() {
        let mut g = Grouping::rows_by(Key(u64::MAX));
        g.row_levels.push(Level::Field {
            field: Key(3),
            subtotal: Placement::After,
        });
        g.col_levels.push(Level::Values);
        g.values.push(AggregateValue::new(Some(Key(4)), "sum"));
        g.values
            .push(AggregateValue::of(Key(5), "percentile").with_params(vec!["0.9".into()]));
        g.slicers
            .push(Condition::text(Key(2), TextQuery::new("x")).not(true));
        g.axis_mut(Axis::Rows).grand_total = Placement::Before;
        g.axis_mut(Axis::Cols).expand = Expand {
            depth: Depth::Levels(2),
            except: vec![Key(1 << 63)],
        };
        let json = serde_json::to_value(WireGrouping::from(&g)).unwrap();
        assert_eq!(json["rowLevels"][0]["subtotal"], "off");
        assert_eq!(json["colLevels"][0]["values"], true);
        assert_eq!(json["rowAxis"]["expand"]["depth"], "all");
        assert_eq!(json["colAxis"]["expand"]["depth"], 2);
        assert_eq!(json["colAxis"]["form"], "header");
        assert_eq!(json["values"][2]["params"][0], "0.9");
        let back: WireGrouping = serde_json::from_value(json).unwrap();
        assert_eq!(Grouping::from(back), g);
    }

    #[test]
    fn an_outline_comes_back_as_it_went_out() {
        let outlines = [
            Outline::leaf(vec![Key(1 << 63), Key(u64::MAX)]),
            Outline::group(vec![], OpenState::Closed, Key(7), Value::Blank),
            Outline::group(
                vec![Key(9)],
                OpenState::Open,
                Key(7),
                Value::Text("Ø".into()),
            )
            .with_span(12)
            .with_path(vec![GroupOf {
                field: Key(6),
                value: Value::Blank,
            }]),
            Outline::subtotal(vec![Key(9), Key(10)], Key(7), Value::Text("a".into())),
            Outline::grand_total(),
        ];
        for o in outlines {
            let json = serde_json::to_value(WireOutline::from(&o)).unwrap();
            let back: WireOutline = serde_json::from_value(json).unwrap();
            assert_eq!(Outline::from(back), o);
        }
        let json = serde_json::to_value(WireOutline::from(&Outline::leaf(vec![]))).unwrap();
        assert_eq!(json["kind"], "leaf");
        assert_eq!(json["open"], "cantOpen");
    }
}
