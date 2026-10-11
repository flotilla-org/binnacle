//! PROTOTYPE stub of the Grid (binnacle-core). No toolkit types: the host pushes
//! input, fills Windows, pulls a Snapshot and drains Effects (the frame loop).
//! Deliberately partial: no header moves, no editing, no picks, no content sizes.

use std::collections::{HashMap, VecDeque};

pub type Key = u64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    Rows,
    Cols,
}
use Axis::*;

impl Axis {
    fn i(self) -> usize {
        match self {
            Rows => 0,
            Cols => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Asc,
    Desc,
}

pub type SortSpec = Vec<(Key, Dir)>;

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    SortRequested(SortSpec),
    Locate { axis: Axis, key: Key },
    AutoSizeRequested { axis: Axis, key: Key },
    SizeChanged { axis: Axis, key: Key, size: f32 },
    Activated { row: Key, col: Key },
    TextTyped(String),
    SelectionChanged,
    FocusChanged,
}

#[derive(Clone, Copy, Debug)]
pub enum Notice {
    Reset(u64),
    Reordered,
}

#[derive(Clone, Copy, Debug)]
pub enum Pointer {
    Down { x: f32, y: f32, clicks: u8, shift: bool },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32 },
}

#[derive(Clone, Copy, Debug)]
pub enum By {
    Item,
    Page,
    Edge,
}

#[derive(Clone, Debug)]
pub enum Action {
    MoveFocus { axis: Axis, forward: bool, by: By, extend: bool },
    SelectAll,
    Activate,
    Cancel,
    TypeText(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cursor {
    Default,
    ResizeCol,
    Move,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Idle,
    DraggingSelection { anchor: (u64, u64) },
    Resizing { axis: Axis, key: Key, start_size: f32, start: f32 },
    HeaderPress { key: Key, shift: bool, start: f32 },
}

#[derive(Clone, Copy, Debug)]
pub struct StripItem {
    pub pos: u64,
    pub key: Key,
    pub offset: f32,
    pub size: f32,
}

/// One end of the focus. Either half may be unknown for a while: the Position after
/// a reorder (until `located`), or the Key after a jump outside the Window (until
/// the Window is filled). Finding: focus needs both directions of resolution.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FocusEnd {
    pub pos: Option<u64>,
    pub key: Option<Key>,
    /// Finding: kept separately so a departed Key can fall back to "the item now
    /// at the last known Position", as #9 decided.
    pub last_pos: Option<u64>,
}

const OVERSCAN: u64 = 2;

struct AxisState {
    count: u64,
    default: f32,
    gap: f32,
    overrides: HashMap<Key, f32>,
    anchor_pos: u64,
    anchor_off: f32,
    win_start: u64,
    win_keys: Vec<Key>,
    win_valid: bool,
    extent: f32,
}

impl AxisState {
    fn new(count: u64, default: f32, gap: f32) -> Self {
        AxisState {
            count,
            default,
            gap,
            overrides: HashMap::new(),
            anchor_pos: 0,
            anchor_off: 0.0,
            win_start: 0,
            win_keys: Vec::new(),
            win_valid: false,
            extent: 0.0,
        }
    }
    fn key_at(&self, pos: u64) -> Option<Key> {
        if !self.win_valid || pos < self.win_start {
            return None;
        }
        self.win_keys.get((pos - self.win_start) as usize).copied()
    }
    fn pos_of(&self, key: Key) -> Option<u64> {
        if !self.win_valid {
            return None;
        }
        self.win_keys.iter().position(|k| *k == key).map(|i| self.win_start + i as u64)
    }
    fn size_at(&self, pos: u64) -> f32 {
        // Outside the Window we don't know the Key, so we can't see its override.
        self.key_at(pos)
            .and_then(|k| self.overrides.get(&k).copied())
            .unwrap_or(self.default)
    }
    fn stride_at(&self, pos: u64) -> f32 {
        self.size_at(pos) + self.gap
    }
    /// Finding: the visible range depends on item sizes, which depend on Keys we
    /// don't have yet. Bound it with the smallest size we could possibly meet.
    fn needed(&self) -> (u64, u64) {
        let min = self.overrides.values().fold(self.default, |a, b| a.min(*b)).max(1.0) + self.gap;
        let n = (self.extent / min).ceil() as u64 + 1;
        let start = self.anchor_pos.saturating_sub(OVERSCAN);
        let end = (self.anchor_pos + n + OVERSCAN).min(self.count);
        (start, end.saturating_sub(start))
    }
    fn visible_items(&self) -> u64 {
        ((self.extent / (self.default + self.gap)).floor() as u64).max(1)
    }
    fn max_anchor(&self) -> u64 {
        self.count.saturating_sub(self.visible_items())
    }
    fn scroll_items(&mut self, n: i64) {
        let p = (self.anchor_pos as i128 + n as i128).clamp(0, self.max_anchor() as i128);
        self.anchor_pos = p as u64;
        self.anchor_off = 0.0;
    }
    fn scroll_to_fraction(&mut self, f: f64) {
        self.anchor_pos = ((f.clamp(0.0, 1.0) * self.count as f64) as u64).min(self.max_anchor());
        self.anchor_off = 0.0;
    }
    fn reveal(&mut self, pos: u64) {
        if pos < self.anchor_pos {
            self.anchor_pos = pos;
            self.anchor_off = 0.0;
            return;
        }
        // Walk back from pos until the viewport is full.
        let mut used = self.stride_at(pos);
        let mut first = pos;
        while first > 0 && used + self.stride_at(first - 1) <= self.extent {
            first -= 1;
            used += self.stride_at(first);
        }
        if first > self.anchor_pos {
            self.anchor_pos = first;
            self.anchor_off = 0.0;
        }
    }
    fn strip(&self) -> Vec<StripItem> {
        let mut out = Vec::new();
        let mut pos = self.anchor_pos;
        let mut offset = -self.anchor_off;
        while pos < self.count && offset < self.extent {
            let Some(key) = self.key_at(pos) else { break }; // unfilled: "loading"
            let size = self.size_at(pos);
            out.push(StripItem { pos, key, offset, size });
            offset += size + self.gap;
            pos += 1;
        }
        out
    }
}

pub struct Snapshot {
    pub generation: u64,
    pub rows: Vec<StripItem>,
    pub cols: Vec<StripItem>,
    pub row_header_w: f32,
    pub col_header_h: f32,
    pub counts: [u64; 2],
    pub thumb: [f64; 2],
    pub sort: SortSpec,
    pub focus: (FocusEnd, FocusEnd),
    pub range: Option<((u64, u64), (u64, u64))>,
    pub mode: Mode,
    pub cursor: Cursor,
    pub capturing: bool,
}

pub struct CellState {
    pub focused: bool,
    pub selected: bool,
}

impl Snapshot {
    pub fn cell_state(&self, r: &StripItem, c: &StripItem) -> CellState {
        let focused = self.focus.0.key == Some(r.key) && self.focus.1.key == Some(c.key);
        let selected = self.range.map_or(false, |((r0, c0), (r1, c1))| {
            r.pos >= r0.min(r1) && r.pos <= r0.max(r1) && c.pos >= c0.min(c1) && c.pos <= c0.max(c1)
        });
        CellState { focused, selected }
    }
}

enum Hit {
    ColHeader { key: Key, border: bool, size: f32 },
    RowHeader { pos: u64 },
    Cell { row: StripItem, col: StripItem },
    Nothing,
}

pub struct Grid {
    axes: [AxisState; 2],
    pub row_header_w: f32,
    pub col_header_h: f32,
    mode: Mode,
    focus: (FocusEnd, FocusEnd),
    range: Option<((u64, u64), (u64, u64))>,
    sort: SortSpec,
    effects: VecDeque<Effect>,
    generation: u64,
    /// Set only when a focus move had to wait for `located`.
    pending_reveal: bool,
    /// Open question surfaced by this prototype: after a reorder, should the view
    /// follow the focused row, or keep its Scroll Anchor? Default: keep.
    pub follow_focus_on_reorder: bool,
    drag_threshold: f32,
}

impl Grid {
    pub fn new(rows: u64, cols: u64, row_size: f32, col_size: f32, gap: f32) -> Self {
        Grid {
            axes: [AxisState::new(rows, row_size, 0.0), AxisState::new(cols, col_size, gap)],
            row_header_w: 8.0,
            col_header_h: 1.0,
            mode: Mode::Idle,
            focus: Default::default(),
            range: None,
            sort: Vec::new(),
            effects: VecDeque::new(),
            generation: 0,
            pending_reveal: false,
            follow_focus_on_reorder: false,
            drag_threshold: 1.0,
        }
    }

    fn ax(&self, a: Axis) -> &AxisState {
        &self.axes[a.i()]
    }
    fn ax_mut(&mut self, a: Axis) -> &mut AxisState {
        &mut self.axes[a.i()]
    }
    fn focus_end(&mut self, a: Axis) -> &mut FocusEnd {
        match a {
            Rows => &mut self.focus.0,
            Cols => &mut self.focus.1,
        }
    }

    fn push(&mut self, e: Effect) {
        let coalesce = matches!(e, Effect::SelectionChanged | Effect::FocusChanged);
        if !(coalesce && self.effects.contains(&e)) {
            self.effects.push_back(e);
        }
    }
    pub fn poll_effect(&mut self) -> Option<Effect> {
        self.effects.pop_front()
    }

    // ---- frame protocol --------------------------------------------------

    /// Phase 1: the host gives the viewport (whole Grid area, headers included);
    /// the Grid answers with the Window it needs per axis, or None if it has it.
    pub fn begin_frame(&mut self, w: f32, h: f32) -> [Option<(u64, u64)>; 2] {
        self.ax_mut(Rows).extent = (h - self.col_header_h).max(0.0);
        self.ax_mut(Cols).extent = (w - self.row_header_w).max(0.0);
        [Rows, Cols].map(|a| {
            let st = self.ax(a);
            let (s, n) = st.needed();
            let have = st.win_valid && s >= st.win_start && s + n <= st.win_start + st.win_keys.len() as u64;
            if have { None } else { Some((s, n)) }
        })
    }

    /// Phase 2: the host fills the Window with Keys.
    pub fn set_window(&mut self, a: Axis, start: u64, keys: Vec<Key>) {
        let st = self.ax_mut(a);
        st.win_start = start;
        st.win_keys = keys;
        st.win_valid = true;
        // Resolve whichever half of the focus is missing on this axis.
        let f = *self.focus_end(a);
        match (f.pos, f.key) {
            (Some(p), None) => self.focus_end(a).key = self.ax(a).key_at(p),
            (None, Some(k)) => self.focus_end(a).pos = self.ax(a).pos_of(k),
            _ => {}
        }
    }

    /// Phase 3: pull the Snapshot. (Owned here; the design has it borrow.)
    pub fn snapshot(&mut self) -> Snapshot {
        self.generation += 1;
        let cursor = match self.mode {
            Mode::Resizing { .. } => Cursor::ResizeCol,
            Mode::HeaderPress { .. } => Cursor::Move,
            _ => Cursor::Default,
        };
        Snapshot {
            generation: self.generation,
            rows: self.ax(Rows).strip(),
            cols: self.ax(Cols).strip(),
            row_header_w: self.row_header_w,
            col_header_h: self.col_header_h,
            counts: [self.ax(Rows).count, self.ax(Cols).count],
            thumb: [Rows, Cols].map(|a| {
                let st = self.ax(a);
                st.anchor_pos as f64 / st.count.max(1) as f64
            }),
            sort: self.sort.clone(),
            focus: self.focus,
            range: self.range,
            capturing: !matches!(self.mode, Mode::Idle),
            mode: self.mode.clone(),
            cursor,
        }
    }

    // ---- answers from the host / Table Source ------------------------------

    pub fn notice(&mut self, a: Axis, n: Notice) {
        match n {
            Notice::Reset(count) => {
                self.ax_mut(a).count = count;
                if a == Rows {
                    self.ax_mut(a).overrides.clear();
                }
            }
            Notice::Reordered => {}
        }
        self.ax_mut(a).win_valid = false;
        // Finding: Change Notices must clamp the Scroll Anchor.
        let st = self.ax_mut(a);
        st.anchor_pos = st.anchor_pos.min(st.max_anchor());
        if a == Rows {
            if self.range.take().is_some() {
                self.push(Effect::SelectionChanged);
            }
        }
        if let Some(key) = self.focus_end(a).key {
            let f = self.focus_end(a);
            f.last_pos = f.pos.or(f.last_pos);
            f.pos = None;
            self.push(Effect::Locate { axis: a, key });
        }
    }

    pub fn located(&mut self, a: Axis, key: Key, pos: Option<u64>) {
        let f = *self.focus_end(a);
        if f.key != Some(key) {
            return; // stale answer
        }
        match pos {
            Some(p) => {
                self.focus_end(a).pos = Some(p);
                if std::mem::take(&mut self.pending_reveal) || self.follow_focus_on_reorder {
                    self.ax_mut(a).reveal(p);
                }
            }
            None => {
                // The Key left: focus the item now at the last known Position, clamped.
                let count = self.ax(a).count;
                let p = f.last_pos.map(|p| p.min(count.saturating_sub(1)));
                *self.focus_end(a) = FocusEnd { pos: p, key: p.and_then(|p| self.ax(a).key_at(p)), last_pos: p };
                self.push(Effect::FocusChanged);
            }
        }
    }

    // ---- input -------------------------------------------------------------

    pub fn scroll(&mut self, a: Axis, items: i64) {
        self.ax_mut(a).scroll_items(items);
    }
    pub fn scroll_to(&mut self, a: Axis, fraction: f64) {
        self.ax_mut(a).scroll_to_fraction(fraction);
    }

    fn hit(&self, x: f32, y: f32) -> Hit {
        let (cx, cy) = (x - self.row_header_w, y - self.col_header_h);
        let find = |items: &[StripItem], gap: f32, v: f32| -> Option<(StripItem, bool)> {
            items.iter().find_map(|it| {
                if v >= it.offset && v < it.offset + it.size {
                    Some((*it, false))
                } else if v >= it.offset + it.size && v < it.offset + it.size + gap.max(0.5) {
                    Some((*it, true))
                } else {
                    None
                }
            })
        };
        let rows = self.ax(Rows).strip();
        let cols = self.ax(Cols).strip();
        let gap = self.ax(Cols).gap;
        match (cx >= 0.0, cy >= 0.0) {
            (true, false) => match find(&cols, gap, cx) {
                Some((c, border)) => Hit::ColHeader { key: c.key, border, size: c.size },
                None => Hit::Nothing,
            },
            (false, true) => match find(&rows, 0.0, cy) {
                Some((r, _)) => Hit::RowHeader { pos: r.pos },
                None => Hit::Nothing,
            },
            (true, true) => match (find(&rows, 0.0, cy), find(&cols, gap, cx)) {
                (Some((r, _)), Some((c, false))) => Hit::Cell { row: r, col: c },
                _ => Hit::Nothing,
            },
            _ => Hit::Nothing,
        }
    }

    pub fn pointer(&mut self, p: Pointer) {
        match p {
            Pointer::Down { x, y, clicks, shift } => match self.hit(x, y) {
                Hit::ColHeader { key, border: true, size } => {
                    if clicks >= 2 {
                        self.push(Effect::AutoSizeRequested { axis: Cols, key });
                    } else {
                        self.mode = Mode::Resizing { axis: Cols, key, start_size: size, start: x };
                    }
                }
                Hit::ColHeader { key, border: false, .. } => {
                    self.mode = Mode::HeaderPress { key, shift, start: x };
                }
                Hit::RowHeader { pos } => {
                    // header_click: Select for rows.
                    let last_col = self.ax(Cols).count.saturating_sub(1);
                    self.range = Some(((pos, 0), (pos, last_col)));
                    self.focus.0 = FocusEnd { pos: Some(pos), key: self.ax(Rows).key_at(pos), last_pos: Some(pos) };
                    self.push(Effect::SelectionChanged);
                    self.push(Effect::FocusChanged);
                }
                Hit::Cell { row, col } => {
                    if clicks >= 2 {
                        self.push(Effect::Activated { row: row.key, col: col.key });
                        return;
                    }
                    let here = (row.pos, col.pos);
                    if shift {
                        let anchor = self.range.map(|r| r.0).or(match self.focus {
                            (FocusEnd { pos: Some(r), .. }, FocusEnd { pos: Some(c), .. }) => Some((r, c)),
                            _ => None,
                        });
                        self.range = anchor.map(|a| (a, here));
                    } else {
                        self.focus = (
                            FocusEnd { pos: Some(row.pos), key: Some(row.key), last_pos: Some(row.pos) },
                            FocusEnd { pos: Some(col.pos), key: Some(col.key), last_pos: Some(col.pos) },
                        );
                        self.range = None;
                        self.mode = Mode::DraggingSelection { anchor: here };
                        self.push(Effect::FocusChanged);
                    }
                    self.push(Effect::SelectionChanged);
                }
                Hit::Nothing => {}
            },
            Pointer::Move { x, y } => match self.mode.clone() {
                Mode::Resizing { axis, key, start_size, start } => {
                    let size = (start_size + (x - start)).max(1.0);
                    self.ax_mut(axis).overrides.insert(key, size);
                }
                Mode::DraggingSelection { anchor } => {
                    if let Hit::Cell { row, col } = self.hit(x, y) {
                        let here = (row.pos, col.pos);
                        if here != anchor {
                            self.range = Some((anchor, here));
                            self.push(Effect::SelectionChanged);
                        }
                    }
                }
                Mode::HeaderPress { start, .. } if (x - start).abs() > self.drag_threshold => {
                    // Would become MovingHeaders; not prototyped.
                }
                _ => {}
            },
            Pointer::Up { x, .. } => match std::mem::replace(&mut self.mode, Mode::Idle) {
                Mode::Resizing { axis, key, .. } => {
                    let size = self.ax(axis).overrides.get(&key).copied().unwrap_or(self.ax(axis).default);
                    self.push(Effect::SizeChanged { axis, key, size });
                }
                Mode::HeaderPress { key, shift, start } if (x - start).abs() <= self.drag_threshold => {
                    self.toggle_sort(key, shift);
                }
                _ => {}
            },
        }
    }

    fn toggle_sort(&mut self, key: Key, shift: bool) {
        let at = self.sort.iter().position(|(k, _)| *k == key);
        if !shift && !(at == Some(0) && self.sort.len() == 1) {
            self.sort = vec![(key, Dir::Asc)];
        } else {
            match at {
                Some(i) if self.sort[i].1 == Dir::Asc => self.sort[i].1 = Dir::Desc,
                Some(i) => {
                    self.sort.remove(i);
                }
                None => self.sort.push((key, Dir::Asc)),
            }
        }
        self.push(Effect::SortRequested(self.sort.clone()));
    }

    pub fn action(&mut self, act: Action) {
        match act {
            Action::MoveFocus { axis, forward, by, extend } => {
                let f = *self.focus_end(axis);
                let Some(cur) = f.pos.or(if f.key.is_none() { Some(0) } else { None }) else {
                    // Key known, Position not: wait for `located`.
                    if let Some(key) = f.key {
                        self.pending_reveal = true;
                        self.push(Effect::Locate { axis, key });
                    }
                    return;
                };
                let count = self.ax(axis).count;
                if count == 0 {
                    return;
                }
                let step = match by {
                    By::Item => 1,
                    By::Page => self.ax(axis).visible_items(),
                    By::Edge => count,
                };
                let next = if forward { cur.saturating_add(step).min(count - 1) } else { cur.saturating_sub(step) };
                let old = match self.focus {
                    (FocusEnd { pos: Some(r), .. }, FocusEnd { pos: Some(c), .. }) => Some((r, c)),
                    _ => None,
                };
                // Key may be unknown if `next` is outside the Window; set_window fills it.
                *self.focus_end(axis) = FocusEnd { pos: Some(next), key: self.ax(axis).key_at(next), last_pos: Some(next) };
                if self.focus_end(axis.other()).pos.is_none() && self.focus_end(axis.other()).key.is_none() {
                    let k = self.ax(axis.other()).key_at(0);
                    *self.focus_end(axis.other()) = FocusEnd { pos: Some(0), key: k, last_pos: Some(0) };
                }
                if extend {
                    let anchor = self.range.map(|r| r.0).or(old);
                    let here = (self.focus.0.pos.unwrap_or(0), self.focus.1.pos.unwrap_or(0));
                    self.range = anchor.map(|a| (a, here));
                } else {
                    self.range = None;
                }
                self.ax_mut(axis).reveal(next);
                self.push(Effect::FocusChanged);
                self.push(Effect::SelectionChanged);
            }
            Action::SelectAll => {
                let (r, c) = (self.ax(Rows).count, self.ax(Cols).count);
                if r > 0 && c > 0 {
                    self.range = Some(((0, 0), (r - 1, c - 1)));
                    self.push(Effect::SelectionChanged);
                }
            }
            Action::Activate => {
                if let (Some(row), Some(col)) = (self.focus.0.key, self.focus.1.key) {
                    self.push(Effect::Activated { row, col });
                }
            }
            Action::Cancel => {
                if let Mode::Resizing { axis, key, start_size, .. } = self.mode {
                    self.ax_mut(axis).overrides.insert(key, start_size);
                }
                self.mode = Mode::Idle;
                if self.range.take().is_some() {
                    self.push(Effect::SelectionChanged);
                }
            }
            Action::TypeText(t) => self.push(Effect::TextTyped(t)),
        }
    }
}

impl Axis {
    pub fn other(self) -> Axis {
        match self {
            Rows => Cols,
            Cols => Rows,
        }
    }
}
