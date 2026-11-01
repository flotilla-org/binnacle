//! Internal C API over the binnacle Grid, for wheelhouse.
//!
//! `include/binnacle.h` is the hand-written header and states the contract;
//! this file mirrors it, constant for constant and struct for struct. Every
//! array a call fills points into buffers the [`BinnacleGrid`] handle owns, so
//! the host never releases anything but the handle. Effects are polled;
//! nothing calls back into the host.

use std::collections::VecDeque;
use std::slice;

mod described;
mod layer;
pub use described::*;
pub use layer::*;

use binnacle_layout::{IdTable, JsonId, Layout};

use binnacle_core::{
    Action, Aggregate, AggregateValue, Align, Axis, AxisConfig, AxisView, Button, ChangeNotice,
    Compared, Comparison, Condition, ConditionKind, ConditionState, Depth, DropTarget, Edge,
    EditEnd, Effect, ExceptCapped, Expand, FilterSpec, FindDirection, FindMatch, FindQuery,
    FocusEnd, FocusStep, Form, Grid, GroupOf, Grouping, HeaderClick, HeaderPlacement, ItemKind,
    Key, Level, Located, Mode, Modifiers, MoveRefusal, OpenState, Outline, Placement, Placements,
    PointerEvent, PointerKind, Rect, Scalar, Searched, SelectionUnit, SetPosition, SortColumn,
    SortDirection, SortSpec, Stamped, StripItem, Support, TextQuery, TextSupport, Totals, Value,
    ViewEdit, ViewSpec, Viewport,
};

/// The version of the ABI `binnacle.h` describes; see the header for when it
/// changes.
pub const BINNACLE_ABI_VERSION: u32 = 22;

pub const BINNACLE_EDGE_LEADING: u32 = 0;
pub const BINNACLE_EDGE_TRAILING: u32 = 1;
pub const BINNACLE_ROWS: u32 = 0;
pub const BINNACLE_COLS: u32 = 1;
/// What `binnacle_grid_hide` returns when the hide would pass the axis's cap.
pub const BINNACLE_HIDE_CAPPED: i32 = 2;
pub const BINNACLE_HIDE_CAP: usize = binnacle_core::HIDE_CAP;

pub const BINNACLE_DAMAGE_LAYOUT: u32 = 1;
pub const BINNACLE_DAMAGE_SELECTION: u32 = 2;
pub const BINNACLE_DAMAGE_POSITIONS: u32 = 4;

pub const BINNACLE_CELL_FOCUSED: u8 = 1;
pub const BINNACLE_CELL_SELECTED: u8 = 2;
pub const BINNACLE_CELL_CURRENT_MATCH: u8 = 4;
pub const BINNACLE_CELL_EDITING: u8 = 8;

pub const BINNACLE_HEADER_SORTED: u32 = 1;
pub const BINNACLE_HEADER_DESCENDING: u32 = 2;
pub const BINNACLE_HEADER_PENDING: u32 = 4;
pub const BINNACLE_HEADER_SELECTED: u32 = 8;
pub const BINNACLE_HEADER_HOVERED: u32 = 16;
pub const BINNACLE_HEADER_FILTERED: u32 = 32;
pub const BINNACLE_HEADER_FILTER_PENDING: u32 = 64;
pub const BINNACLE_HEADER_FILTER_NOT_APPLIED: u32 = 128;
pub const BINNACLE_HEADER_MOVE_PENDING: u32 = 256;

pub const BINNACLE_MODE_OTHER: u32 = 0;
pub const BINNACLE_MODE_IDLE: u32 = 1;
pub const BINNACLE_MODE_DRAGGING_SELECTION: u32 = 2;
pub const BINNACLE_MODE_RESIZING_BAND: u32 = 3;
pub const BINNACLE_MODE_MOVING_HEADERS: u32 = 4;
pub const BINNACLE_MODE_EDITING: u32 = 5;

pub const BINNACLE_DROP_MOVE: u32 = 0;
pub const BINNACLE_DROP_UNPIN: u32 = 1;
pub const BINNACLE_DROP_PIN: u32 = 2;
pub const BINNACLE_DROP_REFUSED: u32 = 3;

pub const BINNACLE_MOVE_REFUSAL_OTHER: u32 = 0;
pub const BINNACLE_MOVE_REFUSAL_BETWEEN_GROUPS: u32 = 1;
pub const BINNACLE_MOVE_REFUSAL_SUMMARY: u32 = 2;

pub const BINNACLE_EFFECT_OTHER: u32 = 0;
pub const BINNACLE_EFFECT_TEXT_TYPED: u32 = 1;
pub const BINNACLE_EFFECT_FOCUS_CHANGED: u32 = 2;
pub const BINNACLE_EFFECT_SELECTION_CHANGED: u32 = 3;
pub const BINNACLE_EFFECT_VIEW_REQUESTED: u32 = 4;
pub const BINNACLE_EFFECT_LOCATE: u32 = 5;
pub const BINNACLE_EFFECT_BAND_SIZE_CHANGED: u32 = 6;
pub const BINNACLE_EFFECT_BAND_AUTO_SIZE_REQUESTED: u32 = 7;
pub const BINNACLE_EFFECT_FIND_REQUESTED: u32 = 8;
pub const BINNACLE_EFFECT_FIND_STEP_REQUESTED: u32 = 9;
pub const BINNACLE_EFFECT_MOVE_REQUESTED: u32 = 10;
pub const BINNACLE_EFFECT_PINS_CHANGED: u32 = 11;
pub const BINNACLE_EFFECT_ACTIVATED: u32 = 12;
pub const BINNACLE_EFFECT_EDIT_REQUESTED: u32 = 13;
pub const BINNACLE_EFFECT_EDIT_CANCELLED: u32 = 14;
pub const BINNACLE_EFFECT_SIZE_CHANGED: u32 = 15;
pub const BINNACLE_EFFECT_AUTO_SIZE_REQUESTED: u32 = 16;
pub const BINNACLE_EFFECT_COPY_REQUESTED: u32 = 17;

pub const BINNACLE_EDIT_COMMIT: u32 = 0;
pub const BINNACLE_EDIT_CANCEL: u32 = 1;

pub const BINNACLE_FIND_NEXT: u32 = 0;
pub const BINNACLE_FIND_PREVIOUS: u32 = 1;

pub const BINNACLE_FIND: u32 = 1;
pub const BINNACLE_FIND_REGEX: u32 = 2;

pub const BINNACLE_SORT_ASCENDING: u32 = 0;
pub const BINNACLE_SORT_DESCENDING: u32 = 1;

pub const BINNACLE_VIEW_EDIT_OTHER: u32 = 0;
pub const BINNACLE_VIEW_EDIT_CYCLE_SORT: u32 = 1;
pub const BINNACLE_VIEW_EDIT_SET_QUICK_FILTER: u32 = 2;
pub const BINNACLE_VIEW_EDIT_REPLACE: u32 = 3;
pub const BINNACLE_VIEW_EDIT_SHOWN_CHANGED: u32 = 4;
pub const BINNACLE_VIEW_EDIT_ADD_CONDITION: u32 = 5;
pub const BINNACLE_VIEW_EDIT_REMOVE_CONDITION: u32 = 6;
pub const BINNACLE_VIEW_EDIT_REPLACE_CONDITION: u32 = 7;
pub const BINNACLE_VIEW_EDIT_REAPPLY: u32 = 8;
pub const BINNACLE_VIEW_EDIT_HIDE: u32 = 9;
pub const BINNACLE_VIEW_EDIT_SHOW: u32 = 10;
pub const BINNACLE_VIEW_EDIT_GROUP: u32 = 11;
pub const BINNACLE_VIEW_EDIT_UNGROUP: u32 = 12;
pub const BINNACLE_VIEW_EDIT_EXPAND: u32 = 13;
pub const BINNACLE_VIEW_EDIT_COLLAPSE: u32 = 14;
pub const BINNACLE_VIEW_EDIT_EXPAND_ALL: u32 = 15;
pub const BINNACLE_VIEW_EDIT_EXPAND_TO: u32 = 16;
pub const BINNACLE_VIEW_EDIT_ADD_SLICER: u32 = 17;
pub const BINNACLE_VIEW_EDIT_REMOVE_SLICER: u32 = 18;
pub const BINNACLE_VIEW_EDIT_REPLACE_SLICER: u32 = 19;
pub const BINNACLE_VIEW_EDIT_SET_FORM: u32 = 20;
pub const BINNACLE_VIEW_EDIT_REMOVE_LEVEL: u32 = 21;
pub const BINNACLE_VIEW_EDIT_MOVE_LEVEL: u32 = 22;
pub const BINNACLE_VIEW_EDIT_ADD_VALUE: u32 = 23;
pub const BINNACLE_VIEW_EDIT_REMOVE_VALUE: u32 = 24;

pub const BINNACLE_CONDITION_OTHER: u32 = 0;
pub const BINNACLE_CONDITION_TEXT: u32 = 1;
pub const BINNACLE_CONDITION_ONE_OF: u32 = 2;
pub const BINNACLE_CONDITION_COMPARE: u32 = 3;
pub const BINNACLE_CONDITION_NATIVE: u32 = 4;

pub const BINNACLE_COMPARE_EQUAL: u32 = 0;
pub const BINNACLE_COMPARE_NOT_EQUAL: u32 = 1;
pub const BINNACLE_COMPARE_LESS: u32 = 2;
pub const BINNACLE_COMPARE_AT_MOST: u32 = 3;
pub const BINNACLE_COMPARE_GREATER: u32 = 4;
pub const BINNACLE_COMPARE_AT_LEAST: u32 = 5;
pub const BINNACLE_COMPARE_BETWEEN: u32 = 6;

pub const BINNACLE_SCALAR_NUMBER: u32 = 0;
pub const BINNACLE_SCALAR_DATE: u32 = 1;

pub const BINNACLE_CONDITION_APPLIED: u32 = 0;
pub const BINNACLE_CONDITION_ADDING: u32 = 1;
pub const BINNACLE_CONDITION_REMOVING: u32 = 2;
pub const BINNACLE_CONDITION_NOT_APPLIED: u32 = 3;

pub const BINNACLE_TEXT_CASE_SENSITIVE: u32 = 1;
pub const BINNACLE_TEXT_REGEX: u32 = 2;

pub const BINNACLE_SEARCHED_SHOWN: u32 = 0;
pub const BINNACLE_SEARCHED_KEYS: u32 = 1;

pub const BINNACLE_SUPPORT_SORT: u32 = 1;
pub const BINNACLE_SUPPORT_TEXT: u32 = 2;
pub const BINNACLE_SUPPORT_TEXT_REGEX: u32 = 4;
pub const BINNACLE_SUPPORT_ONE_OF: u32 = 8;
pub const BINNACLE_SUPPORT_COMPARE_NUMBERS: u32 = 16;
pub const BINNACLE_SUPPORT_COMPARE_DATES: u32 = 32;
pub const BINNACLE_SUPPORT_GROUP: u32 = 64;
pub const BINNACLE_SUPPORT_NATIVE: u32 = 128;
pub const BINNACLE_SUPPORT_PIVOT: u32 = 256;
pub const BINNACLE_SUPPORT_SUBTOTAL_BEFORE: u32 = 512;
pub const BINNACLE_SUPPORT_SUBTOTAL_AFTER: u32 = 1024;

pub const BINNACLE_QUICK_FILTER: u32 = 1;
pub const BINNACLE_QUICK_FILTER_REGEX: u32 = 2;

pub const BINNACLE_PLACEMENT_OFF: u32 = 0;
pub const BINNACLE_PLACEMENT_BEFORE: u32 = 1;
pub const BINNACLE_PLACEMENT_AFTER: u32 = 2;

pub const BINNACLE_LEVEL_FIELD: u32 = 0;
pub const BINNACLE_LEVEL_VALUES: u32 = 1;
pub const BINNACLE_LEVEL_OTHER: u32 = 2;

pub const BINNACLE_FORM_OUTLINE: u32 = 0;
pub const BINNACLE_FORM_HEADER: u32 = 1;

pub const BINNACLE_ITEM_LEAF: u32 = 0;
pub const BINNACLE_ITEM_GROUP: u32 = 1;
pub const BINNACLE_ITEM_SUBTOTAL: u32 = 2;
pub const BINNACLE_ITEM_GRAND_TOTAL: u32 = 3;

pub const BINNACLE_GRAND_TOTAL_KEY: u64 = binnacle_core::GRAND_TOTAL.0;

pub const BINNACLE_VALUES_FIELD: u64 = binnacle_core::VALUES_FIELD.0;

pub const BINNACLE_OPEN_STATE_OPEN: u32 = 0;
pub const BINNACLE_OPEN_STATE_CLOSED: u32 = 1;
pub const BINNACLE_OPEN_STATE_CANT_OPEN: u32 = 2;

pub const BINNACLE_LOCATED_FOUND: u32 = 0;
pub const BINNACLE_LOCATED_GONE: u32 = 1;
pub const BINNACLE_LOCATED_GONE_SUGGEST: u32 = 2;

pub const BINNACLE_NOTICE_RESET: u32 = 0;
pub const BINNACLE_NOTICE_REORDERED: u32 = 1;
pub const BINNACLE_NOTICE_INSERTED: u32 = 2;
pub const BINNACLE_NOTICE_REMOVED: u32 = 3;
pub const BINNACLE_NOTICE_CONTENT_CHANGED: u32 = 4;
pub const BINNACLE_NOTICE_REFILTERED: u32 = 5;
pub const BINNACLE_NOTICE_COLLAPSED: u32 = 6;
pub const BINNACLE_NOTICE_EXPANDED: u32 = 7;
pub const BINNACLE_EXCEPT_CAP: usize = binnacle_core::EXCEPT_CAP;
pub const BINNACLE_EXCEPT_CAPPED: i32 = 2;

pub const BINNACLE_FOCUS_ITEM: u32 = 0;
pub const BINNACLE_FOCUS_PAGE: u32 = 1;
pub const BINNACLE_FOCUS_EDGE: u32 = 2;

pub const BINNACLE_ACTION_TYPE_TEXT: u32 = 1;
pub const BINNACLE_ACTION_ACTIVATE: u32 = 2;
pub const BINNACLE_ACTION_BEGIN_EDIT: u32 = 3;
pub const BINNACLE_ACTION_COPY: u32 = 4;
pub const BINNACLE_ACTION_MOVE_FOCUS: u32 = 5;
pub const BINNACLE_ACTION_SELECT_ALL: u32 = 6;
pub const BINNACLE_ACTION_SELECT_COLUMN: u32 = 7;
pub const BINNACLE_ACTION_SELECT_ROW: u32 = 8;
pub const BINNACLE_ACTION_CANCEL: u32 = 9;
pub const BINNACLE_ACTION_SCROLL: u32 = 10;
pub const BINNACLE_ACTION_SCROLL_TO: u32 = 11;
pub const BINNACLE_ACTION_SCROLL_BY: u32 = 12;
pub const BINNACLE_ACTION_LEAVE_OUTLINE: u32 = 13;
pub const BINNACLE_ACTION_FIND_NEXT: u32 = 14;
pub const BINNACLE_ACTION_FIND_PREVIOUS: u32 = 15;

pub const BINNACLE_ACTION_DONE: i32 = 1;
pub const BINNACLE_ACTION_REFUSED: i32 = 2;

pub const BINNACLE_ALIGN_NEAREST: u32 = 0;
pub const BINNACLE_ALIGN_START: u32 = 1;

pub const BINNACLE_POINTER_DOWN: u32 = 0;
pub const BINNACLE_POINTER_MOVE: u32 = 1;
pub const BINNACLE_POINTER_UP: u32 = 2;

pub const BINNACLE_BUTTON_PRIMARY: u32 = 0;
pub const BINNACLE_BUTTON_SECONDARY: u32 = 1;
pub const BINNACLE_BUTTON_MIDDLE: u32 = 2;

pub const BINNACLE_MOD_SHIFT: u32 = 1;
pub const BINNACLE_MOD_CTRL: u32 = 2;
pub const BINNACLE_MOD_ALT: u32 = 4;
pub const BINNACLE_MOD_META: u32 = 8;

pub const BINNACLE_SELECT_CELLS: u32 = 0;
pub const BINNACLE_SELECT_ROWS: u32 = 1;
pub const BINNACLE_SELECT_COLS: u32 = 2;

pub const BINNACLE_HEADER_CLICK_SORT: u32 = 0;
pub const BINNACLE_HEADER_CLICK_SELECT: u32 = 1;
pub const BINNACLE_HEADER_CLICK_NONE: u32 = 2;

/// The opaque handle: a Grid plus the buffers its C-shaped Snapshot and
/// Effects point into.
pub struct BinnacleGrid {
    grid: Grid,
    /// Effects a Rust host passed on, polled before the Grid's own.
    passed_on: VecDeque<Stamped>,
    strips: [Vec<BinnacleStripItem>; 2],
    /// The last Snapshot's outline ancestors, group value texts and paths
    /// per axis.
    ancestors: [Vec<u64>; 2],
    outline_texts: [String; 2],
    paths: [Vec<BinnacleGroupOf>; 2],
    headers: [Vec<BinnacleHeader>; 2],
    picked: [Vec<u64>; 2],
    /// The last Snapshot's selection range, as runs of Positions per axis.
    runs: [Vec<BinnacleRange>; 2],
    rects: Vec<BinnacleRect>,
    cells: Vec<u8>,
    /// The last polled Effect's text.
    text: String,
    /// The last polled Effect's View Spec.
    view: CView,
    /// The last polled Effect's drill-down, and the runs a copy lists per
    /// axis.
    drill_down: CConditions,
    copy_runs: [Vec<BinnacleRange>; 2],
    /// The last polled Effect's View Edit Keys.
    edit_keys: Vec<u64>,
    /// The last polled Effect's View Edit value, with its texts.
    edit_value: CValues,
    /// The last Snapshot's hidden Keys per axis.
    hidden_listed: [Vec<u64>; 2],
    /// The last Snapshot's conditions per axis, with their states.
    listed: [CConditions; 2],
    listed_states: [Vec<BinnacleListedCondition>; 2],
    listed_sorts: [Vec<BinnacleListedSort>; 2],
    /// The Keys the last `binnacle_grid_held_keys` listed.
    held: Vec<u64>,
    /// The last Snapshot's slicers, levels per axis and values, with their
    /// states, and the aggregates each header's item offers.
    listed_slicers: (CConditions, Vec<BinnacleListedCondition>),
    listed_levels: [Vec<BinnacleListedLevel>; 2],
    listed_values: (CValues, Vec<BinnacleListedValue>),
    offered: [CAggregates; 2],
    /// The last polled Effect's Find Query text and Keys, and the last
    /// Snapshot's.
    find_effect: (String, Vec<u64>),
    find_snapshot: (String, Vec<u64>),
    /// The last polled move's or pins' Keys.
    move_keys: Vec<u64>,
    /// The last Snapshot's pinned Keys per axis and edge, dormant included.
    pins: [[Vec<u64>; 2]; 2],
    /// The last listed Size Overrides' Keys and sizes.
    overrides: (Vec<u64>, Vec<f32>),
    /// The last Snapshot's pending move Keys and move refusal.
    pending_move: Vec<u64>,
    refusal: String,
    /// The last Snapshot's header-form tiers and spans per axis.
    tiers: [Vec<BinnacleExtent>; 2],
    spans: [Vec<BinnacleHeaderSpan>; 2],
    /// The last Snapshot's corner grid per axis: its tiers and the levels
    /// they name.
    corner_tiers: [Vec<BinnacleCornerTier>; 2],
    corner_levels: [Vec<BinnacleCornerLevel>; 2],
    /// The Layout last loaded, whose IDs the host did not name a save
    /// keeps, and the last saved Layout's JSON.
    loaded: Option<Layout<JsonId, JsonId>>,
    layout_json: Vec<u8>,
}

/// Values in C form, with the aggregate names and parameters they point
/// into.
#[derive(Default)]
struct CValues {
    texts: String,
    params: Vec<BinnacleText>,
    items: Vec<BinnacleAggregateValue>,
}

impl CValues {
    /// Replaces the contents with `values`.
    fn fill<'a>(&mut self, values: impl IntoIterator<Item = &'a AggregateValue> + Clone) {
        self.texts.clear();
        self.params.clear();
        self.items.clear();
        // Texts first, then the parameters, then the values, so the
        // pointers taken after stay valid.
        for v in values.clone() {
            self.texts.push_str(&v.aggregate);
            v.params.iter().for_each(|p| self.texts.push_str(p));
        }
        let mut at = 0;
        let texts = &self.texts;
        let mut text = |t: &str| {
            let data = texts.as_ptr().wrapping_add(at);
            at += t.len();
            BinnacleText { data, len: t.len() }
        };
        let mut names = Vec::new();
        for v in values.clone() {
            names.push(text(&v.aggregate));
            self.params.extend(v.params.iter().map(|p| text(p)));
        }
        let mut first = 0;
        let params = &self.params;
        self.items
            .extend(values.into_iter().zip(names).map(|(v, aggregate)| {
                let ps = params.as_ptr().wrapping_add(first);
                first += v.params.len();
                BinnacleAggregateValue {
                    has_field: flag(v.field.is_some()),
                    field: v.field.map_or(0, |k| k.0),
                    aggregate,
                    params: ps,
                    params_len: v.params.len(),
                }
            }));
    }
}

/// Offered aggregates in C form, one list per header, with the names they
/// point into.
#[derive(Default)]
struct CAggregates {
    texts: String,
    params: Vec<BinnacleText>,
    items: Vec<BinnacleAggregate>,
}

impl CAggregates {
    /// Replaces the contents with `lists`, one per header, and returns
    /// where each list starts in `items` and how long it is.
    fn fill(&mut self, lists: &[&[Aggregate]]) -> Vec<(usize, usize)> {
        self.texts.clear();
        self.params.clear();
        self.items.clear();
        for a in lists.iter().flat_map(|l| l.iter()) {
            self.texts.push_str(&a.name);
            a.params.iter().for_each(|p| self.texts.push_str(p));
        }
        let mut at = 0;
        let texts = &self.texts;
        let mut text = |t: &str| {
            let data = texts.as_ptr().wrapping_add(at);
            at += t.len();
            BinnacleText { data, len: t.len() }
        };
        let mut names = Vec::new();
        for a in lists.iter().flat_map(|l| l.iter()) {
            names.push(text(&a.name));
            self.params.extend(a.params.iter().map(|p| text(p)));
        }
        let mut first = 0;
        let params = &self.params;
        let all = lists.iter().flat_map(|l| l.iter());
        self.items.extend(all.zip(names).map(|(a, name)| {
            let ps = params.as_ptr().wrapping_add(first);
            first += a.params.len();
            BinnacleAggregate {
                name,
                params: ps,
                params_len: a.params.len(),
            }
        }));
        let mut start = 0;
        (lists.iter())
            .map(|l| {
                start += l.len();
                (start - l.len(), l.len())
            })
            .collect()
    }
}

/// `BINNACLE_CONDITION_*` for where a listed part stands.
fn state_tag(state: ConditionState) -> u32 {
    match state {
        ConditionState::Applied => BINNACLE_CONDITION_APPLIED,
        ConditionState::Adding => BINNACLE_CONDITION_ADDING,
        ConditionState::Removing => BINNACLE_CONDITION_REMOVING,
        ConditionState::NotApplied => BINNACLE_CONDITION_NOT_APPLIED,
    }
}

/// Conditions in C form, with the texts, Keys and values they point into.
#[derive(Default)]
struct CConditions {
    texts: String,
    keys: Vec<u64>,
    values: Vec<BinnacleValue>,
    items: Vec<BinnacleCondition>,
}

impl CConditions {
    /// Replaces the contents with `conditions`.
    fn fill<'a>(&mut self, conditions: impl IntoIterator<Item = &'a Condition>) {
        self.texts.clear();
        self.keys.clear();
        self.values.clear();
        self.items.clear();
        // Texts and Keys first, then values, so the pointers taken after
        // stay valid.
        let mut spans = Vec::new();
        // Each value's text span, or `None` for a blank.
        let mut values: Vec<Option<(usize, usize)>> = Vec::new();
        for c in conditions {
            #[allow(unreachable_patterns)] // Kinds added to the Grid later are OTHER.
            let (kind, query, listed) = match &c.kind {
                ConditionKind::Text(q) => (BINNACLE_CONDITION_TEXT, Some(q), &[][..]),
                ConditionKind::OneOf(vs) => (BINNACLE_CONDITION_ONE_OF, None, &vs[..]),
                ConditionKind::Compare(_) => (BINNACLE_CONDITION_COMPARE, None, &[][..]),
                ConditionKind::Native(_) => (BINNACLE_CONDITION_NATIVE, None, &[][..]),
                _ => (BINNACLE_CONDITION_OTHER, None, &[][..]),
            };
            let text = self.texts.len();
            let keys = self.keys.len();
            if let ConditionKind::Native(native) = &c.kind {
                self.texts.push_str(native);
            }
            if let Some(q) = query {
                self.texts.push_str(&q.text);
                if let Searched::Keys(ks) = &q.searches {
                    self.keys.extend(ks.iter().map(|k| k.0));
                }
            }
            let first = values.len();
            for v in listed {
                values.push(match v {
                    Value::Text(t) => {
                        self.texts.push_str(t);
                        Some((self.texts.len() - t.len(), t.len()))
                    }
                    Value::Blank => None,
                });
            }
            spans.push((c, kind, query, text, keys, first..values.len()));
        }
        self.values.extend(values.into_iter().map(|v| match v {
            Some((at, len)) => BinnacleValue {
                blank: 0,
                text: BinnacleText {
                    data: self.texts.as_ptr().wrapping_add(at),
                    len,
                },
            },
            None => BinnacleValue {
                blank: 1,
                text: BinnacleText::default(),
            },
        }));
        for (c, kind, query, text, keys, listed) in spans {
            let mut q = BinnacleTextQuery::default();
            if let ConditionKind::Native(native) = &c.kind {
                q.text = BinnacleText {
                    data: self.texts.as_ptr().wrapping_add(text),
                    len: native.len(),
                };
            }
            if let Some(query) = query {
                q.flags = bit(query.case_sensitive, BINNACLE_TEXT_CASE_SENSITIVE)
                    | bit(query.regex, BINNACLE_TEXT_REGEX);
                q.text = BinnacleText {
                    data: self.texts.as_ptr().wrapping_add(text),
                    len: query.text.len(),
                };
                if let Searched::Keys(ks) = &query.searches {
                    q.searched = BINNACLE_SEARCHED_KEYS;
                    q.keys = self.keys.as_ptr().wrapping_add(keys);
                    q.keys_len = ks.len();
                }
            }
            let (compare, scalar, high) = match &c.kind {
                ConditionKind::Compare(comparison) => comparison_to_c(comparison),
                _ => Default::default(),
            };
            self.items.push(BinnacleCondition {
                key: c.key.0,
                kind,
                negated: flag(c.not),
                text: q,
                values: self.values.as_ptr().wrapping_add(listed.start),
                values_len: listed.len(),
                compare,
                scalar,
                high,
            });
        }
    }
}

/// A View Spec in C form, with the sorts, texts, Keys, conditions and
/// grouping it points into.
#[derive(Default)]
struct CView {
    /// Each axis's sort, and its quick filter's text and Keys.
    sorts: [Vec<BinnacleSortColumn>; 2],
    quick_texts: [String; 2],
    quick_keys: [Vec<u64>; 2],
    conditions: [CConditions; 2],
    hidden: [Vec<u64>; 2],
    /// The grouping: each axis's levels and except Keys, the values with
    /// their aggregates' texts, and the slicers.
    levels: [Vec<BinnacleLevel>; 2],
    excepts: [Vec<u64>; 2],
    aggregates: CValues,
    slicers: CConditions,
}

impl CView {
    /// Replaces the contents with `spec`, returning it in C form.
    fn fill(&mut self, spec: &ViewSpec) -> BinnacleViewSpec {
        let mut c = BinnacleViewSpec::default();
        for axis in Axis::ALL {
            c.axes[axis.index()] = self.axis_to_c(axis, spec.axis(axis));
        }
        if let Some(grouping) = &spec.grouping {
            c.has_grouping = 1;
            c.grouping = self.grouping_to_c(grouping);
        }
        c
    }

    /// `view` in C form, copied into `axis`'s buffers.
    fn axis_to_c(&mut self, axis: Axis, view: &AxisView) -> BinnacleAxisView {
        let sort = &mut self.sorts[axis.index()];
        sort.clear();
        sort.extend(view.sort.columns().iter().map(BinnacleSortColumn::from));
        let quick = view.filter.quick.as_ref();
        let text = &mut self.quick_texts[axis.index()];
        let keys = &mut self.quick_keys[axis.index()];
        text.clear();
        keys.clear();
        let mut query = BinnacleTextQuery::default();
        if let Some(q) = quick {
            text.push_str(&q.text);
            query.flags = bit(q.case_sensitive, BINNACLE_TEXT_CASE_SENSITIVE)
                | bit(q.regex, BINNACLE_TEXT_REGEX);
            if let Searched::Keys(ks) = &q.searches {
                query.searched = BINNACLE_SEARCHED_KEYS;
                keys.extend(ks.iter().map(|k| k.0));
            }
            query.text = BinnacleText {
                data: text.as_ptr(),
                len: text.len(),
            };
            query.keys = keys.as_ptr();
            query.keys_len = keys.len();
        }
        let conditions = &mut self.conditions[axis.index()];
        conditions.fill(view.filter.conditions.conditions());
        let hidden = &mut self.hidden[axis.index()];
        hidden.clear();
        hidden.extend(view.hidden.iter().map(|k| k.0));
        BinnacleAxisView {
            sort: sort.as_ptr(),
            sort_len: sort.len(),
            has_quick: flag(quick.is_some()),
            quick: query,
            conditions: conditions.items.as_ptr(),
            conditions_len: conditions.items.len(),
            hidden: hidden.as_ptr(),
            hidden_len: hidden.len(),
        }
    }

    /// `grouping` in C form, copied into the grouping buffers.
    fn grouping_to_c(&mut self, grouping: &Grouping) -> BinnacleGrouping {
        let mut c = BinnacleGrouping::default();
        for axis in Axis::ALL {
            let levels = &mut self.levels[axis.index()];
            levels.clear();
            levels.extend(grouping.levels(axis).iter().map(BinnacleLevel::from));
            let g = grouping.axis(axis);
            let except = &mut self.excepts[axis.index()];
            except.clear();
            except.extend(g.expand.except.iter().map(|k| k.0));
            let (expand_all, expand_depth) = depth_to_c(g.expand.depth);
            c.axes[axis.index()] = BinnacleGroupedAxis {
                levels: levels.as_ptr(),
                levels_len: levels.len(),
                grand_total: placement_tag(g.grand_total),
                form: form_tag(g.form),
                expand_all,
                expand_depth,
                except: except.as_ptr(),
                except_len: except.len(),
            };
        }
        (c.values, c.values_len) = self.values_to_c(&grouping.values);
        self.slicers.fill(&grouping.slicers);
        c.slicers = self.slicers.items.as_ptr();
        c.slicers_len = self.slicers.items.len();
        c
    }

    /// `of` in C form, copied into the aggregates' buffers.
    fn values_to_c(&mut self, of: &[AggregateValue]) -> (*const BinnacleAggregateValue, usize) {
        self.aggregates.fill(of);
        (self.aggregates.items.as_ptr(), self.aggregates.items.len())
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleAxisConfig {
    pub count: u64,
    pub default_size: f32,
    pub header: f32,
    pub gap: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleWindowRequest {
    pub requested: u32,
    pub start: u64,
    pub end: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleWindowRequests {
    pub axes: [BinnacleWindowRequest; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleRange {
    pub start: u64,
    pub end: u64,
}

/// Where a tier lies across its Header Band.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleExtent {
    pub start: f32,
    pub end: f32,
}

/// A level the corner names.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleCornerLevel {
    pub index: u32,
    pub kind: u32,
    pub field: u64,
}

/// One tier of a Header Band where it crosses the corner.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleCornerTier {
    pub across: BinnacleExtent,
    pub levels_start: usize,
    pub levels_len: usize,
}

/// One spanning cell of a tier in header form.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleHeaderSpan {
    pub axis: u32,
    pub tier: u32,
    pub key: u64,
    pub has_field: u32,
    pub field: u64,
    pub open: u32,
    pub first: u64,
    pub last: u64,
    pub rect: BinnacleRect,
    pub visible: BinnacleRect,
    pub cut_before: u32,
    pub cut_after: u32,
}

/// A lit pin zone while headers are dragged.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleZone {
    pub axis: u32,
    pub edge: u32,
    pub offset: f32,
    pub size: f32,
    pub hot: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupOf {
    pub field: u64,
    pub value: BinnacleValue,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleOutline {
    pub kind: u32,
    pub depth: u32,
    pub open: u32,
    pub ancestors: *const u64,
    pub ancestors_len: usize,
    pub has_group: u32,
    pub field: u64,
    pub value: BinnacleValue,
    pub span: u64,
    pub path: *const BinnacleGroupOf,
    pub path_len: usize,
    pub has_set: u32,
    pub set_index: u64,
    pub set_size: u64,
}

impl Default for BinnacleOutline {
    fn default() -> Self {
        BinnacleOutline {
            kind: BINNACLE_ITEM_LEAF,
            depth: 0,
            open: BINNACLE_OPEN_STATE_OPEN,
            ancestors: std::ptr::null(),
            ancestors_len: 0,
            has_group: 0,
            field: 0,
            value: BinnacleValue::default(),
            span: 0,
            path: std::ptr::null(),
            path_len: 0,
            has_set: 0,
            set_index: 0,
            set_size: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleStripItem {
    pub position: u64,
    pub has_key: u32,
    pub key: u64,
    pub offset: f32,
    pub size: f32,
    pub has_outline: u32,
    pub outline: BinnacleOutline,
    pub toggle_pending: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleHeader {
    pub position: u64,
    pub has_key: u32,
    pub key: u64,
    pub rect: BinnacleRect,
    pub flags: u32,
    pub sort_place: u32,
    pub support: u32,
    pub aggregates: *const BinnacleAggregate,
    pub aggregates_len: usize,
}

impl Default for BinnacleHeader {
    fn default() -> Self {
        BinnacleHeader {
            position: 0,
            has_key: 0,
            key: 0,
            rect: BinnacleRect::default(),
            flags: 0,
            sort_place: 0,
            support: 0,
            aggregates: std::ptr::null(),
            aggregates_len: 0,
        }
    }
}

/// An aggregate the Table Source offers over a base field, with the names
/// of the parameters it takes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleAggregate {
    pub name: BinnacleText,
    pub params: *const BinnacleText,
    pub params_len: usize,
}

impl Default for BinnacleAggregate {
    fn default() -> Self {
        BinnacleAggregate {
            name: BinnacleText::default(),
            params: std::ptr::null(),
            params_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleScrollAnchor {
    pub position: u64,
    pub offset: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleThumb {
    pub fraction: f64,
    pub size: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleFocusEnd {
    pub position: u64,
    pub has_key: u32,
    pub key: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleAxisSnapshot {
    pub count: u64,
    pub extent: f32,
    pub header: f32,
    pub gap: f32,
    pub anchor: BinnacleScrollAnchor,
    pub thumb: BinnacleThumb,
    pub strip: *const BinnacleStripItem,
    pub strip_len: usize,
    pub headers: *const BinnacleHeader,
    pub headers_len: usize,
    pub focus: BinnacleFocusEnd,
    pub selection_runs: *const BinnacleRange,
    pub selection_runs_len: usize,
    pub has_total: u32,
    pub total: u64,
    pub quick_filter: u32,
    pub conditions: *const BinnacleListedCondition,
    pub conditions_len: usize,
    pub levels: *const BinnacleListedLevel,
    pub levels_len: usize,
    pub picked: *const u64,
    pub picked_len: usize,
    pub pinned_len: usize,
    pub trailing_len: usize,
    pub scrolling_start: f32,
    pub scrolling_end: f32,
    pub leading_start: f32,
    pub leading_end: f32,
    pub trailing_start: f32,
    pub trailing_end: f32,
    pub movable: u32,
    pub hide_support: u32,
    pub hidden: *const u64,
    pub hidden_len: usize,
    pub hide_cap: usize,
    pub leading_pins: *const u64,
    pub leading_pins_len: usize,
    pub trailing_pins: *const u64,
    pub trailing_pins_len: usize,
    pub tiers: *const BinnacleExtent,
    pub tiers_len: usize,
    pub spans: *const BinnacleHeaderSpan,
    pub spans_len: usize,
    pub corner_tiers: *const BinnacleCornerTier,
    pub corner_tiers_len: usize,
    pub corner_levels: *const BinnacleCornerLevel,
    pub corner_levels_len: usize,
    pub band_focused: u32,
    pub sorts: *const BinnacleListedSort,
    pub sorts_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleSnapshot {
    pub generation: u64,
    pub damage: u32,
    pub axes: [BinnacleAxisSnapshot; 2],
    pub has_focus: u32,
    pub has_focus_rect: u32,
    pub focus_rect: BinnacleRect,
    pub has_selection_range: u32,
    pub selection_rects: *const BinnacleRect,
    pub selection_rects_len: usize,
    pub cell_states: *const u8,
    pub cell_states_len: usize,
    pub mode: u32,
    pub capturing: u32,
    pub has_hovered_header: u32,
    pub hovered_axis: u32,
    pub hovered_index: usize,
    pub has_find: u32,
    pub find: BinnacleFindQuery,
    pub has_find_match: u32,
    pub find_match: BinnacleFindMatch,
    pub find_support: u32,
    pub view_pending: u32,
    pub outline_focused: u32,
    pub has_pending_move: u32,
    pub pending_move_axis: u32,
    pub pending_move_keys: *const u64,
    pub pending_move_keys_len: usize,
    pub has_move_refusal: u32,
    pub move_refusal_axis: u32,
    pub move_refusal: BinnacleText,
    pub has_drop: u32,
    pub drop_axis: u32,
    pub drop_target: u32,
    pub drop_offset: f32,
    pub drop_position: u64,
    pub drop_unpin: u32,
    pub drop_edge: u32,
    pub has_drop_before: u32,
    pub drop_before: u64,
    pub drop_reorder: u32,
    pub drop_refusal: u32,
    pub pin_zones: [BinnacleZone; 2],
    pub pin_zones_len: usize,
    pub slicers: *const BinnacleListedCondition,
    pub slicers_len: usize,
    pub values: *const BinnacleListedValue,
    pub values_len: usize,
    pub focused_tier: u32,
    pub edit_row: u64,
    pub edit_col: u64,
    pub has_editor_rect: u32,
    pub editor_rect: BinnacleRect,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleSortColumn {
    pub key: u64,
    pub direction: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleTextQuery {
    pub text: BinnacleText,
    pub flags: u32,
    pub searched: u32,
    pub keys: *const u64,
    pub keys_len: usize,
}

impl Default for BinnacleTextQuery {
    fn default() -> Self {
        BinnacleTextQuery {
            text: BinnacleText::default(),
            flags: 0,
            searched: BINNACLE_SEARCHED_SHOWN,
            keys: std::ptr::null(),
            keys_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleValue {
    pub blank: u32,
    pub text: BinnacleText,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleFindQuery {
    pub query: BinnacleTextQuery,
    pub by: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleFindMatch {
    pub row: u64,
    pub col: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleScalar {
    pub kind: u32,
    pub number: f64,
    pub date: i64,
}

impl BinnacleScalar {
    fn from_core(s: Scalar) -> Self {
        match s {
            Scalar::Number(number) => BinnacleScalar {
                kind: BINNACLE_SCALAR_NUMBER,
                number,
                date: 0,
            },
            Scalar::Date(date) => BinnacleScalar {
                kind: BINNACLE_SCALAR_DATE,
                number: 0.0,
                date,
            },
        }
    }

    /// The scalar, or `None` for an unknown kind.
    fn to_core(self) -> Option<Scalar> {
        match self.kind {
            BINNACLE_SCALAR_NUMBER => Some(Scalar::Number(self.number)),
            BINNACLE_SCALAR_DATE => Some(Scalar::Date(self.date)),
            _ => None,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleCondition {
    pub key: u64,
    pub kind: u32,
    pub negated: u32,
    pub text: BinnacleTextQuery,
    pub values: *const BinnacleValue,
    pub values_len: usize,
    pub compare: u32,
    pub scalar: BinnacleScalar,
    pub high: BinnacleScalar,
}

impl Default for BinnacleCondition {
    fn default() -> Self {
        BinnacleCondition {
            key: 0,
            kind: BINNACLE_CONDITION_OTHER,
            negated: 0,
            text: BinnacleTextQuery::default(),
            values: std::ptr::null(),
            values_len: 0,
            compare: BINNACLE_COMPARE_EQUAL,
            scalar: BinnacleScalar::default(),
            high: BinnacleScalar::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleListedSort {
    pub state: u32,
    pub column: BinnacleSortColumn,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleListedCondition {
    pub state: u32,
    pub condition: BinnacleCondition,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleListedLevel {
    pub state: u32,
    pub level: BinnacleLevel,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleListedValue {
    pub state: u32,
    pub value: BinnacleAggregateValue,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleAxisView {
    pub sort: *const BinnacleSortColumn,
    pub sort_len: usize,
    pub has_quick: u32,
    pub quick: BinnacleTextQuery,
    pub conditions: *const BinnacleCondition,
    pub conditions_len: usize,
    pub hidden: *const u64,
    pub hidden_len: usize,
}

impl Default for BinnacleAxisView {
    fn default() -> Self {
        BinnacleAxisView {
            sort: std::ptr::null(),
            sort_len: 0,
            has_quick: 0,
            quick: BinnacleTextQuery::default(),
            conditions: std::ptr::null(),
            conditions_len: 0,
            hidden: std::ptr::null(),
            hidden_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleTotal {
    pub known: u32,
    pub count: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleTotals {
    pub axes: [BinnacleTotal; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleLevel {
    pub kind: u32,
    pub field: u64,
    pub subtotal: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleAggregateValue {
    pub has_field: u32,
    pub field: u64,
    pub aggregate: BinnacleText,
    pub params: *const BinnacleText,
    pub params_len: usize,
}

impl Default for BinnacleAggregateValue {
    fn default() -> Self {
        BinnacleAggregateValue {
            has_field: 0,
            field: 0,
            aggregate: BinnacleText::default(),
            params: std::ptr::null(),
            params_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupedAxis {
    pub levels: *const BinnacleLevel,
    pub levels_len: usize,
    pub grand_total: u32,
    pub form: u32,
    pub expand_all: u32,
    pub expand_depth: u32,
    pub except: *const u64,
    pub except_len: usize,
}

impl Default for BinnacleGroupedAxis {
    fn default() -> Self {
        BinnacleGroupedAxis {
            levels: std::ptr::null(),
            levels_len: 0,
            grand_total: BINNACLE_PLACEMENT_OFF,
            form: BINNACLE_FORM_OUTLINE,
            expand_all: 0,
            expand_depth: 0,
            except: std::ptr::null(),
            except_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGrouping {
    pub axes: [BinnacleGroupedAxis; 2],
    pub values: *const BinnacleAggregateValue,
    pub values_len: usize,
    pub slicers: *const BinnacleCondition,
    pub slicers_len: usize,
}

impl Default for BinnacleGrouping {
    fn default() -> Self {
        BinnacleGrouping {
            axes: Default::default(),
            values: std::ptr::null(),
            values_len: 0,
            slicers: std::ptr::null(),
            slicers_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleViewSpec {
    pub axes: [BinnacleAxisView; 2],
    pub has_grouping: u32,
    pub grouping: BinnacleGrouping,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleViewEdit {
    pub kind: u32,
    pub axis: u32,
    pub key: u64,
    pub multi: u32,
    pub keys: *const u64,
    pub keys_len: usize,
    pub expand_all: u32,
    pub expand_depth: u32,
    pub form: u32,
    pub level: BinnacleLevel,
    pub value: BinnacleAggregateValue,
}

impl Default for BinnacleViewEdit {
    fn default() -> Self {
        BinnacleViewEdit {
            kind: BINNACLE_VIEW_EDIT_OTHER,
            axis: 0,
            key: 0,
            multi: 0,
            keys: std::ptr::null(),
            keys_len: 0,
            expand_all: 0,
            expand_depth: 0,
            form: BINNACLE_FORM_OUTLINE,
            level: BinnacleLevel::default(),
            value: BinnacleAggregateValue::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleText {
    pub data: *const u8,
    pub len: usize,
}

impl Default for BinnacleText {
    fn default() -> Self {
        BinnacleText {
            data: std::ptr::null(),
            len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleMoveFocus {
    pub axis: u32,
    pub by: u32,
    pub forward: u32,
    pub extend: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleScroll {
    pub dx: f32,
    pub dy: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleScrollTo {
    pub axis: u32,
    pub fraction: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleScrollBy {
    pub axis: u32,
    pub items: i64,
}

/// What an Action carries beyond its kind; `kind` says which part is set.
#[repr(C)]
#[derive(Clone, Copy)]
pub union BinnacleActionArgs {
    pub type_text: BinnacleText,
    pub move_focus: BinnacleMoveFocus,
    pub scroll: BinnacleScroll,
    pub scroll_to: BinnacleScrollTo,
    pub scroll_by: BinnacleScrollBy,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BinnacleAction {
    pub size: usize,
    pub kind: u32,
    pub args: BinnacleActionArgs,
}

/// Every size a released header gave `BinnacleAction`, oldest first: what
/// [`binnacle_grid_action`] reads.
const ACTION_SIZES: &[usize] = &[size_of::<BinnacleAction>()];

impl Default for BinnacleAction {
    /// This header's size, no kind and every argument zero.
    fn default() -> Self {
        // SAFETY: every field is plain data, for which all-zero bits are valid.
        let mut c: BinnacleAction = unsafe { std::mem::zeroed() };
        c.size = size_of::<BinnacleAction>();
        c
    }
}

impl BinnacleAction {
    /// The Action, or `None` if it is malformed: a size no header had, an
    /// unknown kind, tag or flag, or text that is not UTF-8.
    ///
    /// # Safety
    /// `action` is valid for reading its `size` field and, if that is a
    /// size a header had, that many bytes; type text is valid for its
    /// `len` reads.
    unsafe fn read(action: *const BinnacleAction) -> Option<Action> {
        // SAFETY: the caller's contract.
        let size = unsafe { action.cast::<usize>().as_ref() }.copied()?;
        if !ACTION_SIZES.contains(&size) {
            return None;
        }
        // Fields past an older header's size stay as `default` has them.
        let mut c = BinnacleAction::default();
        // SAFETY: `size` bytes are readable and fit in `c`.
        unsafe {
            std::ptr::copy_nonoverlapping(
                action.cast::<u8>(),
                (&raw mut c).cast::<u8>(),
                size.min(size_of::<BinnacleAction>()),
            );
        }
        let flag = |v: u32| match v {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        // SAFETY (each union read): `kind` names the part the host set, and
        // every part is plain data valid for any bits.
        Some(match c.kind {
            BINNACLE_ACTION_TYPE_TEXT => {
                let t = unsafe { c.args.type_text };
                // SAFETY: the caller's contract.
                let text = std::str::from_utf8(unsafe { borrowed(t.data, t.len) }).ok()?;
                Action::TypeText(text.to_owned())
            }
            BINNACLE_ACTION_ACTIVATE => Action::Activate,
            BINNACLE_ACTION_BEGIN_EDIT => Action::BeginEdit,
            BINNACLE_ACTION_COPY => Action::Copy,
            BINNACLE_ACTION_MOVE_FOCUS => {
                let m = unsafe { c.args.move_focus };
                Action::MoveFocus {
                    axis: axis(m.axis)?,
                    by: match m.by {
                        BINNACLE_FOCUS_ITEM => FocusStep::Item,
                        BINNACLE_FOCUS_PAGE => FocusStep::Page,
                        BINNACLE_FOCUS_EDGE => FocusStep::Edge,
                        _ => return None,
                    },
                    forward: flag(m.forward)?,
                    extend: flag(m.extend)?,
                }
            }
            BINNACLE_ACTION_SELECT_ALL => Action::SelectAll,
            BINNACLE_ACTION_SELECT_COLUMN => Action::SelectColumn,
            BINNACLE_ACTION_SELECT_ROW => Action::SelectRow,
            BINNACLE_ACTION_CANCEL => Action::Cancel,
            BINNACLE_ACTION_SCROLL => {
                let BinnacleScroll { dx, dy } = unsafe { c.args.scroll };
                Action::Scroll { dx, dy }
            }
            BINNACLE_ACTION_SCROLL_TO => {
                let t = unsafe { c.args.scroll_to };
                Action::ScrollTo {
                    axis: axis(t.axis)?,
                    fraction: t.fraction,
                }
            }
            BINNACLE_ACTION_SCROLL_BY => {
                let b = unsafe { c.args.scroll_by };
                Action::ScrollBy {
                    axis: axis(b.axis)?,
                    items: b.items,
                }
            }
            BINNACLE_ACTION_LEAVE_OUTLINE => Action::LeaveOutline,
            BINNACLE_ACTION_FIND_NEXT => Action::FindNext,
            BINNACLE_ACTION_FIND_PREVIOUS => Action::FindPrevious,
            _ => return None,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleEffect {
    pub generation: u64,
    pub kind: u32,
    pub text: BinnacleText,
    pub row: u64,
    pub col: u64,
    pub view: BinnacleViewSpec,
    pub edit: BinnacleViewEdit,
    pub axis: u32,
    pub key: u64,
    pub size: f32,
    pub has_find: u32,
    pub find: BinnacleFindQuery,
    pub has_from: u32,
    pub from: BinnacleFindMatch,
    pub direction: u32,
    pub keys: *const u64,
    pub keys_len: usize,
    pub to: u64,
    pub edge: u32,
    pub has_drill_down: u32,
    pub drill_down: *const BinnacleCondition,
    pub drill_down_len: usize,
    pub copy_runs: [*const BinnacleRange; 2],
    pub copy_runs_len: [usize; 2],
    pub copy_labels: [u32; 2],
}

/// Every Size Override on an axis, in Key order: `keys[i]` has `sizes[i]`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleSizeOverrides {
    pub keys: *const u64,
    pub sizes: *const f32,
    pub len: usize,
}

impl Default for BinnacleEffect {
    fn default() -> Self {
        BinnacleEffect {
            generation: 0,
            kind: BINNACLE_EFFECT_OTHER,
            text: BinnacleText::default(),
            row: 0,
            col: 0,
            view: BinnacleViewSpec::default(),
            edit: BinnacleViewEdit::default(),
            axis: 0,
            key: 0,
            size: 0.0,
            has_find: 0,
            find: BinnacleFindQuery::default(),
            has_from: 0,
            from: BinnacleFindMatch::default(),
            direction: BINNACLE_FIND_NEXT,
            keys: std::ptr::null(),
            keys_len: 0,
            to: 0,
            edge: 0,
            has_drill_down: 0,
            drill_down: std::ptr::null(),
            drill_down_len: 0,
            copy_runs: [std::ptr::null(); 2],
            copy_runs_len: [0; 2],
            copy_labels: [0; 2],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnaclePointerEvent {
    pub kind: u32,
    pub x: f32,
    pub y: f32,
    pub button: u32,
    pub modifiers: u32,
    pub clicks: u32,
}

fn axis(tag: u32) -> Option<Axis> {
    match tag {
        BINNACLE_ROWS => Some(Axis::Rows),
        BINNACLE_COLS => Some(Axis::Cols),
        _ => None,
    }
}

fn axis_tag(axis: Axis) -> u32 {
    match axis {
        Axis::Rows => BINNACLE_ROWS,
        Axis::Cols => BINNACLE_COLS,
    }
}

fn edge_tag(edge: Edge) -> u32 {
    match edge {
        Edge::Leading => BINNACLE_EDGE_LEADING,
        Edge::Trailing => BINNACLE_EDGE_TRAILING,
    }
}

fn flag(b: bool) -> u32 {
    b.into()
}

fn bit<T: Default>(set: bool, mask: T) -> T {
    if set { mask } else { T::default() }
}

/// A borrowed C array as a slice; NULL is empty.
///
/// # Safety
/// A non-NULL `ptr` must be valid for `len` reads.
unsafe fn borrowed<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() {
        &[]
    } else {
        // SAFETY: the caller's contract.
        unsafe { slice::from_raw_parts(ptr, len) }
    }
}

/// Runs `f` on the handle: 1 if it ran and returned `Some`, 0 for NULL or
/// invalid input.
///
/// # Safety
/// `grid` is NULL or a live handle from [`binnacle_grid_new`].
unsafe fn with_handle(
    grid: *mut BinnacleGrid,
    f: impl FnOnce(&mut BinnacleGrid) -> Option<()>,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { grid.as_mut() }) else {
        return 0;
    };
    f(h).is_some().into()
}

/// Writes through an out-parameter, or `None` for NULL.
///
/// # Safety
/// `out` is NULL or valid for a write.
unsafe fn write_out<T>(out: *mut T, value: T) -> Option<()> {
    // SAFETY: the caller's contract.
    *unsafe { out.as_mut() }? = value;
    Some(())
}

impl BinnacleAxisConfig {
    fn to_core(self) -> Option<AxisConfig> {
        let size_ok = self.default_size.is_finite() && self.default_size > 0.0;
        let gap_ok = self.gap.is_finite() && self.gap >= 0.0;
        (size_ok && gap_ok).then(|| {
            AxisConfig::new(self.count, self.default_size)
                .header(self.header)
                .gap(self.gap)
        })
    }
}

impl From<&StripItem> for BinnacleStripItem {
    fn from(item: &StripItem) -> Self {
        BinnacleStripItem {
            position: item.position,
            has_key: flag(item.key.is_some()),
            key: item.key.map_or(0, |k| k.0),
            offset: item.offset,
            size: item.size,
            has_outline: 0,
            outline: BinnacleOutline::default(),
            toggle_pending: 0,
        }
    }
}

/// `outline` in C form, its ancestors already copied to `ancestors`, its
/// group value's text to `text` and its path to `path`.
fn outline_to_c(
    outline: &Outline,
    ancestors: *const u64,
    text: *const u8,
    path: *const BinnacleGroupOf,
) -> BinnacleOutline {
    let (has_group, field, value) = match &outline.group {
        Some(GroupOf { field, value }) => (1, field.0, value_to_c(value, text)),
        None => (0, 0, BinnacleValue::default()),
    };
    BinnacleOutline {
        kind: match outline.kind {
            ItemKind::Leaf => BINNACLE_ITEM_LEAF,
            ItemKind::Group => BINNACLE_ITEM_GROUP,
            ItemKind::Subtotal => BINNACLE_ITEM_SUBTOTAL,
            ItemKind::GrandTotal => BINNACLE_ITEM_GRAND_TOTAL,
        },
        depth: outline.depth,
        open: match outline.open {
            OpenState::Open => BINNACLE_OPEN_STATE_OPEN,
            OpenState::Closed => BINNACLE_OPEN_STATE_CLOSED,
            OpenState::CantOpen => BINNACLE_OPEN_STATE_CANT_OPEN,
        },
        ancestors,
        ancestors_len: outline.ancestors.len(),
        has_group,
        field,
        value,
        span: outline.span,
        path,
        path_len: outline.path.len(),
        has_set: u32::from(outline.set.is_some()),
        set_index: outline.set.map_or(0, |s| s.index),
        set_size: outline.set.map_or(0, |s| s.size),
    }
}

/// `value` in C form, its text already copied to `text`.
fn value_to_c(value: &Value, text: *const u8) -> BinnacleValue {
    match value {
        Value::Text(t) => BinnacleValue {
            blank: 0,
            text: BinnacleText {
                data: text,
                len: t.len(),
            },
        },
        Value::Blank => BinnacleValue {
            blank: 1,
            text: BinnacleText::default(),
        },
    }
}

impl BinnacleOutline {
    /// The outline, or `None` if a tag or flag is unknown or the group
    /// value's text is not UTF-8.
    ///
    /// # Safety
    /// A non-NULL `ancestors` is valid for `ancestors_len` reads, a non-NULL
    /// `path` for `path_len`, and each value's text as
    /// [`BinnacleValue::to_core`] requires.
    unsafe fn to_core(self) -> Option<Outline> {
        let kind = match self.kind {
            BINNACLE_ITEM_LEAF => ItemKind::Leaf,
            BINNACLE_ITEM_GROUP => ItemKind::Group,
            BINNACLE_ITEM_SUBTOTAL => ItemKind::Subtotal,
            BINNACLE_ITEM_GRAND_TOTAL => ItemKind::GrandTotal,
            _ => return None,
        };
        let open = match self.open {
            BINNACLE_OPEN_STATE_OPEN => OpenState::Open,
            BINNACLE_OPEN_STATE_CLOSED => OpenState::Closed,
            BINNACLE_OPEN_STATE_CANT_OPEN => OpenState::CantOpen,
            _ => return None,
        };
        let group = match self.has_group {
            0 => None,
            1 => Some(GroupOf {
                field: Key(self.field),
                // SAFETY: the caller's contract.
                value: unsafe { self.value.to_core() }?,
            }),
            _ => return None,
        };
        let set = match self.has_set {
            0 => None,
            1 => Some(SetPosition {
                index: self.set_index,
                size: self.set_size,
            }),
            _ => return None,
        };
        // SAFETY: the caller's contract.
        let ancestors = unsafe { borrowed(self.ancestors, self.ancestors_len) };
        // SAFETY: the caller's contract.
        let path = unsafe { borrowed(self.path, self.path_len) };
        let path = (path.iter())
            .map(|g| {
                Some(GroupOf {
                    field: Key(g.field),
                    // SAFETY: the caller's contract.
                    value: unsafe { g.value.to_core() }?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let mut outline = Outline::leaf(ancestors.iter().map(|&k| Key(k)).collect())
            .with_span(self.span)
            .with_path(path);
        outline.kind = kind;
        outline.depth = self.depth;
        outline.open = open;
        outline.group = group;
        outline.set = set;
        Some(outline)
    }
}

impl From<Rect> for BinnacleRect {
    fn from(r: Rect) -> Self {
        BinnacleRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }
    }
}

/// The form a `BINNACLE_FORM_*` tag names.
fn form(tag: u32) -> Option<Form> {
    match tag {
        BINNACLE_FORM_OUTLINE => Some(Form::Outline),
        BINNACLE_FORM_HEADER => Some(Form::Header),
        _ => None,
    }
}

fn form_tag(form: Form) -> u32 {
    match form {
        Form::Outline => BINNACLE_FORM_OUTLINE,
        Form::Header => BINNACLE_FORM_HEADER,
    }
}

/// `BINNACLE_COMPARE_*`, the scalar and, for between, the high end.
fn comparison_to_c(comparison: &Comparison) -> (u32, BinnacleScalar, BinnacleScalar) {
    let one = |op, s| (op, BinnacleScalar::from_core(s), BinnacleScalar::default());
    match *comparison {
        Comparison::Equal(s) => one(BINNACLE_COMPARE_EQUAL, s),
        Comparison::NotEqual(s) => one(BINNACLE_COMPARE_NOT_EQUAL, s),
        Comparison::Less(s) => one(BINNACLE_COMPARE_LESS, s),
        Comparison::AtMost(s) => one(BINNACLE_COMPARE_AT_MOST, s),
        Comparison::Greater(s) => one(BINNACLE_COMPARE_GREATER, s),
        Comparison::AtLeast(s) => one(BINNACLE_COMPARE_AT_LEAST, s),
        Comparison::Between(low, high) => (
            BINNACLE_COMPARE_BETWEEN,
            BinnacleScalar::from_core(low),
            BinnacleScalar::from_core(high),
        ),
    }
}

/// `BINNACLE_SUPPORT_*` bits for an item's support.
fn support_bits(support: Support) -> u32 {
    let compare = match support.compare {
        None => 0,
        Some(Compared::Numbers) => BINNACLE_SUPPORT_COMPARE_NUMBERS,
        Some(Compared::Dates) => BINNACLE_SUPPORT_COMPARE_DATES,
    };
    compare
        | bit(support.sort, BINNACLE_SUPPORT_SORT)
        | bit(support.one_of, BINNACLE_SUPPORT_ONE_OF)
        | bit(support.group, BINNACLE_SUPPORT_GROUP)
        | bit(support.native, BINNACLE_SUPPORT_NATIVE)
        | bit(support.pivot, BINNACLE_SUPPORT_PIVOT)
        | bit(support.subtotals.before, BINNACLE_SUPPORT_SUBTOTAL_BEFORE)
        | bit(support.subtotals.after, BINNACLE_SUPPORT_SUBTOTAL_AFTER)
        | support.text.map_or(0, |t| {
            BINNACLE_SUPPORT_TEXT | bit(t.regex, BINNACLE_SUPPORT_TEXT_REGEX)
        })
}

impl From<HeaderPlacement> for BinnacleHeader {
    fn from(h: HeaderPlacement) -> Self {
        let (direction, place) = h.sort.applied.unzip();
        BinnacleHeader {
            position: h.position,
            has_key: flag(h.key.is_some()),
            key: h.key.map_or(0, |k| k.0),
            rect: h.rect.into(),
            flags: bit(direction.is_some(), BINNACLE_HEADER_SORTED)
                | bit(
                    direction == Some(SortDirection::Descending),
                    BINNACLE_HEADER_DESCENDING,
                )
                | bit(h.sort.pending, BINNACLE_HEADER_PENDING)
                | bit(h.selected, BINNACLE_HEADER_SELECTED)
                | bit(h.hovered, BINNACLE_HEADER_HOVERED)
                | bit(h.filter.filtered, BINNACLE_HEADER_FILTERED)
                | bit(h.filter.pending, BINNACLE_HEADER_FILTER_PENDING)
                | bit(h.filter.not_applied, BINNACLE_HEADER_FILTER_NOT_APPLIED)
                | bit(h.move_pending, BINNACLE_HEADER_MOVE_PENDING),
            sort_place: place.map_or(0, |p| u32::try_from(p).unwrap_or(u32::MAX)),
            ..BinnacleHeader::default()
        }
    }
}

impl From<&SortColumn> for BinnacleSortColumn {
    fn from(c: &SortColumn) -> Self {
        BinnacleSortColumn {
            key: c.key.0,
            direction: match c.direction {
                SortDirection::Ascending => BINNACLE_SORT_ASCENDING,
                SortDirection::Descending => BINNACLE_SORT_DESCENDING,
            },
        }
    }
}

impl BinnacleSortColumn {
    fn to_core(self) -> Option<SortColumn> {
        let direction = match self.direction {
            BINNACLE_SORT_ASCENDING => SortDirection::Ascending,
            BINNACLE_SORT_DESCENDING => SortDirection::Descending,
            _ => return None,
        };
        Some(SortColumn {
            key: Key(self.key),
            direction,
        })
    }
}

impl BinnacleTextQuery {
    /// The Text Query, or `None` if the text is not UTF-8 or a tag or flag
    /// is unknown.
    ///
    /// # Safety
    /// A non-NULL `text.data` is valid for `text.len` reads and a non-NULL
    /// `keys` for `keys_len`.
    unsafe fn to_core(self) -> Option<TextQuery> {
        if self.flags & !(BINNACLE_TEXT_CASE_SENSITIVE | BINNACLE_TEXT_REGEX) != 0 {
            return None;
        }
        // SAFETY: the caller's contract.
        let text = std::str::from_utf8(unsafe { borrowed(self.text.data, self.text.len) }).ok()?;
        let searches = match self.searched {
            BINNACLE_SEARCHED_SHOWN => Searched::Shown,
            // SAFETY: the caller's contract.
            BINNACLE_SEARCHED_KEYS => Searched::Keys(
                unsafe { borrowed(self.keys, self.keys_len) }
                    .iter()
                    .map(|&k| Key(k))
                    .collect(),
            ),
            _ => return None,
        };
        Some(
            TextQuery::new(text)
                .case_sensitive(self.flags & BINNACLE_TEXT_CASE_SENSITIVE != 0)
                .regex(self.flags & BINNACLE_TEXT_REGEX != 0)
                .searching(searches),
        )
    }
}

/// `query` in C form, its text and Keys copied into `(text, keys)`.
fn find_to_c(query: &FindQuery, (text, keys): &mut (String, Vec<u64>)) -> BinnacleFindQuery {
    let q = &query.query;
    text.clear();
    text.push_str(&q.text);
    keys.clear();
    let mut c = BinnacleTextQuery {
        text: BinnacleText {
            data: text.as_ptr(),
            len: text.len(),
        },
        flags: bit(q.case_sensitive, BINNACLE_TEXT_CASE_SENSITIVE)
            | bit(q.regex, BINNACLE_TEXT_REGEX),
        ..BinnacleTextQuery::default()
    };
    if let Searched::Keys(ks) = &q.searches {
        keys.extend(ks.iter().map(|k| k.0));
        c.searched = BINNACLE_SEARCHED_KEYS;
        c.keys = keys.as_ptr();
        c.keys_len = keys.len();
    }
    BinnacleFindQuery {
        query: c,
        by: axis_tag(query.by),
    }
}

impl BinnacleFindQuery {
    /// The Find Query, or `None` if its text is not UTF-8 or a tag or flag
    /// is unknown.
    ///
    /// # Safety
    /// As [`BinnacleTextQuery::to_core`] for `query`.
    unsafe fn to_core(self) -> Option<FindQuery> {
        // SAFETY: the caller's contract.
        let query = unsafe { self.query.to_core() }?;
        Some(FindQuery::new(query).by(axis(self.by)?))
    }
}

impl From<FindMatch> for BinnacleFindMatch {
    fn from(m: FindMatch) -> Self {
        BinnacleFindMatch {
            row: m.row.0,
            col: m.col.0,
        }
    }
}

/// `BINNACLE_FIND*` bits for find's support.
fn find_bits(support: Option<TextSupport>) -> u32 {
    support.map_or(0, |s| BINNACLE_FIND | bit(s.regex, BINNACLE_FIND_REGEX))
}

impl BinnacleCondition {
    /// The condition, or `None` if its kind or a tag or flag is unknown or
    /// its text is not UTF-8. A Text condition searches its own item,
    /// whatever its query names.
    ///
    /// # Safety
    /// As [`BinnacleTextQuery::to_core`] for `text`; a non-NULL `values` is
    /// valid for `values_len` reads, each value's text as for `text`.
    unsafe fn to_core(self) -> Option<Condition> {
        let not = match self.negated {
            0 => false,
            1 => true,
            _ => return None,
        };
        match self.kind {
            // SAFETY: the caller's contract.
            BINNACLE_CONDITION_TEXT => {
                Some(Condition::text(Key(self.key), unsafe { self.text.to_core() }?).not(not))
            }
            BINNACLE_CONDITION_ONE_OF => {
                // SAFETY: the caller's contract.
                let values = unsafe { borrowed(self.values, self.values_len) }
                    .iter()
                    // SAFETY: the caller's contract.
                    .map(|v| unsafe { v.to_core() })
                    .collect::<Option<Vec<_>>>()?;
                Some(Condition::one_of(Key(self.key), values).not(not))
            }
            BINNACLE_CONDITION_COMPARE => {
                let s = self.scalar.to_core()?;
                let comparison = match self.compare {
                    BINNACLE_COMPARE_EQUAL => Comparison::Equal(s),
                    BINNACLE_COMPARE_NOT_EQUAL => Comparison::NotEqual(s),
                    BINNACLE_COMPARE_LESS => Comparison::Less(s),
                    BINNACLE_COMPARE_AT_MOST => Comparison::AtMost(s),
                    BINNACLE_COMPARE_GREATER => Comparison::Greater(s),
                    BINNACLE_COMPARE_AT_LEAST => Comparison::AtLeast(s),
                    BINNACLE_COMPARE_BETWEEN => Comparison::Between(s, self.high.to_core()?),
                    _ => return None,
                };
                Some(Condition::compare(Key(self.key), comparison).not(not))
            }
            BINNACLE_CONDITION_NATIVE => {
                // SAFETY: the caller's contract.
                let bytes = unsafe { borrowed(self.text.text.data, self.text.text.len) };
                let text = std::str::from_utf8(bytes).ok()?;
                Some(Condition::native(Key(self.key), text).not(not))
            }
            _ => None,
        }
    }
}

impl BinnacleValue {
    /// The value, or `None` if `blank` is not 0 or 1 or the text is not
    /// UTF-8.
    ///
    /// # Safety
    /// A non-NULL `text.data` is valid for `text.len` reads.
    unsafe fn to_core(self) -> Option<Value> {
        match self.blank {
            0 => {
                // SAFETY: the caller's contract.
                let bytes = unsafe { borrowed(self.text.data, self.text.len) };
                Some(Value::Text(std::str::from_utf8(bytes).ok()?.to_owned()))
            }
            1 => Some(Value::Blank),
            _ => None,
        }
    }
}

/// The placement a C tag names, if any.
fn placement(tag: u32) -> Option<Placement> {
    match tag {
        BINNACLE_PLACEMENT_OFF => Some(Placement::Off),
        BINNACLE_PLACEMENT_BEFORE => Some(Placement::Before),
        BINNACLE_PLACEMENT_AFTER => Some(Placement::After),
        _ => None,
    }
}

fn placement_tag(placement: Placement) -> u32 {
    match placement {
        Placement::Off => BINNACLE_PLACEMENT_OFF,
        Placement::Before => BINNACLE_PLACEMENT_BEFORE,
        Placement::After => BINNACLE_PLACEMENT_AFTER,
    }
}

impl From<&Level> for BinnacleLevel {
    fn from(level: &Level) -> Self {
        #[allow(unreachable_patterns)] // Levels added to the Grid later are OTHER.
        let (kind, field, subtotal) = match level {
            Level::Field { field, subtotal } => {
                (BINNACLE_LEVEL_FIELD, field.0, placement_tag(*subtotal))
            }
            Level::Values => (BINNACLE_LEVEL_VALUES, 0, BINNACLE_PLACEMENT_OFF),
            _ => (BINNACLE_LEVEL_OTHER, 0, BINNACLE_PLACEMENT_OFF),
        };
        BinnacleLevel {
            kind,
            field,
            subtotal,
        }
    }
}

impl BinnacleLevel {
    /// The level, or `None` if its kind or placement is unknown; a VALUES
    /// level ignores both of its other fields.
    fn to_core(self) -> Option<Level> {
        match self.kind {
            BINNACLE_LEVEL_FIELD => Some(Level::Field {
                field: Key(self.field),
                subtotal: placement(self.subtotal)?,
            }),
            BINNACLE_LEVEL_VALUES => Some(Level::Values),
            _ => None,
        }
    }
}

impl BinnacleAggregateValue {
    /// The value, or `None` if `has_field` is not 0 or 1 or the aggregate or
    /// a parameter is not UTF-8.
    ///
    /// # Safety
    /// A non-NULL `aggregate.data` is valid for `aggregate.len` reads, a
    /// non-NULL `params` for `params_len`, and each parameter's `data` as
    /// `aggregate.data` is.
    unsafe fn to_core(self) -> Option<AggregateValue> {
        let field = match self.has_field {
            0 => None,
            1 => Some(Key(self.field)),
            _ => return None,
        };
        let text = |t: BinnacleText| {
            // SAFETY: the caller's contract.
            let bytes = unsafe { borrowed(t.data, t.len) };
            std::str::from_utf8(bytes).ok().map(str::to_owned)
        };
        // SAFETY: the caller's contract.
        let params = unsafe { borrowed(self.params, self.params_len) };
        let params = params
            .iter()
            .map(|&p| text(p))
            .collect::<Option<Vec<_>>>()?;
        Some(AggregateValue::new(field, text(self.aggregate)?).with_params(params))
    }
}

impl BinnacleGrouping {
    /// The grouping, or `None` if a tag or flag is unknown or text is not
    /// UTF-8.
    ///
    /// # Safety
    /// Each non-NULL `levels` is valid for `levels_len` reads, `except` for
    /// `except_len`, `values` for `values_len` (each aggregate as
    /// [`BinnacleAggregateValue::to_core`] requires) and `slicers` for
    /// `slicers_len` (each as [`BinnacleCondition::to_core`] requires).
    unsafe fn to_core(self) -> Option<Grouping> {
        let mut grouping = Grouping::default();
        for axis in Axis::ALL {
            let a = self.axes[axis.index()];
            // SAFETY: the caller's contract.
            *grouping.levels_mut(axis) = unsafe { borrowed(a.levels, a.levels_len) }
                .iter()
                .map(|l| l.to_core())
                .collect::<Option<Vec<_>>>()?;
            let depth = match a.expand_all {
                0 => Depth::Levels(a.expand_depth),
                1 => Depth::All,
                _ => return None,
            };
            // SAFETY: the caller's contract.
            let except = unsafe { borrowed(a.except, a.except_len) };
            let g = grouping.axis_mut(axis);
            g.grand_total = placement(a.grand_total)?;
            g.form = form(a.form)?;
            g.expand = Expand {
                depth,
                except: except.iter().map(|&k| Key(k)).collect(),
            };
        }
        // SAFETY: the caller's contract.
        grouping.values = unsafe { borrowed(self.values, self.values_len) }
            .iter()
            // SAFETY: the caller's contract.
            .map(|v| unsafe { v.to_core() })
            .collect::<Option<Vec<_>>>()?;
        // SAFETY: the caller's contract.
        grouping.slicers = unsafe { borrowed(self.slicers, self.slicers_len) }
            .iter()
            // SAFETY: the caller's contract.
            .map(|c| unsafe { c.to_core() })
            .collect::<Option<Vec<_>>>()?;
        Some(grouping)
    }
}

impl BinnacleViewSpec {
    /// The View Spec, or `None` if a direction, tag or flag is unknown or
    /// text is not UTF-8.
    ///
    /// # Safety
    /// Each non-NULL `sort` is valid for `sort_len` reads, `conditions` for
    /// `conditions_len` and `hidden` for `hidden_len`, and each quick filter
    /// present and condition as
    /// [`BinnacleTextQuery::to_core`] requires; a grouping present as
    /// [`BinnacleGrouping::to_core`] requires.
    unsafe fn to_core(self) -> Option<ViewSpec> {
        let grouping = match self.has_grouping {
            0 => None,
            // SAFETY: the caller's contract.
            1 => Some(unsafe { self.grouping.to_core() }?),
            _ => return None,
        };
        let mut spec = ViewSpec::default().with_grouping(grouping);
        for axis in Axis::ALL {
            let view = self.axes[axis.index()];
            // SAFETY: the caller's contract.
            let columns = unsafe { borrowed(view.sort, view.sort_len) }
                .iter()
                .map(|c| c.to_core())
                .collect::<Option<Vec<_>>>()?;
            let quick = match view.has_quick {
                0 => None,
                // SAFETY: the caller's contract.
                1 => Some(unsafe { view.quick.to_core() }?),
                _ => return None,
            };
            // SAFETY: the caller's contract.
            let conditions = unsafe { borrowed(view.conditions, view.conditions_len) }
                .iter()
                // SAFETY: the caller's contract.
                .map(|c| unsafe { c.to_core() })
                .collect::<Option<Vec<_>>>()?;
            let filter = FilterSpec::default()
                .with_quick(quick)
                .with_conditions(conditions.into());
            // SAFETY: the caller's contract.
            let hidden = unsafe { borrowed(view.hidden, view.hidden_len) };
            spec = spec
                .with_sort(axis, SortSpec::new(columns))
                .with_filter(axis, filter)
                .with_hidden(axis, hidden.iter().map(|&k| Key(k)).collect());
        }
        Some(spec)
    }
}

impl BinnacleTotals {
    fn to_core(self) -> Totals {
        let total = |a: Axis| {
            let t = self.axes[a.index()];
            (t.known != 0).then_some(t.count)
        };
        Totals {
            rows: total(Axis::Rows),
            cols: total(Axis::Cols),
        }
    }
}

/// `BINNACLE_QUICK_FILTER*` bits for a quick filter's support.
fn quick_filter_bits(support: Option<TextSupport>) -> u32 {
    support.map_or(0, |s| {
        BINNACLE_QUICK_FILTER | bit(s.regex, BINNACLE_QUICK_FILTER_REGEX)
    })
}

impl From<FocusEnd> for BinnacleFocusEnd {
    fn from(end: FocusEnd) -> Self {
        BinnacleFocusEnd {
            position: end.position,
            has_key: flag(end.key.is_some()),
            key: end.key.map_or(0, |k| k.0),
        }
    }
}

impl BinnaclePointerEvent {
    fn to_core(self) -> Option<PointerEvent> {
        let kind = match self.kind {
            BINNACLE_POINTER_DOWN => PointerKind::Down,
            BINNACLE_POINTER_MOVE => PointerKind::Move,
            BINNACLE_POINTER_UP => PointerKind::Up,
            _ => return None,
        };
        let button = match self.button {
            BINNACLE_BUTTON_PRIMARY => Button::Primary,
            BINNACLE_BUTTON_SECONDARY => Button::Secondary,
            BINNACLE_BUTTON_MIDDLE => Button::Middle,
            _ => return None,
        };
        let all = BINNACLE_MOD_SHIFT | BINNACLE_MOD_CTRL | BINNACLE_MOD_ALT | BINNACLE_MOD_META;
        if self.modifiers & !all != 0 {
            return None;
        }
        let held = |bit| self.modifiers & bit != 0;
        Some(PointerEvent {
            kind,
            x: self.x,
            y: self.y,
            button,
            modifiers: Modifiers {
                shift: held(BINNACLE_MOD_SHIFT),
                ctrl: held(BINNACLE_MOD_CTRL),
                alt: held(BINNACLE_MOD_ALT),
                meta: held(BINNACLE_MOD_META),
            },
            clicks: self.clicks,
        })
    }
}

/// The [`BINNACLE_ABI_VERSION`] this library was built with.
#[unsafe(no_mangle)]
pub extern "C" fn binnacle_abi_version() -> u32 {
    BINNACLE_ABI_VERSION
}

/// A new Grid, or NULL if either config is invalid. Free it with
/// [`binnacle_grid_free`].
#[unsafe(no_mangle)]
pub extern "C" fn binnacle_grid_new(
    rows: BinnacleAxisConfig,
    cols: BinnacleAxisConfig,
) -> *mut BinnacleGrid {
    let (Some(rows), Some(cols)) = (rows.to_core(), cols.to_core()) else {
        return std::ptr::null_mut();
    };
    Box::into_raw(BinnacleGrid::from_grid(Grid::new(rows, cols)))
}

/// The embedding API, for a Rust host that drives the Grid itself (pumping
/// it with a Table Source) while its native layer uses the same handle
/// through C.
impl BinnacleGrid {
    /// A handle over `grid`. `Box::into_raw` gives the pointer C uses, as
    /// [`binnacle_grid_new`] returns it, freed by [`binnacle_grid_free`].
    pub fn from_grid(grid: Grid) -> Box<BinnacleGrid> {
        Box::new(BinnacleGrid {
            grid,
            passed_on: VecDeque::new(),
            strips: Default::default(),
            ancestors: Default::default(),
            outline_texts: Default::default(),
            paths: Default::default(),
            tiers: Default::default(),
            spans: Default::default(),
            corner_tiers: Default::default(),
            corner_levels: Default::default(),
            headers: Default::default(),
            picked: Default::default(),
            runs: Default::default(),
            rects: Vec::new(),
            cells: Vec::new(),
            text: String::new(),
            view: CView::default(),
            drill_down: Default::default(),
            copy_runs: Default::default(),
            edit_keys: Vec::new(),
            edit_value: Default::default(),
            hidden_listed: Default::default(),
            listed: Default::default(),
            listed_states: Default::default(),
            listed_sorts: Default::default(),
            held: Vec::new(),
            listed_slicers: Default::default(),
            listed_levels: Default::default(),
            listed_values: Default::default(),
            offered: Default::default(),
            find_effect: Default::default(),
            find_snapshot: Default::default(),
            move_keys: Vec::new(),
            pins: Default::default(),
            overrides: Default::default(),
            pending_move: Vec::new(),
            refusal: String::new(),
            loaded: None,
            layout_json: Vec::new(),
        })
    }

    /// The Grid this handle drives.
    pub fn grid_mut(&mut self) -> &mut Grid {
        &mut self.grid
    }

    /// Hands `f` the Grid and a sink for the Effects it passes on, in
    /// order: [`binnacle_grid_poll_effect`] returns them before any the Grid
    /// still holds. For a pump whose `seen` borrows the Grid, passing on
    /// what the pump did not answer.
    pub fn pump_with<T>(&mut self, f: impl FnOnce(&mut Grid, &mut dyn FnMut(Stamped)) -> T) -> T {
        let passed_on = &mut self.passed_on;
        f(&mut self.grid, &mut |stamped| passed_on.push_back(stamped))
    }
}

/// # Safety
/// `grid` is NULL or a live handle from [`binnacle_grid_new`], not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_free(grid: *mut BinnacleGrid) {
    if !grid.is_null() {
        // SAFETY: the caller's contract.
        drop(unsafe { Box::from_raw(grid) });
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_selection_unit(
    grid: *mut BinnacleGrid,
    unit: u32,
) -> i32 {
    let unit = match unit {
        BINNACLE_SELECT_CELLS => SelectionUnit::Cells,
        BINNACLE_SELECT_ROWS => SelectionUnit::Rows,
        BINNACLE_SELECT_COLS => SelectionUnit::Cols,
        _ => return 0,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_selection_unit(unit);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_header_click(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    click: u32,
) -> i32 {
    let click = match click {
        BINNACLE_HEADER_CLICK_SORT => HeaderClick::Sort,
        BINNACLE_HEADER_CLICK_SELECT => HeaderClick::Select,
        BINNACLE_HEADER_CLICK_NONE => HeaderClick::None,
        _ => return 0,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_header_click(axis, click))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_drag_threshold(
    grid: *mut BinnacleGrid,
    threshold: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_drag_threshold(threshold);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_begin_frame(
    grid: *mut BinnacleGrid,
    width: f32,
    height: f32,
    out: *mut BinnacleWindowRequests,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let mut requests = BinnacleWindowRequests::default();
            for (axis, range) in h.grid.begin_frame(Viewport::new(width, height)) {
                requests.axes[axis.index()] = BinnacleWindowRequest {
                    requested: 1,
                    start: range.start,
                    end: range.end,
                };
            }
            write_out(out, requests)
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; a non-NULL `keys` is valid for `len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_window(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    start: u64,
    keys: *const u64,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_window(axis, start, &keys))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; non-NULL `keys` and `sizes` are each valid
/// for `len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_window_sized(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    start: u64,
    keys: *const u64,
    sizes: *const f32,
    len: usize,
    min_size: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    let sizes = unsafe { borrowed(sizes, len) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_window_sized(axis, start, &keys, sizes, min_size))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; a non-NULL `outlines` is valid for `len`
/// reads, each as [`BinnacleOutline::to_core`] requires.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_outlines(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    start: u64,
    outlines: *const BinnacleOutline,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let outlines = unsafe { borrowed(outlines, len) }
        .iter()
        // SAFETY: the caller's contract.
        .map(|o| unsafe { o.to_core() })
        .collect::<Option<Vec<_>>>();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_outlines(axis(axis_tag)?, start, &outlines?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_size(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    size: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_size(axis, Key(key), size))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_pin(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    edge_tag: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let edge = edge(edge_tag)?;
            axis(axis_tag).map(|axis| h.grid.pin(axis, edge, Key(key)))
        })
    }
}

/// Lists into `keys` and `len` every Key of `axis` the Grid keeps state on
/// apart from its Windows; valid until the next call on `grid`.
///
/// # Safety
/// `grid` is NULL or a live handle; `keys` and `len` are NULL or valid for
/// a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_held_keys(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    keys: *mut *const u64,
    len: *mut usize,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let axis = axis(axis_tag)?;
            h.held = h.grid.held_keys(axis).iter().map(|k| k.0).collect();
            write_out(keys, h.held.as_ptr())?;
            write_out(len, h.held.len())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `keys` is valid for `len` reads (or NULL
/// with `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_forget(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    keys: *const u64,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.forget(axis, &keys))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `keys` is valid for `len` reads (or NULL
/// with `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_pins(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    edge_tag: u32,
    keys: *const u64,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let edge = edge(edge_tag)?;
            axis(axis_tag).map(|axis| h.grid.set_pins(axis, edge, &keys))
        })
    }
}

/// A host's ID for an item, as the UTF-8 text of its JSON (`"name"`, `7`
/// or `["ns","a"]`), and the Key it gives the item.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleNamedKey {
    pub id: BinnacleText,
    pub key: u64,
}

/// The IDs and Keys of one axis's items, for a Layout.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleAxisNames {
    pub names: *const BinnacleNamedKey,
    pub names_len: usize,
}

impl Default for BinnacleAxisNames {
    fn default() -> Self {
        BinnacleAxisNames {
            names: std::ptr::null(),
            names_len: 0,
        }
    }
}

/// How a host names a table's items, per axis.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleNames {
    pub axes: [BinnacleAxisNames; 2],
}

/// The host's names as an ID table, or `None` for NULL or an ID that is not
/// the JSON of an ID.
///
/// # Safety
/// `names` is NULL or valid for a read, each axis's `names` valid for
/// `names_len` reads (or NULL with 0) and each ID's text for its length.
unsafe fn id_table(names: *const BinnacleNames) -> Option<IdTable<JsonId, JsonId>> {
    // SAFETY: the caller's contract.
    let names = unsafe { names.as_ref() }?;
    let axis = |a: BinnacleAxisNames| -> Option<Vec<(JsonId, Key)>> {
        // SAFETY: the caller's contract.
        (unsafe { borrowed(a.names, a.names_len) }.iter())
            .map(|n| {
                // SAFETY: the caller's contract.
                let text = unsafe { borrowed(n.id.data, n.id.len) };
                Some((JsonId::from_json(text)?, Key(n.key)))
            })
            .collect()
    };
    let [rows, cols] = names.axes;
    Some(IdTable::new(axis(rows)?, axis(cols)?))
}

/// # Safety
/// `grid` is NULL or a live handle; `names` as for [`id_table`]; `json` is
/// valid for `len` reads (or NULL with `len` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_load_layout(
    grid: *mut BinnacleGrid,
    names: *const BinnacleNames,
    json: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(ids) = (unsafe { id_table(names) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    let Ok(layout) = Layout::<JsonId, JsonId>::from_json(unsafe { borrowed(json, len) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            layout.restore(&mut h.grid, &ids);
            h.loaded = Some(layout);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `names` as for [`id_table`]; `out` is
/// NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_save_layout(
    grid: *mut BinnacleGrid,
    names: *const BinnacleNames,
    out: *mut BinnacleText,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    let Some(ids) = (unsafe { id_table(names) }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let mut layout = Layout::read(&mut h.grid, &ids);
            if let Some(loaded) = &h.loaded {
                layout = layout.keeping(loaded, &ids);
            }
            h.layout_json = layout.to_json();
            write_out(
                out,
                BinnacleText {
                    data: h.layout_json.as_ptr(),
                    len: h.layout_json.len(),
                },
            )
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_size_overrides(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    out: *mut BinnacleSizeOverrides,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let axis = axis(axis_tag)?;
            let (keys, sizes) = &mut h.overrides;
            keys.clear();
            sizes.clear();
            for (key, size) in h.grid.size_overrides(axis) {
                keys.push(key.0);
                sizes.push(size);
            }
            out.write(BinnacleSizeOverrides {
                keys: keys.as_ptr(),
                sizes: sizes.as_ptr(),
                len: keys.len(),
            });
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_freeze(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.freeze(axis, Key(key)))
        })
    }
}

/// The edge a C tag names, if any.
fn edge(tag: u32) -> Option<Edge> {
    match tag {
        BINNACLE_EDGE_LEADING => Some(Edge::Leading),
        BINNACLE_EDGE_TRAILING => Some(Edge::Trailing),
        _ => None,
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_unpin(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.unpin(axis, Key(key)))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_header(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    thickness: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_header(axis, thickness))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; a non-NULL `sizes` is valid for `len`
/// reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_tiers(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    sizes: *const f32,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let sizes = unsafe { borrowed(sizes, len) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_tiers(axis, sizes))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_form(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    form_tag: u32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_form(axis(axis_tag)?, form(form_tag)?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_pin_zone(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    size: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_pin_zone(axis, size))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_min_scrolling(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    length: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_min_scrolling(axis, length))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_default_size(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    size: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_default_size(axis, size))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_end_gap(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    gap: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_end_gap(axis, gap))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_edge_gap(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    gap: f32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_edge_gap(axis, gap))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_snapshot(
    grid: *mut BinnacleGrid,
    out: *mut BinnacleSnapshot,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| write_out(out, h.snapshot())) }
}

impl BinnacleGrid {
    /// Takes the Snapshot and copies it into the handle's buffers.
    fn snapshot(&mut self) -> BinnacleSnapshot {
        let snap = self.grid.snapshot();
        let damage = snap.damage();
        for axis in Axis::ALL {
            let strip = &mut self.strips[axis.index()];
            let ancestors = &mut self.ancestors[axis.index()];
            let texts = &mut self.outline_texts[axis.index()];
            let paths = &mut self.paths[axis.index()];
            strip.clear();
            ancestors.clear();
            texts.clear();
            paths.clear();
            // Ancestors and texts first, then the paths, then the items, so
            // the pointers taken after stay valid.
            let mut spans = Vec::new();
            let mut path_texts = Vec::new();
            for item in snap.drawn(axis) {
                let outline = snap.outline(axis, item.position).map(|o| {
                    let (keys, text) = (ancestors.len(), texts.len());
                    ancestors.extend(o.ancestors.iter().map(|k| k.0));
                    if let Some(GroupOf {
                        value: Value::Text(t),
                        ..
                    }) = &o.group
                    {
                        texts.push_str(t);
                    }
                    for g in &o.path {
                        path_texts.push(texts.len());
                        if let Value::Text(t) = &g.value {
                            texts.push_str(t);
                        }
                    }
                    (o, keys, text)
                });
                spans.push((item, outline));
            }
            let mut at = path_texts.into_iter();
            for (_, outline) in &spans {
                for g in outline.iter().flat_map(|(o, ..)| &o.path) {
                    let text = texts.as_ptr().wrapping_add(at.next().unwrap_or(0));
                    paths.push(BinnacleGroupOf {
                        field: g.field.0,
                        value: value_to_c(&g.value, text),
                    });
                }
            }
            let mut path = 0;
            strip.extend(spans.into_iter().map(|(item, outline)| {
                let Some((o, keys, text)) = outline else {
                    return BinnacleStripItem::from(item);
                };
                let first = path;
                path += o.path.len();
                let pending = item.key.is_some_and(|k| snap.toggle_pending(axis, k));
                BinnacleStripItem {
                    has_outline: 1,
                    toggle_pending: flag(pending),
                    outline: outline_to_c(
                        o,
                        ancestors.as_ptr().wrapping_add(keys),
                        texts.as_ptr().wrapping_add(text),
                        paths.as_ptr().wrapping_add(first),
                    ),
                    ..BinnacleStripItem::from(item)
                }
            }));
            let headers = &mut self.headers[axis.index()];
            headers.clear();
            let placed: Vec<HeaderPlacement> = snap.headers(axis).collect();
            let offered: Vec<&[Aggregate]> = (placed.iter())
                .map(|h| h.key.map_or(&[][..], |k| snap.aggregates(axis.other(), k)))
                .collect();
            let c = &mut self.offered[axis.index()];
            let spans = c.fill(&offered);
            headers.extend(placed.into_iter().zip(spans).map(|(h, (at, len))| {
                let support = h
                    .key
                    .map_or(0, |k| support_bits(snap.support(axis.other(), k)));
                BinnacleHeader {
                    support,
                    aggregates: c.items.as_ptr().wrapping_add(at),
                    aggregates_len: len,
                    ..BinnacleHeader::from(h)
                }
            }));
            let hidden = &mut self.hidden_listed[axis.index()];
            hidden.clear();
            hidden.extend(snap.hidden(axis).iter().map(|k| k.0));
            let sorts = &mut self.listed_sorts[axis.index()];
            sorts.clear();
            sorts.extend(
                snap.sorts(axis)
                    .iter()
                    .map(|(c, state)| BinnacleListedSort {
                        state: state_tag(*state),
                        column: c.into(),
                    }),
            );
            let listed = snap.conditions(axis);
            let c = &mut self.listed[axis.index()];
            c.fill(listed.iter().map(|(c, _)| c));
            let states = &mut self.listed_states[axis.index()];
            states.clear();
            states.extend(listed.iter().zip(&c.items).map(|((_, state), &condition)| {
                BinnacleListedCondition {
                    state: state_tag(*state),
                    condition,
                }
            }));
            let levels = &mut self.listed_levels[axis.index()];
            levels.clear();
            levels.extend(
                snap.levels(axis)
                    .iter()
                    .map(|(l, state)| BinnacleListedLevel {
                        state: state_tag(*state),
                        level: BinnacleLevel::from(l),
                    }),
            );
        }
        let slicers = snap.slicers();
        let (c, states) = &mut self.listed_slicers;
        c.fill(slicers.iter().map(|(c, _)| c));
        states.clear();
        states.extend(
            slicers
                .iter()
                .zip(&c.items)
                .map(|((_, state), &condition)| BinnacleListedCondition {
                    state: state_tag(*state),
                    condition,
                }),
        );
        let values = snap.values();
        let (c, states) = &mut self.listed_values;
        c.fill(values.iter().map(|(v, _)| v));
        states.clear();
        states.extend(values.iter().zip(&c.items).map(|((_, state), &value)| {
            BinnacleListedValue {
                state: state_tag(*state),
                value,
            }
        }));
        for axis in Axis::ALL {
            let picked = &mut self.picked[axis.index()];
            picked.clear();
            picked.extend(snap.picked(axis).iter().map(|k| k.0));
            for edge in Edge::ALL {
                let pins = &mut self.pins[axis.index()][edge.index()];
                pins.clear();
                pins.extend(snap.pins(axis, edge).map(|k| k.0));
            }
        }
        self.rects.clear();
        self.rects.extend(
            snap.selection_rects()
                .iter()
                .map(|&r| BinnacleRect::from(r)),
        );
        self.cells.clear();
        for row in snap.drawn(Axis::Rows) {
            for col in snap.drawn(Axis::Cols) {
                let state = snap.cell_state(row, col);
                self.cells.push(
                    bit(state.focused, BINNACLE_CELL_FOCUSED)
                        | bit(state.selected, BINNACLE_CELL_SELECTED)
                        | bit(state.current_match, BINNACLE_CELL_CURRENT_MATCH)
                        | bit(state.editing, BINNACLE_CELL_EDITING),
                );
            }
        }
        for axis in Axis::ALL {
            let tiers = &mut self.tiers[axis.index()];
            tiers.clear();
            tiers.extend((snap.tiers(axis).iter()).map(|t| BinnacleExtent {
                start: t.start,
                end: t.end,
            }));
            let spans = &mut self.spans[axis.index()];
            spans.clear();
            spans.extend(snap.spans(axis).iter().map(|s| BinnacleHeaderSpan {
                axis: axis_tag(s.axis),
                tier: s.tier,
                key: s.key.0,
                has_field: flag(s.field.is_some()),
                field: s.field.map_or(0, |k| k.0),
                open: match s.open {
                    OpenState::Open => BINNACLE_OPEN_STATE_OPEN,
                    OpenState::Closed => BINNACLE_OPEN_STATE_CLOSED,
                    OpenState::CantOpen => BINNACLE_OPEN_STATE_CANT_OPEN,
                },
                first: s.first,
                last: s.last,
                rect: BinnacleRect::from(s.rect),
                visible: BinnacleRect::from(s.visible),
                cut_before: flag(s.cut_before),
                cut_after: flag(s.cut_after),
            }));
            let (tiers, levels) = (
                &mut self.corner_tiers[axis.index()],
                &mut self.corner_levels[axis.index()],
            );
            tiers.clear();
            levels.clear();
            for t in snap.corner().tiers(axis) {
                tiers.push(BinnacleCornerTier {
                    across: BinnacleExtent {
                        start: t.across.start,
                        end: t.across.end,
                    },
                    levels_start: levels.len(),
                    levels_len: t.levels.len(),
                });
                levels.extend(t.levels.iter().map(|l| BinnacleCornerLevel {
                    index: l.index,
                    kind: match l.field {
                        Some(_) => BINNACLE_LEVEL_FIELD,
                        None => BINNACLE_LEVEL_VALUES,
                    },
                    field: l.field.map_or(0, |k| k.0),
                }));
            }
        }
        let range = snap.selection_range();
        for axis in Axis::ALL {
            let runs = &mut self.runs[axis.index()];
            runs.clear();
            if let Some(r) = range {
                runs.extend((r.span(axis).iter()).map(|r| BinnacleRange {
                    start: r.start,
                    end: r.end,
                }));
            }
        }
        let axis_snapshot = |axis: Axis| {
            let anchor = snap.scroll_anchor(axis);
            let thumb = snap.thumb(axis);
            let strip = &self.strips[axis.index()];
            let headers = &self.headers[axis.index()];
            let listed = &self.listed_states[axis.index()];
            let levels = &self.listed_levels[axis.index()];
            let picked = &self.picked[axis.index()];
            let hidden = &self.hidden_listed[axis.index()];
            let runs = &self.runs[axis.index()];
            let [leading_pins, trailing_pins] = &self.pins[axis.index()];
            let tiers = &self.tiers[axis.index()];
            let spans = &self.spans[axis.index()];
            let corner_tiers = &self.corner_tiers[axis.index()];
            let corner_levels = &self.corner_levels[axis.index()];
            let sorts = &self.listed_sorts[axis.index()];
            BinnacleAxisSnapshot {
                count: snap.count(axis),
                extent: snap.extent(axis),
                header: snap.header(axis),
                gap: snap.gap(axis),
                anchor: BinnacleScrollAnchor {
                    position: anchor.position,
                    offset: anchor.offset,
                },
                thumb: BinnacleThumb {
                    fraction: thumb.fraction,
                    size: thumb.size,
                },
                strip: strip.as_ptr(),
                strip_len: strip.len(),
                headers: headers.as_ptr(),
                headers_len: headers.len(),
                focus: snap
                    .focus(axis)
                    .map(BinnacleFocusEnd::from)
                    .unwrap_or_default(),
                has_total: flag(snap.total(axis).is_some()),
                total: snap.total(axis).unwrap_or(0),
                quick_filter: quick_filter_bits(snap.quick_filter(axis)),
                conditions: listed.as_ptr(),
                conditions_len: listed.len(),
                levels: levels.as_ptr(),
                levels_len: levels.len(),
                picked: picked.as_ptr(),
                picked_len: picked.len(),
                pinned_len: snap.pinned(axis, Edge::Leading).len(),
                trailing_len: snap.pinned(axis, Edge::Trailing).len(),
                scrolling_start: snap.scrolling(axis).start,
                scrolling_end: snap.scrolling(axis).end,
                leading_start: snap.pinned_region(axis, Edge::Leading).start,
                leading_end: snap.pinned_region(axis, Edge::Leading).end,
                trailing_start: snap.pinned_region(axis, Edge::Trailing).start,
                trailing_end: snap.pinned_region(axis, Edge::Trailing).end,
                movable: flag(snap.movable(axis)),
                hide_support: flag(snap.hide_support(axis)),
                hidden: hidden.as_ptr(),
                hidden_len: hidden.len(),
                hide_cap: snap.hide_cap(axis),
                leading_pins: leading_pins.as_ptr(),
                leading_pins_len: leading_pins.len(),
                trailing_pins: trailing_pins.as_ptr(),
                trailing_pins_len: trailing_pins.len(),
                tiers: tiers.as_ptr(),
                tiers_len: tiers.len(),
                spans: spans.as_ptr(),
                spans_len: spans.len(),
                corner_tiers: corner_tiers.as_ptr(),
                corner_tiers_len: corner_tiers.len(),
                corner_levels: corner_levels.as_ptr(),
                corner_levels_len: corner_levels.len(),
                band_focused: flag(snap.band_focused(axis)),
                sorts: sorts.as_ptr(),
                sorts_len: sorts.len(),
                selection_runs: runs.as_ptr(),
                selection_runs_len: runs.len(),
            }
        };
        let hovered = snap.hovered_header().and_then(|(axis, item)| {
            let i = snap.drawn(axis).position(|s| s.position == item.position)?;
            Some((axis, i))
        });
        let find = snap.find().map(|f| find_to_c(f, &mut self.find_snapshot));
        let find_match = snap.find_match();
        let focus_rect = snap.focus_rect();
        let pending_move = snap.pending_move();
        self.pending_move.clear();
        if let Some((_, keys)) = pending_move {
            self.pending_move.extend(keys.iter().map(|k| k.0));
        }
        let refusal = snap.move_refusal();
        self.refusal.clear();
        if let Some((_, reason)) = refusal {
            self.refusal.push_str(reason);
        }
        #[allow(unreachable_patterns)] // Modes added to the Grid later are OTHER.
        let mode = match snap.mode() {
            Mode::Idle => BINNACLE_MODE_IDLE,
            Mode::DraggingSelection => BINNACLE_MODE_DRAGGING_SELECTION,
            Mode::ResizingBand { .. } => BINNACLE_MODE_RESIZING_BAND,
            Mode::MovingHeaders { .. } => BINNACLE_MODE_MOVING_HEADERS,
            Mode::Editing { .. } => BINNACLE_MODE_EDITING,
            _ => BINNACLE_MODE_OTHER,
        };
        let edited = match snap.mode() {
            Mode::Editing { row, col } => (row.0, col.0),
            _ => (0, 0),
        };
        let editor_rect = snap.editor_rect();
        let drop = snap.drop_indicator();
        let mut drop_refusal = BINNACLE_MOVE_REFUSAL_OTHER;
        let (target, position, unpin, edge, before, reorder) = match drop.map(|d| d.target) {
            Some(DropTarget::Move { to, unpin }) => {
                (BINNACLE_DROP_MOVE, to, unpin, None, None, false)
            }
            Some(DropTarget::Refused { to, why }) => {
                #[allow(unreachable_patterns)] // Refusals added later are OTHER.
                let why = match why {
                    MoveRefusal::BetweenGroups => BINNACLE_MOVE_REFUSAL_BETWEEN_GROUPS,
                    MoveRefusal::Summary => BINNACLE_MOVE_REFUSAL_SUMMARY,
                    _ => BINNACLE_MOVE_REFUSAL_OTHER,
                };
                drop_refusal = why;
                (BINNACLE_DROP_REFUSED, to, false, None, None, false)
            }
            Some(DropTarget::Unpin { at }) => (BINNACLE_DROP_UNPIN, at, false, None, None, false),
            Some(DropTarget::Pin {
                edge,
                before,
                reorder,
            }) => (BINNACLE_DROP_PIN, 0, false, Some(edge), before, reorder),
            Some(DropTarget::Stay) | None => (0, 0, false, None, None, false),
        };
        let mut pin_zones = [BinnacleZone::default(); 2];
        let zones = snap.pin_zones();
        for (out, z) in pin_zones.iter_mut().zip(&zones) {
            *out = BinnacleZone {
                axis: axis_tag(z.axis),
                edge: edge_tag(z.edge),
                offset: z.offset,
                size: z.size,
                hot: flag(z.hot),
            };
        }
        BinnacleSnapshot {
            generation: snap.generation(),
            damage: bit(damage.layout, BINNACLE_DAMAGE_LAYOUT)
                | bit(damage.selection, BINNACLE_DAMAGE_SELECTION)
                | bit(damage.positions, BINNACLE_DAMAGE_POSITIONS),
            axes: Axis::ALL.map(axis_snapshot),
            has_focus: flag(snap.focus(Axis::Rows).is_some()),
            has_focus_rect: flag(focus_rect.is_some()),
            focus_rect: focus_rect.map(BinnacleRect::from).unwrap_or_default(),
            has_selection_range: flag(range.is_some()),
            selection_rects: self.rects.as_ptr(),
            selection_rects_len: self.rects.len(),
            cell_states: self.cells.as_ptr(),
            cell_states_len: self.cells.len(),
            mode,
            capturing: flag(snap.capturing()),
            has_hovered_header: flag(hovered.is_some()),
            hovered_axis: hovered.map_or(0, |(axis, _)| axis_tag(axis)),
            hovered_index: hovered.map_or(0, |(_, i)| i),
            has_find: flag(find.is_some()),
            find: find.unwrap_or_default(),
            has_find_match: flag(find_match.is_some()),
            find_match: find_match.map(BinnacleFindMatch::from).unwrap_or_default(),
            find_support: find_bits(snap.find_support()),
            view_pending: flag(snap.view_pending()),
            outline_focused: flag(snap.outline_focused()),
            has_pending_move: flag(pending_move.is_some()),
            pending_move_axis: pending_move.map_or(0, |(axis, _)| axis_tag(axis)),
            pending_move_keys: self.pending_move.as_ptr(),
            pending_move_keys_len: self.pending_move.len(),
            has_move_refusal: flag(refusal.is_some()),
            move_refusal_axis: refusal.map_or(0, |(axis, _)| axis_tag(axis)),
            move_refusal: BinnacleText {
                data: self.refusal.as_ptr(),
                len: self.refusal.len(),
            },
            has_drop: flag(drop.is_some()),
            drop_axis: drop.map_or(0, |d| axis_tag(d.axis)),
            drop_target: target,
            drop_offset: drop.map_or(0.0, |d| d.offset),
            drop_position: position,
            drop_unpin: flag(unpin),
            drop_edge: edge.map_or(0, edge_tag),
            has_drop_before: flag(before.is_some()),
            drop_before: before.map_or(0, |k| k.0),
            drop_reorder: flag(reorder),
            drop_refusal,
            pin_zones,
            pin_zones_len: zones.len().min(2),
            slicers: self.listed_slicers.1.as_ptr(),
            slicers_len: self.listed_slicers.1.len(),
            values: self.listed_values.1.as_ptr(),
            values_len: self.listed_values.1.len(),
            focused_tier: snap.focused_tier(),
            edit_row: edited.0,
            edit_col: edited.1,
            has_editor_rect: flag(editor_rect.is_some()),
            editor_rect: editor_rect.map(BinnacleRect::from).unwrap_or_default(),
        }
    }

    /// Takes the oldest Effect, passed-on ones first, keeping its text in
    /// the handle.
    fn poll_effect(&mut self) -> Option<BinnacleEffect> {
        let stamped = match self.passed_on.pop_front() {
            Some(stamped) => stamped,
            None => self.grid.poll_effect()?,
        };
        let mut effect = BinnacleEffect {
            generation: stamped.generation,
            ..BinnacleEffect::default()
        };
        #[allow(unreachable_patterns)] // Effects added to the Grid later are OTHER.
        match stamped.effect {
            Effect::TextTyped { text } => {
                self.text = text;
                effect.kind = BINNACLE_EFFECT_TEXT_TYPED;
                effect.text = BinnacleText {
                    data: self.text.as_ptr(),
                    len: self.text.len(),
                };
            }
            Effect::FocusChanged { row, col } => {
                effect.kind = BINNACLE_EFFECT_FOCUS_CHANGED;
                effect.row = row.0;
                effect.col = col.0;
            }
            Effect::Activated {
                row,
                col,
                drill_down,
            } => {
                effect.kind = BINNACLE_EFFECT_ACTIVATED;
                effect.row = row.0;
                effect.col = col.0;
                if let Some(d) = drill_down {
                    self.drill_down.fill(&d.conditions);
                    effect.has_drill_down = 1;
                    effect.drill_down = self.drill_down.items.as_ptr();
                    effect.drill_down_len = self.drill_down.items.len();
                }
            }
            Effect::CopyRequested {
                rows,
                cols,
                row_labels,
                col_labels,
            } => {
                effect.kind = BINNACLE_EFFECT_COPY_REQUESTED;
                for (i, runs) in [rows, cols].into_iter().enumerate() {
                    let buf = &mut self.copy_runs[i];
                    buf.clear();
                    buf.extend(runs.into_iter().map(|r| BinnacleRange {
                        start: r.start,
                        end: r.end,
                    }));
                    effect.copy_runs[i] = buf.as_ptr();
                    effect.copy_runs_len[i] = buf.len();
                }
                effect.copy_labels = [flag(row_labels), flag(col_labels)];
            }
            Effect::EditRequested { row, col } => {
                effect.kind = BINNACLE_EFFECT_EDIT_REQUESTED;
                effect.row = row.0;
                effect.col = col.0;
            }
            Effect::EditCancelled { row, col } => {
                effect.kind = BINNACLE_EFFECT_EDIT_CANCELLED;
                effect.row = row.0;
                effect.col = col.0;
            }
            Effect::SizeChanged { axis, key, size } => {
                effect.kind = BINNACLE_EFFECT_SIZE_CHANGED;
                effect.axis = axis_tag(axis);
                effect.key = key.0;
                effect.size = size;
            }
            Effect::AutoSizeRequested { axis, key } => {
                effect.kind = BINNACLE_EFFECT_AUTO_SIZE_REQUESTED;
                effect.axis = axis_tag(axis);
                effect.key = key.0;
            }
            Effect::SelectionChanged => effect.kind = BINNACLE_EFFECT_SELECTION_CHANGED,
            Effect::ViewRequested { spec, edit } => {
                effect.kind = BINNACLE_EFFECT_VIEW_REQUESTED;
                effect.view = self.view.fill(&spec);
                let on = |kind, axis| BinnacleViewEdit {
                    kind,
                    axis: axis_tag(axis),
                    ..BinnacleViewEdit::default()
                };
                effect.edit = match edit {
                    ViewEdit::CycleSort { axis, key, multi } => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_CYCLE_SORT,
                        axis: axis_tag(axis),
                        key: key.0,
                        multi: flag(multi),
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::SetQuickFilter { axis, .. } => {
                        on(BINNACLE_VIEW_EDIT_SET_QUICK_FILTER, axis)
                    }
                    ViewEdit::Replace => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_REPLACE,
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::ShownChanged { axis } => on(BINNACLE_VIEW_EDIT_SHOWN_CHANGED, axis),
                    ViewEdit::Reapply => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_REAPPLY,
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::AddCondition { axis, condition } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(BINNACLE_VIEW_EDIT_ADD_CONDITION, axis)
                    },
                    ViewEdit::RemoveCondition { axis, condition } => BinnacleViewEdit {
                        key: condition.key.0,
                        ..on(BINNACLE_VIEW_EDIT_REMOVE_CONDITION, axis)
                    },
                    ViewEdit::ReplaceCondition { axis, to, .. } => BinnacleViewEdit {
                        key: to.key.0,
                        ..on(BINNACLE_VIEW_EDIT_REPLACE_CONDITION, axis)
                    },
                    ViewEdit::Hide { axis, keys } => {
                        self.edit_keys = keys.iter().map(|k| k.0).collect();
                        BinnacleViewEdit {
                            keys: self.edit_keys.as_ptr(),
                            keys_len: self.edit_keys.len(),
                            ..on(BINNACLE_VIEW_EDIT_HIDE, axis)
                        }
                    }
                    ViewEdit::Show { axis, keys } => {
                        self.edit_keys = keys.iter().map(|k| k.0).collect();
                        BinnacleViewEdit {
                            keys: self.edit_keys.as_ptr(),
                            keys_len: self.edit_keys.len(),
                            ..on(BINNACLE_VIEW_EDIT_SHOW, axis)
                        }
                    }
                    ViewEdit::Group { axis, field } => BinnacleViewEdit {
                        key: field.0,
                        ..on(BINNACLE_VIEW_EDIT_GROUP, axis)
                    },
                    ViewEdit::Ungroup { axis } => on(BINNACLE_VIEW_EDIT_UNGROUP, axis),
                    ViewEdit::Expand { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(BINNACLE_VIEW_EDIT_EXPAND, axis)
                    },
                    ViewEdit::Collapse { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(BINNACLE_VIEW_EDIT_COLLAPSE, axis)
                    },
                    ViewEdit::ExpandAll { axis, key } => BinnacleViewEdit {
                        key: key.0,
                        ..on(BINNACLE_VIEW_EDIT_EXPAND_ALL, axis)
                    },
                    ViewEdit::AddSlicer { condition } => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_ADD_SLICER,
                        key: condition.key.0,
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::RemoveSlicer { condition } => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_REMOVE_SLICER,
                        key: condition.key.0,
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::ReplaceSlicer { to, .. } => BinnacleViewEdit {
                        kind: BINNACLE_VIEW_EDIT_REPLACE_SLICER,
                        key: to.key.0,
                        ..BinnacleViewEdit::default()
                    },
                    ViewEdit::ExpandTo { axis, depth } => {
                        let (expand_all, expand_depth) = depth_to_c(depth);
                        BinnacleViewEdit {
                            expand_all,
                            expand_depth,
                            ..on(BINNACLE_VIEW_EDIT_EXPAND_TO, axis)
                        }
                    }
                    ViewEdit::SetForm { axis, form } => BinnacleViewEdit {
                        form: form_tag(form),
                        ..on(BINNACLE_VIEW_EDIT_SET_FORM, axis)
                    },
                    ViewEdit::RemoveLevel { axis, field } => BinnacleViewEdit {
                        level: level_named(field),
                        ..on(BINNACLE_VIEW_EDIT_REMOVE_LEVEL, axis)
                    },
                    ViewEdit::MoveLevel { axis, field } => BinnacleViewEdit {
                        level: level_named(field),
                        ..on(BINNACLE_VIEW_EDIT_MOVE_LEVEL, axis)
                    },
                    ViewEdit::AddValue { value } => {
                        self.edit_value.fill([&value]);
                        BinnacleViewEdit {
                            kind: BINNACLE_VIEW_EDIT_ADD_VALUE,
                            value: self.edit_value.items[0],
                            ..BinnacleViewEdit::default()
                        }
                    }
                    ViewEdit::RemoveValue { value } => {
                        self.edit_value.fill([&value]);
                        BinnacleViewEdit {
                            kind: BINNACLE_VIEW_EDIT_REMOVE_VALUE,
                            value: self.edit_value.items[0],
                            ..BinnacleViewEdit::default()
                        }
                    }
                    _ => BinnacleViewEdit::default(),
                };
            }
            Effect::Locate { axis, key } => {
                effect.kind = BINNACLE_EFFECT_LOCATE;
                effect.axis = axis_tag(axis);
                effect.key = key.0;
            }
            Effect::BandSizeChanged { axis, thickness } => {
                effect.kind = BINNACLE_EFFECT_BAND_SIZE_CHANGED;
                effect.axis = axis_tag(axis);
                effect.size = thickness;
            }
            Effect::BandAutoSizeRequested { axis } => {
                effect.kind = BINNACLE_EFFECT_BAND_AUTO_SIZE_REQUESTED;
                effect.axis = axis_tag(axis);
            }
            Effect::FindRequested { query } => {
                effect.kind = BINNACLE_EFFECT_FIND_REQUESTED;
                if let Some(query) = query {
                    effect.has_find = 1;
                    effect.find = find_to_c(&query, &mut self.find_effect);
                }
            }
            Effect::FindStepRequested {
                query,
                from,
                direction,
            } => {
                effect.kind = BINNACLE_EFFECT_FIND_STEP_REQUESTED;
                effect.has_find = 1;
                effect.find = find_to_c(&query, &mut self.find_effect);
                effect.has_from = flag(from.is_some());
                effect.from = from.map(BinnacleFindMatch::from).unwrap_or_default();
                effect.direction = match direction {
                    FindDirection::Next => BINNACLE_FIND_NEXT,
                    FindDirection::Previous => BINNACLE_FIND_PREVIOUS,
                };
            }
            Effect::MoveRequested { axis, keys, to } => {
                self.move_keys.clear();
                self.move_keys.extend(keys.iter().map(|k| k.0));
                effect.kind = BINNACLE_EFFECT_MOVE_REQUESTED;
                effect.axis = axis_tag(axis);
                effect.keys = self.move_keys.as_ptr();
                effect.keys_len = self.move_keys.len();
                effect.to = to;
            }
            Effect::PinsChanged { axis, edge, keys } => {
                self.move_keys.clear();
                self.move_keys.extend(keys.iter().map(|k| k.0));
                effect.kind = BINNACLE_EFFECT_PINS_CHANGED;
                effect.axis = axis_tag(axis);
                effect.edge = edge_tag(edge);
                effect.keys = self.move_keys.as_ptr();
                effect.keys_len = self.move_keys.len();
            }
            _ => effect.kind = BINNACLE_EFFECT_OTHER,
        }
        Some(effect)
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_poll_effect(
    grid: *mut BinnacleGrid,
    out: *mut BinnacleEffect,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| write_out(out, h.poll_effect()?)) }
}

/// A View Spec from C: NULL is the base view.
///
/// # Safety
/// `spec` is NULL or valid for a read, with arrays as
/// [`BinnacleViewSpec::to_core`] requires.
unsafe fn view_spec(spec: *const BinnacleViewSpec) -> Option<ViewSpec> {
    // SAFETY: the caller's contract.
    match unsafe { spec.as_ref() } {
        None => Some(ViewSpec::default()),
        // SAFETY: the caller's contract.
        Some(spec) => unsafe { spec.to_core() },
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `spec` is NULL (the base view) or valid
/// for a read, each of its non-NULL arrays for its length's reads; `totals`
/// is NULL (none known) or valid for a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_view_applied(
    grid: *mut BinnacleGrid,
    spec: *const BinnacleViewSpec,
    totals: *const BinnacleTotals,
) -> i32 {
    // SAFETY: the caller's contract.
    let spec = unsafe { view_spec(spec) };
    // SAFETY: the caller's contract.
    let totals = unsafe { totals.as_ref() }.map_or_else(Totals::default, |t| t.to_core());
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| spec.map(|s| h.grid.view_applied(s, totals))) }
}

/// # Safety
/// As for [`binnacle_grid_view_applied`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_view_changed(
    grid: *mut BinnacleGrid,
    spec: *const BinnacleViewSpec,
    totals: *const BinnacleTotals,
) -> i32 {
    // SAFETY: the caller's contract.
    let spec = unsafe { view_spec(spec) };
    // SAFETY: the caller's contract.
    let totals = unsafe { totals.as_ref() }.map_or_else(Totals::default, |t| t.to_core());
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| spec.map(|s| h.grid.view_changed(s, totals))) }
}

/// # Safety
/// `grid` is NULL or a live handle; `spec` as for
/// [`binnacle_grid_view_applied`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_request_view(
    grid: *mut BinnacleGrid,
    spec: *const BinnacleViewSpec,
) -> i32 {
    // SAFETY: the caller's contract.
    let spec = unsafe { view_spec(spec) };
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| spec.map(|s| h.grid.request_view(s))) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_group_by(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    field: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.group_by(axis, Key(field)))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_cycle_sort(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    multi: u32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let multi = match multi {
                0 => false,
                1 => true,
                _ => return None,
            };
            axis(axis_tag).map(|axis| h.grid.cycle_sort(axis, Key(key), multi))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_ungroup(grid: *mut BinnacleGrid, axis_tag: u32) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| axis(axis_tag).map(|axis| h.grid.ungroup(axis))) }
}

/// The level grouping by `field`, or the values pseudo-level for `None`, as
/// a View Edit names it.
fn level_named(field: Option<Key>) -> BinnacleLevel {
    match field {
        Some(f) => BinnacleLevel::from(&Level::field(f)),
        None => BinnacleLevel::from(&Level::Values),
    }
}

/// The field a level from C groups by (`Some(None)` for the values
/// pseudo-level), or `None` if NULL or of an unknown kind; its Subtotal is
/// ignored.
///
/// # Safety
/// `level` is NULL or valid for a read.
unsafe fn level_field(level: *const BinnacleLevel) -> Option<Option<Key>> {
    // SAFETY: the caller's contract.
    let level = unsafe { level.as_ref() }?;
    match level.kind {
        BINNACLE_LEVEL_FIELD => Some(Some(Key(level.field))),
        BINNACLE_LEVEL_VALUES => Some(None),
        _ => None,
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `level` is NULL or valid for a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_remove_level(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    level: *const BinnacleLevel,
) -> i32 {
    // SAFETY: the caller's contract.
    let field = unsafe { level_field(level) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.remove_level(axis(axis_tag)?, field?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `level` is NULL or valid for a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_move_level(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    level: *const BinnacleLevel,
) -> i32 {
    // SAFETY: the caller's contract.
    let field = unsafe { level_field(level) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.move_level(axis(axis_tag)?, field?);
            Some(())
        })
    }
}

/// A value from C, or `None` if NULL or invalid.
///
/// # Safety
/// `value` is NULL or valid for a read, and as
/// [`BinnacleAggregateValue::to_core`] requires.
unsafe fn aggregate_value(value: *const BinnacleAggregateValue) -> Option<AggregateValue> {
    // SAFETY: the caller's contract.
    unsafe { value.as_ref()?.to_core() }
}

/// # Safety
/// `grid` is NULL or a live handle; `value` as for [`aggregate_value`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_add_value(
    grid: *mut BinnacleGrid,
    value: *const BinnacleAggregateValue,
) -> i32 {
    // SAFETY: the caller's contract.
    let value = unsafe { aggregate_value(value) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.add_value(value?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `value` as for [`aggregate_value`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_remove_value(
    grid: *mut BinnacleGrid,
    value: *const BinnacleAggregateValue,
) -> i32 {
    // SAFETY: the caller's contract.
    let value = unsafe { aggregate_value(value) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.remove_value(&value?);
            Some(())
        })
    }
}

/// `depth` as `BinnacleGroupedAxis` and `BinnacleViewEdit` carry it: whether
/// every level, then how many.
fn depth_to_c(depth: Depth) -> (u32, u32) {
    match depth {
        Depth::All => (1, 0),
        Depth::Levels(n) => (0, n),
    }
}

/// Runs a toggle on the group headed by `key`: 1, or
/// [`BINNACLE_EXCEPT_CAPPED`] if refused at the cap.
///
/// # Safety
/// `grid` is NULL or a live handle.
unsafe fn toggled(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    f: fn(&mut Grid, Axis, Key) -> Result<(), ExceptCapped>,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { grid.as_mut() }) else {
        return 0;
    };
    match axis(axis_tag).map(|a| f(&mut h.grid, a, Key(key))) {
        None => 0,
        Some(Ok(())) => 1,
        Some(Err(_)) => BINNACLE_EXCEPT_CAPPED,
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_expand(grid: *mut BinnacleGrid, axis: u32, key: u64) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { toggled(grid, axis, key, Grid::expand) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_collapse(
    grid: *mut BinnacleGrid,
    axis: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { toggled(grid, axis, key, Grid::collapse) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_toggle(grid: *mut BinnacleGrid, axis: u32, key: u64) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { toggled(grid, axis, key, Grid::toggle) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_expand_all(
    grid: *mut BinnacleGrid,
    axis: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { toggled(grid, axis, key, Grid::expand_all) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_expand_to(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    all: u32,
    depth: u32,
) -> i32 {
    let depth = match all {
        0 => Some(Depth::Levels(depth)),
        1 => Some(Depth::All),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.expand_to(axis(axis_tag)?, depth?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_reapply_view(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.reapply_view();
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `query` is NULL (no quick filter) or
/// valid for a read, its text for `text.len` reads and its keys for
/// `keys_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_quick_filter(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    query: *const BinnacleTextQuery,
) -> i32 {
    // SAFETY: the caller's contract.
    let query = match unsafe { query.as_ref() } {
        None => Some(None),
        // SAFETY: the caller's contract.
        Some(q) => unsafe { q.to_core() }.map(Some),
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let (axis, query) = (axis(axis_tag)?, query?);
            h.grid.set_quick_filter(axis, query);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_hide_cap(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    cap: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_hide_cap(axis(axis_tag)?, cap);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_hide_support(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    hides: u32,
) -> i32 {
    let hides = match hides {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_hide_support(axis(axis_tag)?, hides?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `keys` is NULL or valid for `len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_hide(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    keys: *const u64,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { grid.as_mut() }) else {
        return 0;
    };
    match axis(axis_tag).map(|a| h.grid.hide(a, &keys)) {
        None => 0,
        Some(Ok(())) => 1,
        Some(Err(_)) => BINNACLE_HIDE_CAPPED,
    }
}

/// # Safety
/// As for [`binnacle_grid_hide`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_show(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    keys: *const u64,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let keys: Vec<Key> = unsafe { borrowed(keys, len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.show(axis(axis_tag)?, &keys);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_quick_filter_support(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    support: u32,
) -> i32 {
    let support = match support {
        0 => Some(None),
        BINNACLE_QUICK_FILTER => Some(Some(TextSupport::new(false))),
        s if s == BINNACLE_QUICK_FILTER | BINNACLE_QUICK_FILTER_REGEX => {
            Some(Some(TextSupport::new(true)))
        }
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let (axis, support) = (axis(axis_tag)?, support?);
            h.grid.set_quick_filter_support(axis, support);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_support(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    support: u32,
) -> i32 {
    let known = BINNACLE_SUPPORT_SORT
        | BINNACLE_SUPPORT_TEXT
        | BINNACLE_SUPPORT_TEXT_REGEX
        | BINNACLE_SUPPORT_ONE_OF
        | BINNACLE_SUPPORT_COMPARE_NUMBERS
        | BINNACLE_SUPPORT_COMPARE_DATES
        | BINNACLE_SUPPORT_GROUP
        | BINNACLE_SUPPORT_NATIVE
        | BINNACLE_SUPPORT_PIVOT
        | BINNACLE_SUPPORT_SUBTOTAL_BEFORE
        | BINNACLE_SUPPORT_SUBTOTAL_AFTER;
    let compare =
        match support & (BINNACLE_SUPPORT_COMPARE_NUMBERS | BINNACLE_SUPPORT_COMPARE_DATES) {
            0 => Some(None),
            BINNACLE_SUPPORT_COMPARE_NUMBERS => Some(Some(Compared::Numbers)),
            BINNACLE_SUPPORT_COMPARE_DATES => Some(Some(Compared::Dates)),
            _ => None,
        };
    let text = match support & (BINNACLE_SUPPORT_TEXT | BINNACLE_SUPPORT_TEXT_REGEX) {
        0 => Some(None),
        BINNACLE_SUPPORT_TEXT => Some(Some(TextSupport::new(false))),
        s if s == BINNACLE_SUPPORT_TEXT | BINNACLE_SUPPORT_TEXT_REGEX => {
            Some(Some(TextSupport::new(true)))
        }
        _ => None,
    };
    let support = (text.zip(compare))
        .filter(|_| support & !known == 0)
        .map(|(text, compare)| {
            Support::default()
                .with_sort(support & BINNACLE_SUPPORT_SORT != 0)
                .with_text(text)
                .with_one_of(support & BINNACLE_SUPPORT_ONE_OF != 0)
                .with_compare(compare)
                .with_group(support & BINNACLE_SUPPORT_GROUP != 0)
                .with_native(support & BINNACLE_SUPPORT_NATIVE != 0)
                .with_pivot(support & BINNACLE_SUPPORT_PIVOT != 0)
                .with_subtotals(Placements {
                    before: support & BINNACLE_SUPPORT_SUBTOTAL_BEFORE != 0,
                    after: support & BINNACLE_SUPPORT_SUBTOTAL_AFTER != 0,
                })
        });
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let (axis, support) = (axis(axis_tag)?, support?);
            h.grid.set_support(axis, Key(key), support);
            Some(())
        })
    }
}

/// A condition from C, or `None` if NULL or invalid.
///
/// # Safety
/// `condition` is NULL or valid for a read, its text for `text.len` reads
/// and its keys for `keys_len`.
unsafe fn condition(condition: *const BinnacleCondition) -> Option<Condition> {
    // SAFETY: the caller's contract.
    unsafe { condition.as_ref()?.to_core() }
}

/// # Safety
/// `grid` is NULL or a live handle; `condition` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_add_condition(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    condition: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let c = unsafe { self::condition(condition) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.add_condition(axis(axis_tag)?, c?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `condition` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_remove_condition(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    condition: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let c = unsafe { self::condition(condition) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.remove_condition(axis(axis_tag)?, &c?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `from` and `to` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_replace_condition(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    from: *const BinnacleCondition,
    to: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let (from, to) = unsafe { (condition(from), condition(to)) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.replace_condition(axis(axis_tag)?, &from?, to?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `condition` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_add_slicer(
    grid: *mut BinnacleGrid,
    condition: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let c = unsafe { self::condition(condition) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.add_slicer(c?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `condition` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_remove_slicer(
    grid: *mut BinnacleGrid,
    condition: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let c = unsafe { self::condition(condition) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.remove_slicer(&c?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `from` and `to` as for [`condition`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_replace_slicer(
    grid: *mut BinnacleGrid,
    from: *const BinnacleCondition,
    to: *const BinnacleCondition,
) -> i32 {
    // SAFETY: the caller's contract.
    let (from, to) = unsafe { (condition(from), condition(to)) };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.replace_slicer(&from?, to?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; a non-NULL `aggregates` is valid for
/// `len` reads, each name and parameter for its `len` and each `params`
/// for its `params_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_aggregates(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    aggregates: *const BinnacleAggregate,
    len: usize,
) -> i32 {
    let text = |t: &BinnacleText| {
        // SAFETY: the caller's contract.
        let bytes = unsafe { borrowed(t.data, t.len) };
        std::str::from_utf8(bytes).ok().map(str::to_owned)
    };
    // SAFETY: the caller's contract.
    let list: Option<Vec<Aggregate>> = unsafe { borrowed(aggregates, len) }
        .iter()
        .map(|a| {
            // SAFETY: the caller's contract.
            let params = unsafe { borrowed(a.params, a.params_len) };
            let params = params.iter().map(text).collect::<Option<Vec<_>>>()?;
            Some(Aggregate::new(text(&a.name)?).with_params(params))
        })
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_aggregates(axis(axis_tag)?, Key(key), list?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_located(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    answer: u32,
    position: u64,
) -> i32 {
    let answer = match answer {
        BINNACLE_LOCATED_FOUND => Located::Found(position),
        BINNACLE_LOCATED_GONE => Located::Gone { suggest: None },
        BINNACLE_LOCATED_GONE_SUGGEST => Located::Gone {
            suggest: Some(position),
        },
        _ => return 0,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.located(axis, Key(key), answer))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_notify(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    notice: u32,
    at: u64,
    n: u64,
) -> i32 {
    let notice = match notice {
        BINNACLE_NOTICE_RESET => ChangeNotice::Reset(n),
        BINNACLE_NOTICE_REORDERED => ChangeNotice::Reordered,
        BINNACLE_NOTICE_INSERTED => ChangeNotice::Inserted { at, n },
        BINNACLE_NOTICE_REMOVED => ChangeNotice::Removed { at, n },
        BINNACLE_NOTICE_CONTENT_CHANGED => ChangeNotice::ContentChanged(at..at.saturating_add(n)),
        BINNACLE_NOTICE_REFILTERED => ChangeNotice::Refiltered(n),
        BINNACLE_NOTICE_COLLAPSED => ChangeNotice::Collapsed { at, n },
        BINNACLE_NOTICE_EXPANDED => ChangeNotice::Expanded { at, n },
        _ => return 0,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.notify(axis, notice))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_pointer(
    grid: *mut BinnacleGrid,
    event: BinnaclePointerEvent,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| event.to_core().map(|e| h.grid.pointer(e))) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_movable(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    movable: u32,
) -> i32 {
    let movable = match movable {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_movable(axis(axis_tag)?, movable?);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; a non-NULL `refusal` is valid for `len`
/// reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_move_answered(
    grid: *mut BinnacleGrid,
    refusal: *const u8,
    len: usize,
) -> i32 {
    let refusal = match refusal.is_null() {
        true => Some(None),
        // SAFETY: the caller's contract.
        false => std::str::from_utf8(unsafe { borrowed(refusal, len) })
            .ok()
            .map(|r| Some(r.to_owned())),
    };
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| refusal.map(|r| h.grid.move_answered(r))) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_edit_started(
    grid: *mut BinnacleGrid,
    row: u64,
    col: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.edit_started(Key(row), Key(col));
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_edit_ended(grid: *mut BinnacleGrid, end: u32) -> i32 {
    let end = match end {
        BINNACLE_EDIT_COMMIT => Some(EditEnd::Commit),
        BINNACLE_EDIT_CANCEL => Some(EditEnd::Cancel),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| end.map(|end| h.grid.edit_ended(end))) }
}

/// Gives the Grid an Action: [`BINNACLE_ACTION_DONE`], or
/// [`BINNACLE_ACTION_REFUSED`] where the Grid's state rules it out
/// (`Grid::refuses`), or 0 for a malformed one.
///
/// # Safety
/// `grid` is NULL or a live handle; `action` is NULL or as
/// `BinnacleAction::read` needs it.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_action(
    grid: *mut BinnacleGrid,
    action: *const BinnacleAction,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { grid.as_mut() }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    let Some(action) = (unsafe { BinnacleAction::read(action) }) else {
        return 0;
    };
    let refused = h.grid.refuses(&action);
    h.grid.action(action);
    match refused {
        true => BINNACLE_ACTION_REFUSED,
        false => BINNACLE_ACTION_DONE,
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_pick(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.pick(axis, Key(key)))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_unpick(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.unpick(axis, Key(key)))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_clear_picks(grid: *mut BinnacleGrid, axis_tag: u32) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.clear_picks(axis))
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_focus(grid: *mut BinnacleGrid, row: u64, col: u64) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.set_focus(Key(row), Key(col));
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_reveal(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    key: u64,
    align: u32,
) -> i32 {
    let align = match align {
        BINNACLE_ALIGN_NEAREST => Some(Align::Nearest),
        BINNACLE_ALIGN_START => Some(Align::Start),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            let (axis, align) = (axis(axis_tag)?, align?);
            h.grid.reveal_key(axis, Key(key), align);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `query` is NULL (close find) or valid
/// for a read, its text for `text.len` reads and its keys for `keys_len`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_find(
    grid: *mut BinnacleGrid,
    query: *const BinnacleFindQuery,
) -> i32 {
    // SAFETY: the caller's contract.
    let query = match unsafe { query.as_ref() } {
        None => Some(None),
        // SAFETY: the caller's contract.
        Some(q) => unsafe { q.to_core() }.map(Some),
    };
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| query.map(|q| h.grid.set_find(q))) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_set_find_support(
    grid: *mut BinnacleGrid,
    support: u32,
) -> i32 {
    let support = match support {
        0 => Some(None),
        BINNACLE_FIND => Some(Some(TextSupport::new(false))),
        s if s == BINNACLE_FIND | BINNACLE_FIND_REGEX => Some(Some(TextSupport::new(true))),
        _ => None,
    };
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| support.map(|s| h.grid.set_find_support(s))) }
}

/// # Safety
/// `grid` is NULL or a live handle; `found` is valid for a read; each
/// ancestors pointer is valid for its length of reads, or NULL with length 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_find_stepped_within(
    grid: *mut BinnacleGrid,
    found: *const BinnacleFindMatch,
    row_ancestors: *const u64,
    row_ancestors_len: usize,
    col_ancestors: *const u64,
    col_ancestors_len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(found) = (unsafe { found.as_ref() }) else {
        return 0;
    };
    let found = FindMatch::new(Key(found.row), Key(found.col));
    // SAFETY: the caller's contract.
    let rows: Vec<Key> = unsafe { borrowed(row_ancestors, row_ancestors_len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    let cols: Vec<Key> = unsafe { borrowed(col_ancestors, col_ancestors_len) }
        .iter()
        .map(|&k| Key(k))
        .collect();
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.find_stepped_within(found, [&rows, &cols]);
            Some(())
        })
    }
}

/// # Safety
/// `grid` is NULL or a live handle; `found` is NULL (no match) or valid for
/// a read.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_find_stepped(
    grid: *mut BinnacleGrid,
    found: *const BinnacleFindMatch,
) -> i32 {
    // SAFETY: the caller's contract.
    let found = unsafe { found.as_ref() }.map(|m| FindMatch::new(Key(m.row), Key(m.col)));
    // SAFETY: the caller's contract.
    unsafe {
        with_handle(grid, |h| {
            h.grid.find_stepped(found);
            Some(())
        })
    }
}
