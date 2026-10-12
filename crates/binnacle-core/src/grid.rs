use std::collections::VecDeque;
use std::ops::Range;

use crate::axis::AxisState;
use crate::effect::{Action, Effect, FocusStep, Stamped};
use crate::focus::{self, FocusEnd};
use crate::gesture::Gesture;
use crate::measure::{AxisMeasure, WindowChange};
use crate::pointer::{AxisHit, Hit};
use crate::selection::{Cell, HeaderClick, Selection, SelectionUnit, Span};
use crate::snapshot::{Damage, Rect, Snapshot};
use crate::types::{Axis, Generation, Key, Position, Viewport};

/// How one axis starts out: its count, default item size and header band.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisConfig {
    pub count: u64,
    pub default_size: f32,
    pub header: f32,
    pub gap: f32,
}

impl AxisConfig {
    /// `default_size` is in the host's unit and must be positive and finite. Sizes
    /// below the `f32` spacing at the viewport's far edge cannot be laid out
    /// reliably: the Strip ends where offsets stop advancing, short of the edge.
    pub fn new(count: u64, default_size: f32) -> Self {
        AxisConfig {
            count,
            default_size,
            header: 0.0,
            gap: 0.0,
        }
    }

    /// The thickness of the header band leading this axis (see [`Snapshot::header`]).
    pub fn header(self, thickness: f32) -> Self {
        AxisConfig {
            header: thickness,
            ..self
        }
    }

    /// The gridline thickness between items (0 allowed). It sits only between
    /// items, never after the last. Must be finite and not negative.
    pub fn gap(self, gap: f32) -> Self {
        AxisConfig { gap, ..self }
    }
}

/// The Windows [`Grid::begin_frame`] asks the host to fill, at most one per axis.
/// An axis whose current Window already covers what is needed is absent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowRequests([Option<(Axis, Range<Position>)>; 2]);

impl WindowRequests {
    pub fn get(&self, axis: Axis) -> Option<Range<Position>> {
        self.0[axis.index()].as_ref().map(|(_, r)| r.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }
}

impl IntoIterator for WindowRequests {
    type Item = (Axis, Range<Position>);
    type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<(Axis, Range<Position>)>, 2>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().flatten()
    }
}

/// The view controller for one table.
pub struct Grid {
    axes: [AxisState; 2],
    pub(crate) generation: Generation,
    effects: VecDeque<Stamped>,
    pub(crate) pending_damage: Damage,
    pub(crate) damage: Damage,
    /// The focused cell, one end per axis; `None` while unplaced.
    pub(crate) focus: Option<[FocusEnd; 2]>,
    /// The Keys named by the last `FocusChanged` the host drained.
    heard: Option<(Key, Key)>,
    /// A focus move asked for the focused cell to be shown and no scroll has
    /// overridden it; kept until the frame's Snapshot, since the viewport and
    /// sizes the move was made against may change before then.
    reveal: bool,
    pub(crate) selection: Selection,
    pub(crate) unit: SelectionUnit,
    /// Indexed by the axis whose items' headers are clicked.
    pub(crate) header_click: [HeaderClick; 2],
    pub(crate) drag_threshold: f32,
    border_grab: f32,
    pub(crate) gesture: Gesture,
    /// Where the last pointer event was, for hover.
    pub(crate) pointer_at: Option<(f32, f32)>,
    /// The header under the pointer as of the last Snapshot.
    pub(crate) hovered: Option<(Axis, Position)>,
    /// The axis whose header border is under the pointer as of the last
    /// Snapshot.
    pub(crate) hovered_border: Option<Axis>,
    /// Selection fill rects for the last Snapshot.
    pub(crate) fills: Vec<Rect>,
}

/// The drag threshold until [`Grid::set_drag_threshold`] changes it: one unit,
/// a terminal cell. Pixel hosts usually want a few pixels.
pub const DEFAULT_DRAG_THRESHOLD: f32 = 1.0;

impl Grid {
    pub fn new(rows: AxisConfig, cols: AxisConfig) -> Self {
        let axis = |c: AxisConfig| {
            AxisState::new(
                AxisMeasure::new(c.count, c.default_size, c.gap),
                c.header.max(0.0),
            )
        };
        Grid {
            axes: [axis(rows), axis(cols)],
            generation: 0,
            effects: VecDeque::new(),
            pending_damage: Damage::ALL,
            damage: Damage::NONE,
            focus: None,
            heard: None,
            reveal: false,
            selection: Selection::default(),
            unit: SelectionUnit::Cells,
            header_click: [HeaderClick::Select, HeaderClick::Sort],
            drag_threshold: DEFAULT_DRAG_THRESHOLD,
            border_grab: 0.0,
            gesture: Gesture::Idle,
            pointer_at: None,
            hovered: None,
            hovered_border: None,
            fills: Vec::new(),
        }
    }

    /// What a plain click on a cell selects. Default [`SelectionUnit::Cells`].
    pub fn set_selection_unit(&mut self, unit: SelectionUnit) {
        self.unit = unit;
    }

    /// What clicking the header of an item on `axis` does: `Axis::Rows` for
    /// row headers, `Axis::Cols` for column headers.
    pub fn set_header_click(&mut self, axis: Axis, click: HeaderClick) {
        self.header_click[axis.index()] = click;
    }

    /// How far, in the host's unit, a pressed pointer must move before the
    /// press becomes a drag. Values that are not finite and non-negative are
    /// ignored.
    pub fn set_drag_threshold(&mut self, threshold: f32) {
        if threshold.is_finite() && threshold >= 0.0 {
            self.drag_threshold = threshold;
        }
    }

    /// How far, in the host's unit, a header border reaches into the items
    /// either side of the Gap between them. Default 0: only the Gap is the
    /// border, so with Gap 0 nothing can be resized until this is set (pixel
    /// hosts usually want a few pixels). Values that are not finite and
    /// non-negative are ignored.
    pub fn set_border_grab(&mut self, grab: f32) {
        if grab.is_finite() && grab >= 0.0 {
            self.border_grab = grab;
        }
    }

    pub(crate) fn axis(&self, axis: Axis) -> &AxisState {
        &self.axes[axis.index()]
    }

    fn axis_mut(&mut self, axis: Axis) -> &mut AxisState {
        &mut self.axes[axis.index()]
    }

    /// Starts a frame: takes the viewport, advances the generation and returns the
    /// Windows the host must fill with [`Grid::set_window`] before taking the
    /// Snapshot.
    pub fn begin_frame(&mut self, viewport: Viewport) -> WindowRequests {
        self.generation += 1;
        let mut requests = WindowRequests::default();
        for axis in Axis::ALL {
            let extent = viewport.extent(axis);
            let st = self.axis_mut(axis);
            if st.extent != extent {
                st.extent = extent;
                self.pending_damage.layout = true;
            }
            if self.axis_mut(axis).clamp_anchor() {
                self.pending_damage.layout = true;
            }
        }
        if self.reveal {
            self.reveal_focus();
        }
        for axis in Axis::ALL {
            let st = self.axis(axis);
            let needed = st.needed();
            if !st.window_covers(&needed) {
                requests.0[axis.index()] = Some((axis, needed));
            }
        }
        requests
    }

    /// Fills the Window on `axis` with the Keys for Positions `start..`. Keys
    /// past the count are ignored; Positions left unfilled draw as loading.
    pub fn set_window(&mut self, axis: Axis, start: Position, keys: &[Key]) {
        let change = self.axis_mut(axis).measure.set_keys(start, keys);
        self.window_changed(axis, change);
    }

    /// Like [`Grid::set_window`], with the content size of each item (for
    /// example its wrapped text height) and a hint that no content size on this
    /// axis is below `min_size`, which bounds later Windows. A Size Override
    /// still wins over a content size; sizes below the hint are raised to it.
    pub fn set_window_sized(
        &mut self,
        axis: Axis,
        start: Position,
        keys: &[Key],
        sizes: &[f32],
        min_size: f32,
    ) {
        let change = self
            .axis_mut(axis)
            .measure
            .set_keys_sized(start, keys, sizes, min_size);
        self.window_changed(axis, change);
    }

    fn window_changed(&mut self, axis: Axis, change: WindowChange) {
        self.pending_damage.positions |= change.keys;
        self.pending_damage.layout |= change.sizes;
        if let Some(span) = &mut self.selection.span {
            let measure = &self.axes[axis.index()].measure;
            for end in [&mut span.anchor, &mut span.head] {
                let end = &mut end[axis.index()];
                if end.key.is_none() {
                    end.key = measure.key_at(end.position);
                }
            }
        }
        if let Some(f) = &mut self.focus {
            let end = &mut f[axis.index()];
            if end.key.is_none() {
                end.key = self.axes[axis.index()].measure.key_at(end.position);
                if end.key.is_some() {
                    self.pending_damage.selection = true;
                    self.announce_focus();
                }
            }
        }
    }

    /// Sets the Size Override for the item with `key` on `axis`, which follows
    /// that item wherever it moves. Sizes that are not positive and finite are
    /// ignored.
    pub fn set_size(&mut self, axis: Axis, key: Key, size: f32) {
        self.axis_mut(axis).measure.set_size(key, size);
        self.pending_damage.layout = true;
    }

    /// Removes the Size Override for the item with `key` on `axis`, so its
    /// content size or the default applies again.
    pub fn clear_size(&mut self, axis: Axis, key: Key) {
        self.axis_mut(axis).measure.clear_size(key);
        self.pending_damage.layout = true;
    }

    /// The Size Override for the item with `key` on `axis`, if any.
    pub fn size_override(&self, axis: Axis, key: Key) -> Option<f32> {
        self.axis(axis).measure.size_override(key)
    }

    pub fn action(&mut self, action: Action) {
        match action {
            Action::TypeText(text) => self.push(Effect::TextTyped { text }),
            Action::MoveFocus {
                axis,
                by,
                forward,
                extend,
            } => {
                let from = self.focus;
                if !self.move_focus(axis, by, forward) {
                    return;
                }
                match (extend, self.focus) {
                    (true, Some(to)) if from.is_some() => self.extend_to(from, to),
                    (_, Some(to)) => self.set_span(self.unit_span(to)),
                    _ => {}
                }
            }
            Action::SelectAll => self.select_all(),
            Action::SelectColumn => self.select_column(),
            Action::Cancel => self.cancel(),
            Action::Scroll { dx, dy } => {
                self.reveal = false;
                let moved =
                    self.axis_mut(Axis::Cols).scroll(dx) | self.axis_mut(Axis::Rows).scroll(dy);
                self.pending_damage.layout |= moved;
            }
            Action::ScrollTo { axis, fraction } => {
                self.reveal = false;
                self.pending_damage.layout |= self.axis_mut(axis).scroll_to(fraction);
            }
            Action::ScrollBy { axis, items } => {
                self.reveal = false;
                self.pending_damage.layout |= self.axis_mut(axis).scroll_items(items);
            }
        }
    }

    /// The header border under the viewport point (`x`, `y`), as the axis,
    /// Position and Key of the item it ends. Only items with known Keys have
    /// borders.
    pub(crate) fn border_at(&self, x: f32, y: f32) -> Option<(Axis, Position, Key)> {
        let (rows, cols) = (self.axis(Axis::Rows), self.axis(Axis::Cols));
        let (axis, position) = match (rows.hit(y), cols.hit(x)) {
            (AxisHit::Header, AxisHit::Header) => return None,
            (AxisHit::Header, _) => (Axis::Cols, cols.border(x, self.border_grab)?),
            (_, AxisHit::Header) => (Axis::Rows, rows.border(y, self.border_grab)?),
            _ => return None,
        };
        let key = self.axis(axis).measure.key_at(position)?;
        Some((axis, position, key))
    }

    /// What the viewport point (`x`, `y`) lands on, against the current Scroll
    /// Anchors.
    pub(crate) fn hit(&self, x: f32, y: f32) -> Hit {
        Hit::of(self.axis(Axis::Rows).hit(y), self.axis(Axis::Cols).hit(x))
    }

    pub(crate) fn set_focus_cell(&mut self, cell: [FocusEnd; 2]) {
        if self.focus != Some(cell) {
            self.focus = Some(cell);
            self.pending_damage.selection = true;
            self.announce_focus();
        }
    }

    /// Moves focus; returns false if it cannot be placed (an empty axis).
    fn move_focus(&mut self, axis: Axis, by: FocusStep, forward: bool) -> bool {
        if Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0) {
            return false;
        }
        let from = self.focus.map(|f| f[axis.index()].position);
        let mut moved = self.focus.unwrap_or_else(|| {
            [Axis::Rows, Axis::Cols].map(|a| {
                let st = self.axis(a);
                FocusEnd::at(st, focus::target(st, None, FocusStep::Item, true))
            })
        });
        let st = self.axis(axis);
        let to = focus::target(st, from, by, forward);
        if from != Some(to) {
            moved[axis.index()] = FocusEnd::at(st, to);
        }
        self.set_focus_cell(moved);
        self.reveal = true;
        self.reveal_focus();
        true
    }

    /// The cell at `row` and `col`, with whatever Keys the Window holds.
    pub(crate) fn cell_at(&self, row: Position, col: Position) -> Cell {
        [
            FocusEnd::at(self.axis(Axis::Rows), row),
            FocusEnd::at(self.axis(Axis::Cols), col),
        ]
    }

    /// What selecting `cell` alone selects: nothing beyond focus for
    /// [`SelectionUnit::Cells`], its row for [`SelectionUnit::Rows`].
    fn unit_span(&self, cell: Cell) -> Option<Span> {
        match self.unit {
            SelectionUnit::Cells => None,
            SelectionUnit::Rows => Some(Span {
                anchor: cell,
                head: cell,
                whole: [false, true],
            }),
        }
    }

    /// A plain click on `cell`: focus it and select it alone, dropping picks.
    pub(crate) fn select_cell(&mut self, cell: Cell) {
        self.set_focus_cell(cell);
        self.set_selection(Selection {
            span: self.unit_span(cell),
            picks: Vec::new(),
        });
    }

    /// Focuses `head` and stretches the range to it from its anchor, or from
    /// `from` (the focus before the move) if there is no range yet. Whole axes
    /// stay whole.
    pub(crate) fn extend_to(&mut self, from: Option<Cell>, head: Cell) {
        let whole = match (self.selection.span, self.unit_span(head)) {
            (Some(span), _) | (None, Some(span)) => span.whole,
            (None, None) => [false, false],
        };
        self.extend_with(from, head, whole);
    }

    pub(crate) fn extend_with(&mut self, from: Option<Cell>, head: Cell, whole: [bool; 2]) {
        let anchor = self
            .selection
            .span
            .map(|s| s.anchor)
            .or(from)
            .unwrap_or(head);
        self.set_focus_cell(head);
        let same = Axis::ALL
            .iter()
            .all(|a| anchor[a.index()].position == head[a.index()].position);
        let span = (!same || whole != [false, false]).then_some(Span {
            anchor,
            head,
            whole,
        });
        self.set_span(span);
    }

    pub(crate) fn toggle_pick(&mut self, cell: Cell) {
        if let Some(key) = cell[Axis::Rows.index()].key {
            let mut selection = self.selection.clone();
            selection.toggle_pick(key);
            self.set_selection(selection);
        }
    }

    fn select_all(&mut self) {
        if Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0) {
            return;
        }
        let cell = self.focus.unwrap_or_else(|| self.cell_at(0, 0));
        self.set_span(Some(Span {
            anchor: cell,
            head: cell,
            whole: [true, true],
        }));
    }

    /// Selects every row of the range's columns, or of the focused column.
    fn select_column(&mut self) {
        let (anchor, head) = match (self.selection.span, self.focus) {
            (Some(s), _) if !s.whole[Axis::Cols.index()] => (s.anchor, s.head),
            (_, Some(f)) => (f, f),
            _ => return,
        };
        self.set_span(Some(Span {
            anchor,
            head,
            whole: [true, false],
        }));
    }

    fn set_span(&mut self, span: Option<Span>) {
        let mut selection = self.selection.clone();
        selection.span = span;
        self.set_selection(selection);
    }

    /// Replaces the selection, queueing `SelectionChanged` if it changed and
    /// none is queued yet.
    pub(crate) fn set_selection(&mut self, selection: Selection) {
        if self.selection == selection {
            return;
        }
        self.selection = selection;
        self.pending_damage.selection = true;
        if !self
            .effects
            .iter()
            .any(|s| s.effect == Effect::SelectionChanged)
        {
            self.push(Effect::SelectionChanged);
        }
    }

    /// Scrolls both axes to show the focused cell.
    fn reveal_focus(&mut self) {
        let Some(f) = self.focus else { return };
        for axis in Axis::ALL {
            if self.axis_mut(axis).reveal(f[axis.index()].position) {
                self.pending_damage.layout = true;
            }
        }
    }

    /// Called whenever the focus changes: makes the queue hold at most one
    /// `FocusChanged`, naming the focused cell once its Keys are known, unless
    /// the host has already heard of it. A queued one naming an older cell is
    /// dropped.
    pub(crate) fn announce_focus(&mut self) {
        let keys = match self.focus {
            Some(
                [
                    FocusEnd { key: Some(row), .. },
                    FocusEnd { key: Some(col), .. },
                ],
            ) => Some((row, col)),
            _ => None,
        };
        let queued = self
            .effects
            .iter()
            .position(|s| matches!(s.effect, Effect::FocusChanged { .. }));
        if let Some(i) = queued {
            if keys.is_some_and(|(row, col)| {
                self.effects[i].effect == Effect::FocusChanged { row, col }
            }) {
                return;
            }
            self.effects.remove(i);
        }
        if let Some((row, col)) = keys.filter(|&k| self.heard != Some(k)) {
            self.push(Effect::FocusChanged { row, col });
        }
    }

    pub(crate) fn push(&mut self, effect: Effect) {
        self.effects.push_back(Stamped {
            generation: self.generation,
            effect,
        });
    }

    /// Takes the oldest queued Effect, if any.
    pub fn poll_effect(&mut self) -> Option<Stamped> {
        let stamped = self.effects.pop_front()?;
        if let Effect::FocusChanged { row, col } = stamped.effect {
            self.heard = Some((row, col));
        }
        Some(stamped)
    }

    /// Computes the selection fill rects from the laid-out Strips: one for the
    /// visible part of the range, one per visible picked row.
    fn fill_selection(&mut self) {
        let mut fills = std::mem::take(&mut self.fills);
        fills.clear();
        let (rows, cols) = (&self.axis(Axis::Rows).strip, &self.axis(Axis::Cols).strip);
        let counts = Axis::ALL.map(|a| self.axis(a).measure.count());
        let extent = |items: &mut dyn Iterator<Item = &crate::snapshot::StripItem>| {
            let first = items.next()?;
            let last = items.last().unwrap_or(first);
            Some((first.offset, last.end() - first.offset))
        };
        if let Some(range) = self.selection.span.map(|s| s.range(counts)) {
            let r = extent(&mut rows.iter().filter(|i| range.rows.contains(&i.position)));
            let c = extent(&mut cols.iter().filter(|i| range.cols.contains(&i.position)));
            if let (Some((y, height)), Some((x, width))) = (r, c) {
                fills.push(Rect {
                    x,
                    y,
                    width,
                    height,
                });
            }
        }
        if let Some((x, width)) = extent(&mut cols.iter()) {
            for row in rows {
                if row.key.is_some_and(|k| self.selection.picked(k)) {
                    fills.push(Rect {
                        x,
                        y: row.offset,
                        width,
                        height: row.size,
                    });
                }
            }
        }
        self.fills = fills;
    }

    /// Lays out the frame and returns the Snapshot, whose damage covers every
    /// change since the previous Snapshot.
    pub fn snapshot(&mut self) -> Snapshot<'_> {
        for st in &mut self.axes {
            // The Window may have brought sizes that move the anchor's bounds.
            if st.clamp_anchor() {
                self.pending_damage.layout = true;
            }
        }
        if std::mem::take(&mut self.reveal) {
            self.reveal_focus();
        }
        for st in &mut self.axes {
            st.layout();
        }
        let border = self
            .pointer_at
            .and_then(|(x, y)| self.border_at(x, y))
            .map(|(axis, ..)| axis);
        let hovered = self.pointer_at.and_then(|(x, y)| match self.hit(x, y) {
            Hit::Header { axis, at } if !at.in_gap && border.is_none() => Some((axis, at.position)),
            _ => None,
        });
        if (hovered, border) != (self.hovered, self.hovered_border) {
            self.hovered = hovered;
            self.hovered_border = border;
            self.pending_damage.selection = true;
        }
        self.fill_selection();
        self.damage = std::mem::replace(&mut self.pending_damage, Damage::NONE);
        Snapshot { grid: self }
    }
}
