//! Internal C API over the binnacle Grid, for wheelhouse.
//!
//! `include/binnacle.h` is the hand-written header and states the contract;
//! this file mirrors it, constant for constant and struct for struct. Every
//! array a call fills points into buffers the [`BinnacleGrid`] handle owns, so
//! the host never releases anything but the handle. Effects are polled;
//! nothing calls back into the host.

use std::slice;

use binnacle_core::{
    Action, Axis, AxisConfig, Button, Effect, FocusEnd, FocusStep, Grid, HeaderClick, Key, Mode,
    Modifiers, PointerEvent, PointerKind, Rect, SelectionUnit, StripItem, Viewport,
};

pub const BINNACLE_ROWS: u32 = 0;
pub const BINNACLE_COLS: u32 = 1;

pub const BINNACLE_DAMAGE_LAYOUT: u32 = 1;
pub const BINNACLE_DAMAGE_SELECTION: u32 = 2;
pub const BINNACLE_DAMAGE_POSITIONS: u32 = 4;

pub const BINNACLE_CELL_FOCUSED: u8 = 1;
pub const BINNACLE_CELL_SELECTED: u8 = 2;

pub const BINNACLE_MODE_OTHER: u32 = 0;
pub const BINNACLE_MODE_IDLE: u32 = 1;
pub const BINNACLE_MODE_DRAGGING_SELECTION: u32 = 2;

pub const BINNACLE_EFFECT_OTHER: u32 = 0;
pub const BINNACLE_EFFECT_TEXT_TYPED: u32 = 1;
pub const BINNACLE_EFFECT_FOCUS_CHANGED: u32 = 2;
pub const BINNACLE_EFFECT_SELECTION_CHANGED: u32 = 3;

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

pub const BINNACLE_HEADER_CLICK_SORT: u32 = 0;
pub const BINNACLE_HEADER_CLICK_SELECT: u32 = 1;
pub const BINNACLE_HEADER_CLICK_NONE: u32 = 2;

/// The opaque handle: a Grid plus the buffers its C-shaped Snapshot and
/// Effects point into.
pub struct BinnacleGrid {
    grid: Grid,
    strips: [Vec<BinnacleStripItem>; 2],
    picked: Vec<u64>,
    rects: Vec<BinnacleRect>,
    cells: Vec<u8>,
    /// The last polled Effect's text.
    text: String,
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
    pub focus: BinnacleFocusEnd,
    pub selection_range: BinnacleRange,
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
    pub picked: *const u64,
    pub picked_len: usize,
    pub selection_rects: *const BinnacleRect,
    pub selection_rects_len: usize,
    pub cell_states: *const u8,
    pub cell_states_len: usize,
    pub mode: u32,
    pub capturing: u32,
    pub has_hovered_header: u32,
    pub hovered_axis: u32,
    pub hovered_index: usize,
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
pub struct BinnacleEffect {
    pub generation: u64,
    pub kind: u32,
    pub text: BinnacleText,
    pub row: u64,
    pub col: u64,
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

fn index(axis: Axis) -> usize {
    axis_tag(axis) as usize
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
unsafe fn with(grid: *mut BinnacleGrid, f: impl FnOnce(&mut BinnacleGrid) -> Option<()>) -> i32 {
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
unsafe fn put<T>(out: *mut T, value: T) -> Option<()> {
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
        picked: Vec::new(),
        rects: Vec::new(),
        cells: Vec::new(),
        text: String::new(),
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
        _ => return 0,
    };
    // SAFETY: the caller's contract.
    unsafe {
        with(grid, |h| {
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
        with(grid, |h| {
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
        with(grid, |h| {
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
        with(grid, |h| {
            let mut requests = BinnacleWindowRequests::default();
            for (axis, range) in h.grid.begin_frame(Viewport::new(width, height)) {
                requests.axes[index(axis)] = BinnacleWindowRequest {
                    requested: 1,
                    start: range.start,
                    end: range.end,
                };
            }
            put(out, requests)
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
        with(grid, |h| {
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
        with(grid, |h| {
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
        with(grid, |h| {
            axis(axis_tag).map(|axis| h.grid.set_size(axis, Key(key), size))
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
    unsafe { with(grid, |h| put(out, h.snapshot())) }
}

impl BinnacleGrid {
    /// Takes the Snapshot and copies it into the handle's buffers.
    fn snapshot(&mut self) -> BinnacleSnapshot {
        let snap = self.grid.snapshot();
        let damage = snap.damage();
        for axis in Axis::ALL {
            let strip = &mut self.strips[index(axis)];
            strip.clear();
            strip.extend(snap.strip(axis).iter().map(BinnacleStripItem::from));
        }
        self.picked.clear();
        self.picked.extend(snap.picked().iter().map(|k| k.0));
        self.rects.clear();
        self.rects.extend(
            snap.selection_rects()
                .iter()
                .map(|&r| BinnacleRect::from(r)),
        );
        self.cells.clear();
        for row in snap.rows() {
            for col in snap.cols() {
                let state = snap.cell_state(row, col);
                self.cells.push(
                    bit(state.focused, BINNACLE_CELL_FOCUSED)
                        | bit(state.selected, BINNACLE_CELL_SELECTED),
                );
            }
        }
        let range = snap.selection_range();
        let axis_snapshot = |axis: Axis| {
            let anchor = snap.scroll_anchor(axis);
            let thumb = snap.thumb(axis);
            let strip = &self.strips[index(axis)];
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
                focus: snap
                    .focus(axis)
                    .map(BinnacleFocusEnd::from)
                    .unwrap_or_default(),
                selection_range: range.as_ref().map_or_else(Default::default, |r| {
                    let span = r.span(axis);
                    BinnacleRange {
                        start: span.start,
                        end: span.end,
                    }
                }),
            }
        };
        let hovered = snap.hovered_header().and_then(|(axis, item)| {
            let i = snap
                .strip(axis)
                .iter()
                .position(|s| s.position == item.position)?;
            Some((axis, i))
        });
        let focus_rect = snap.focus_rect();
        #[allow(unreachable_patterns)] // Modes added to the Grid later are OTHER.
        let mode = match snap.mode() {
            Mode::Idle => BINNACLE_MODE_IDLE,
            Mode::DraggingSelection => BINNACLE_MODE_DRAGGING_SELECTION,
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
            picked: self.picked.as_ptr(),
            picked_len: self.picked.len(),
            selection_rects: self.rects.as_ptr(),
            selection_rects_len: self.rects.len(),
            cell_states: self.cells.as_ptr(),
            cell_states_len: self.cells.len(),
            mode,
            capturing: flag(snap.capturing()),
            has_hovered_header: flag(hovered.is_some()),
            hovered_axis: hovered.map_or(0, |(axis, _)| axis_tag(axis)),
            hovered_index: hovered.map_or(0, |(_, i)| i),
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
    unsafe { with(grid, |h| put(out, h.poll_effect()?)) }
}

/// # Safety
/// `grid` is NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn binnacle_grid_pointer(
    grid: *mut BinnacleGrid,
    event: BinnaclePointerEvent,
) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with(grid, |h| event.to_core().map(|e| h.grid.pointer(e))) }
}

/// Sends `action`, or returns 0 if it is `None` (an invalid tag).
///
/// # Safety
/// `grid` is NULL or a live handle.
unsafe fn act(grid: *mut BinnacleGrid, action: Option<Action>) -> i32 {
    // SAFETY: the caller's contract.
    unsafe { with(grid, |h| action.map(|a| h.grid.action(a))) }
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
