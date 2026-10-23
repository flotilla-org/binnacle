use std::collections::VecDeque;
use std::ops::Range;

use crate::axis::AxisState;
use crate::corner::CornerGrid;
use crate::effect::{Action, Effect, FocusStep, Stamped};
use crate::find::{FindDirection, FindState};
use crate::focus::{self, FocusEnd, Move};
use crate::gesture::Gesture;
use crate::grouping::{ItemKind, Outline};
use crate::measure::{AxisMeasure, WindowChange};
use crate::moves::MoveState;
use crate::notice::Locating;
use crate::pointer::{AxisHit, Hit};
use crate::refilter::Refiltering;
use crate::selection::{Cell, CellRange, HeaderClick, Selection, SelectionUnit, Span};
use crate::snapshot::{Damage, Rect, Snapshot};
use crate::span::HeaderSpan;
use crate::types::{Axis, Edge, Generation, Key, Position, Viewport};
use crate::view::ViewState;

/// How one axis starts out: its count, default item size, header band, Gap
/// and size range.
///
/// Sizes are in the host's unit. Every item size the Grid uses lies in the
/// size range `[min_size, max_size]` (by default 1 to 2^20): the default
/// size, Size Overrides from [`Grid::set_size`], and content sizes and their
/// minimum-size hint from [`Grid::set_window_sized`] are clamped into it, and
/// the Gap is capped at `max_size`. The range keeps layout precise in `f32`
/// and bounds each Window at about the viewport's length over `min_size`
/// items, plus overscan. Layout steps in `f32`, so where the `f32` spacing at
/// the viewport's far edge is a sizeable share of `min_size` each item
/// advances a little less and a few more fit. A `min_size` below that spacing
/// cannot be laid out reliably: the Strip ends where offsets stop advancing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisConfig {
    pub count: u64,
    pub default_size: f32,
    pub header: f32,
    pub gap: f32,
    pub min_size: f32,
    pub max_size: f32,
}

impl AxisConfig {
    /// `default_size` must be positive and finite; [`Grid::new`] panics
    /// otherwise, then clamps it into the size range.
    pub fn new(count: u64, default_size: f32) -> Self {
        AxisConfig {
            count,
            default_size,
            header: 0.0,
            gap: 0.0,
            min_size: AxisMeasure::DEFAULT_MIN_SIZE,
            max_size: AxisMeasure::DEFAULT_MAX_SIZE,
        }
    }

    /// The smallest item size. Must be positive and finite ([`Grid::new`]
    /// panics otherwise).
    pub fn min_size(self, min_size: f32) -> Self {
        AxisConfig { min_size, ..self }
    }

    /// The largest item size. Must be finite and at least `min_size`
    /// ([`Grid::new`] panics otherwise).
    pub fn max_size(self, max_size: f32) -> Self {
        AxisConfig { max_size, ..self }
    }

    /// The thickness of the header band leading this axis (see [`Snapshot::header`]).
    pub fn header(self, thickness: f32) -> Self {
        AxisConfig {
            header: thickness,
            ..self
        }
    }

    /// The gridline thickness between items (0 allowed). It sits only between
    /// items, never after the last. Must be finite and not negative; capped at
    /// `max_size`.
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
    pub(crate) axes: [AxisState; 2],
    pub(crate) generation: Generation,
    pub(crate) effects: VecDeque<Stamped>,
    pub(crate) pending_damage: Damage,
    pub(crate) damage: Damage,
    /// The focused cell, one end per axis; `None` while unplaced.
    pub(crate) focus: Option<[FocusEnd; 2]>,
    /// Focus is in the outline column, on the focused row's band cell; the
    /// column end is where it goes back to.
    pub(crate) outline_focus: bool,
    /// The tier of the row Header Band focus is in, in header form.
    pub(crate) band_tier: u32,
    /// `*` asked to open everything under this Group Item: closed groups
    /// under it are opened as Windows bring them, until another toggle.
    pub(crate) expanding_all: Option<(Axis, Key)>,
    /// The Keys named by the last `FocusChanged` the host drained.
    heard: Option<(Key, Key)>,
    /// A focus move asked for the focused cell to be shown and no scroll has
    /// overridden it; kept until the frame's Snapshot, since the viewport and
    /// sizes the move was made against may change before then.
    pub(crate) reveal: bool,
    pub(crate) locating: Locating,
    pub(crate) refiltering: Refiltering,
    pub(crate) follow_focus_on_reorder: bool,
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
    /// The axis whose header border or band edge is under the pointer as of
    /// the last Snapshot.
    pub(crate) hovered_border: Option<Axis>,
    /// The range for the last Snapshot, as the Positions it covers.
    pub(crate) range: Option<CellRange>,
    /// Selection fill rects for the last Snapshot.
    pub(crate) fills: Vec<Rect>,
    /// Each axis's header-form tiers across its Header Band, and its spans,
    /// for the last Snapshot.
    pub(crate) tiers: [Vec<Range<f32>>; 2],
    pub(crate) spans: [Vec<HeaderSpan>; 2],
    /// The corner grid where the two bands' tiers cross.
    pub(crate) corner: CornerGrid,
    pub(crate) view: ViewState,
    pub(crate) find: FindState,
    pub(crate) moves: MoveState,
}

/// The drag threshold until [`Grid::set_drag_threshold`] changes it: one unit,
/// a terminal cell. Pixel hosts usually want a few pixels.
pub const DEFAULT_DRAG_THRESHOLD: f32 = 1.0;

impl Grid {
    pub fn new(rows: AxisConfig, cols: AxisConfig) -> Self {
        let axis = |c: AxisConfig| {
            AxisState::new(
                AxisMeasure::with_size_range(
                    c.count,
                    c.default_size,
                    c.gap,
                    c.min_size,
                    c.max_size,
                ),
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
            outline_focus: false,
            band_tier: 0,
            expanding_all: None,
            heard: None,
            reveal: false,
            locating: Locating::default(),
            refiltering: Refiltering::default(),
            follow_focus_on_reorder: false,
            selection: Selection::default(),
            unit: SelectionUnit::Cells,
            header_click: [HeaderClick::Select, HeaderClick::Sort],
            drag_threshold: DEFAULT_DRAG_THRESHOLD,
            border_grab: 0.0,
            gesture: Gesture::Idle,
            pointer_at: None,
            hovered: None,
            hovered_border: None,
            range: None,
            fills: Vec::new(),
            tiers: [Vec::new(), Vec::new()],
            spans: [Vec::new(), Vec::new()],
            corner: CornerGrid::default(),
            view: ViewState::default(),
            find: FindState::default(),
            moves: MoveState::default(),
        }
    }

    /// What a plain click on a cell selects. Default [`SelectionUnit::Cells`].
    pub fn set_selection_unit(&mut self, unit: SelectionUnit) {
        self.unit = unit;
    }

    /// What clicking the header of an item on `axis` does: `Axis::Rows` for
    /// row headers, `Axis::Cols` for column headers. [`HeaderClick::Sort`] on
    /// row headers sorts the columns by the clicked row.
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

    /// Whether a `Reordered` Change Notice scrolls to show the focused cell
    /// once its new Position is known. Default off: the Scroll Anchor stays.
    pub fn set_follow_focus_on_reorder(&mut self, follow: bool) {
        self.follow_focus_on_reorder = follow;
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

    pub(crate) fn axis_mut(&mut self, axis: Axis) -> &mut AxisState {
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
            self.refilter_settle(axis);
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
    /// Sizes and hint are clamped into the axis's size range.
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

    /// The outlines of the items at Positions `start..` of a grouped axis,
    /// sent after the Window they belong to ([`Grid::set_window`] drops
    /// them); Positions outside the Window are ignored.
    pub fn set_outlines(&mut self, axis: Axis, start: Position, outlines: &[Outline]) {
        let st = self.axis_mut(axis);
        for (i, outline) in outlines.iter().enumerate() {
            let position = start.checked_add(i as u64);
            if let Some(key) = position.and_then(|p| st.measure.key_at(p)) {
                st.outlines.insert(key, outline.clone());
            }
        }
        self.pending_damage.positions = true;
        self.pin_totals(axis, start, outlines);
        self.keep_expanding(axis);
    }

    /// Pins at the trailing edge each Grand Total item among `outlines`
    /// (from `start`) not pinned by default yet, while the axis's Grand
    /// Total is placed after: one per value, which the Grid knows only by
    /// the Window.
    fn pin_totals(&mut self, axis: Axis, start: Position, outlines: &[Outline]) {
        let after =
            (self.view.applied.grouping.as_ref()).is_some_and(|g| g.grand_total_after(axis));
        if !after {
            return;
        }
        let st = self.axis(axis);
        let keys: Vec<Key> = (outlines.iter().enumerate())
            .filter(|(_, o)| o.kind == ItemKind::GrandTotal)
            .filter_map(|(i, _)| st.measure.key_at(start.checked_add(i as u64)?))
            .filter(|k| !self.view.totals_pinned[axis.index()].contains(k))
            .collect();
        for key in keys {
            self.view.totals_pinned[axis.index()].push(key);
            self.pin_before(axis, Edge::Trailing, &[key], None);
        }
    }

    fn window_changed(&mut self, axis: Axis, change: WindowChange) {
        self.axis_mut(axis).outlines.clear();
        self.axis_mut(axis).sync_pins();
        self.finish_freeze(axis);
        self.pending_damage.positions |= change.keys;
        self.pending_damage.layout |= change.sizes;
        if let Some(span) = &mut self.selection.span {
            let st = &self.axes[axis.index()];
            for end in [&mut span.anchor, &mut span.head] {
                let end = &mut end[axis.index()];
                if end.key.is_none() {
                    end.key = st.key_at(end.position);
                }
            }
        }
        if let Some(f) = &mut self.focus {
            let end = &mut f[axis.index()];
            if end.key.is_none() {
                end.key = self.axes[axis.index()].key_at(end.position);
                if end.key.is_some() {
                    self.pending_damage.selection = true;
                    self.announce_focus();
                }
            }
        }
    }

    /// Sets the Size Override for the item with `key` on `axis`, which follows
    /// that item wherever it moves, clamped into the axis's size range. Sizes
    /// that are not positive and finite are ignored.
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

    /// Sets the thickness of the Header Band leading `axis` (see
    /// [`Snapshot::header`]). 0 hides the band, and a hidden band has no edge
    /// to drag. A drag of that band's edge in progress carries on from here,
    /// and `Cancel` no longer undoes this. Capped at the axis's `max_size`;
    /// values that are not finite and non-negative are ignored.
    pub fn set_header(&mut self, axis: Axis, thickness: f32) {
        if !(thickness.is_finite() && thickness >= 0.0) {
            return;
        }
        let thickness = thickness.min(self.axis(axis).measure.max_size());
        self.put_header(axis, thickness);
        self.rebase_band_drag(axis, thickness);
    }

    /// Sets the band's thickness as given, which may be one the Grid was
    /// built with outside the size range.
    pub(crate) fn put_header(&mut self, axis: Axis, thickness: f32) {
        let st = self.axis_mut(axis);
        if st.header != thickness {
            st.header = thickness;
            self.pending_damage.layout = true;
        }
    }

    /// Sets the sizes of the tiers of the Header Band that heads `axis`'s
    /// items while `axis` is grouped in header form (on [`Axis::Rows`] the
    /// row Header Band's level columns), one per level from the band's
    /// outer edge. Levels past `sizes` share what the band has left; what
    /// the tiers leave heads the items themselves. Sizes that are not
    /// finite and non-negative are taken as 0.
    pub fn set_tiers(&mut self, axis: Axis, sizes: &[f32]) {
        let sizes: Vec<f32> = (sizes.iter())
            .map(|&s| if s.is_finite() && s >= 0.0 { s } else { 0.0 })
            .collect();
        let st = self.axis_mut(axis);
        if st.tier_sizes != sizes {
            st.tier_sizes = sizes;
            self.pending_damage.layout = true;
        }
    }

    /// The thickness of the Header Band leading `axis`, as the next
    /// Snapshot will show it.
    pub fn header(&self, axis: Axis) -> f32 {
        self.axis(axis).header
    }

    /// Sets the Gap between items on `axis`, keeping the Scroll Anchor's item
    /// where it is in the viewport unless the end clamp pulls the view back.
    /// Capped at the axis's `max_size`; values that are not finite and
    /// non-negative are ignored.
    pub fn set_gap(&mut self, axis: Axis, gap: f32) {
        if self.axis_mut(axis).set_gap(gap) {
            self.pending_damage.layout = true;
        }
    }

    /// Sets the Edge Gap on `axis`: the least space between a pinned Strip
    /// and the scrolling Strip, where an adapter draws the pinned edge. The
    /// Gap is used where it is larger. ratatui gives rows, which have no
    /// Gap, a line for their edge this way. Values that are not finite and
    /// non-negative are ignored.
    pub fn set_edge_gap(&mut self, axis: Axis, gap: f32) {
        if self.axis_mut(axis).set_edge_gap(gap) {
            self.pending_damage.layout = true;
        }
    }

    /// Sets the End Gap on `axis` (0 at first): the room kept past the last
    /// item when scrolled to the end, where an adapter draws a closing
    /// line. The end clamp stops the End Gap short of the viewport's end,
    /// and a trailing pinned Strip ends that far short of it; short of the
    /// end, items still reach the viewport's end. A view at the end stays
    /// at the end. Values that are not finite and non-negative are ignored.
    pub fn set_end_gap(&mut self, axis: Axis, gap: f32) {
        if self.axis_mut(axis).set_end_gap(gap) {
            self.pending_damage.layout = true;
        }
    }

    /// Sets the least length along `axis` the pinned Strips leave to the
    /// scrolling Strip (0 at first): pinned Strips too long to leave it are
    /// clipped at their inside ends, the trailing one first, and nothing
    /// unpins. Negative or non-finite lengths are ignored.
    pub fn set_min_scrolling(&mut self, axis: Axis, length: f32) {
        if self.axis_mut(axis).set_min_scrolling(length) {
            self.pending_damage.layout = true;
        }
    }

    /// The Size Override for the item with `key` on `axis`, if any.
    pub fn size_override(&self, axis: Axis, key: Key) -> Option<f32> {
        self.axis(axis).measure.size_override(key)
    }

    /// The Keys pinned to `edge` of `axis`, in pin order, dormant pins
    /// included, as the next Snapshot lists them ([`Snapshot::pins`]).
    pub fn pins(&self, axis: Axis, edge: Edge) -> impl Iterator<Item = Key> + '_ {
        self.axis(axis).pins[edge.index()].iter().map(|p| p.key)
    }

    /// Every Size Override on `axis` in Key order, for Keys shown or not:
    /// what a saved layout keeps.
    pub fn size_overrides(&self, axis: Axis) -> impl Iterator<Item = (Key, f32)> + '_ {
        self.axis(axis).measure.size_overrides()
    }

    pub fn action(&mut self, action: Action) {
        match action {
            Action::TypeText(text) => {
                if !self.tree_typed(&text) {
                    self.push(Effect::TextTyped { text })
                }
            }
            Action::Activate if self.tree_activate() => {}
            Action::Activate => {
                if let Some((row, col)) = self.focused_keys() {
                    self.activate(row, col);
                }
            }
            Action::Copy => self.copy(),
            Action::BeginEdit => {
                if let Some((row, col)) = self.focused_keys()
                    && !self.summary_cell(row, col)
                {
                    self.push(Effect::EditRequested { row, col });
                }
            }
            Action::MoveFocus {
                axis,
                by,
                forward,
                extend,
            } => {
                if axis == Axis::Cols && self.tree_move(by, forward) {
                    return;
                }
                let m = Move {
                    axis,
                    by,
                    forward,
                    extend,
                    times: 1,
                };
                if !self.locating.any() && !self.pins_waiting() {
                    return self.move_and_select(m);
                }
                let moves = &mut self.locating.moves;
                match moves.last_mut() {
                    Some(last) if Move { times: 1, ..*last } == m => {
                        last.times = last.times.saturating_add(1)
                    }
                    _ => moves.push(m),
                }
            }
            Action::SelectAll => self.select_all(),
            Action::SelectColumn => self.select_whole(Axis::Cols),
            Action::SelectRow => self.select_whole(Axis::Rows),
            Action::Cancel => {
                if self.gesture.capturing() {
                    self.cancel();
                    self.relocate();
                } else {
                    self.end_edit();
                }
            }
            Action::Scroll { dx, dy } => {
                self.reveal = false;
                if dx != 0.0 {
                    self.refilter_scrolled(Axis::Cols);
                }
                if dy != 0.0 {
                    self.refilter_scrolled(Axis::Rows);
                }
                let moved =
                    self.axis_mut(Axis::Cols).scroll(dx) | self.axis_mut(Axis::Rows).scroll(dy);
                self.pending_damage.layout |= moved;
            }
            Action::ScrollTo { axis, fraction } => {
                self.reveal = false;
                self.refilter_scrolled(axis);
                self.pending_damage.layout |= self.axis_mut(axis).scroll_to(fraction);
            }
            Action::ScrollBy { axis, items } => {
                self.reveal = false;
                self.refilter_scrolled(axis);
                self.pending_damage.layout |= self.axis_mut(axis).scroll_items(items);
            }
            Action::LeaveOutline => self.leave_outline(),
            Action::FindNext => self.find_step(FindDirection::Next),
            Action::FindPrevious => self.find_step(FindDirection::Previous),
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
        let key = self.axis(axis).key_at(position)?;
        Some((axis, position, key))
    }

    /// The axis whose Header Band has its edge under the viewport point (`x`,
    /// `y`). Like an item border, a band edge is grabbed in the other band,
    /// where the two meet: the last Gap's length of the band, widened by the
    /// border grab either side. A band of thickness 0 has no edge. Where both
    /// edges reach, the row-number band's (on [`Axis::Cols`]) wins.
    pub(crate) fn band_edge_at(&self, x: f32, y: f32) -> Option<Axis> {
        [(Axis::Cols, x, y), (Axis::Rows, y, x)]
            .into_iter()
            .find(|&(axis, along, across)| {
                let (st, other) = (self.axis(axis), self.axis(axis.other()));
                let (h, grab) = (st.header, self.border_grab);
                let start = h - st.measure.gap() - grab;
                h > 0.0
                    && matches!(other.hit(across), AxisHit::Header)
                    && along >= start.max(0.0)
                    && along < (h + grab).min(st.extent)
            })
            .map(|(axis, ..)| axis)
    }

    /// What the viewport point (`x`, `y`) lands on, against the current Scroll
    /// Anchors.
    pub(crate) fn hit(&self, x: f32, y: f32) -> Hit {
        Hit::of(self.axis(Axis::Rows).hit(y), self.axis(Axis::Cols).hit(x))
    }

    pub(crate) fn set_focus_cell(&mut self, cell: [FocusEnd; 2]) {
        self.before_focus_set(cell);
        if self.focus != Some(cell) {
            self.focus = Some(cell);
            self.pending_damage.selection = true;
            self.announce_focus();
        }
    }

    /// Moves focus to the cell with `row` and `col` (type-ahead, "go to
    /// row"), selecting as a focus move does and scrolling to show it. A Key
    /// outside the Window is found with `Locate`; until `located` answers,
    /// focus keeps its last Position on that axis and later focus moves wait.
    /// A Key the Table Source no longer has leaves focus where it was on that
    /// axis. A gesture in progress keeps going, but `Cancel` no longer undoes
    /// this. Nothing happens while either axis is empty.
    pub fn set_focus(&mut self, row: Key, col: Key) {
        self.place_focus(row, col, true);
    }

    /// As [`Grid::set_focus`], selecting the focus as a focus move does only
    /// if `select`; the view follows either way.
    pub(crate) fn place_focus(&mut self, row: Key, col: Key, select: bool) {
        if Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0) {
            return;
        }
        let keys = [row, col];
        let mut cell = self.focus.unwrap_or_else(|| {
            let at = |a: Axis| self.axis(a).anchor.position;
            self.cell_at(at(Axis::Rows), at(Axis::Cols))
        });
        let mut unknown = Vec::new();
        for axis in Axis::ALL {
            let i = axis.index();
            let st = self.axis(axis);
            let position = st.measure.position_of(keys[i]).unwrap_or_else(|| {
                unknown.push(axis);
                cell[i].position.min(st.measure.count() - 1)
            });
            cell[i] = FocusEnd {
                position,
                key: Some(keys[i]),
            };
        }
        self.set_focus_cell(cell);
        for &axis in &unknown {
            self.locate(axis, keys[axis.index()]);
        }
        if unknown.is_empty() {
            if select {
                self.set_span(self.unit_span(cell));
            }
            self.reveal = true;
            self.reveal_focus();
        } else if select {
            self.set_span(None);
            self.locating.select_when_placed = true;
        } else {
            self.locating.reveal_when_placed = true;
        }
        self.rebase_gesture();
    }

    /// Scrolls `axis` the least distance that shows the item at `position`
    /// (its start, if it is longer than the body), leaving focus where it is.
    /// A Position past the end is ignored.
    pub fn reveal(&mut self, axis: Axis, position: Position) {
        if position >= self.axis(axis).measure.count() {
            return;
        }
        self.reveal = false;
        self.refilter_scrolled(axis);
        self.pending_damage.layout |= self.axis_mut(axis).reveal(position);
    }

    /// Scrolls `axis` so the item at `position` starts the body, or as near
    /// as the end allows. A Position past the end is ignored.
    pub fn scroll_to_item(&mut self, axis: Axis, position: Position) {
        if position >= self.axis(axis).measure.count() {
            return;
        }
        self.reveal = false;
        self.refilter_scrolled(axis);
        self.pending_damage.layout |= self.axis_mut(axis).scroll_to_item(position);
    }

    /// A `MoveFocus`: moves focus, then replaces the range or stretches it.
    pub(crate) fn move_and_select(&mut self, m: Move) {
        let from = self.focus;
        if !self.move_focus(m.axis, m.by, m.forward, m.times) {
            return;
        }
        match (m.extend, self.focus) {
            (true, Some(to)) if from.is_some() => self.extend_to(from, to),
            (_, Some(to)) => self.set_span(self.unit_span(to)),
            _ => {}
        }
    }

    /// Moves focus; returns false if it cannot be placed (an empty axis).
    fn move_focus(&mut self, axis: Axis, by: FocusStep, forward: bool, times: u64) -> bool {
        if Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0) {
            return false;
        }
        // Unfocused, the first move only places focus; the rest go from there.
        if self.focus.is_none() && times > 1 {
            self.move_focus(axis, by, forward, 1);
            return self.move_focus(axis, by, forward, times - 1);
        }
        let from = self.focus.map(|f| f[axis.index()]);
        let mut moved = self.focus.unwrap_or_else(|| {
            [Axis::Rows, Axis::Cols]
                .map(|a| focus::target(self.axis(a), None, FocusStep::Item, true, 1))
        });
        let to = focus::target(self.axis(axis), from, by, forward, times);
        if from.map(|f| (f.position, f.key)) != Some((to.position, to.key)) {
            moved[axis.index()] = to;
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
    /// [`SelectionUnit::Cells`], its row for [`SelectionUnit::Rows`], its
    /// column for [`SelectionUnit::Cols`].
    pub(crate) fn unit_span(&self, cell: Cell) -> Option<Span> {
        let whole = match self.unit {
            SelectionUnit::Cells => return None,
            SelectionUnit::Rows => [false, true],
            SelectionUnit::Cols => [true, false],
        };
        Some(Span {
            anchor: cell,
            head: cell,
            whole,
        })
    }

    /// The axis whose item a Ctrl/Cmd-click on a cell picks: the columns for
    /// [`SelectionUnit::Cols`], else the rows.
    pub(crate) fn pick_axis(&self) -> Axis {
        match self.unit {
            SelectionUnit::Cols => Axis::Cols,
            SelectionUnit::Cells | SelectionUnit::Rows => Axis::Rows,
        }
    }

    /// A plain click on `cell`: focus it and select it alone, dropping picks.
    pub(crate) fn select_cell(&mut self, cell: Cell) {
        self.set_focus_cell(cell);
        self.set_selection(Selection {
            span: self.unit_span(cell),
            picks: Default::default(),
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
        // A focus whose Position is unknown cannot anchor a range.
        let from = from.filter(|_| !self.locating.any());
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

    /// Toggles the pick of `cell`'s item on `axis`, once its Key is known.
    pub(crate) fn toggle_pick(&mut self, axis: Axis, cell: Cell) {
        if let Some(key) = cell[axis.index()].key {
            let mut selection = self.selection.clone();
            selection.toggle_pick(axis, key);
            self.set_selection(selection);
        }
    }

    fn select_all(&mut self) {
        self.locating.moves.clear();
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

    /// Selects the whole of the range's items on `axis` (every row of its
    /// columns, for `Axis::Cols`), or of the focused item.
    fn select_whole(&mut self, axis: Axis) {
        self.locating.moves.clear();
        let (anchor, head) = match (self.selection.span, self.focus) {
            (Some(s), _) if !s.whole[axis.index()] => (s.anchor, s.head),
            (_, Some(f)) => (f, f),
            _ => return,
        };
        let mut whole = [false; 2];
        whole[axis.other().index()] = true;
        self.set_span(Some(Span {
            anchor,
            head,
            whole,
        }));
    }

    pub(crate) fn set_span(&mut self, span: Option<Span>) {
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
        // A range set anew no longer waits on a `Refiltered`.
        if selection.span != self.selection.span {
            self.refiltering.corners = [None, None];
        }
        self.selection = selection;
        self.announce_selection();
    }

    /// Queues `SelectionChanged` unless one is already queued.
    pub(crate) fn announce_selection(&mut self) {
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
    pub(crate) fn reveal_focus(&mut self) {
        let Some(f) = self.focus else { return };
        for axis in Axis::ALL {
            if self.axis_mut(axis).reveal_focused(f[axis.index()]) {
                self.pending_damage.layout = true;
            }
        }
    }

    /// The focused cell's row and column Keys, once both are known.
    pub(crate) fn focused_keys(&self) -> Option<(Key, Key)> {
        match self.focus {
            Some(
                [
                    FocusEnd { key: Some(row), .. },
                    FocusEnd { key: Some(col), .. },
                ],
            ) => Some((row, col)),
            _ => None,
        }
    }

    /// Called whenever the focus changes: makes the queue hold at most one
    /// `FocusChanged`, naming the focused cell once its Keys are known, unless
    /// the host has already heard of it. A queued one naming an older cell is
    /// dropped.
    pub(crate) fn announce_focus(&mut self) {
        let keys = self.focused_keys();
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

    /// Takes the oldest queued Effect, if any. Requests the Table Source
    /// answers (`ViewRequested`, `Locate`, `MoveRequested`, `FindRequested`,
    /// `FindStepRequested`) come out before the rest, so a
    /// host that answers as it drains does so before it hears of focus and
    /// selection, which the answers may change again.
    pub fn poll_effect(&mut self) -> Option<Stamped> {
        let request = self.effects.iter().position(|s| {
            matches!(
                s.effect,
                Effect::ViewRequested { .. }
                    | Effect::Locate { .. }
                    | Effect::MoveRequested { .. }
                    | Effect::FindRequested { .. }
                    | Effect::FindStepRequested { .. }
            )
        });
        let stamped = match request {
            Some(i) => self.effects.remove(i)?,
            None => self.effects.pop_front()?,
        };
        if let Effect::FocusChanged { row, col } = stamped.effect {
            self.heard = Some((row, col));
        }
        Some(stamped)
    }

    /// Computes the selection fill rects from the laid-out Strips: one for the
    /// visible part of the range in each pair of a row Strip and a column
    /// Strip (leading pinned, scrolling, trailing pinned), then one per visible picked row and
    /// column Strip, then one per visible picked column and row Strip. A
    /// pinned Strip's part is clipped to its region; one it clips whole is
    /// left out.
    fn fill_selection(&mut self) {
        let mut fills = std::mem::take(&mut self.fills);
        fills.clear();
        // Each Strip with where it draws: a pinned one's region, while the
        // scrolling one's items are left unclipped.
        let strips = |axis: Axis| {
            let st = self.axis(axis);
            let [leading, trailing] = &st.pinned;
            let [lead, trail] = st.regions.clone();
            let all = f32::NEG_INFINITY..f32::INFINITY;
            [(leading, lead), (&st.strip, all), (trailing, trail)]
        };
        let (rows, cols) = (strips(Axis::Rows), strips(Axis::Cols));
        let extent =
            |(items, region): &(&Vec<crate::snapshot::StripItem>, std::ops::Range<f32>),
             keep: &dyn Fn(&crate::snapshot::StripItem) -> bool| {
                let mut items = items.iter().filter(|i| keep(i));
                let first = items.next()?;
                let last = items.last().unwrap_or(first);
                let (start, end) = (first.offset.max(region.start), last.end().min(region.end));
                (start < end).then_some((start, end - start))
            };
        if let Some(range) = &self.range {
            for rows in &rows {
                for cols in &cols {
                    let r = extent(rows, &|i| range.covers(Axis::Rows, i.position));
                    let c = extent(cols, &|i| range.covers(Axis::Cols, i.position));
                    if let (Some((y, height)), Some((x, width))) = (r, c) {
                        fills.push(Rect {
                            x,
                            y,
                            width,
                            height,
                        });
                    }
                }
            }
        }
        let picked = |axis, i: &crate::snapshot::StripItem| {
            i.key.is_some_and(|k| self.selection.picked(axis, k))
        };
        for strip in &rows {
            for row in strip.0.iter().filter(|i| picked(Axis::Rows, i)) {
                let Some((y, height)) = extent(&(strip.0, strip.1.clone()), &|i| i == row) else {
                    continue;
                };
                for cols in &cols {
                    if let Some((x, width)) = extent(cols, &|_| true) {
                        fills.push(Rect {
                            x,
                            y,
                            width,
                            height,
                        });
                    }
                }
            }
        }
        for strip in &cols {
            for col in strip.0.iter().filter(|i| picked(Axis::Cols, i)) {
                let Some((x, width)) = extent(&(strip.0, strip.1.clone()), &|i| i == col) else {
                    continue;
                };
                for rows in &rows {
                    if let Some((y, height)) = extent(rows, &|_| true) {
                        fills.push(Rect {
                            x,
                            y,
                            width,
                            height,
                        });
                    }
                }
            }
        }
        self.fills = fills;
    }

    /// Lays out the frame and returns the Snapshot, whose damage covers every
    /// change since the previous Snapshot.
    pub fn snapshot(&mut self) -> Snapshot<'_> {
        for axis in Axis::ALL {
            let focused = self.focus.and_then(|f| f[axis.index()].key);
            self.axis_mut(axis).focused = focused;
        }
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
            if st.layout() {
                self.pending_damage.layout = true;
            }
        }
        let border = self.pointer_at.and_then(|(x, y)| {
            self.band_edge_at(x, y)
                .or_else(|| self.border_at(x, y).map(|(axis, ..)| axis))
        });
        let hovered = self.pointer_at.and_then(|(x, y)| match self.hit(x, y) {
            Hit::Header { axis, at } if !at.in_gap && border.is_none() => Some((axis, at.position)),
            _ => None,
        });
        if (hovered, border) != (self.hovered, self.hovered_border) {
            self.hovered = hovered;
            self.hovered_border = border;
            self.pending_damage.selection = true;
        }
        self.range = (self.selection.span)
            .filter(|_| !self.refiltering.locating_corners())
            .map(|s| s.range(&self.axes));
        self.fill_selection();
        self.lay_out_spans();
        self.lay_out_corner();
        self.damage = std::mem::replace(&mut self.pending_damage, Damage::NONE);
        Snapshot { grid: self }
    }
}
