//! The grouping layer for a Table Source written in TypeScript over an
//! async aggregate backend (`web/src/grouped.ts`): the layer stays here,
//! sans-IO, and its batches and answers cross as plain objects. Keys are
//! `u64` (a `bigint`); stamps, request ids, counts and indices are numbers.

use binnacle_core::{Axis, ChangeNotice, ItemKind, Key, Level, Located, Value, ViewSpec};
use binnacle_group::{
    Answer, Batch, Body, GroupRow, GroupingLayer, Path, Request, RequestId, Stamp, Want,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::codes::position;
use crate::effect::{WireCondition, WireFilter, WireSortColumn, WireTextQuery, sort_columns};
use crate::grid::to_js;
use crate::grouping::{WireAggregateValue, WireOutline};

/// A figure as a backend gives it: a number, text, or `null` for a blank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireFigure {
    Number(f64),
    Text(String),
}

pub type Figure = Option<WireFigure>;

/// One level's field and the group's value there (`null` for a blank).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireStep {
    pub field: u64,
    pub value: Option<String>,
}

fn path_out(path: &Path) -> Vec<WireStep> {
    (path.iter())
        .map(|(field, value)| WireStep {
            field: field.0,
            value: text(value),
        })
        .collect()
}

fn path_in(path: Vec<WireStep>) -> Path {
    (path.into_iter())
        .map(|s| (Key(s.field), Value::of(s.value.as_deref())))
        .collect()
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Text(t) => Some(t.clone()),
        Value::Blank => None,
    }
}

/// What every request of a batch shares.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireScope {
    pub levels: Vec<u64>,
    pub values: Vec<WireAggregateValue>,
    pub slicers: Vec<WireCondition>,
    pub filter: WireFilter,
    pub hidden: Vec<u64>,
    pub sort: Vec<WireSortColumn>,
}

/// What a request asks for: `"groups"`, `"leaves"`, `"total"`, or
/// `{ locate: key }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WireWant {
    Groups,
    Leaves,
    Total,
    Locate(u64),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireRequest {
    pub id: f64,
    pub path: Vec<WireStep>,
    pub want: WireWant,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireBatch {
    pub stamp: f64,
    pub scope: WireScope,
    pub requests: Vec<WireRequest>,
}

impl From<&Batch> for WireBatch {
    fn from(batch: &Batch) -> Self {
        let scope = &batch.scope;
        let request = |r: &Request| {
            Some(WireRequest {
                id: r.id.0 as f64,
                path: path_out(&r.path),
                want: match r.want {
                    Want::Groups => WireWant::Groups,
                    Want::Leaves => WireWant::Leaves,
                    Want::Total => WireWant::Total,
                    Want::Locate(key) => WireWant::Locate(key.0),
                    // No pivot on the wire yet: request_view drops column
                    // levels.
                    Want::Columns => return None,
                },
                start: r.range.start as f64,
                end: r.range.end as f64,
            })
        };
        WireBatch {
            stamp: batch.stamp.0 as f64,
            scope: WireScope {
                levels: scope.levels.iter().map(|k| k.0).collect(),
                values: (scope.values.iter())
                    .map(|v| WireAggregateValue {
                        field: v.field.map(|k| k.0),
                        aggregate: v.aggregate.clone(),
                        params: v.params.clone(),
                    })
                    .collect(),
                slicers: scope.slicers.iter().map(WireCondition::from).collect(),
                filter: WireFilter {
                    quick: scope.filter.quick.as_ref().map(WireTextQuery::from),
                    conditions: (scope.filter.conditions.conditions().iter())
                        .map(WireCondition::from)
                        .collect(),
                },
                hidden: scope.hidden.iter().map(|k| k.0).collect(),
                sort: sort_columns(&scope.sort),
            },
            requests: batch.requests.iter().filter_map(request).collect(),
        }
    }
}

/// One group as a backend gives it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireGroupRow {
    pub value: Option<String>,
    pub children: f64,
    pub aggregates: Vec<Figure>,
}

/// What an answer holds, tagged by `type`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WireBody {
    Groups {
        total: f64,
        groups: Vec<WireGroupRow>,
    },
    Leaves {
        keys: Vec<u64>,
    },
    Located {
        key: u64,
        found: bool,
    },
    Total {
        aggregates: Vec<Figure>,
    },
}

/// A backend's answer: `id` is `null` for data the layer did not ask for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WireAnswer {
    pub stamp: f64,
    pub id: Option<f64>,
    #[serde(default)]
    pub path: Vec<WireStep>,
    #[serde(default)]
    pub start: f64,
    pub body: WireBody,
}

impl From<WireAnswer> for Answer<Figure> {
    fn from(w: WireAnswer) -> Self {
        let body = match w.body {
            WireBody::Groups { total, groups } => Body::Groups {
                total: position(total),
                groups: (groups.into_iter())
                    .map(|g| GroupRow {
                        value: Value::of(g.value.as_deref()),
                        children: position(g.children),
                        aggregates: g.aggregates,
                        cells: Vec::new(),
                    })
                    .collect(),
            },
            WireBody::Leaves { keys } => Body::Leaves {
                keys: keys.into_iter().map(Key).collect(),
            },
            WireBody::Located { key, found } => Body::Located {
                key: Key(key),
                found,
            },
            WireBody::Total { aggregates } => Body::Total {
                aggregates,
                cells: Vec::new(),
            },
        };
        Answer {
            stamp: Stamp(position(w.stamp)),
            id: w.id.map(|id| RequestId(position(id))),
            path: path_in(w.path),
            start: position(w.start),
            body,
        }
    }
}

/// The spec the layer applied, with the rows' Change Notices as
/// `[code, a, b]` (as `codes::notice` reads them).
#[derive(Serialize)]
struct WireViewed {
    applied: crate::effect::WireViewSpec,
    notices: Vec<[f64; 3]>,
}

fn notice(n: &ChangeNotice) -> [f64; 3] {
    let p = |n: u64| n as f64;
    match *n {
        ChangeNotice::Reordered => [0.0, f64::NAN, f64::NAN],
        ChangeNotice::Reset(c) => [1.0, p(c), f64::NAN],
        ChangeNotice::Inserted { at, n } => [2.0, p(at), p(n)],
        ChangeNotice::Removed { at, n } => [3.0, p(at), p(n)],
        ChangeNotice::ContentChanged(ref r) => [4.0, p(r.start), p(r.end)],
        ChangeNotice::Refiltered(c) => [5.0, p(c), f64::NAN],
        ChangeNotice::Collapsed { at, n } => [6.0, p(at), p(n)],
        ChangeNotice::Expanded { at, n } => [7.0, p(at), p(n)],
    }
}

#[derive(Serialize)]
struct WireItem {
    key: u64,
    outline: WireOutline,
}

/// A Summary Item for drawing: its kind, its group's path, how many
/// children it has and its figures, one per value.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSummary {
    kind: crate::grouping::WireItemKind,
    path: Vec<WireStep>,
    children: f64,
    aggregates: Vec<Figure>,
}

/// The grouping layer over a backend in TypeScript: the host gives it each
/// View Spec, sends each batch to the backend and hands back its answers
/// as they come, in any order, and reads the rows from it once grouped.
#[wasm_bindgen]
pub struct WebGroupingLayer {
    layer: GroupingLayer<Figure>,
}

#[wasm_bindgen]
impl WebGroupingLayer {
    /// A layer caching up to `cache` closed groups and Leaves besides
    /// those the latest Windows read; the default size for `NaN`.
    #[wasm_bindgen(constructor)]
    pub fn new(cache: f64) -> WebGroupingLayer {
        let layer = match cache.is_nan() {
            true => GroupingLayer::new(),
            false => GroupingLayer::with_cache(position(cache)),
        };
        WebGroupingLayer { layer }
    }

    /// Takes the View Spec asked for, Keys as the source names them.
    pub fn request_view(&mut self, spec: JsValue) -> Result<(), JsError> {
        let spec: crate::effect::WireViewSpec =
            serde_wasm_bindgen::from_value(spec).map_err(|e| JsError::new(&e.to_string()))?;
        let mut spec = ViewSpec::from(spec);
        // The wire has no pivot yet, so the applied spec has no column
        // levels and no values pseudo-level.
        if let Some(g) = spec.grouping.as_mut() {
            g.col_levels.clear();
            g.row_levels.retain(|l| *l != Level::Values);
        }
        self.layer.request_view(spec);
        Ok(())
    }

    /// Starts the rows afresh under the same spec, after a change of data.
    pub fn refresh(&mut self) {
        self.layer.refresh();
    }

    /// The user edited rows under the groups: asks every figure held again,
    /// pending until answered, every item staying where it is.
    pub fn refigure(&mut self) {
        self.layer.refigure();
    }

    /// The `ContentChanged` ranges for figures answered anew since
    /// [`WebGroupingLayer::refigure`], as start and end pairs.
    pub fn poll_changes(&mut self) -> Vec<f64> {
        let mut out = Vec::new();
        while let Some((_, change)) = self.layer.poll_change() {
            if let ChangeNotice::ContentChanged(range) = change {
                out.extend([range.start as f64, range.end as f64]);
            }
        }
        out
    }

    /// Whether the rows are grouped, and so the layer's.
    pub fn grouped(&self) -> bool {
        self.layer.grouped(Axis::Rows)
    }

    /// The next batch to send the backend, or `null`.
    pub fn poll_batch(&mut self) -> JsValue {
        match self.layer.poll_batch() {
            Some(batch) => to_js(&WireBatch::from(&batch)),
            None => JsValue::NULL,
        }
    }

    /// Takes one answer from the backend; one for a superseded batch is
    /// dropped.
    pub fn answer(&mut self, answer: JsValue) -> Result<(), JsError> {
        let answer: WireAnswer =
            serde_wasm_bindgen::from_value(answer).map_err(|e| JsError::new(&e.to_string()))?;
        self.layer.answer(answer.into());
        Ok(())
    }

    /// The spec applied since the last call with the rows' Change Notices,
    /// or `null`.
    pub fn poll_viewed(&mut self) -> JsValue {
        match self.layer.poll_viewed() {
            Some(viewed) => to_js(&WireViewed {
                applied: (&viewed.applied).into(),
                notices: viewed.notices.iter().map(|(_, n)| notice(n)).collect(),
            }),
            None => JsValue::NULL,
        }
    }

    /// How many rows are shown, or `NaN` while unknown.
    pub fn count(&self) -> f64 {
        self.layer.count(Axis::Rows).map_or(f64::NAN, |n| n as f64)
    }

    /// The items at Positions `start..end` up to the first not yet
    /// answered, each `{ key, outline }`; the next batch asks for the rest.
    pub fn window(&mut self, start: f64, end: f64) -> JsValue {
        let items = self
            .layer
            .window(Axis::Rows, position(start)..position(end));
        let items: Vec<WireItem> = (items.iter())
            .map(|(key, outline)| WireItem {
                key: key.0,
                outline: outline.into(),
            })
            .collect();
        to_js(&items)
    }

    /// Where `key` sits: `[0, position]`, `[1, NaN]` if gone, or `[]`
    /// while the backend is asked.
    pub fn locate(&mut self, key: u64) -> Vec<f64> {
        match self.layer.locate(Axis::Rows, Key(key)) {
            Some(Located::Found(p)) => vec![0.0, p as f64],
            Some(Located::Gone { .. }) => vec![1.0, f64::NAN],
            None => Vec::new(),
        }
    }

    /// The Summary Item with `key`, or `null` if the layer does not hold it.
    pub fn summary(&self, key: u64) -> JsValue {
        let Some(s) = self.layer.summary(Key(key)) else {
            return JsValue::NULL;
        };
        use crate::grouping::WireItemKind;
        to_js(&WireSummary {
            kind: match s.kind {
                ItemKind::Leaf => WireItemKind::Leaf,
                ItemKind::Group => WireItemKind::Group,
                ItemKind::Subtotal => WireItemKind::Subtotal,
                ItemKind::GrandTotal => WireItemKind::GrandTotal,
            },
            path: path_out(s.path),
            children: s.children as f64,
            aggregates: s.aggregates.to_vec(),
        })
    }

    /// Whether state on the item with Key `key` of axis `a` still fits the
    /// levels: a Summary Item's path fits them; anything else always does.
    pub fn fits(&self, a: u8, key: u64) -> bool {
        self.layer.fits(crate::codes::axis(a), Key(key))
    }

    /// How many groups and Leaves the layer holds.
    pub fn held(&self) -> f64 {
        self.layer.held() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binnacle_core::{AggregateValue, Grouping, SortColumn, SortSpec};

    #[test]
    fn a_batch_goes_out_with_its_scope_and_requests() {
        let mut layer = GroupingLayer::<Figure>::new();
        let mut grouping = Grouping::rows_by(Key(3));
        grouping.values = vec![AggregateValue::of(Key(4), "sum")];
        let spec = ViewSpec::default()
            .with_sort(
                Axis::Rows,
                SortSpec::new(vec![SortColumn::descending(Key(4))]),
            )
            .with_grouping(Some(grouping));
        layer.request_view(spec);
        let batch = WireBatch::from(&layer.poll_batch().unwrap());
        let json = serde_json::to_value(&batch).unwrap();
        assert_eq!(json["scope"]["levels"][0], 3);
        assert_eq!(json["scope"]["values"][0]["aggregate"], "sum");
        assert_eq!(json["scope"]["sort"][0]["direction"], "descending");
        assert_eq!(json["requests"][0]["want"], "groups");
        assert_eq!(json["requests"][0]["path"], serde_json::json!([]));
        let locate = serde_json::to_value(WireWant::Locate(9)).unwrap();
        assert_eq!(locate, serde_json::json!({ "locate": 9 }));
    }

    #[test]
    fn answers_come_in_as_plain_objects() {
        let json = serde_json::json!({
            "stamp": 2,
            "id": null,
            "path": [{ "field": 3, "value": null }],
            "start": 4,
            "body": { "type": "groups", "total": 9, "groups": [
                { "value": "Bergen", "children": 2, "aggregates": [1.5, "x", null] }
            ] }
        });
        let answer: Answer<Figure> = serde_json::from_value::<WireAnswer>(json).unwrap().into();
        assert_eq!(answer.stamp, Stamp(2));
        assert_eq!(answer.id, None);
        assert_eq!(answer.path, vec![(Key(3), Value::Blank)]);
        assert_eq!(answer.start, 4);
        let Body::Groups { total, groups } = answer.body else {
            panic!("groups");
        };
        assert_eq!(total, 9);
        assert_eq!(groups[0].value, Value::Text("Bergen".into()));
        assert_eq!(
            groups[0].aggregates,
            vec![
                Some(WireFigure::Number(1.5)),
                Some(WireFigure::Text("x".into())),
                None
            ]
        );
        let leaves = serde_json::json!({
            "stamp": 1, "id": 7, "body": { "type": "leaves", "keys": [5, 6] }
        });
        let answer: Answer<Figure> = serde_json::from_value::<WireAnswer>(leaves).unwrap().into();
        assert_eq!(answer.id, Some(RequestId(7)));
        assert_eq!(
            answer.body,
            Body::Leaves {
                keys: vec![Key(5), Key(6)]
            }
        );
    }
}
