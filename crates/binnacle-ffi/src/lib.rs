//! Internal C API over the binnacle Grid, for wheelhouse.
//!
//! `include/binnacle.h` is the hand-written header and states the contract;
//! this file mirrors it, constant for constant and struct for struct. Every
//! array a call fills points into buffers the [`BinnacleGrid`] handle owns, so
//! the host never releases anything but the handle. Effects are polled;
//! nothing calls back into the host.

use std::slice;

use binnacle_core::{
    Action, Axis, AxisConfig, Button, ChangeNotice, Condition, ConditionKind, ConditionState, Edge,
    Effect, FilterSpec, FindDirection, FindMatch, FindQuery, FocusEnd, FocusStep, Grid,
    HeaderClick, HeaderPlacement, Key, Located, Mode, Modifiers, PointerEvent, PointerKind, Rect,
    Searched, SelectionUnit, SortColumn, SortDirection, SortSpec, StripItem, Support, TextQuery,
    TextSupport, Totals, Value, ViewEdit, ViewSpec, Viewport,
};

pub const BINNACLE_EDGE_LEADING: u32 = 0;
pub const BINNACLE_EDGE_TRAILING: u32 = 1;
pub const BINNACLE_ROWS: u32 = 0;
pub const BINNACLE_COLS: u32 = 1;

pub const BINNACLE_DAMAGE_LAYOUT: u32 = 1;
pub const BINNACLE_DAMAGE_SELECTION: u32 = 2;
pub const BINNACLE_DAMAGE_POSITIONS: u32 = 4;

pub const BINNACLE_CELL_FOCUSED: u8 = 1;
pub const BINNACLE_CELL_SELECTED: u8 = 2;
pub const BINNACLE_CELL_CURRENT_MATCH: u8 = 4;

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

pub const BINNACLE_CONDITION_OTHER: u32 = 0;
pub const BINNACLE_CONDITION_TEXT: u32 = 1;
pub const BINNACLE_CONDITION_ONE_OF: u32 = 2;

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

pub const BINNACLE_QUICK_FILTER: u32 = 1;
pub const BINNACLE_QUICK_FILTER_REGEX: u32 = 2;

pub const BINNACLE_LOCATED_FOUND: u32 = 0;
pub const BINNACLE_LOCATED_GONE: u32 = 1;
pub const BINNACLE_LOCATED_GONE_SUGGEST: u32 = 2;

pub const BINNACLE_NOTICE_RESET: u32 = 0;
pub const BINNACLE_NOTICE_REORDERED: u32 = 1;
pub const BINNACLE_NOTICE_INSERTED: u32 = 2;
pub const BINNACLE_NOTICE_REMOVED: u32 = 3;
pub const BINNACLE_NOTICE_CONTENT_CHANGED: u32 = 4;
pub const BINNACLE_NOTICE_REFILTERED: u32 = 5;

pub const BINNACLE_FOCUS_ITEM: u32 = 0;
pub const BINNACLE_FOCUS_PAGE: u32 = 1;
pub const BINNACLE_FOCUS_EDGE: u32 = 2;

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
    strips: [Vec<BinnacleStripItem>; 2],
    headers: [Vec<BinnacleHeader>; 2],
    picked: [Vec<u64>; 2],
    /// The last Snapshot's selection range, as runs of Positions per axis.
    runs: [Vec<BinnacleRange>; 2],
    rects: Vec<BinnacleRect>,
    cells: Vec<u8>,
    /// The last polled Effect's text.
    text: String,
    /// The last polled Effect's View Spec: each axis's sort, and its quick
    /// filter's text and Keys.
    sorts: [Vec<BinnacleSortColumn>; 2],
    quick_texts: [String; 2],
    quick_keys: [Vec<u64>; 2],
    conditions: [CConditions; 2],
    hidden: [Vec<u64>; 2],
    /// The last polled Effect's View Edit Keys.
    edit_keys: Vec<u64>,
    /// The last Snapshot's hidden Keys per axis.
    hidden_listed: [Vec<u64>; 2],
    /// The last Snapshot's conditions per axis, with their states.
    listed: [CConditions; 2],
    listed_states: [Vec<BinnacleListedCondition>; 2],
    /// The last polled Effect's Find Query text and Keys, and the last
    /// Snapshot's.
    find_effect: (String, Vec<u64>),
    find_snapshot: (String, Vec<u64>),
    /// The last polled move's Keys.
    move_keys: Vec<u64>,
    /// The last Snapshot's pending move Keys and move refusal.
    pending_move: Vec<u64>,
    refusal: String,
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
                _ => (BINNACLE_CONDITION_OTHER, None, &[][..]),
            };
            let text = self.texts.len();
            let keys = self.keys.len();
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
            self.items.push(BinnacleCondition {
                key: c.key.0,
                kind,
                negated: flag(c.not),
                text: q,
                values: self.values.as_ptr().wrapping_add(listed.start),
                values_len: listed.len(),
            });
        }
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleStripItem {
    pub position: u64,
    pub has_key: u32,
    pub key: u64,
    pub offset: f32,
    pub size: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BinnacleHeader {
    pub position: u64,
    pub has_key: u32,
    pub key: u64,
    pub rect: BinnacleRect,
    pub flags: u32,
    pub sort_place: u32,
    pub support: u32,
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
    pub picked: *const u64,
    pub picked_len: usize,
    pub pinned_len: usize,
    pub trailing_len: usize,
    pub scrolling_start: f32,
    pub scrolling_end: f32,
    pub movable: u32,
    pub hide_support: u32,
    pub hidden: *const u64,
    pub hidden_len: usize,
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
    pub has_pending_move: u32,
    pub pending_move_axis: u32,
    pub pending_move_keys: *const u64,
    pub pending_move_keys_len: usize,
    pub has_move_refusal: u32,
    pub move_refusal_axis: u32,
    pub move_refusal: BinnacleText,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleCondition {
    pub key: u64,
    pub kind: u32,
    pub negated: u32,
    pub text: BinnacleTextQuery,
    pub values: *const BinnacleValue,
    pub values_len: usize,
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
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleListedCondition {
    pub state: u32,
    pub condition: BinnacleCondition,
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
pub struct BinnacleViewSpec {
    pub axes: [BinnacleAxisView; 2],
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
        }
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

/// `BINNACLE_SUPPORT_*` bits for an item's support.
fn support_bits(support: Support) -> u32 {
    bit(support.sort, BINNACLE_SUPPORT_SORT)
        | bit(support.one_of, BINNACLE_SUPPORT_ONE_OF)
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
            support: 0,
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

impl BinnacleViewSpec {
    /// The View Spec, or `None` if a direction, tag or flag is unknown or
    /// text is not UTF-8.
    ///
    /// # Safety
    /// Each non-NULL `sort` is valid for `sort_len` reads, `conditions` for
    /// `conditions_len` and `hidden` for `hidden_len`, and each quick filter
    /// present and condition as
    /// [`BinnacleTextQuery::to_core`] requires.
    unsafe fn to_core(self) -> Option<ViewSpec> {
        let mut spec = ViewSpec::default();
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
    Box::into_raw(Box::new(BinnacleGrid {
        grid: Grid::new(rows, cols),
        strips: Default::default(),
        headers: Default::default(),
        picked: Default::default(),
        runs: Default::default(),
        rects: Vec::new(),
        cells: Vec::new(),
        text: String::new(),
        sorts: [Vec::new(), Vec::new()],
        quick_texts: [String::new(), String::new()],
        quick_keys: [Vec::new(), Vec::new()],
        conditions: Default::default(),
        hidden: Default::default(),
        edit_keys: Vec::new(),
        hidden_listed: Default::default(),
        listed: Default::default(),
        listed_states: Default::default(),
        find_effect: Default::default(),
        find_snapshot: Default::default(),
        move_keys: Vec::new(),
        pending_move: Vec::new(),
        refusal: String::new(),
    }))
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
            strip.clear();
            strip.extend(snap.drawn(axis).map(BinnacleStripItem::from));
            let headers = &mut self.headers[axis.index()];
            headers.clear();
            headers.extend(snap.headers(axis).map(|h| {
                let support = h
                    .key
                    .map_or(0, |k| support_bits(snap.support(axis.other(), k)));
                BinnacleHeader {
                    support,
                    ..BinnacleHeader::from(h)
                }
            }));
            let hidden = &mut self.hidden_listed[axis.index()];
            hidden.clear();
            hidden.extend(snap.hidden(axis).iter().map(|k| k.0));
            let listed = snap.conditions(axis);
            let c = &mut self.listed[axis.index()];
            c.fill(listed.iter().map(|(c, _)| c));
            let states = &mut self.listed_states[axis.index()];
            states.clear();
            states.extend(listed.iter().zip(&c.items).map(|((_, state), &condition)| {
                BinnacleListedCondition {
                    state: match state {
                        ConditionState::Applied => BINNACLE_CONDITION_APPLIED,
                        ConditionState::Adding => BINNACLE_CONDITION_ADDING,
                        ConditionState::Removing => BINNACLE_CONDITION_REMOVING,
                        ConditionState::NotApplied => BINNACLE_CONDITION_NOT_APPLIED,
                    },
                    condition,
                }
            }));
        }
        for axis in Axis::ALL {
            let picked = &mut self.picked[axis.index()];
            picked.clear();
            picked.extend(snap.picked(axis).iter().map(|k| k.0));
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
                        | bit(state.current_match, BINNACLE_CELL_CURRENT_MATCH),
                );
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
            let picked = &self.picked[axis.index()];
            let hidden = &self.hidden_listed[axis.index()];
            let runs = &self.runs[axis.index()];
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
                picked: picked.as_ptr(),
                picked_len: picked.len(),
                pinned_len: snap.pinned(axis, Edge::Leading).len(),
                trailing_len: snap.pinned(axis, Edge::Trailing).len(),
                scrolling_start: snap.scrolling(axis).start,
                scrolling_end: snap.scrolling(axis).end,
                movable: flag(snap.movable(axis)),
                hide_support: flag(snap.hide_support(axis)),
                hidden: hidden.as_ptr(),
                hidden_len: hidden.len(),
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
            _ => BINNACLE_MODE_OTHER,
        };
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
        }
    }

    /// Takes the oldest Effect, keeping its text in the handle.
    fn poll_effect(&mut self) -> Option<BinnacleEffect> {
        let stamped = self.grid.poll_effect()?;
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
            Effect::SelectionChanged => effect.kind = BINNACLE_EFFECT_SELECTION_CHANGED,
            Effect::ViewRequested { spec, edit } => {
                effect.kind = BINNACLE_EFFECT_VIEW_REQUESTED;
                for axis in Axis::ALL {
                    let sort = &mut self.sorts[axis.index()];
                    sort.clear();
                    sort.extend(
                        spec.axis(axis)
                            .sort
                            .columns()
                            .iter()
                            .map(BinnacleSortColumn::from),
                    );
                    let quick = spec.axis(axis).filter.quick.as_ref();
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
                    conditions.fill(spec.axis(axis).filter.conditions.conditions());
                    let hidden = &mut self.hidden[axis.index()];
                    hidden.clear();
                    hidden.extend(spec.axis(axis).hidden.iter().map(|k| k.0));
                    effect.view.axes[axis.index()] = BinnacleAxisView {
                        sort: sort.as_ptr(),
                        sort_len: sort.len(),
                        has_quick: flag(quick.is_some()),
                        quick: query,
                        conditions: conditions.items.as_ptr(),
                        conditions_len: conditions.items.len(),
                        hidden: hidden.as_ptr(),
                        hidden_len: hidden.len(),
                    };
                }
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
    unsafe {
        with_handle(grid, |h| {
            h.grid.hide(axis(axis_tag)?, &keys);
            Some(())
        })
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
        | BINNACLE_SUPPORT_ONE_OF;
    let text = match support & (BINNACLE_SUPPORT_TEXT | BINNACLE_SUPPORT_TEXT_REGEX) {
        0 => Some(None),
        BINNACLE_SUPPORT_TEXT => Some(Some(TextSupport::new(false))),
        s if s == BINNACLE_SUPPORT_TEXT | BINNACLE_SUPPORT_TEXT_REGEX => {
            Some(Some(TextSupport::new(true)))
        }
        _ => None,
    };
    let support = text.filter(|_| support & !known == 0).map(|text| {
        Support::default()
            .with_sort(support & BINNACLE_SUPPORT_SORT != 0)
            .with_text(text)
            .with_one_of(support & BINNACLE_SUPPORT_ONE_OF != 0)
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

/// Sends `action`, or returns 0 if it is `None` (an invalid tag).
///
/// # Safety
/// `grid` is NULL or a live handle.
unsafe fn act(grid: *mut BinnacleGrid, action: Option<Action>) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_handle(grid, |h| action.map(|a| h.grid.action(a))) }
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
/// `grid` is NULL or a live handle; a non-NULL `text` is valid for `len` reads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_type_text(
    grid: *mut BinnacleGrid,
    text: *const u8,
    len: usize,
) -> i32 {
    // SAFETY: the caller's contract.
    let text = std::str::from_utf8(unsafe { borrowed(text, len) });
    // SAFETY: the caller's contract.
    unsafe { act(grid, text.ok().map(|t| Action::TypeText(t.to_owned()))) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_move_focus(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    by: u32,
    forward: u32,
    extend: u32,
) -> i32 {
    let by = match by {
        BINNACLE_FOCUS_ITEM => Some(FocusStep::Item),
        BINNACLE_FOCUS_PAGE => Some(FocusStep::Page),
        BINNACLE_FOCUS_EDGE => Some(FocusStep::Edge),
        _ => None,
    };
    let action = axis(axis_tag).zip(by).map(|(axis, by)| Action::MoveFocus {
        axis,
        by,
        forward: forward != 0,
        extend: extend != 0,
    });
    // SAFETY: the caller's contract.
    unsafe { act(grid, action) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_select_all(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::SelectAll)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_select_column(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::SelectColumn)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_select_row(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::SelectRow)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_cancel(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::Cancel)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_scroll(grid: *mut BinnacleGrid, dx: f32, dy: f32) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::Scroll { dx, dy })) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_scroll_to(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    fraction: f64,
) -> i32 {
    let action = axis(axis_tag).map(|axis| Action::ScrollTo { axis, fraction });
    // SAFETY: the caller's contract.
    unsafe { act(grid, action) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_scroll_by(
    grid: *mut BinnacleGrid,
    axis_tag: u32,
    items: i64,
) -> i32 {
    let action = axis(axis_tag).map(|axis| Action::ScrollBy { axis, items });
    // SAFETY: the caller's contract.
    unsafe { act(grid, action) }
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
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_find_next(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::FindNext)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_find_previous(grid: *mut BinnacleGrid) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { act(grid, Some(Action::FindPrevious)) }
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
