//! The In-Memory Table Source for `fromRows` in `web/src/rows.ts`. The rows
//! stay in JS: TypeScript extracts each sorted column's keys and stages them
//! here, then sorts. Items are named by Logical Index (`u32`) both ways.
//!
//! Grouped rows go through the grouping layer, answered from the rows in
//! view: TypeScript stages each field the grouping reads (its levels and
//! values) as text and numbers, sorts the rows by the levels' fields first,
//! then groups ([`WebMemory::group`]).

use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use binnacle_core::{
    Axis, ChangeNotice, Depth, Expand, FilterSpec, Grouping, ItemKind, Key, Located, MoveRefusal,
    Outline, Position, SortColumn, SortSpec, VALUES_FIELD, Value, ViewSpec,
};
use binnacle_group::{
    AggregateBackend, Answer, Batch, Figure, GroupingLayer, MemoryAggregates, settle,
};
use binnacle_memory::{InMemoryTableSource, MovePolicy, SortKeys};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::codes::{self, axis, position};
use crate::effect::WireCondition;
use crate::grid::to_js;
use crate::grouping::{WireGroupOf, WireGrouping, WireItemKind, WireOutline};

/// One sort column's keys, staged until [`WebMemory::sort`].
enum Staged {
    Numbers(Vec<Option<f64>>),
    /// Every string's UTF-8 bytes end to end; `ends[i]` is where row `i`'s end.
    Strings {
        bytes: Vec<u8>,
        ends: Vec<u32>,
        nulls: Vec<u8>,
    },
}

impl Staged {
    fn keys(&self) -> SortKeys<'_> {
        match self {
            Staged::Numbers(v) => SortKeys::F64(v.clone()),
            Staged::Strings { bytes, ends, nulls } => SortKeys::Bytes(
                ends.iter()
                    .enumerate()
                    .map(|(i, &end)| {
                        let start = if i == 0 { 0 } else { ends[i - 1] as usize };
                        (nulls[i] == 0).then(|| Cow::Borrowed(&bytes[start..end as usize]))
                    })
                    .collect(),
            ),
        }
    }
}

/// Strings staged as for [`WebMemory::stage_strings`], as Values.
fn values(bytes: &[u8], ends: &[u32], nulls: &[u8]) -> Vec<Value> {
    (ends.iter().enumerate())
        .map(|(i, &end)| {
            let start = if i == 0 { 0 } else { ends[i - 1] as usize };
            let text = String::from_utf8_lossy(&bytes[start..end as usize]);
            Value::of((nulls[i] == 0).then_some(&*text))
        })
        .collect()
}

/// A field's staged values, one per row by Logical Index: its text and its
/// number.
#[derive(Default)]
struct Field {
    values: Vec<Value>,
    numbers: Vec<Option<f64>>,
}

/// The grouping layer's backend: the rows in view, grouped once per stamp.
struct Backend<'a> {
    table: &'a InMemoryTableSource,
    fields: &'a HashMap<Key, Field>,
    aggregates: &'a mut Option<MemoryAggregates>,
}

impl AggregateBackend<Figure> for Backend<'_> {
    fn answer(&mut self, batch: &Batch) -> Vec<Answer<Figure>> {
        if self.aggregates.as_ref().map(|a| a.stamp()) != Some(batch.stamp) {
            let keys = self
                .table
                .window(Axis::Rows, 0..self.table.count(Axis::Rows));
            let fields = self.fields;
            let item = |i: usize| keys[i].0 as usize;
            *self.aggregates = Some(MemoryAggregates::new(
                batch,
                &keys,
                |field, i| {
                    let values = fields.get(&field).map(|f| &f.values);
                    values
                        .and_then(|v| v.get(item(i)).cloned())
                        .unwrap_or(Value::Blank)
                },
                |field, i| {
                    let numbers = fields.get(&field).map(|f| &f.numbers);
                    numbers.and_then(|n| n.get(item(i)).copied().flatten())
                },
            ));
        }
        self.aggregates
            .as_mut()
            .map_or(Vec::new(), |a| a.answer(batch))
    }
}

/// One item of a grouped Window: its layer Key, its Logical Index if a
/// Leaf, its outline (a Summary Item's field is its column index, or
/// [`VALUES_FIELD`] for the values pseudo-level), a Summary Item's
/// aggregates, one per value (`null` for a blank), with how many children
/// its group has, and the index of the value it stands for, if one does (a
/// group of the values pseudo-level, a Subtotal of one value, a Grand Total
/// split by value).
#[derive(Debug, Serialize)]
struct GroupedItem {
    key: u64,
    item: Option<u32>,
    outline: WireOutline,
    aggregates: Option<Vec<Option<f64>>>,
    children: Option<f64>,
    value: Option<u32>,
}

/// An item of the grouped rows as find walks them, every group open: its
/// layer Key, its Logical Index if a Leaf, whether a closed group hides it
/// and the Group Item Keys above it; a Group Item's level column index and
/// label (`null` for a blank).
#[derive(Serialize)]
struct FoundItem {
    key: u64,
    item: Option<u32>,
    hidden: bool,
    ancestors: Vec<u64>,
    field: Option<u64>,
    label: Option<String>,
}

/// A Summary Item as [`WebMemory::summary`] reads it: its kind, its group's
/// innermost field and value (none for the Grand Total), its aggregates,
/// how many children its group has, and the value it stands for, as a
/// [`GroupedItem`]'s.
#[derive(Serialize)]
struct SummaryOf {
    kind: WireItemKind,
    group: Option<WireGroupOf>,
    aggregates: Vec<Option<f64>>,
    children: f64,
    value: Option<u32>,
}

#[wasm_bindgen]
pub struct WebMemory {
    table: InMemoryTableSource,
    staged: Vec<Staged>,
    layer: GroupingLayer<Figure>,
    aggregates: Option<MemoryAggregates>,
    /// The values of each field the grouping reads, by column index.
    fields: HashMap<Key, Field>,
    staged_fields: HashMap<Key, Field>,
    /// The spec the layer was last asked for, and each axis's Change
    /// Notices and `expand` as it applied them (the rows, then the
    /// columns).
    spec: ViewSpec,
    notices: [Vec<ChangeNotice>; 2],
    applied: [Expand; 2],
    /// The sort terms the layer last applied, by axis: on a pivot it leaves
    /// out a term naming an item the data lacks.
    sorted: [Vec<u64>; 2],
    policy: MovePolicy,
}

#[wasm_bindgen]
impl WebMemory {
    #[wasm_bindgen(constructor)]
    pub fn new(rows: u32, cols: u32) -> WebMemory {
        WebMemory {
            table: InMemoryTableSource::new(rows as usize, cols as usize),
            staged: Vec::new(),
            layer: GroupingLayer::new(),
            aggregates: None,
            fields: HashMap::new(),
            staged_fields: HashMap::new(),
            spec: ViewSpec::default(),
            notices: Default::default(),
            applied: Default::default(),
            sorted: [Vec::new(), Vec::new()],
            policy: MovePolicy::Bake,
        }
    }

    /// The items on axis `a`: Group Items included while the rows are
    /// grouped.
    pub fn count(&self, a: u8) -> f64 {
        match self.layer.count(axis(a)) {
            Some(n) => n as f64,
            None => self.table.count(axis(a)) as f64,
        }
    }

    /// The Logical Indices at Positions `start..end`, clipped to the count.
    pub fn window(&self, a: u8, start: f64, end: f64) -> Vec<u32> {
        self.table
            .window(axis(a), position(start)..position(end))
            .into_iter()
            .map(|k| k.0 as u32)
            .collect()
    }

    /// Where Logical Index `item` sits: `[code, position]` as
    /// [`codes::located`] reads them.
    pub fn locate(&mut self, a: u8, item: u32) -> Vec<f64> {
        let located = match self.layer.grouped(axis(a)) {
            true => self.locate_grouped(axis(a), Key(item as u64)),
            false => self.table.locate(axis(a), Key(item as u64)),
        };
        match located {
            Located::Found(p) => vec![0.0, p as f64],
            Located::Gone { suggest } => vec![1.0, suggest.map_or(f64::NAN, |p| p as f64)],
        }
    }

    /// Where the Summary Item with the layer's Key `key` sits on axis `a`,
    /// as [`WebMemory::locate`] answers.
    pub fn locate_summary(&mut self, a: u8, key: u64) -> Vec<f64> {
        match self.locate_grouped(axis(a), Key(key)) {
            Located::Found(p) => vec![0.0, p as f64],
            Located::Gone { .. } => vec![1.0, f64::NAN],
        }
    }

    /// Stages column `field`'s values, one per row: as UTF-8 strings, as
    /// [`WebMemory::stage_strings`] takes them, which group the rows and
    /// which counts count, and as numbers (`number_nulls[i] != 0` for none),
    /// which sums and averages take.
    pub fn stage_field(
        &mut self,
        field: u32,
        bytes: Vec<u8>,
        ends: Vec<u32>,
        nulls: Vec<u8>,
        numbers: Vec<f64>,
        number_nulls: Vec<u8>,
    ) {
        let numbers = (numbers.iter().zip(&number_nulls))
            .map(|(&n, &null)| (null == 0).then_some(n))
            .collect();
        let values = values(&bytes, &ends, &nulls);
        self.staged_fields
            .insert(Key(field as u64), Field { values, numbers });
    }

    /// Groups the rows as `grouping` says (a `Grouping` naming columns by
    /// index), with the fields it reads staged, over the rows in view as
    /// they are sorted and filtered now (its slicers among the filters),
    /// grouping them again if they were already. `sort` and `directions` are
    /// the user's sort columns (indices, or a pivot column's layer Key) and
    /// directions (0 ascending), which order the groups. `having` lists the
    /// rows' comparisons on a field a value aggregates (conditions naming
    /// columns by index) or on a pivot column (by its layer Key): they leave
    /// out the groups whose figure fails them, not the rows. `col_sort` and
    /// `col_directions` order a pivot's columns as `sort` orders the groups,
    /// each a column level's field (by value) or a row's layer Key (by its
    /// figure, `GRAND_TOTAL` for the Grand Total row's), and `col_having`
    /// lists comparisons on a row's figure (naming it so) that leave out the
    /// pivot columns failing them. Returns the new count for the
    /// `Refiltered`.
    #[allow(clippy::too_many_arguments)]
    pub fn group(
        &mut self,
        grouping: JsValue,
        sort: &[u64],
        directions: &[u8],
        having: JsValue,
        col_sort: &[u64],
        col_directions: &[u8],
        col_having: JsValue,
    ) -> Result<f64, JsValue> {
        let grouping: WireGrouping = serde_wasm_bindgen::from_value(grouping)?;
        let conditions = |having: JsValue| -> Result<FilterSpec, JsValue> {
            let having: Vec<WireCondition> = serde_wasm_bindgen::from_value(having)?;
            let having = having.into_iter().filter_map(WireCondition::into_core);
            Ok(FilterSpec::default().with_conditions(having.collect::<Vec<_>>().into()))
        };
        let spec = |keys: &mut dyn Iterator<Item = u64>, directions: &[u8]| {
            let columns = (keys.zip(directions))
                .map(|(c, &d)| match d {
                    0 => SortColumn::ascending(Key(c)),
                    _ => SortColumn::descending(Key(c)),
                })
                .collect();
            SortSpec::new(columns)
        };
        let sort = spec(&mut sort.iter().copied(), directions);
        let col_sort = spec(&mut col_sort.iter().copied(), col_directions);
        let (rows, cols) = (conditions(having)?, conditions(col_having)?);
        Ok(self.group_by(grouping.into(), sort, rows, col_sort, cols))
    }

    /// The Keys of the sort terms on axis `a` as the grouping last applied
    /// them: a pivot leaves out a term naming an item its data lacks (a
    /// pivot column gone), which applies again when the item returns.
    pub fn applied_sort(&self, a: u8) -> Vec<u64> {
        self.sorted[axis(a).index()].clone()
    }

    /// Whether state on the item with layer Key `key` of axis `a` (a width,
    /// a pin, a sort term naming it) still fits the levels: a Summary
    /// Item's or pivot column's path fits them; anything else always does.
    pub fn fits(&self, a: u8, key: u64) -> bool {
        self.layer.fits(axis(a), Key(key))
    }

    /// Ungroups the rows; returns the count for the `Refiltered`.
    pub fn ungroup(&mut self) -> f64 {
        self.fields.clear();
        self.layer.request_view(ViewSpec::default());
        self.settle();
        self.count(0)
    }

    /// Groups the rows again after they were sorted, filtered or hidden;
    /// returns the count for the `Refiltered`.
    pub fn regroup(&mut self) -> f64 {
        if self.layer.grouped(Axis::Rows) {
            self.layer.refresh();
            self.settle();
        }
        self.count(0)
    }

    /// Opens the grouped rows' groups as `depth` (negative for all) and
    /// `except` (Group Item Keys, the other way) say, over the rows as they
    /// are grouped. Returns the Change Notice the rows need as its code and
    /// two Positions, as [`codes::notice`] reads them: `Collapsed` or
    /// `Expanded` for one group, `Refiltered` for more, none (empty) if no
    /// group shown changed.
    pub fn expand(&mut self, depth: f64, except: &[u64]) -> Vec<f64> {
        self.expand_on(0, depth, except)
    }

    /// [`WebMemory::expand`] on axis `a`: a pivot's columns open and close
    /// the same way.
    pub fn expand_on(&mut self, a: u8, depth: f64, except: &[u64]) -> Vec<f64> {
        let axis = axis(a);
        let Some(grouping) = self.spec.grouping.as_mut() else {
            return Vec::new();
        };
        grouping.axis_mut(axis).expand = Expand {
            depth: match depth < 0.0 {
                true => Depth::All,
                false => Depth::Levels(depth as u32),
            },
            except: except.iter().copied().map(Key).collect(),
        };
        self.layer.request_view(self.spec.clone());
        self.settle();
        let notices = std::mem::take(&mut self.notices[axis.index()]);
        match notices.as_slice() {
            [] => Vec::new(),
            [ChangeNotice::Collapsed { at, n }] => vec![6.0, *at as f64, *n as f64],
            [ChangeNotice::Expanded { at, n }] => vec![7.0, *at as f64, *n as f64],
            _ => vec![5.0, self.count(a), f64::NAN],
        }
    }

    /// The Group Item Keys of the rows' `expand.except` as last applied:
    /// those whose paths no longer fit the levels are pruned.
    pub fn except(&self) -> Vec<u64> {
        self.except_on(0)
    }

    /// [`WebMemory::except`] on axis `a`.
    pub fn except_on(&self, a: u8) -> Vec<u64> {
        let applied = &self.applied[axis(a).index()];
        applied.except.iter().map(|k| k.0).collect()
    }

    /// Whether the rows are grouped.
    pub fn grouped(&self) -> bool {
        self.layer.grouped(Axis::Rows)
    }

    /// Whether the table is a pivot: its columns come from the data too.
    pub fn pivot(&self) -> bool {
        self.layer.pivot()
    }

    /// The figure of a pivot's cell where the row and the column with the
    /// layer's Keys `row` and `col` cross, `undefined` for a blank or a
    /// Key the layer does not know.
    pub fn cell(&self, row: u64, col: u64) -> Option<f64> {
        self.layer.cell(Key(row), Key(col)).and_then(|f| f.number())
    }

    /// The items of grouped axis `a` at Positions `start..end`, clipped to
    /// the count: an array of objects, each Leaf by its Logical Index and
    /// every item with its outline.
    pub fn grouped_window(&mut self, a: u8, start: f64, end: f64) -> JsValue {
        to_js(&self.window_items(axis(a), position(start)..position(end)))
    }

    /// Every item of the grouped rows in order as if every group were
    /// open, Summary Items included, for find to walk: an array of objects.
    pub fn grouped_items(&mut self) -> JsValue {
        self.settle();
        let Some(aggregates) = self.aggregates.as_ref() else {
            return to_js(&Vec::<FoundItem>::new());
        };
        let items: Vec<FoundItem> = (aggregates.items(&mut self.layer).into_iter())
            .map(|i| FoundItem {
                key: i.key.0,
                item: (i.kind == ItemKind::Leaf).then_some(i.key.0 as u32),
                hidden: i.hidden,
                ancestors: i.ancestors.iter().map(|k| k.0).collect(),
                field: (i.kind == ItemKind::Group)
                    .then_some(i.group.map(|(f, _)| f.0))
                    .flatten(),
                label: match (i.kind, i.group) {
                    (ItemKind::Group, Some((_, Value::Text(t)))) => Some(t.clone()),
                    _ => None,
                },
            })
            .collect();
        to_js(&items)
    }

    /// The fields the grouping reads were staged again after the user
    /// edited rows: works out every group's aggregates again, keeping the
    /// groups and their Leaves where they are until the rows group again.
    /// Returns the `ContentChanged` ranges the rows need, as start and end
    /// pairs.
    pub fn refigure(&mut self) -> Vec<f64> {
        if !self.layer.grouped(Axis::Rows) {
            return Vec::new();
        }
        self.fields = std::mem::take(&mut self.staged_fields);
        if let Some(aggregates) = self.aggregates.as_mut() {
            let keys = (self.table).window(Axis::Rows, 0..self.table.count(Axis::Rows));
            let fields = &self.fields;
            let item = |i: usize| keys[i].0 as usize;
            aggregates.refigure(
                &keys,
                |field, i| {
                    let values = fields.get(&field).map(|f| &f.values);
                    values
                        .and_then(|v| v.get(item(i)).cloned())
                        .unwrap_or(Value::Blank)
                },
                |field, i| {
                    let numbers = fields.get(&field).map(|f| &f.numbers);
                    numbers.and_then(|n| n.get(item(i)).copied().flatten())
                },
            );
        }
        self.layer.refigure();
        self.settle();
        let mut out = Vec::new();
        while let Some((_, change)) = self.layer.poll_change() {
            if let ChangeNotice::ContentChanged(range) = change {
                out.extend([range.start as f64, range.end as f64]);
            }
        }
        out
    }

    /// Moves the grouped Leaves with these Logical Indices to before
    /// Position `to`, within their group: the order shown is kept and the
    /// rows group again. Returns 0 if they moved, else why not: 1 the drop
    /// lies outside their group, 2 a Summary Item (by its layer Key) is
    /// among them.
    pub fn move_grouped(&mut self, keys: &[u64], to: f64) -> u8 {
        let keys: Vec<Key> = keys.iter().copied().map(Key).collect();
        let Some(aggregates) = self.aggregates.as_ref() else {
            return 1;
        };
        let table = &mut self.table;
        let placed =
            aggregates.place_move(
                &self.layer,
                Axis::Rows,
                &keys,
                position(to),
                |k| match table.locate(Axis::Rows, k) {
                    Located::Found(p) => Some(p),
                    Located::Gone { .. } => None,
                },
            );
        let to = match placed {
            Ok(to) => to,
            Err(MoveRefusal::Summary) => return 2,
            Err(_) => return 1,
        };
        // The rows are sorted by the levels: the order shown is baked.
        self.table.set_move_policy(MovePolicy::Bake);
        self.table.move_items(Axis::Rows, &keys, to);
        self.table.set_move_policy(self.policy);
        self.layer.refresh();
        self.settle();
        0
    }

    /// The Summary Item with the layer's Key `key` as the rows are grouped
    /// now, `undefined` if the layer does not know it: in header form a
    /// group has no Position, but its span still names it by this Key.
    pub fn summary(&self, key: u64) -> JsValue {
        let Some(s) = self.layer.summary(Key(key)) else {
            return JsValue::UNDEFINED;
        };
        to_js(&SummaryOf {
            kind: match s.kind {
                ItemKind::Leaf => WireItemKind::Leaf,
                ItemKind::Group => WireItemKind::Group,
                ItemKind::Subtotal => WireItemKind::Subtotal,
                ItemKind::GrandTotal => WireItemKind::GrandTotal,
            },
            group: s.path.last().map(|(field, value)| WireGroupOf {
                field: field.0,
                value: match value {
                    Value::Text(t) => Some(t.clone()),
                    Value::Blank => None,
                },
            }),
            aggregates: s.aggregates.iter().map(|f| f.number()).collect(),
            children: s.children as f64,
            value: s
                .path
                .last()
                .and_then(|(field, value)| self.value_index(*field, value)),
        })
    }

    /// Stages the next sort item's keys as numbers, one per base item of
    /// the axis to sort; `nulls[i] != 0` makes item `i` null.
    pub fn stage_numbers(&mut self, values: &[f64], nulls: &[u8]) {
        let keys = values
            .iter()
            .zip(nulls)
            .map(|(&v, &null)| (null == 0).then_some(v))
            .collect();
        self.staged.push(Staged::Numbers(keys));
    }

    /// Stages the next sort item's keys as UTF-8 strings, which sort by
    /// code point: item `i`'s bytes end at `ends[i]`.
    pub fn stage_strings(&mut self, bytes: Vec<u8>, ends: Vec<u32>, nulls: Vec<u8>) {
        self.staged.push(Staged::Strings { bytes, ends, nulls });
    }

    /// Sorts axis `a` by the Sort Spec of Logical Indices of the other
    /// axis's items and directions (see [`codes::sort_spec`]), one staged
    /// item each, in order. Returns whether the items shown moved: the axis
    /// then needs a `Reordered` notice.
    pub fn sort(&mut self, a: u8, items: &[u64], directions: &[u8]) -> bool {
        let staged = std::mem::take(&mut self.staged);
        let keys = staged.iter().map(Staged::keys).collect();
        let spec = codes::sort_spec(items, directions);
        self.table.resort(axis(a), &spec, keys).is_some()
    }

    /// Keeps the base items on axis `a` with `keep[i] != 0`; returns the
    /// new count for the `Refiltered` the axis needs.
    pub fn filter(&mut self, a: u8, keep: &[u8]) -> f64 {
        let keep = keep.iter().map(|&k| k != 0).collect();
        self.table.apply_filter(axis(a), keep);
        self.table.count(axis(a)) as f64
    }

    /// Keeps every item on axis `a` again; returns the count for the
    /// `Refiltered`.
    pub fn clear_filter(&mut self, a: u8) -> f64 {
        self.table.clear_filter(axis(a));
        self.table.count(axis(a)) as f64
    }

    /// Base item `item` on axis `a` changed in the data itself and the
    /// filter now keeps it (`keep != 0`) or not: it comes or goes at once.
    /// Returns whether it did, so the axis needs a `Refiltered`.
    pub fn refilter(&mut self, a: u8, item: u32, keep: u8) -> bool {
        (self.table)
            .refilter_item(axis(a), item as usize, keep != 0)
            .is_some()
    }

    /// The user edited base item `item` on axis `a` and the filter now keeps
    /// it or not: an item in view stays, unmatched while the filter drops
    /// it. Returns whether an item out of view came in, so the axis needs a
    /// `Refiltered`.
    pub fn defer(&mut self, a: u8, item: u32, keep: u8) -> bool {
        (self.table)
            .defer_item(axis(a), item as usize, keep != 0)
            .is_some()
    }

    /// The data changed and the filter now keeps each base item on axis `a`
    /// as `keep` says: an item whose flag changed comes or goes at once, the
    /// rest stay as they are. Returns whether any came or went, so the axis
    /// needs a `Refiltered`.
    pub fn refilter_all(&mut self, a: u8, keep: &[u8]) -> bool {
        let keep: Vec<bool> = keep.iter().map(|&k| k != 0).collect();
        self.table.refilter_items(axis(a), &keep).is_some()
    }

    /// For each item on axis `a` with these Logical Indices, 1 if it is
    /// unmatched: in view though the filter drops it.
    pub fn unmatched(&mut self, a: u8, items: &[u32]) -> Vec<u8> {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        let flags = self.table.unmatched(axis(a), &keys);
        let grouped = axis(a) == Axis::Rows && self.layer.grouped(Axis::Rows);
        let aggregates = self.aggregates.as_ref().filter(|_| grouped);
        let regrouped = |key: Key| {
            aggregates.is_some_and(|aggregates| {
                aggregates.regrouped(key, |field| {
                    let values = self.fields.get(&field).map(|f| &f.values);
                    let value = values.and_then(|v| v.get(key.0 as usize));
                    value.cloned().unwrap_or(Value::Blank)
                })
            })
        };
        (flags.into_iter().zip(keys))
            .map(|(flag, key)| u8::from(flag || regrouped(key)))
            .collect()
    }

    /// Whether any item on axis `a` is unmatched.
    pub fn has_unmatched(&self, a: u8) -> bool {
        self.table.has_unmatched(axis(a))
    }

    /// Hides the base items on axis `a` with these Logical Indices and shows
    /// every other, apart from the filter (indices past the last are
    /// skipped). Returns whether any came or went, so the axis needs a
    /// `Refiltered`.
    pub fn hide(&mut self, a: u8, items: &[u32]) -> bool {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        self.table.apply_hidden(axis(a), &keys).is_some()
    }

    /// Moves the items on axis `a` with these Logical Indices to before
    /// Position `to`, read in the filtered order. On a sorted axis the
    /// order shown is baked and the axis's sort cleared, unless moves on a
    /// sorted axis are refused: then nothing moves and this is `false`.
    /// The axis needs a `Reordered` notice if it moved.
    pub fn move_items(&mut self, a: u8, items: &[u32], to: f64) -> bool {
        let keys: Vec<Key> = items.iter().map(|&i| Key(i as u64)).collect();
        self.table
            .move_items(axis(a), &keys, position(to))
            .is_some()
    }

    /// Whether the items on axis `a` can move now.
    pub fn movable(&self, a: u8) -> bool {
        self.table.movable(axis(a))
    }

    /// Whether moves on a sorted axis are refused rather than baked.
    pub fn refuse_sorted_moves(&mut self, refuse: bool) {
        self.policy = if refuse {
            MovePolicy::Refuse
        } else {
            MovePolicy::Bake
        };
        self.table.set_move_policy(self.policy);
    }
}

impl WebMemory {
    fn group_by(
        &mut self,
        grouping: Grouping,
        sort: SortSpec,
        having: FilterSpec,
        col_sort: SortSpec,
        col_having: FilterSpec,
    ) -> f64 {
        self.fields = std::mem::take(&mut self.staged_fields);
        let spec = ViewSpec::default()
            .with_sort(Axis::Rows, sort)
            .with_filter(Axis::Rows, having)
            .with_sort(Axis::Cols, col_sort)
            .with_filter(Axis::Cols, col_having)
            .with_grouping(Some(grouping));
        self.spec = spec.clone();
        self.layer.request_view(spec);
        // The same levels over rows sorted or filtered anew.
        self.layer.refresh();
        self.settle();
        self.count(0)
    }

    /// The items of grouped `axis` at `range`, as [`WebMemory::grouped_window`]
    /// gives them.
    fn window_items(&mut self, axis: Axis, range: Range<Position>) -> Vec<GroupedItem> {
        let want = range
            .end
            .min(self.count(axis.index() as u8) as u64)
            .saturating_sub(range.start) as usize;
        let mut items = self.layer.window(axis, range.clone());
        while items.len() < want {
            self.settle();
            let more = self.layer.window(axis, range.clone());
            if more.len() <= items.len() {
                break;
            }
            items = more;
        }
        (items.into_iter())
            .map(|(key, outline)| {
                let summary = self.layer.summary(key);
                GroupedItem {
                    key: key.0,
                    item: (outline.kind == ItemKind::Leaf).then_some(key.0 as u32),
                    outline: (&outline).into(),
                    aggregates: summary.map(|s| s.aggregates.iter().map(|f| f.number()).collect()),
                    children: summary.map(|s| s.children as f64),
                    value: self.value_of(&outline),
                }
            })
            .collect()
    }

    /// The index of the value `outline` stands for, if any.
    fn value_of(&self, outline: &Outline) -> Option<u32> {
        let group = outline.group.as_ref()?;
        self.value_index(group.field, &group.value)
    }

    /// The index of the value a group of `field` with `value` stands for,
    /// if `field` is the values pseudo-level's.
    fn value_index(&self, field: Key, value: &Value) -> Option<u32> {
        let (Value::Text(identity), true) = (value, field == VALUES_FIELD) else {
            return None;
        };
        let grouping = self.spec.grouping.as_ref()?;
        grouping.value_of(identity).map(|(i, _)| i as u32)
    }

    fn settle(&mut self) {
        let mut backend = Backend {
            table: &self.table,
            fields: &self.fields,
            aggregates: &mut self.aggregates,
        };
        settle(&mut self.layer, &mut backend);
        self.notices = Default::default();
        while let Some(viewed) = self.layer.poll_viewed() {
            for (a, n) in viewed.notices {
                self.notices[a.index()].push(n);
            }
            for axis in Axis::ALL {
                let sort = viewed.applied.axis(axis).sort.columns();
                self.sorted[axis.index()] = sort.iter().map(|c| c.key.0).collect();
            }
            if let Some(g) = viewed.applied.grouping {
                self.applied = Axis::ALL.map(|a| g.axis(a).expand.clone());
            }
        }
    }

    fn locate_grouped(&mut self, axis: Axis, key: Key) -> Located {
        loop {
            if let Some(located) = self.layer.locate(axis, key) {
                return located;
            }
            self.settle();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binnacle_core::{
        AggregateValue, Comparison, Condition, GRAND_TOTAL, Level, Placement, Scalar,
    };

    const ROWS: u8 = 0;
    const COLS: u8 = 1;

    fn order(m: &WebMemory) -> Vec<u32> {
        m.window(ROWS, 0.0, m.count(ROWS))
    }

    #[test]
    fn hidden_items_leave_apart_from_the_filter_until_shown() {
        let mut m = WebMemory::new(5, 3);
        assert!(m.hide(ROWS, &[1, 3, 9]));
        assert_eq!(order(&m), vec![0, 2, 4]);
        m.filter(ROWS, &[1, 1, 0, 1, 1]);
        assert_eq!(order(&m), vec![0, 4]);
        m.clear_filter(ROWS);
        assert_eq!(order(&m), vec![0, 2, 4]);
        assert!(!m.hide(ROWS, &[3, 1]), "the same items, so nothing moves");
        assert!(m.hide(COLS, &[0]));
        assert_eq!(m.window(COLS, 0.0, m.count(COLS)), vec![1, 2]);
        assert!(m.hide(ROWS, &[]));
        assert_eq!(order(&m), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn staged_strings_sort_by_code_point_with_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        // "b", null, "a", "é" (two bytes, after every ASCII letter).
        let bytes = b"ba\xc3\xa9".to_vec();
        m.stage_strings(bytes, vec![1, 1, 2, 4], vec![0, 1, 0, 0]);
        m.sort(ROWS, &[0], &[1]);
        assert_eq!(order(&m), vec![3, 0, 2, 1]);
    }

    #[test]
    fn staged_numbers_put_nan_after_numbers_and_nulls_last() {
        let mut m = WebMemory::new(4, 1);
        m.stage_numbers(&[f64::NAN, 2.0, 0.0, 1.0], &[0, 0, 1, 0]);
        m.sort(ROWS, &[0], &[0]);
        assert_eq!(order(&m), vec![3, 1, 0, 2]);
    }

    #[test]
    fn filtering_answers_the_new_count_and_locates_dropped_rows_as_gone() {
        let mut m = WebMemory::new(4, 1);
        assert_eq!(m.filter(ROWS, &[1, 0, 1, 1]), 3.0);
        assert_eq!(m.locate(ROWS, 2), vec![0.0, 1.0]);
        assert_eq!(m.locate(ROWS, 1)[0], 1.0);
        assert_eq!(m.clear_filter(ROWS), 4.0);
    }

    #[test]
    fn the_columns_filter_apart_from_the_rows() {
        let mut m = WebMemory::new(2, 4);
        assert_eq!(m.filter(COLS, &[0, 1, 0, 1]), 2.0);
        assert_eq!(m.window(COLS, 0.0, 4.0), vec![1, 3]);
        assert_eq!(order(&m), vec![0, 1]);
        assert!(!m.defer(COLS, 3, 0));
        assert_eq!(m.unmatched(COLS, &[1, 3]), vec![0, 1]);
        assert!(m.has_unmatched(COLS) && !m.has_unmatched(ROWS));
        assert!(m.refilter(COLS, 0, 1));
        assert_eq!(m.window(COLS, 0.0, 4.0), vec![0, 1, 3]);
        assert_eq!(m.clear_filter(COLS), 4.0);
    }

    #[test]
    fn an_edited_row_stays_unmatched_until_the_rows_are_filtered_again() {
        let mut m = WebMemory::new(3, 1);
        m.filter(ROWS, &[1, 1, 0]);
        assert!(!m.defer(ROWS, 1, 0));
        assert_eq!(order(&m), vec![0, 1]);
        assert_eq!(m.unmatched(ROWS, &[0, 1, 2]), vec![0, 1, 0]);
        assert!(m.has_unmatched(ROWS));
        assert!(m.refilter(ROWS, 1, 0));
        assert_eq!(order(&m), vec![0]);
        assert!(!m.has_unmatched(ROWS));
    }

    #[test]
    fn staged_keys_of_a_row_sort_the_columns() {
        let mut m = WebMemory::new(1, 3);
        m.stage_numbers(&[2.0, 0.0, 1.0], &[0, 1, 0]);
        m.sort(COLS, &[0], &[0]);
        assert_eq!(m.window(COLS, 0.0, 3.0), vec![2, 0, 1]);
        assert_eq!(order(&m), vec![0]);
    }

    #[test]
    fn moved_items_take_their_new_positions_on_either_axis() {
        let mut m = WebMemory::new(3, 3);
        assert!(m.move_items(COLS, &[2], 0.0));
        assert_eq!(m.window(COLS, 0.0, 3.0), vec![2, 0, 1]);
        assert!(m.move_items(ROWS, &[0], 3.0));
        assert_eq!(order(&m), vec![1, 2, 0]);
    }

    #[test]
    fn grouped_rows_count_their_group_items_and_leaves_keep_their_place() {
        let mut m = WebMemory::new(5, 2);
        // Column 1 is "b", "a", "b", null, "a", sorted by it first.
        m.stage_strings(b"babab".to_vec(), vec![1, 2, 3, 3, 4], vec![0, 0, 0, 1, 0]);
        m.sort(ROWS, &[1], &[0]);
        assert_eq!(order(&m), vec![1, 4, 0, 2, 3]);
        let stage = |m: &mut WebMemory| {
            let (numbers, nulls) = (vec![0.0; 5], vec![1; 5]);
            let (bytes, ends) = (b"babab".to_vec(), vec![1, 2, 3, 3, 4]);
            m.stage_field(1, bytes, ends, vec![0, 0, 0, 1, 0], numbers, nulls);
        };
        stage(&mut m);
        let mut grouping = Grouping::rows_by(Key(1));
        grouping.rows.grand_total = Placement::After;
        assert_eq!(
            m.group_by(
                grouping.clone(),
                SortSpec::unsorted(),
                FilterSpec::default(),
                SortSpec::unsorted(),
                FilterSpec::default()
            ),
            9.0,
            "three groups, five Leaves and the Grand Total"
        );
        assert_eq!(m.locate_summary(ROWS, GRAND_TOTAL.0), vec![0.0, 8.0]);
        stage(&mut m);
        grouping.rows.grand_total = Placement::Off;
        assert_eq!(
            m.group_by(
                grouping,
                SortSpec::unsorted(),
                FilterSpec::default(),
                SortSpec::unsorted(),
                FilterSpec::default()
            ),
            8.0,
            "three groups and five Leaves"
        );
        assert!(m.grouped());
        // a: 1, 4; b: 0, 2; blank: 3.
        assert_eq!(m.locate(ROWS, 4), vec![0.0, 2.0]);
        assert_eq!(m.locate(ROWS, 3), vec![0.0, 7.0]);
        m.filter(ROWS, &[1, 1, 0, 1, 1]);
        assert_eq!(m.regroup(), 7.0);
        assert_eq!(m.locate(ROWS, 2)[0], 1.0, "filtered out");
        assert_eq!(m.ungroup(), 4.0);
        assert!(!m.grouped());
        assert_eq!(m.locate(ROWS, 3), vec![0.0, 3.0]);
    }

    /// Stages column `field`'s texts, none of them numbers.
    fn stage_texts(m: &mut WebMemory, field: u32, texts: &[&str]) {
        let bytes = texts.concat().into_bytes();
        let ends = (texts.iter())
            .scan(0, |end, t| {
                *end += t.len() as u32;
                Some(*end)
            })
            .collect();
        let n = texts.len();
        m.stage_field(field, bytes, ends, vec![0; n], vec![0.0; n], vec![1; n]);
    }

    #[test]
    fn a_pivot_lists_its_columns_from_the_data_and_crosses_them_with_the_rows() {
        let mut m = WebMemory::new(5, 3);
        let stage = |m: &mut WebMemory| {
            stage_texts(m, 0, &["N", "S", "N", "N", "S"]);
            stage_texts(m, 1, &["Q2", "Q1", "Q1", "Q2", "Q2"]);
            let (numbers, nulls) = (vec![1.0, 2.0, 3.0, 4.0, 5.0], vec![0; 5]);
            m.stage_field(2, b"".to_vec(), vec![0; 5], vec![1; 5], numbers, nulls);
        };
        stage(&mut m);
        let mut grouping = Grouping::rows_by(Key(0));
        grouping.col_levels = vec![Level::field(Key(1)), Level::Values];
        grouping.values = vec![AggregateValue::of(Key(2), "sum"), AggregateValue::count()];
        grouping.cols.grand_total = Placement::After;
        let rows = m.group_by(
            grouping.clone(),
            SortSpec::unsorted(),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        assert!(m.pivot());
        assert_eq!(rows, 2.0, "N and S, no Leaves");
        assert_eq!(
            m.count(COLS),
            6.0,
            "Q1 and Q2 each by sum and count, then All by each"
        );
        let cols = m.window_items(Axis::Cols, 0..6);
        let named: Vec<(Option<String>, Option<u32>)> = (cols.iter())
            .map(|c| {
                (
                    c.outline.group.as_ref().and_then(|g| g.value.clone()),
                    c.value,
                )
            })
            .collect();
        let sum = Some("sum(2)".to_string());
        let count = Some("count()".to_string());
        assert_eq!(
            named,
            vec![
                (sum.clone(), Some(0)),
                (count.clone(), Some(1)),
                (sum.clone(), Some(0)),
                (count.clone(), Some(1)),
                (sum, Some(0)),
                (count, Some(1)),
            ]
        );
        assert!(
            cols.iter()
                .all(|c| c.outline.group.as_ref().unwrap().field == VALUES_FIELD.0)
        );
        let rows = m.window_items(Axis::Rows, 0..2);
        let n = rows[0].key;
        let cell = |m: &WebMemory, col: usize| m.cell(n, cols[col].key);
        // N holds rows 0, 2 and 3: Q2 by 0 and 3, Q1 by 2.
        assert_eq!(cell(&m, 0), Some(3.0), "N's Q1 sum");
        assert_eq!(cell(&m, 2), Some(5.0), "N's Q2 sum");
        assert_eq!(cell(&m, 3), Some(2.0), "N's Q2 count");
        assert_eq!(cell(&m, 4), Some(8.0), "N's sum over all quarters");
        assert_eq!(m.locate_summary(COLS, cols[5].key), vec![0.0, 5.0]);
        // By the Grand Total row's sum, most first: Q2's 10 before Q1's 5.
        stage(&mut m);
        let by_total = SortSpec::new(vec![SortColumn::descending(GRAND_TOTAL)]);
        m.group_by(
            grouping,
            SortSpec::unsorted(),
            FilterSpec::default(),
            by_total,
            FilterSpec::default(),
        );
        let cols = m.window_items(Axis::Cols, 0..1);
        let n = m.window_items(Axis::Rows, 0..1)[0].key;
        assert_eq!(m.cell(n, cols[0].key), Some(5.0), "N's Q2 sum leads");
    }

    #[test]
    fn a_pivots_rows_sort_by_a_pivot_column_left_out_while_the_data_lacks_it() {
        let mut m = WebMemory::new(5, 3);
        // N holds rows 0, 2 and 3 (Q2 5, Q1 3); S rows 1 and 4 (Q1 2, Q2 10).
        let stage = |m: &mut WebMemory| {
            stage_texts(m, 0, &["N", "S", "N", "N", "S"]);
            stage_texts(m, 1, &["Q2", "Q1", "Q1", "Q2", "Q2"]);
            let (numbers, nulls) = (vec![1.0, 2.0, 3.0, 4.0, 10.0], vec![0; 5]);
            m.stage_field(2, b"".to_vec(), vec![0; 5], vec![1; 5], numbers, nulls);
        };
        let mut grouping = Grouping::rows_by(Key(0));
        grouping.col_levels = vec![Level::field(Key(1))];
        grouping.values = vec![AggregateValue::of(Key(2), "sum")];
        stage(&mut m);
        m.group_by(
            grouping.clone(),
            SortSpec::unsorted(),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        let cols = m.window_items(Axis::Cols, 0..2);
        let (q1, q2) = (cols[0].key, cols[1].key);
        let first = |m: &mut WebMemory| {
            let row = m.window_items(Axis::Rows, 0..1).remove(0);
            row.outline.group.and_then(|g| g.value)
        };
        assert_eq!(first(&mut m).as_deref(), Some("N"));

        let by = |key| SortSpec::new(vec![SortColumn::descending(Key(key))]);
        stage(&mut m);
        m.group_by(
            grouping.clone(),
            by(q2),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        assert_eq!(first(&mut m).as_deref(), Some("S"), "S's Q2 10 leads");
        assert_eq!(m.applied_sort(ROWS), vec![q2]);
        assert!(m.fits(COLS, q2));

        // Q1's rows filtered out: no Q1 column, so its term is left out.
        m.filter(ROWS, &[1, 0, 0, 1, 1]);
        stage(&mut m);
        m.group_by(
            grouping.clone(),
            by(q1),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        assert_eq!(m.applied_sort(ROWS), Vec::<u64>::new());
        m.clear_filter(ROWS);
        stage(&mut m);
        m.group_by(
            grouping.clone(),
            by(q1),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        assert_eq!(m.applied_sort(ROWS), vec![q1], "back with Q1");
        assert_eq!(first(&mut m).as_deref(), Some("N"), "N's Q1 3 leads");

        // Without column levels no pivot column's path fits.
        grouping.col_levels.clear();
        stage(&mut m);
        m.group_by(
            grouping,
            SortSpec::unsorted(),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        assert!(!m.fits(COLS, q1));
    }

    #[test]
    fn a_pivots_columns_filtered_by_the_grand_total_row_keep_those_whose_figure_passes() {
        let mut m = WebMemory::new(4, 3);
        stage_texts(&mut m, 0, &["N", "S", "N", "S"]);
        stage_texts(&mut m, 1, &["Q1", "Q1", "Q2", "Q3"]);
        let (numbers, nulls) = (vec![1.0, 2.0, 30.0, 40.0], vec![0; 4]);
        m.stage_field(2, b"".to_vec(), vec![0; 4], vec![1; 4], numbers, nulls);
        let mut grouping = Grouping::rows_by(Key(0));
        grouping.col_levels = vec![Level::field(Key(1))];
        grouping.values = vec![AggregateValue::of(Key(2), "sum")];
        grouping.cols.grand_total = Placement::After;
        let big = Condition::compare(GRAND_TOTAL, Comparison::Greater(Scalar::Number(5.0)));
        let having = FilterSpec::default().with_conditions(vec![big].into());
        m.group_by(
            grouping,
            SortSpec::unsorted(),
            FilterSpec::default(),
            SortSpec::unsorted(),
            having,
        );
        // Q1 sums to 3 and goes; Q2 (30), Q3 (40) and All stay.
        assert_eq!(m.count(COLS), 3.0);
        let cols = m.window_items(Axis::Cols, 0..3);
        let values: Vec<_> = (cols.iter())
            .map(|c| c.outline.group.as_ref().and_then(|g| g.value.clone()))
            .collect();
        assert_eq!(values, vec![Some("Q2".into()), Some("Q3".into()), None]);
    }

    #[test]
    fn a_comparison_on_a_summed_field_leaves_out_the_groups_whose_sum_fails_it() {
        let mut m = WebMemory::new(4, 2);
        // Column 0 groups: "a", "a", "b", "b"; column 1 sums: 1, 2, 10, 20.
        let (bytes, ends, nulls) = (b"aabb".to_vec(), vec![1, 2, 3, 4], vec![0; 4]);
        m.stage_field(0, bytes, ends, nulls, vec![0.0; 4], vec![1; 4]);
        let (bytes, ends) = (b"1210".to_vec(), vec![1, 2, 4, 4]);
        let numbers = vec![1.0, 2.0, 10.0, 20.0];
        m.stage_field(1, bytes, ends, vec![0; 4], numbers, vec![0; 4]);
        let mut grouping = Grouping::rows_by(Key(0));
        grouping.values = vec![AggregateValue::of(Key(1), "sum")];
        grouping.rows.grand_total = Placement::After;
        let big = Condition::compare(Key(1), Comparison::Greater(Scalar::Number(5.0)));
        let having = FilterSpec::default().with_conditions(vec![big].into());
        // Group "b" (sum 30) and its two Leaves, and the Grand Total.
        assert_eq!(
            m.group_by(
                grouping,
                SortSpec::unsorted(),
                having,
                SortSpec::unsorted(),
                FilterSpec::default()
            ),
            4.0
        );
        assert_eq!(m.locate(ROWS, 0)[0], 1.0, "under a group left out");
        assert_eq!(m.locate(ROWS, 2), vec![0.0, 1.0]);
    }

    #[test]
    fn an_edit_in_grouped_rows_refigures_in_place_and_leaves_move_within_their_group() {
        let mut m = WebMemory::new(5, 2);
        // Column 1 is "b", "a", "b", null, "a", sorted by it first.
        m.stage_strings(b"babab".to_vec(), vec![1, 2, 3, 3, 4], vec![0, 0, 0, 1, 0]);
        m.sort(ROWS, &[1], &[0]);
        let stage = |m: &mut WebMemory, bytes: &[u8]| {
            let (numbers, nulls) = (vec![0.0; 5], vec![1; 5]);
            let ends = vec![1, 2, 3, 3, 4];
            m.stage_field(1, bytes.to_vec(), ends, vec![0, 0, 0, 1, 0], numbers, nulls);
        };
        stage(&mut m, b"babab");
        m.group_by(
            Grouping::rows_by(Key(1)),
            SortSpec::unsorted(),
            FilterSpec::default(),
            SortSpec::unsorted(),
            FilterSpec::default(),
        );
        // a: 1, 4; b: 0, 2; blank: 3.
        assert_eq!(m.locate(ROWS, 0), vec![0.0, 4.0]);

        // Row 0 now reads "a": it stays in b, unmatched, and the groups'
        // figures change in place.
        stage(&mut m, b"aabab");
        let changed = m.refigure();
        assert!(!changed.is_empty());
        assert!(
            changed
                .chunks(2)
                .all(|r| r[0] < r[1] && r[1] <= m.count(ROWS))
        );
        assert_eq!(m.locate(ROWS, 0), vec![0.0, 4.0]);
        assert_eq!(m.unmatched(ROWS, &[0, 1, 2]), vec![1, 0, 0]);

        assert_eq!(m.move_grouped(&[4], 1.0), 0);
        assert_eq!(m.locate(ROWS, 4), vec![0.0, 1.0]);
        assert_eq!(m.locate(ROWS, 1), vec![0.0, 2.0]);
        assert_eq!(m.move_grouped(&[4], 5.0), 1, "into b");
        assert_eq!(m.move_grouped(&[GRAND_TOTAL.0], 1.0), 2);
        assert_eq!(m.locate(ROWS, 4), vec![0.0, 1.0]);
    }

    #[test]
    fn a_sorted_axis_refuses_moves_if_asked_to() {
        let mut m = WebMemory::new(3, 1);
        m.refuse_sorted_moves(true);
        m.stage_numbers(&[2.0, 0.0, 1.0], &[0, 0, 0]);
        m.sort(ROWS, &[0], &[0]);
        assert!(!m.movable(ROWS));
        assert!(!m.move_items(ROWS, &[0], 0.0));
        assert_eq!(order(&m), vec![1, 2, 0]);
        assert!(m.movable(COLS));
    }
}
