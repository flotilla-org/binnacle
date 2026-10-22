//! The grouping layer (`binnacle-group`) for a host whose aggregate backend
//! answers when it likes, as the wasm bridge's `WebGroupingLayer` gives it
//! to TypeScript: the layer stays sans-IO, its batches are polled and its
//! answers handed back. Every array a call fills points into buffers the
//! [`BinnacleGroupLayer`] handle owns, one set per call.

use binnacle_core::{Axis, AxisView, ChangeNotice, ItemKind, Key, Located, Outline, Totals, Value};
use binnacle_group::{Answer, Batch, Body, GroupRow, GroupingLayer, Path, RequestId, Stamp, Want};

use crate::{
    BINNACLE_ITEM_GRAND_TOTAL, BINNACLE_ITEM_GROUP, BINNACLE_ITEM_LEAF, BINNACLE_ITEM_SUBTOTAL,
    BINNACLE_LOCATED_FOUND, BINNACLE_LOCATED_GONE, BINNACLE_LOCATED_GONE_SUGGEST,
    BINNACLE_NOTICE_COLLAPSED, BINNACLE_NOTICE_CONTENT_CHANGED, BINNACLE_NOTICE_EXPANDED,
    BINNACLE_NOTICE_INSERTED, BINNACLE_NOTICE_REFILTERED, BINNACLE_NOTICE_REMOVED,
    BINNACLE_NOTICE_REORDERED, BINNACLE_NOTICE_RESET, BinnacleAggregateValue, BinnacleAxisView,
    BinnacleCondition, BinnacleGroupOf, BinnacleOutline, BinnacleText, BinnacleTotal,
    BinnacleTotals, BinnacleValue, BinnacleViewSpec, CView, axis, axis_tag, borrowed, flag,
    outline_to_c, value_to_c, view_spec, write_out,
};

pub const BINNACLE_WANT_GROUPS: u32 = 0;
pub const BINNACLE_WANT_LEAVES: u32 = 1;
pub const BINNACLE_WANT_LOCATE: u32 = 2;
pub const BINNACLE_WANT_TOTAL: u32 = 3;

pub const BINNACLE_BODY_GROUPS: u32 = 0;
pub const BINNACLE_BODY_LEAVES: u32 = 1;
pub const BINNACLE_BODY_LOCATED: u32 = 2;
pub const BINNACLE_BODY_TOTAL: u32 = 3;

pub const BINNACLE_FIGURE_BLANK: u32 = 0;
pub const BINNACLE_FIGURE_NUMBER: u32 = 1;
pub const BINNACLE_FIGURE_TEXT: u32 = 2;

/// A figure a backend computes; `None` in an aggregate is a blank.
#[derive(Clone, Debug, PartialEq)]
pub enum Figure {
    Number(f64),
    Text(String),
}

/// The opaque handle: a grouping layer plus the buffers its C-shaped
/// batches, notices, Windows and Summary Items point into.
pub struct BinnacleGroupLayer {
    layer: GroupingLayer<Option<Figure>>,
    /// The last polled batch: its scope, and its requests' paths.
    scope: CView,
    levels: Vec<u64>,
    batch_paths: CPaths,
    requests: Vec<BinnacleGroupRequest>,
    /// The last polled applied spec and its notices.
    applied: CView,
    notices: Vec<BinnacleGroupNotice>,
    /// The last Window's items, their ancestors and group value texts.
    items: Vec<BinnacleGroupItem>,
    ancestors: Vec<u64>,
    item_texts: String,
    item_paths: Vec<BinnacleGroupOf>,
    /// The last Summary Item's path and figures.
    summary_path: CPaths,
    figures: CFigures,
}

/// Paths in C form, with the value texts their steps point into.
#[derive(Default)]
struct CPaths {
    texts: String,
    steps: Vec<BinnacleGroupStep>,
}

impl CPaths {
    /// Replaces the contents with `paths`, their steps one after another.
    fn fill<'a>(&mut self, paths: impl IntoIterator<Item = &'a Path>) {
        self.texts.clear();
        self.steps.clear();
        // Texts first, then the steps, so the pointers taken after stay
        // valid.
        let mut spans = Vec::new();
        for path in paths {
            for (field, value) in path {
                let at = self.texts.len();
                if let Value::Text(t) = value {
                    self.texts.push_str(t);
                }
                spans.push((field.0, value, at));
            }
        }
        for (field, value, at) in spans {
            self.steps.push(BinnacleGroupStep {
                field,
                value: value_to_c(value, self.texts.as_ptr().wrapping_add(at)),
            });
        }
    }

    fn at(&self, start: usize) -> *const BinnacleGroupStep {
        self.steps.as_ptr().wrapping_add(start)
    }
}

/// Figures in C form, with the texts they point into.
#[derive(Default)]
struct CFigures {
    texts: String,
    figures: Vec<BinnacleFigure>,
}

impl CFigures {
    fn fill(&mut self, figures: &[Option<Figure>]) {
        self.texts.clear();
        self.figures.clear();
        for f in figures {
            if let Some(Figure::Text(t)) = f {
                self.texts.push_str(t);
            }
        }
        let mut at = 0;
        for f in figures {
            self.figures.push(match f {
                None => BinnacleFigure::default(),
                Some(Figure::Number(number)) => BinnacleFigure {
                    kind: BINNACLE_FIGURE_NUMBER,
                    number: *number,
                    ..BinnacleFigure::default()
                },
                Some(Figure::Text(t)) => {
                    let text = BinnacleText {
                        data: self.texts.as_ptr().wrapping_add(at),
                        len: t.len(),
                    };
                    at += t.len();
                    BinnacleFigure {
                        kind: BINNACLE_FIGURE_TEXT,
                        text,
                        ..BinnacleFigure::default()
                    }
                }
            });
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleFigure {
    pub kind: u32,
    pub number: f64,
    pub text: BinnacleText,
}

impl Default for BinnacleFigure {
    fn default() -> Self {
        BinnacleFigure {
            kind: BINNACLE_FIGURE_BLANK,
            number: 0.0,
            text: BinnacleText::default(),
        }
    }
}

impl BinnacleFigure {
    /// The figure, or `Err` for an unknown kind or text that is not UTF-8.
    ///
    /// # Safety
    /// A non-NULL `text.data` is valid for `text.len` reads.
    unsafe fn to_core(self) -> Result<Option<Figure>, ()> {
        match self.kind {
            BINNACLE_FIGURE_BLANK => Ok(None),
            BINNACLE_FIGURE_NUMBER => Ok(Some(Figure::Number(self.number))),
            BINNACLE_FIGURE_TEXT => {
                // SAFETY: the caller's contract.
                let bytes = unsafe { borrowed(self.text.data, self.text.len) };
                let text = std::str::from_utf8(bytes).map_err(|_| ())?;
                Ok(Some(Figure::Text(text.to_owned())))
            }
            _ => Err(()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleGroupStep {
    pub field: u64,
    pub value: BinnacleValue,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupRequest {
    pub id: u64,
    pub path: *const BinnacleGroupStep,
    pub path_len: usize,
    pub want: u32,
    pub key: u64,
    pub start: u64,
    pub end: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupScope {
    pub levels: *const u64,
    pub levels_len: usize,
    pub values: *const BinnacleAggregateValue,
    pub values_len: usize,
    pub slicers: *const BinnacleCondition,
    pub slicers_len: usize,
    pub rows: BinnacleAxisView,
}

impl Default for BinnacleGroupScope {
    fn default() -> Self {
        BinnacleGroupScope {
            levels: std::ptr::null(),
            levels_len: 0,
            values: std::ptr::null(),
            values_len: 0,
            slicers: std::ptr::null(),
            slicers_len: 0,
            rows: BinnacleAxisView::default(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupBatch {
    pub stamp: u64,
    pub scope: BinnacleGroupScope,
    pub requests: *const BinnacleGroupRequest,
    pub requests_len: usize,
}

impl Default for BinnacleGroupBatch {
    fn default() -> Self {
        BinnacleGroupBatch {
            stamp: 0,
            scope: BinnacleGroupScope::default(),
            requests: std::ptr::null(),
            requests_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleGroupRow {
    pub value: BinnacleValue,
    pub children: u64,
    pub figures: *const BinnacleFigure,
    pub figures_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleGroupAnswer {
    pub stamp: u64,
    pub has_id: u32,
    pub id: u64,
    pub path: *const BinnacleGroupStep,
    pub path_len: usize,
    pub start: u64,
    pub body: u32,
    pub total: u64,
    pub groups: *const BinnacleGroupRow,
    pub groups_len: usize,
    pub keys: *const u64,
    pub keys_len: usize,
    pub key: u64,
    pub found: u32,
    pub figures: *const BinnacleFigure,
    pub figures_len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleGroupNotice {
    pub axis: u32,
    pub notice: u32,
    pub at: u64,
    pub n: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BinnacleGroupViewed {
    pub applied: BinnacleViewSpec,
    pub totals: BinnacleTotals,
    pub notices: *const BinnacleGroupNotice,
    pub notices_len: usize,
}

impl Default for BinnacleGroupViewed {
    fn default() -> Self {
        BinnacleGroupViewed {
            applied: BinnacleViewSpec::default(),
            totals: BinnacleTotals::default(),
            notices: std::ptr::null(),
            notices_len: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BinnacleGroupItem {
    pub key: u64,
    pub outline: BinnacleOutline,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BinnacleGroupSummary {
    pub kind: u32,
    pub path: *const BinnacleGroupStep,
    pub path_len: usize,
    pub children: u64,
    pub figures: *const BinnacleFigure,
    pub figures_len: usize,
}

impl Default for BinnacleGroupSummary {
    fn default() -> Self {
        BinnacleGroupSummary {
            kind: BINNACLE_ITEM_GROUP,
            path: std::ptr::null(),
            path_len: 0,
            children: 0,
            figures: std::ptr::null(),
            figures_len: 0,
        }
    }
}

fn notice_to_c(axis: Axis, notice: &ChangeNotice) -> BinnacleGroupNotice {
    let (notice, at, n) = match *notice {
        ChangeNotice::Reset(n) => (BINNACLE_NOTICE_RESET, 0, n),
        ChangeNotice::Reordered => (BINNACLE_NOTICE_REORDERED, 0, 0),
        ChangeNotice::Inserted { at, n } => (BINNACLE_NOTICE_INSERTED, at, n),
        ChangeNotice::Removed { at, n } => (BINNACLE_NOTICE_REMOVED, at, n),
        ChangeNotice::ContentChanged(ref r) => {
            (BINNACLE_NOTICE_CONTENT_CHANGED, r.start, r.end - r.start)
        }
        ChangeNotice::Refiltered(n) => (BINNACLE_NOTICE_REFILTERED, 0, n),
        ChangeNotice::Collapsed { at, n } => (BINNACLE_NOTICE_COLLAPSED, at, n),
        ChangeNotice::Expanded { at, n } => (BINNACLE_NOTICE_EXPANDED, at, n),
    };
    BinnacleGroupNotice {
        axis: axis_tag(axis),
        notice,
        at,
        n,
    }
}

fn totals_to_c(totals: &Totals) -> BinnacleTotals {
    let total = |t: Option<u64>| BinnacleTotal {
        known: flag(t.is_some()),
        count: t.unwrap_or(0),
    };
    BinnacleTotals {
        axes: [total(totals.rows), total(totals.cols)],
    }
}

fn kind_tag(kind: ItemKind) -> u32 {
    match kind {
        ItemKind::Leaf => BINNACLE_ITEM_LEAF,
        ItemKind::Group => BINNACLE_ITEM_GROUP,
        ItemKind::Subtotal => BINNACLE_ITEM_SUBTOTAL,
        ItemKind::GrandTotal => BINNACLE_ITEM_GRAND_TOTAL,
    }
}

/// A path read from C, or `None` for a value that is not one.
///
/// # Safety
/// A non-NULL `steps` is valid for `len` reads, each step's text as for
/// [`BinnacleValue::to_core`].
unsafe fn path_in(steps: *const BinnacleGroupStep, len: usize) -> Option<Path> {
    // SAFETY: the caller's contract.
    let steps = unsafe { borrowed(steps, len) };
    (steps.iter())
        // SAFETY: the caller's contract.
        .map(|s| Some((Key(s.field), unsafe { s.value.to_core() }?)))
        .collect()
}

/// Figures read from C, or `None` if one is not a figure.
///
/// # Safety
/// As for [`BinnacleFigure::to_core`], for `len` figures from `figures`.
unsafe fn figures_in(figures: *const BinnacleFigure, len: usize) -> Option<Vec<Option<Figure>>> {
    // SAFETY: the caller's contract.
    let figures = unsafe { borrowed(figures, len) };
    (figures.iter())
        // SAFETY: the caller's contract.
        .map(|f| unsafe { f.to_core() }.ok())
        .collect()
}

impl BinnacleGroupAnswer {
    /// The answer, or `None` for an unknown tag or a value that is not one.
    ///
    /// # Safety
    /// Each non-NULL array is valid for its length's reads, and so are the
    /// arrays its elements point to.
    unsafe fn to_core(self) -> Option<Answer<Option<Figure>>> {
        let body = match self.body {
            BINNACLE_BODY_GROUPS => {
                // SAFETY: the caller's contract.
                let rows = unsafe { borrowed(self.groups, self.groups_len) };
                let groups = (rows.iter())
                    .map(|g| {
                        Some(GroupRow {
                            // SAFETY: the caller's contract.
                            value: unsafe { g.value.to_core() }?,
                            children: g.children,
                            // SAFETY: the caller's contract.
                            aggregates: unsafe { figures_in(g.figures, g.figures_len) }?,
                        })
                    })
                    .collect::<Option<_>>()?;
                Body::Groups {
                    total: self.total,
                    groups,
                }
            }
            BINNACLE_BODY_LEAVES => Body::Leaves {
                // SAFETY: the caller's contract.
                keys: unsafe { borrowed(self.keys, self.keys_len) }
                    .iter()
                    .map(|&k| Key(k))
                    .collect(),
            },
            BINNACLE_BODY_LOCATED => Body::Located {
                key: Key(self.key),
                found: match self.found {
                    0 => false,
                    1 => true,
                    _ => return None,
                },
            },
            BINNACLE_BODY_TOTAL => Body::Total {
                // SAFETY: the caller's contract.
                aggregates: unsafe { figures_in(self.figures, self.figures_len) }?,
            },
            _ => return None,
        };
        let id = match self.has_id {
            0 => None,
            1 => Some(RequestId(self.id)),
            _ => return None,
        };
        Some(Answer {
            stamp: Stamp(self.stamp),
            id,
            // SAFETY: the caller's contract.
            path: unsafe { path_in(self.path, self.path_len) }?,
            start: self.start,
            body,
        })
    }
}

impl BinnacleGroupLayer {
    fn batch_to_c(&mut self, batch: &Batch) -> BinnacleGroupBatch {
        let scope = &batch.scope;
        self.levels.clear();
        self.levels.extend(scope.levels.iter().map(|k| k.0));
        let rows = AxisView {
            filter: scope.filter.clone(),
            sort: scope.sort.clone(),
            hidden: scope.hidden.clone(),
        };
        let rows = self.scope.axis_to_c(Axis::Rows, &rows);
        let (values, values_len) = self.scope.values_to_c(&scope.values);
        self.scope.slicers.fill(&scope.slicers);
        self.batch_paths
            .fill(batch.requests.iter().map(|r| &r.path));
        self.requests.clear();
        let mut at = 0;
        for r in &batch.requests {
            let (want, key) = match r.want {
                Want::Groups => (BINNACLE_WANT_GROUPS, 0),
                Want::Leaves => (BINNACLE_WANT_LEAVES, 0),
                Want::Locate(key) => (BINNACLE_WANT_LOCATE, key.0),
                Want::Total => (BINNACLE_WANT_TOTAL, 0),
            };
            self.requests.push(BinnacleGroupRequest {
                id: r.id.0,
                path: self.batch_paths.at(at),
                path_len: r.path.len(),
                want,
                key,
                start: r.range.start,
                end: r.range.end,
            });
            at += r.path.len();
        }
        BinnacleGroupBatch {
            stamp: batch.stamp.0,
            scope: BinnacleGroupScope {
                levels: self.levels.as_ptr(),
                levels_len: self.levels.len(),
                values,
                values_len,
                slicers: self.scope.slicers.items.as_ptr(),
                slicers_len: self.scope.slicers.items.len(),
                rows,
            },
            requests: self.requests.as_ptr(),
            requests_len: self.requests.len(),
        }
    }

    fn window_to_c(&mut self, items: &[(Key, Outline)]) {
        self.items.clear();
        self.ancestors.clear();
        self.item_texts.clear();
        self.item_paths.clear();
        // Ancestors and texts first, then the paths, then the items, so the
        // pointers taken after stay valid.
        let mut spans = Vec::new();
        let mut path_texts = Vec::new();
        for (key, outline) in items {
            let (keys, text) = (self.ancestors.len(), self.item_texts.len());
            self.ancestors.extend(outline.ancestors.iter().map(|k| k.0));
            if let Some(binnacle_core::GroupOf {
                value: Value::Text(t),
                ..
            }) = &outline.group
            {
                self.item_texts.push_str(t);
            }
            for g in &outline.path {
                path_texts.push(self.item_texts.len());
                if let Value::Text(t) = &g.value {
                    self.item_texts.push_str(t);
                }
            }
            spans.push((key, outline, keys, text));
        }
        let mut at = path_texts.into_iter();
        for g in spans.iter().flat_map(|(_, o, ..)| &o.path) {
            let text = self
                .item_texts
                .as_ptr()
                .wrapping_add(at.next().unwrap_or(0));
            self.item_paths.push(BinnacleGroupOf {
                field: g.field.0,
                value: value_to_c(&g.value, text),
            });
        }
        let mut path = 0;
        for (key, outline, keys, text) in spans {
            let first = path;
            path += outline.path.len();
            self.items.push(BinnacleGroupItem {
                key: key.0,
                outline: outline_to_c(
                    outline,
                    self.ancestors.as_ptr().wrapping_add(keys),
                    self.item_texts.as_ptr().wrapping_add(text),
                    self.item_paths.as_ptr().wrapping_add(first),
                ),
            });
        }
    }
}

/// Runs `f` on the handle; 0 for NULL or if `f` gives `None`, else 1.
///
/// # Safety
/// `layer` is NULL or a live handle.
unsafe fn with_layer(
    layer: *mut BinnacleGroupLayer,
    f: impl FnOnce(&mut BinnacleGroupLayer) -> Option<()>,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(h) = (unsafe { layer.as_mut() }) else {
        return 0;
    };
    f(h).is_some().into()
}

/// A new grouping layer caching up to `cache` closed groups and Leaves
/// besides those the latest Windows read, or the default if `has_cache` is
/// 0; NULL for a flag that is not 0 or 1. Free it with
/// [`binnacle_group_layer_free`].
#[unsafe(no_mangle)]
pub extern "C" fn binnacle_group_layer_new(has_cache: u32, cache: u64) -> *mut BinnacleGroupLayer {
    let layer = match has_cache {
        0 => GroupingLayer::new(),
        1 => GroupingLayer::with_cache(cache),
        _ => return std::ptr::null_mut(),
    };
    Box::into_raw(Box::new(BinnacleGroupLayer {
        layer,
        scope: CView::default(),
        levels: Vec::new(),
        batch_paths: CPaths::default(),
        requests: Vec::new(),
        applied: CView::default(),
        notices: Vec::new(),
        items: Vec::new(),
        ancestors: Vec::new(),
        item_texts: String::new(),
        item_paths: Vec::new(),
        summary_path: CPaths::default(),
        figures: CFigures::default(),
    }))
}

/// # Safety
/// `layer` is NULL or a live handle from [`binnacle_group_layer_new`], not
/// used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_free(layer: *mut BinnacleGroupLayer) {
    if !layer.is_null() {
        // SAFETY: the caller's contract.
        drop(unsafe { Box::from_raw(layer) });
    }
}

/// # Safety
/// `layer` is NULL or a live handle; `spec` as for
/// [`crate::binnacle_grid_view_applied`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_request_view(
    layer: *mut BinnacleGroupLayer,
    spec: *const BinnacleViewSpec,
) -> i32 {
    // SAFETY: the caller's contract.
    let spec = unsafe { view_spec(spec) };
    // SAFETY: the caller's contract.
    unsafe { with_layer(layer, |h| spec.map(|s| h.layer.request_view(s))) }
}

/// # Safety
/// `layer` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_refresh(layer: *mut BinnacleGroupLayer) -> i32 {
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            h.layer.refresh();
            Some(())
        })
    }
}

/// Whether `axis` is grouped, and so the layer's: 1 if so, else 0.
///
/// # Safety
/// `layer` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_grouped(
    layer: *mut BinnacleGroupLayer,
    axis_tag: u32,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_layer(layer, |h| h.layer.grouped(axis(axis_tag)?).then_some(())) }
}

/// # Safety
/// `layer` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_poll_batch(
    layer: *mut BinnacleGroupLayer,
    out: *mut BinnacleGroupBatch,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            let batch = h.layer.poll_batch()?;
            let c = h.batch_to_c(&batch);
            write_out(out, c)
        })
    }
}

/// # Safety
/// `layer` is NULL or a live handle; `answer` is NULL or valid for a read,
/// as [`BinnacleGroupAnswer::to_core`] requires.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_answer(
    layer: *mut BinnacleGroupLayer,
    answer: *const BinnacleGroupAnswer,
) -> i32 {
    // SAFETY: the caller's contract.
    let Some(&answer) = (unsafe { answer.as_ref() }) else {
        return 0;
    };
    // SAFETY: the caller's contract.
    let answer = unsafe { answer.to_core() };
    // SAFETY: the caller's contract.
    unsafe { with_layer(layer, |h| answer.map(|a| h.layer.answer(a))) }
}

/// # Safety
/// `layer` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_poll_viewed(
    layer: *mut BinnacleGroupLayer,
    out: *mut BinnacleGroupViewed,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            let viewed = h.layer.poll_viewed()?;
            let applied = h.applied.fill(&viewed.applied);
            h.notices.clear();
            (h.notices).extend(viewed.notices.iter().map(|(a, n)| notice_to_c(*a, n)));
            write_out(
                out,
                BinnacleGroupViewed {
                    applied,
                    totals: totals_to_c(&viewed.totals),
                    notices: h.notices.as_ptr(),
                    notices_len: h.notices.len(),
                },
            )
        })
    }
}

/// How many items `axis` shows, into `out`: 1 if known, else 0.
///
/// # Safety
/// `layer` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_count(
    layer: *mut BinnacleGroupLayer,
    axis_tag: u32,
    out: *mut u64,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with_layer(layer, |h| write_out(out, h.layer.count(axis(axis_tag)?)?)) }
}

/// # Safety
/// `layer` is NULL or a live handle; `items` and `len` are NULL or valid
/// for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_window(
    layer: *mut BinnacleGroupLayer,
    axis_tag: u32,
    start: u64,
    end: u64,
    items: *mut *const BinnacleGroupItem,
    len: *mut usize,
) -> i32 {
    if items.is_null() || len.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            let window = h.layer.window(axis(axis_tag)?, start..end);
            h.window_to_c(&window);
            write_out(items, h.items.as_ptr())?;
            write_out(len, h.items.len())
        })
    }
}

/// # Safety
/// `layer` is NULL or a live handle; `answer` and `position` are NULL or
/// valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_locate(
    layer: *mut BinnacleGroupLayer,
    axis_tag: u32,
    key: u64,
    answer: *mut u32,
    position: *mut u64,
) -> i32 {
    if answer.is_null() || position.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            let (tag, at) = match h.layer.locate(axis(axis_tag)?, Key(key))? {
                Located::Found(p) => (BINNACLE_LOCATED_FOUND, p),
                Located::Gone { suggest: None } => (BINNACLE_LOCATED_GONE, 0),
                Located::Gone { suggest: Some(p) } => (BINNACLE_LOCATED_GONE_SUGGEST, p),
            };
            write_out(answer, tag)?;
            write_out(position, at)
        })
    }
}

/// # Safety
/// `layer` is NULL or a live handle; `out` is NULL or valid for a write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_summary(
    layer: *mut BinnacleGroupLayer,
    key: u64,
    out: *mut BinnacleGroupSummary,
) -> i32 {
    if out.is_null() {
        return 0;
    }
    // SAFETY: the caller's contract.
    unsafe {
        with_layer(layer, |h| {
            let s = h.layer.summary(Key(key))?;
            let (kind, children, path) = (kind_tag(s.kind), s.children, s.path.clone());
            h.figures.fill(s.aggregates);
            h.summary_path.fill([&path]);
            write_out(
                out,
                BinnacleGroupSummary {
                    kind,
                    path: h.summary_path.at(0),
                    path_len: path.len(),
                    children,
                    figures: h.figures.figures.as_ptr(),
                    figures_len: h.figures.figures.len(),
                },
            )
        })
    }
}

/// How many groups and Leaves the layer holds; 0 for NULL.
///
/// # Safety
/// `layer` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_group_layer_held(layer: *const BinnacleGroupLayer) -> u64 {
    // SAFETY: the caller's contract.
    unsafe { layer.as_ref() }.map_or(0, |h| h.layer.held())
}
