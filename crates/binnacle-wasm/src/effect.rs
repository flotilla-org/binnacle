//! Effects as plain tagged objects for `serde-wasm-bindgen`: Keys as `u64`
//! (a `bigint` in JS), Positions and generations as `f64`, axes and
//! directions as strings. `web/src/effects.ts` holds the matching union.

use binnacle_core::{
    Axis, Condition, ConditionKind, Effect, FilterSpec, Key, Searched, SortColumn, SortDirection,
    SortSpec, Stamped, TextQuery, ViewEdit, ViewSpec,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WireAxis {
    Rows,
    Cols,
}

impl From<Axis> for WireAxis {
    fn from(axis: Axis) -> Self {
        match axis {
            Axis::Rows => WireAxis::Rows,
            Axis::Cols => WireAxis::Cols,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WireDirection {
    Ascending,
    Descending,
}

impl From<SortDirection> for WireDirection {
    fn from(direction: SortDirection) -> Self {
        match direction {
            SortDirection::Ascending => WireDirection::Ascending,
            SortDirection::Descending => WireDirection::Descending,
        }
    }
}

impl From<WireDirection> for SortDirection {
    fn from(direction: WireDirection) -> Self {
        match direction {
            WireDirection::Ascending => SortDirection::Ascending,
            WireDirection::Descending => SortDirection::Descending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireSortColumn {
    pub key: u64,
    pub direction: WireDirection,
}

pub fn sort_columns(spec: &SortSpec) -> Vec<WireSortColumn> {
    spec.columns()
        .iter()
        .map(|c| WireSortColumn {
            key: c.key.0,
            direction: c.direction.into(),
        })
        .collect()
}

/// A Text Query; `searches` is `null` for the shown items of the other axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireTextQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub regex: bool,
    pub searches: Option<Vec<u64>>,
}

impl From<&TextQuery> for WireTextQuery {
    fn from(q: &TextQuery) -> Self {
        WireTextQuery {
            text: q.text.clone(),
            case_sensitive: q.case_sensitive,
            regex: q.regex,
            searches: match &q.searches {
                Searched::Shown => None,
                Searched::Keys(keys) => Some(keys.iter().map(|k| k.0).collect()),
            },
        }
    }
}

impl From<WireTextQuery> for TextQuery {
    fn from(q: WireTextQuery) -> Self {
        let searches = match q.searches {
            None => Searched::Shown,
            Some(keys) => Searched::Keys(keys.into_iter().map(Key).collect()),
        };
        TextQuery::new(q.text)
            .case_sensitive(q.case_sensitive)
            .regex(q.regex)
            .searching(searches)
    }
}

/// What a condition asks of its item's values, tagged like the Effects.
/// `Other` is a kind this bridge does not carry yet; it never comes back.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WireConditionKind {
    Text {
        query: WireTextQuery,
    },
    #[serde(skip_deserializing)]
    Other,
}

/// One condition: its item's Key, its kind and `not`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireCondition {
    pub key: u64,
    pub kind: WireConditionKind,
    pub not: bool,
}

impl From<&Condition> for WireCondition {
    fn from(c: &Condition) -> Self {
        WireCondition {
            key: c.key.0,
            kind: match &c.kind {
                ConditionKind::Text(q) => WireConditionKind::Text { query: q.into() },
                _ => WireConditionKind::Other,
            },
            not: c.not,
        }
    }
}

impl WireCondition {
    /// The condition, or `None` for a kind this bridge does not carry.
    pub fn into_core(self) -> Option<Condition> {
        match self.kind {
            WireConditionKind::Text { query } => {
                Some(Condition::text(Key(self.key), query.into()).not(self.not))
            }
            WireConditionKind::Other => None,
        }
    }
}

/// One axis's filter: its quick filter, or `null`, and its conditions. A
/// filter coming back without conditions has none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WireFilter {
    #[serde(default)]
    pub quick: Option<WireTextQuery>,
    #[serde(default)]
    pub conditions: Vec<WireCondition>,
}

/// One axis's part of a View Spec. A spec coming back without a filter has
/// none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireAxisView {
    #[serde(default)]
    pub filter: WireFilter,
    pub sort: Vec<WireSortColumn>,
}

/// A View Spec both ways across the boundary: out in `ViewRequested` and the
/// Snapshot, back in `view_applied`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireViewSpec {
    pub rows: WireAxisView,
    pub cols: WireAxisView,
}

impl From<&ViewSpec> for WireViewSpec {
    fn from(spec: &ViewSpec) -> Self {
        let axis = |a: Axis| WireAxisView {
            filter: WireFilter {
                quick: spec.axis(a).filter.quick.as_ref().map(WireTextQuery::from),
                conditions: (spec.axis(a).filter.conditions.conditions().iter())
                    .map(WireCondition::from)
                    .collect(),
            },
            sort: sort_columns(&spec.axis(a).sort),
        };
        WireViewSpec {
            rows: axis(Axis::Rows),
            cols: axis(Axis::Cols),
        }
    }
}

impl From<WireViewSpec> for ViewSpec {
    fn from(spec: WireViewSpec) -> Self {
        let sort = |sort: Vec<WireSortColumn>| {
            SortSpec::new(
                sort.into_iter()
                    .map(|c| SortColumn {
                        key: Key(c.key),
                        direction: c.direction.into(),
                    })
                    .collect(),
            )
        };
        let filter = |f: WireFilter| {
            let conditions: Vec<Condition> = (f.conditions.into_iter())
                .filter_map(WireCondition::into_core)
                .collect();
            FilterSpec::default()
                .with_quick(f.quick.map(TextQuery::from))
                .with_conditions(conditions.into())
        };
        let mut view = ViewSpec::default();
        for (axis, part) in [(Axis::Rows, spec.rows), (Axis::Cols, spec.cols)] {
            view = view
                .with_sort(axis, sort(part.sort))
                .with_filter(axis, filter(part.filter));
        }
        view
    }
}

/// A View Edit, tagged like the Effects. `Other` is an edit this bridge does
/// not carry yet.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum WireViewEdit {
    CycleSort {
        axis: WireAxis,
        key: u64,
        multi: bool,
    },
    SetQuickFilter {
        axis: WireAxis,
        query: Option<WireTextQuery>,
    },
    Replace,
    ShownChanged {
        axis: WireAxis,
    },
    AddCondition {
        axis: WireAxis,
        condition: WireCondition,
    },
    RemoveCondition {
        axis: WireAxis,
        condition: WireCondition,
    },
    ReplaceCondition {
        axis: WireAxis,
        from: WireCondition,
        to: WireCondition,
    },
    Other,
}

impl From<ViewEdit> for WireViewEdit {
    fn from(edit: ViewEdit) -> Self {
        match edit {
            ViewEdit::CycleSort { axis, key, multi } => WireViewEdit::CycleSort {
                axis: axis.into(),
                key: key.0,
                multi,
            },
            ViewEdit::SetQuickFilter { axis, query } => WireViewEdit::SetQuickFilter {
                axis: axis.into(),
                query: query.as_ref().map(WireTextQuery::from),
            },
            ViewEdit::Replace => WireViewEdit::Replace,
            ViewEdit::ShownChanged { axis } => WireViewEdit::ShownChanged { axis: axis.into() },
            ViewEdit::AddCondition { axis, condition } => WireViewEdit::AddCondition {
                axis: axis.into(),
                condition: (&condition).into(),
            },
            ViewEdit::RemoveCondition { axis, condition } => WireViewEdit::RemoveCondition {
                axis: axis.into(),
                condition: (&condition).into(),
            },
            ViewEdit::ReplaceCondition { axis, from, to } => WireViewEdit::ReplaceCondition {
                axis: axis.into(),
                from: (&from).into(),
                to: (&to).into(),
            },
            _ => WireViewEdit::Other,
        }
    }
}

/// One Effect with the generation it was stamped with.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WireStamped {
    pub generation: f64,
    #[serde(flatten)]
    pub effect: WireEffect,
}

// Serialized one at a time, as the core's Effects are polled.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
pub enum WireEffect {
    TextTyped {
        text: String,
    },
    Activated {
        row: u64,
        col: u64,
    },
    EditRequested {
        row: u64,
        col: u64,
    },
    EditCancelled {
        row: u64,
        col: u64,
    },
    FocusChanged {
        row: u64,
        col: u64,
    },
    SelectionChanged,
    ViewRequested {
        spec: WireViewSpec,
        edit: WireViewEdit,
    },
    Locate {
        axis: WireAxis,
        key: u64,
    },
    SizeChanged {
        axis: WireAxis,
        key: u64,
        size: f32,
    },
    AutoSizeRequested {
        axis: WireAxis,
        key: u64,
    },
    BandSizeChanged {
        axis: WireAxis,
        thickness: f32,
    },
    BandAutoSizeRequested {
        axis: WireAxis,
    },
    MoveRequested {
        axis: WireAxis,
        keys: Vec<u64>,
        to: f64,
    },
}

impl From<Stamped> for WireStamped {
    fn from(stamped: Stamped) -> Self {
        WireStamped {
            generation: stamped.generation as f64,
            effect: stamped.effect.into(),
        }
    }
}

impl From<Effect> for WireEffect {
    fn from(effect: Effect) -> Self {
        let k = |key: Key| key.0;
        match effect {
            Effect::TextTyped { text } => WireEffect::TextTyped { text },
            Effect::Activated { row, col } => WireEffect::Activated {
                row: k(row),
                col: k(col),
            },
            Effect::EditRequested { row, col } => WireEffect::EditRequested {
                row: k(row),
                col: k(col),
            },
            Effect::EditCancelled { row, col } => WireEffect::EditCancelled {
                row: k(row),
                col: k(col),
            },
            Effect::FocusChanged { row, col } => WireEffect::FocusChanged {
                row: k(row),
                col: k(col),
            },
            Effect::SelectionChanged => WireEffect::SelectionChanged,
            Effect::ViewRequested { spec, edit } => WireEffect::ViewRequested {
                spec: (&spec).into(),
                edit: edit.into(),
            },
            Effect::Locate { axis, key } => WireEffect::Locate {
                axis: axis.into(),
                key: k(key),
            },
            Effect::SizeChanged { axis, key, size } => WireEffect::SizeChanged {
                axis: axis.into(),
                key: k(key),
                size,
            },
            Effect::AutoSizeRequested { axis, key } => WireEffect::AutoSizeRequested {
                axis: axis.into(),
                key: k(key),
            },
            Effect::BandSizeChanged { axis, thickness } => WireEffect::BandSizeChanged {
                axis: axis.into(),
                thickness,
            },
            Effect::BandAutoSizeRequested { axis } => {
                WireEffect::BandAutoSizeRequested { axis: axis.into() }
            }
            Effect::MoveRequested { axis, keys, to } => WireEffect::MoveRequested {
                axis: axis.into(),
                keys: keys.into_iter().map(k).collect(),
                to: to as f64,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn wire(effect: Effect) -> serde_json::Value {
        serde_json::to_value(WireStamped::from(Stamped {
            generation: 7,
            effect,
        }))
        .unwrap()
    }

    #[test]
    fn an_effect_is_tagged_with_its_type_and_generation() {
        assert_eq!(
            wire(Effect::FocusChanged {
                row: Key(3),
                col: Key(4)
            }),
            json!({ "generation": 7.0, "type": "FocusChanged", "row": 3, "col": 4 })
        );
        assert_eq!(
            wire(Effect::SelectionChanged),
            json!({ "generation": 7.0, "type": "SelectionChanged" })
        );
    }

    #[test]
    fn keys_keep_all_64_bits() {
        assert_eq!(
            wire(Effect::Locate {
                axis: Axis::Rows,
                key: Key(u64::MAX)
            }),
            json!({ "generation": 7.0, "type": "Locate", "axis": "rows", "key": u64::MAX })
        );
    }

    #[test]
    fn a_view_request_lists_each_axis_sort_and_the_edit() {
        let spec = ViewSpec::default().with_sort(
            Axis::Rows,
            SortSpec::new(vec![
                SortColumn::descending(Key(2)),
                SortColumn::ascending(Key(0)),
            ]),
        );
        let edit = ViewEdit::CycleSort {
            axis: Axis::Rows,
            key: Key(2),
            multi: true,
        };
        assert_eq!(
            wire(Effect::ViewRequested { spec, edit }),
            json!({ "generation": 7.0, "type": "ViewRequested",
                "spec": {
                    "rows": { "filter": { "quick": null, "conditions": [] }, "sort": [
                        { "key": 2, "direction": "descending" },
                        { "key": 0, "direction": "ascending" },
                    ] },
                    "cols": { "filter": { "quick": null, "conditions": [] }, "sort": [] },
                },
                "edit": { "type": "CycleSort", "axis": "rows", "key": 2, "multi": true },
            })
        );
    }

    #[test]
    fn a_quick_filter_and_its_edit_carry_the_text_query() {
        let query = TextQuery::new("Bér")
            .regex(true)
            .searching(Searched::Keys(vec![Key(4)]));
        let spec = ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_quick(Some(query.clone())),
        );
        let edit = ViewEdit::SetQuickFilter {
            axis: Axis::Rows,
            query: Some(query),
        };
        let quick =
            json!({ "text": "Bér", "caseSensitive": false, "regex": true, "searches": [4] });
        assert_eq!(
            wire(Effect::ViewRequested { spec, edit }),
            json!({ "generation": 7.0, "type": "ViewRequested",
                "spec": {
                    "rows": { "filter": { "quick": quick, "conditions": [] }, "sort": [] },
                    "cols": { "filter": { "quick": null, "conditions": [] }, "sort": [] },
                },
                "edit": { "type": "SetQuickFilter", "axis": "rows", "query": quick },
            })
        );
    }

    #[test]
    fn a_condition_and_its_edit_carry_the_key_kind_and_not() {
        let not_ber = Condition::text(Key(4), TextQuery::new("Bér")).not(true);
        let spec = ViewSpec::default().with_filter(
            Axis::Rows,
            FilterSpec::default().with_conditions(vec![not_ber.clone()].into()),
        );
        let edit = ViewEdit::AddCondition {
            axis: Axis::Rows,
            condition: not_ber,
        };
        let condition = json!({ "key": 4, "not": true, "kind": { "type": "Text", "query":
            { "text": "Bér", "caseSensitive": false, "regex": false, "searches": [4] } } });
        assert_eq!(
            wire(Effect::ViewRequested { spec, edit }),
            json!({ "generation": 7.0, "type": "ViewRequested",
                "spec": {
                    "rows": { "filter": { "quick": null, "conditions": [condition] }, "sort": [] },
                    "cols": { "filter": { "quick": null, "conditions": [] }, "sort": [] },
                },
                "edit": { "type": "AddCondition", "axis": "rows", "condition": condition },
            })
        );
    }

    #[test]
    fn a_condition_coming_back_searches_its_own_column() {
        let back: WireCondition = serde_json::from_value(json!({ "key": 3, "not": false,
            "kind": { "type": "Text", "query":
                { "text": "a", "caseSensitive": true, "regex": false, "searches": null } } }))
        .unwrap();
        assert_eq!(
            back.into_core(),
            Some(Condition::text(
                Key(3),
                TextQuery::new("a").case_sensitive(true)
            ))
        );
    }

    #[test]
    fn a_spec_without_a_filter_comes_back_unfiltered() {
        let back: WireViewSpec =
            serde_json::from_value(json!({ "rows": { "sort": [] }, "cols": { "sort": [] } }))
                .unwrap();
        assert_eq!(ViewSpec::from(back), ViewSpec::default());
    }

    #[test]
    fn a_view_spec_comes_back_as_it_went_out() {
        let quick = TextQuery::new("x").case_sensitive(true);
        let spec = ViewSpec::default()
            .with_filter(
                Axis::Rows,
                FilterSpec::default()
                    .with_quick(Some(quick))
                    .with_conditions(
                        vec![Condition::text(Key(5), TextQuery::new("^y").regex(true)).not(true)]
                            .into(),
                    ),
            )
            .with_sort(
                Axis::Rows,
                SortSpec::new(vec![SortColumn::descending(Key(2))]),
            )
            .with_sort(
                Axis::Cols,
                SortSpec::new(vec![SortColumn::ascending(Key(9))]),
            );
        let json = serde_json::to_value(WireViewSpec::from(&spec)).unwrap();
        let back: WireViewSpec = serde_json::from_value(json).unwrap();
        assert_eq!(ViewSpec::from(back), spec);
    }

    #[test]
    fn a_move_request_carries_keys_and_a_position() {
        assert_eq!(
            wire(Effect::MoveRequested {
                axis: Axis::Cols,
                keys: vec![Key(5), Key(9)],
                to: 2
            }),
            json!({ "generation": 7.0, "type": "MoveRequested", "axis": "cols", "keys": [5, 9], "to": 2.0 })
        );
    }

    #[test]
    fn sizes_and_band_effects_carry_their_numbers() {
        assert_eq!(
            wire(Effect::SizeChanged {
                axis: Axis::Cols,
                key: Key(1),
                size: 80.5
            }),
            json!({ "generation": 7.0, "type": "SizeChanged", "axis": "cols", "key": 1, "size": 80.5 })
        );
        assert_eq!(
            wire(Effect::BandSizeChanged {
                axis: Axis::Rows,
                thickness: 30.0
            }),
            json!({ "generation": 7.0, "type": "BandSizeChanged", "axis": "rows", "thickness": 30.0 })
        );
    }
}
