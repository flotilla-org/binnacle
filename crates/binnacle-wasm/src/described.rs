//! The Described Table source for `fromDescribed` in `web/src/described.ts`:
//! the source stays here, sans-IO. TypeScript fetches the watch it asks
//! for and feeds the bytes back; cells cross as Typed Values, as
//! `TypedValue` in `web/src/format.ts` writes them. Rows and columns are
//! named by the source's Keys (`bigint`).

use binnacle_core::{
    Axis, Blanks, Compared, Distinct, DistinctQuery, FilterSpec, FindDirection, FindMatch,
    FindQuery, Key, Located, TextQuery, TextSupport, Totals, ViewSpec,
};
use binnacle_described::{
    Category, Column, Decoder, DescribedSource, Encoding, HostRequest, Message, Received, Scope,
};
use binnacle_format::Value;
use binnacle_layout::IdMap;
use binnacle_memory::PumpSource;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::codes::{MAX_COUNT, axis, encode_notices, position};
use crate::effect::{WireCondition, WireFilter, WireFindMatch, WireFindQuery, WireViewSpec};
use crate::format::JsZone;
use crate::grid::to_js;

/// What a watch asks for: `GET {target}`, the path and query apart too.
#[derive(Serialize)]
struct WireRequest {
    table: String,
    path: String,
    query: String,
    target: String,
    limit: f64,
}

/// Each axis's unfiltered count, where known.
#[derive(Serialize)]
struct WireTotals {
    #[serde(skip_serializing_if = "Option::is_none")]
    rows: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cols: Option<f64>,
}

impl From<Totals> for WireTotals {
    fn from(t: Totals) -> Self {
        WireTotals {
            rows: t.rows.map(|n| n as f64),
            cols: t.cols.map(|n| n as f64),
        }
    }
}

/// A View Spec the source applies unprompted, after a `table` message.
#[derive(Serialize)]
struct WireView {
    spec: WireViewSpec,
    totals: WireTotals,
}

/// What a `feed` or `ended` did, but for the Change Notices, which cross
/// as a `Float64Array`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireFed {
    view: Option<WireView>,
    columns_changed: bool,
    resyncs: u32,
    /// How many messages did not decode.
    errors: u32,
    /// The text of the latest that did not.
    latest_error: Option<String>,
}

#[derive(Serialize)]
struct WireReference {
    table: String,
    fields: Vec<String>,
}

#[derive(Serialize)]
struct WireCategory {
    value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

/// One column, as `DescribedColumn` in TypeScript.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireColumn {
    key: u64,
    name: String,
    title: Option<String>,
    header: String,
    #[serde(rename = "type")]
    field_type: String,
    value_type: &'static str,
    unit: Option<String>,
    nullable: bool,
    roles: Vec<String>,
    role: &'static str,
    categories: Option<Vec<WireCategory>>,
    reference: Option<WireReference>,
    nested: bool,
    mismatches: f64,
}

#[derive(Serialize)]
struct WireCount {
    value: f64,
    estimate: bool,
}

#[derive(Serialize)]
struct WireStatus {
    described: bool,
    synced: bool,
    partial: bool,
    count: Option<WireCount>,
    incarnation: Option<String>,
    sequence: Option<f64>,
    rows: f64,
    mismatches: f64,
}

#[derive(Serialize)]
struct WireRegex {
    regex: bool,
}

fn text_support(t: Option<TextSupport>) -> Option<WireRegex> {
    t.map(|t| WireRegex { regex: t.regex })
}

/// Support, as `Support` in `web/src/source.ts` has it: no grouping, pivots
/// or Subtotals on the web yet.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireSupport {
    sort: bool,
    /// `false` for none.
    text: serde_json::Value,
    one_of: bool,
    compare: Option<&'static str>,
    native: bool,
    group: bool,
    pivot: bool,
    subtotals: WirePlacements,
}

#[derive(Serialize)]
struct WirePlacements {
    before: bool,
    after: bool,
}

/// A checklist's question, as `DistinctQuery` in TypeScript, by Keys.
#[derive(Deserialize)]
struct WireDistinctQuery {
    search: String,
    limit: f64,
    narrow: WireFilter,
}

#[derive(Serialize)]
struct WireDistinctValue {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<f64>,
}

/// A checklist's answer: `blanks` a count, `true` if uncounted, or absent.
#[derive(Serialize)]
struct WireDistinct {
    values: Vec<WireDistinctValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blanks: Option<serde_json::Value>,
    partial: bool,
    narrowed: bool,
}

impl From<Distinct> for WireDistinct {
    fn from(d: Distinct) -> Self {
        WireDistinct {
            values: (d.values.into_iter())
                .map(|v| WireDistinctValue {
                    text: v.text,
                    count: v.count.map(|n| n as f64),
                })
                .collect(),
            blanks: d.blanks.map(|b| match b {
                Blanks::Counted(n) => serde_json::json!(n as f64),
                Blanks::Uncounted => serde_json::json!(true),
            }),
            partial: d.partial,
            narrowed: d.narrowed,
        }
    }
}

/// The View Spec a source applied and its totals; the Change Notices cross
/// beside them as quads.
#[derive(Serialize)]
struct WireViewed {
    applied: WireViewSpec,
    totals: WireTotals,
}

#[derive(Serialize)]
struct WireFindStep {
    row: u64,
    col: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<f64>,
    wrapped: bool,
}

#[derive(Serialize)]
struct WireFindCount {
    total: f64,
    partial: bool,
    hidden: f64,
}

fn js_error(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn from_js<T: for<'de> Deserialize<'de>>(value: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(js_error)
}

/// The largest integer a JavaScript number holds exactly.
const MAX_SAFE: u64 = (1 << 53) - 1;

/// JSON as JavaScript reads it: objects, not maps, and every number a
/// number but an integer past 2^53, which is a `bigint`, so it crosses
/// exactly.
fn json_to_js(json: &serde_json::Value) -> JsValue {
    use serde_json::Value as Json;
    match json {
        Json::Null => JsValue::NULL,
        Json::Bool(b) => JsValue::from_bool(*b),
        Json::Number(n) => match (n.as_i64(), n.as_u64()) {
            (Some(i), _) if i.unsigned_abs() > MAX_SAFE => JsValue::from(i),
            (_, Some(u)) if u > MAX_SAFE => JsValue::from(u),
            _ => JsValue::from_f64(n.as_f64().unwrap_or(f64::NAN)),
        },
        Json::String(s) => JsValue::from_str(s),
        Json::Array(items) => items
            .iter()
            .map(json_to_js)
            .collect::<js_sys::Array>()
            .into(),
        Json::Object(fields) => {
            let o = js_sys::Object::new();
            for (k, v) in fields {
                set(&o, k, &json_to_js(v));
            }
            o.into()
        }
    }
}

fn set(object: &js_sys::Object, key: &str, value: &JsValue) {
    let _ = js_sys::Reflect::set(object, &JsValue::from_str(key), value);
}

/// A Typed Value as TypeScript writes it: the inverse of `decode` in
/// `format.rs`.
fn typed(value: &Value) -> JsValue {
    let object = |pairs: &[(&str, JsValue)]| {
        let o = js_sys::Object::new();
        for (k, v) in pairs {
            set(&o, k, v);
        }
        JsValue::from(o)
    };
    match value {
        Value::Null => JsValue::NULL,
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Int(i) => JsValue::from(*i),
        Value::UInt(u) => JsValue::from(*u),
        Value::Number(x) => JsValue::from_f64(*x),
        Value::Text(s) => JsValue::from_str(s),
        Value::Bytes(b) => js_sys::Uint8Array::from(&b[..]).into(),
        Value::DateTime(at) => object(&[("instant", JsValue::from(at.unix_nanos()))]),
        Value::Category { value, label } => {
            let mut pairs = vec![("category", JsValue::from_str(value))];
            if let Some(label) = label {
                pairs.push(("label", JsValue::from_str(label)));
            }
            object(&pairs)
        }
        Value::Nested(json) => object(&[("nested", json_to_js(json))]),
        Value::Invalid { raw, expected } => object(&[
            ("invalid", JsValue::from_str(raw)),
            ("expected", JsValue::from_str(expected.name())),
        ]),
    }
}

fn column(table: &str, c: &Column) -> WireColumn {
    WireColumn {
        key: c.key().0,
        name: c.name().to_owned(),
        title: c.title().map(String::from),
        header: c.header().to_owned(),
        field_type: c.field_type().name().to_owned(),
        value_type: c.value_type().name(),
        unit: c.unit().map(String::from),
        nullable: c.nullable(),
        roles: c.roles().to_vec(),
        role: c.role().name(),
        categories: c.categories().map(|cs| {
            (cs.iter())
                .map(|Category { value, label }| WireCategory {
                    value: value.clone(),
                    label: label.clone(),
                })
                .collect()
        }),
        reference: c.reference().map(|r| WireReference {
            table: r.resource.clone().unwrap_or_else(|| table.to_owned()),
            fields: vec![r.field.clone()],
        }),
        nested: c.nested(),
        mismatches: c.mismatches() as f64,
    }
}

fn located(l: Located) -> Vec<f64> {
    match l {
        Located::Found(p) => vec![0.0, p as f64],
        Located::Gone { suggest } => vec![1.0, suggest.map_or(f64::NAN, |p| p as f64)],
    }
}

/// A Described Table source: it asks for its watch ([`WebDescribed::poll_request`]),
/// takes the bytes of each response ([`WebDescribed::open`],
/// [`WebDescribed::feed`], [`WebDescribed::ended`]) and answers the Grid's
/// questions as an In-Memory Table Source over the rows.
#[wasm_bindgen]
pub struct WebDescribed {
    source: DescribedSource,
    decoder: Option<Decoder>,
}

#[wasm_bindgen]
impl WebDescribed {
    /// The rows of `table` in `namespace`, narrowed by each of `wheres`, its
    /// dates' Stable Text in `zone` (seconds east of UTC at a time in ms;
    /// the runtime's local zone when absent). An empty namespace throws.
    #[wasm_bindgen(constructor)]
    pub fn new(
        table: String,
        namespace: String,
        wheres: Vec<String>,
        zone: Option<js_sys::Function>,
    ) -> Result<WebDescribed, JsError> {
        if namespace.is_empty() {
            return Err(JsError::new("a described table needs a namespace"));
        }
        let scope = (wheres.into_iter()).fold(Scope::new(namespace), Scope::with_where);
        let source = DescribedSource::new(table, scope).zone(JsZone(zone));
        Ok(WebDescribed {
            source,
            decoder: None,
        })
    }

    /// The watch to open next, `{ table, path, query, target, limit }`, or
    /// `undefined`.
    #[wasm_bindgen(js_name = pollRequest)]
    pub fn poll_request(&mut self) -> JsValue {
        match self.source.poll_request() {
            Some(HostRequest::OpenWatch(w)) => to_js(&WireRequest {
                table: w.table.clone(),
                path: w.path(),
                query: w.query(),
                target: w.target(),
                limit: w.limit as f64,
            }),
            _ => JsValue::UNDEFINED,
        }
    }

    /// A new response body begins, of `content_type`; an unknown type throws.
    pub fn open(&mut self, content_type: &str) -> Result<(), JsError> {
        let encoding = Encoding::from_content_type(content_type)
            .ok_or_else(|| JsError::new(&format!("no rows stream is {content_type}")))?;
        self.decoder = Some(Decoder::new(encoding));
        Ok(())
    }

    /// The next bytes of the body: every whole message is taken. A message
    /// that does not decode is skipped and its error listed.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<JsValue, JsError> {
        let decoder = (self.decoder.as_mut()).ok_or_else(|| JsError::new("open a body first"))?;
        decoder.feed(bytes);
        let mut fed = Fed::default();
        while let Some(message) = decoder.next() {
            fed.take(&mut self.source, message);
        }
        Ok(fed.finish(&mut self.source, Vec::new()))
    }

    /// The body ended: the last message left without its line break is
    /// taken, then the source asks to open the watch again and resyncs.
    pub fn ended(&mut self) -> JsValue {
        let mut fed = Fed::default();
        if let Some(message) = self.decoder.as_mut().and_then(Decoder::finish) {
            fed.take(&mut self.source, message);
        }
        self.decoder = None;
        let notices = match self.source.ended() {
            Received::Applied(notices) => notices,
            _ => Vec::new(),
        };
        fed.finish(&mut self.source, notices)
    }

    /// The Typed Values where `row_keys` and `col_keys` cross, one array per
    /// row; `undefined` for a row or column the source does not hold.
    pub fn cells(&self, row_keys: &[u64], col_keys: &[u64]) -> js_sys::Array {
        (row_keys.iter())
            .map(|&r| {
                (col_keys.iter())
                    .map(|&c| {
                        self.source
                            .cell(Key(r), Key(c))
                            .map_or(JsValue::UNDEFINED, typed)
                    })
                    .collect::<js_sys::Array>()
            })
            .collect()
    }

    /// Each axis's count before any filter: `{ rows, cols }`.
    pub fn totals(&self) -> JsValue {
        to_js(&WireTotals::from(self.source.totals()))
    }

    /// Every column, in order.
    pub fn columns(&self) -> JsValue {
        let table = self.source.table();
        let columns: Vec<WireColumn> = (self.source.columns().iter())
            .map(|c| column(table, c))
            .collect();
        to_js(&columns)
    }

    /// Property `name` of column `col`'s field as sent, or `undefined`.
    pub fn property(&self, col: u64, name: &str) -> JsValue {
        (self.source.property(Key(col), name)).map_or(JsValue::UNDEFINED, json_to_js)
    }

    /// Row `row`'s primary-key tuple as a JSON array, or `undefined`.
    #[wasm_bindgen(js_name = rowTuple)]
    pub fn row_tuple(&self, row: u64) -> JsValue {
        (self.source.row_id(Key(row))).map_or(JsValue::UNDEFINED, |id| json_to_js(id.value()))
    }

    pub fn status(&self) -> JsValue {
        let s = self.source.status();
        to_js(&WireStatus {
            described: s.described,
            synced: s.synced,
            partial: s.partial,
            count: s.count.map(|(n, estimate)| WireCount {
                value: n as f64,
                estimate,
            }),
            incarnation: s.incarnation.clone(),
            sequence: s.sequence.map(|n| n as f64),
            rows: self.source.row_count() as f64,
            mismatches: self.source.mismatches() as f64,
        })
    }

    pub fn count(&self, a: u8) -> f64 {
        self.source.count(axis(a)) as f64
    }

    /// The Keys at Positions `start..end` of axis `a`, clipped to the count.
    pub fn window(&mut self, a: u8, start: f64, end: f64) -> Vec<u64> {
        let a = axis(a);
        let end = position(end).min(self.source.count(a));
        let start = position(start).min(end);
        (self.source.window(a, start..end).keys.into_iter())
            .map(|k| k.0)
            .collect()
    }

    /// How axis `a` may be viewed by item `key` of the other axis.
    pub fn support(&mut self, a: u8, key: u64) -> JsValue {
        let s = self.source.support(axis(a), Key(key));
        to_js(&WireSupport {
            sort: s.sort,
            text: match s.text {
                Some(t) => serde_json::json!({ "regex": t.regex }),
                None => serde_json::json!(false),
            },
            one_of: s.one_of,
            compare: s.compare.map(|c| match c {
                Compared::Numbers => "numbers",
                Compared::Dates => "dates",
            }),
            native: s.native,
            group: false,
            pivot: false,
            subtotals: WirePlacements {
                before: false,
                after: false,
            },
        })
    }

    /// Axis `a`'s quick filter, `{ regex }`, or `null` for none.
    #[wasm_bindgen(js_name = quickFilter)]
    pub fn quick_filter(&mut self, a: u8) -> JsValue {
        to_js(&text_support(self.source.quick_filter(axis(a))))
    }

    /// Whether axis `a` takes hidden items.
    pub fn hides(&mut self, a: u8) -> bool {
        self.source.hide_support(axis(a))
    }

    /// Find, `{ regex }`, or `null` for none.
    #[wasm_bindgen(js_name = findSupport)]
    pub fn find_support(&mut self) -> JsValue {
        to_js(&text_support(self.source.find_support()))
    }

    /// Presents the rows as `spec` (a `WireViewSpec`) asks, any grouping
    /// dropped: `{ applied, totals, notices }`, the notices as quads.
    pub fn view(&mut self, spec: JsValue) -> Result<JsValue, JsError> {
        let mut spec: WireViewSpec = from_js(spec)?;
        spec.grouping = None;
        let viewed = self.source.view(&ViewSpec::from(spec));
        let out = to_js(&WireViewed {
            applied: (&viewed.applied).into(),
            totals: viewed.totals.into(),
        });
        let notices = encode_notices(&viewed.notices);
        set(
            out.unchecked_ref(),
            "notices",
            &js_sys::Float64Array::from(&notices[..]),
        );
        Ok(out)
    }

    /// The distinct values of item `key` across axis `a`, for a checklist:
    /// `query` is `{ search, limit, narrow }`, `narrow` a `WireFilter`.
    pub fn distinct(&mut self, a: u8, key: u64, query: JsValue) -> Result<JsValue, JsError> {
        let q: WireDistinctQuery = from_js(query)?;
        let conditions: Vec<_> = (q.narrow.conditions.into_iter())
            .filter_map(WireCondition::into_core)
            .collect();
        let narrow = FilterSpec::default()
            .with_quick(q.narrow.quick.map(TextQuery::from))
            .with_conditions(conditions.into());
        let limit = position(q.limit.clamp(0.0, MAX_COUNT as f64).floor()) as usize;
        let query = DistinctQuery::new(limit).search(q.search).narrow(narrow);
        Ok(match self.source.distinct(axis(a), Key(key), &query) {
            Some(d) => to_js(&WireDistinct::from(d)),
            None => JsValue::NULL,
        })
    }

    /// Where item `key` of axis `a` sits: `[code, position]`, as
    /// `WebMemory::locate` answers.
    pub fn locate(&mut self, a: u8, key: u64) -> Vec<f64> {
        located(self.source.locate(axis(a), Key(key)))
    }

    /// The match after `from` (a `WireFindMatch`, or `null`) in `query`'s
    /// order, or before it for `"previous"`: `{ row, col, index, wrapped }`,
    /// or `null`.
    #[wasm_bindgen(js_name = findNext)]
    pub fn find_next(
        &mut self,
        query: JsValue,
        from: JsValue,
        direction: &str,
    ) -> Result<JsValue, JsError> {
        let query = FindQuery::from(from_js::<WireFindQuery>(query)?);
        let from = match from.is_null() || from.is_undefined() {
            true => None,
            false => Some(FindMatch::from(from_js::<WireFindMatch>(from)?)),
        };
        let direction = match direction {
            "previous" => FindDirection::Previous,
            _ => FindDirection::Next,
        };
        Ok(match self.source.find_next(&query, from, direction) {
            Some(step) => to_js(&WireFindStep {
                row: step.at.row.0,
                col: step.at.col.0,
                index: step.index.map(|i| i as f64),
                wrapped: step.wrapped,
            }),
            None => JsValue::NULL,
        })
    }

    /// Whether each cell where `row_keys` and `col_keys` cross matches
    /// `query`, row by row, end to end.
    #[wasm_bindgen(js_name = findMarks)]
    pub fn find_marks(
        &mut self,
        row_keys: &[u64],
        col_keys: &[u64],
        query: JsValue,
    ) -> Result<Vec<u8>, JsError> {
        let query = FindQuery::from(from_js::<WireFindQuery>(query)?);
        let keys = |ks: &[u64]| ks.iter().map(|&k| Key(k)).collect::<Vec<_>>();
        let marks = self
            .source
            .find_marks(&keys(row_keys), &keys(col_keys), &query);
        Ok(marks.into_iter().map(u8::from).collect())
    }

    /// How many cells match `query`: `{ total, partial, hidden }`.
    #[wasm_bindgen(js_name = findCount)]
    pub fn find_count(&mut self, query: JsValue) -> Result<JsValue, JsError> {
        let query = FindQuery::from(from_js::<WireFindQuery>(query)?);
        let c = self.source.find_count(&query);
        Ok(to_js(&WireFindCount {
            total: c.total as f64,
            partial: c.partial,
            hidden: c.hidden as f64,
        }))
    }

    /// The generic default Layout as JSON: Nested columns hidden, key and
    /// state columns pinned leading.
    #[wasm_bindgen(js_name = defaultLayout)]
    pub fn default_layout(&self) -> Result<JsValue, JsError> {
        let json = self.source.presentation(None, None).to_json();
        let text = String::from_utf8(json).map_err(js_error)?;
        js_sys::JSON::parse(&text).map_err(|_| JsError::new("a Layout that is not JSON"))
    }
}

/// What the messages of one `feed` or `ended` did.
#[derive(Default)]
struct Fed {
    notices: Vec<(Axis, binnacle_core::ChangeNotice)>,
    columns_changed: bool,
    resyncs: u32,
    errors: u32,
    latest_error: Option<String>,
}

impl Fed {
    fn take(
        &mut self,
        source: &mut DescribedSource,
        message: Result<Message, binnacle_described::DecodeError>,
    ) {
        let message = match message {
            Ok(m) => m,
            Err(e) => {
                self.errors = self.errors.saturating_add(1);
                self.latest_error = Some(e.to_string());
                return;
            }
        };
        match &message {
            Message::Table(_) => self.columns_changed = true,
            Message::Resync => self.resyncs += 1,
            _ => {}
        }
        if let Received::Applied(notices) = source.receive(message) {
            self.notices.extend(notices);
        }
    }

    fn finish(
        self,
        source: &mut DescribedSource,
        more: Vec<(Axis, binnacle_core::ChangeNotice)>,
    ) -> JsValue {
        let view = source.poll_view().map(|(spec, totals)| WireView {
            spec: (&spec).into(),
            totals: totals.into(),
        });
        let out = to_js(&WireFed {
            view,
            columns_changed: self.columns_changed,
            resyncs: self.resyncs,
            errors: self.errors,
            latest_error: self.latest_error,
        });
        let mut notices = self.notices;
        notices.extend(more);
        let quads = encode_notices(&notices);
        set(
            out.unchecked_ref(),
            "notices",
            &js_sys::Float64Array::from(&quads[..]),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_distinct_answer_goes_out_as_typescript_reads_it() {
        let d = Distinct {
            values: vec![binnacle_core::DistinctValue {
                text: "a".into(),
                count: Some(2),
            }],
            blanks: Some(Blanks::Uncounted),
            partial: false,
            narrowed: true,
        };
        let json = serde_json::to_value(WireDistinct::from(d)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "values": [{ "text": "a", "count": 2.0 }],
                "blanks": true,
                "partial": false,
                "narrowed": true,
            })
        );
    }

    #[test]
    fn a_distinct_query_reads_its_narrowing_filter() {
        let q: WireDistinctQuery = serde_json::from_value(serde_json::json!({
            "search": "x",
            "limit": 50,
            "narrow": { "quick": null, "conditions": [] },
        }))
        .unwrap();
        assert_eq!((q.search.as_str(), q.limit), ("x", 50.0));
        assert!(q.narrow.quick.is_none());
    }
}
