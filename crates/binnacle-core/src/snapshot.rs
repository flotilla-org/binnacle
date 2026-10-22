use crate::find::{FindMatch, FindQuery};
use crate::focus::FocusEnd;
use crate::gesture::Mode;
use crate::grid::Grid;
use crate::grouping::{Aggregate, AggregateValue, GRAND_TOTAL, Level, Outline};
use crate::selection::CellRange;
use crate::sort::HeaderSort;
use std::ops::Range;

use crate::types::{Axis, Edge, Generation, Key, Position};
use crate::view::{Condition, ConditionState, HeaderFilter, Support, TextSupport, ViewSpec};

/// One visible item on an axis. `offset` is from the viewport's leading edge
/// along the axis; `key` is `None` while the Table Source has not filled that
/// Position (the host draws it as loading).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripItem {
    pub position: Position,
    pub key: Option<Key>,
    pub offset: f32,
    pub size: f32,
}

impl StripItem {
    pub fn end(&self) -> f32 {
        self.offset + self.size
    }
}

/// One axis's scroll state: the item at the viewport's leading edge (just past
/// the header band) and how far into it the view has scrolled. An offset past
/// the item's size is in the Gap that follows it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollAnchor {
    pub position: Position,
    pub offset: f32,
}

/// An estimated scrollbar thumb for one axis, in `f64` so it stays meaningful
/// on huge axes. `fraction` is how far through its travel the thumb is (0 at
/// the start, 1 at the end); `size` is the share of the axis the viewport
/// shows. Sizes the Grid has not seen are assumed default.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Thumb {
    pub fraction: f64,
    pub size: f64,
}

/// A viewport-relative rectangle in the host's unit.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// How to draw one cell, from [`Snapshot::cell_state`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellState {
    pub focused: bool,
    /// Inside the range, or in a picked row or column.
    pub selected: bool,
    /// The host's editor is open on this cell.
    pub editing: bool,
    /// The current find match, drawn apart from the other matches.
    pub current_match: bool,
}

/// One header to draw, from [`Snapshot::headers`]: the header of a visible
/// item on `axis` (a row header is on [`Axis::Rows`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderPlacement {
    pub axis: Axis,
    pub position: Position,
    /// `None` while the Table Source has not filled the item's Position.
    pub key: Option<Key>,
    /// The header's rectangle in its band, unclipped like the Strip item.
    pub rect: Rect,
    /// How it shows the sort of the other axis: a column header the rows'
    /// sort, a row header the columns'.
    pub sort: HeaderSort,
    /// How it shows the conditions read in it (row headers show none).
    pub filter: HeaderFilter,
    /// Its whole item is selected: the range spans every item across it, or
    /// the item is picked.
    pub selected: bool,
    /// The pointer is over it (see [`Snapshot::hovered_header`]).
    pub hovered: bool,
    /// A move of its item waits for the Table Source's answer: drawn
    /// pending, like a sort, while the order stays as it is.
    pub move_pending: bool,
}

/// The pointer shape the host should show, if its toolkit can change it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Cursor {
    #[default]
    Default,
    /// Over a column header border, or resizing a column.
    ResizeCol,
    /// Over a row header border, or resizing a row.
    ResizeRow,
    /// Moving headers.
    Move,
}

/// Where a resize in progress puts the resized item's moving edge: a line
/// across the viewport at `offset` along `axis` (viewport-relative, like Strip
/// offsets), with the item's Key and current size. That is its trailing edge,
/// or its leading edge in the trailing pinned Strip, which grows from the
/// viewport's far end. Resizing the Header Band
/// leading `axis`, `key` is `None` and `size` is the band's thickness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGuide {
    pub axis: Axis,
    pub key: Option<Key>,
    pub offset: f32,
    pub size: f32,
}

/// What releasing headers being dragged would do. Pinning, unpinning and
/// reordering pins are the Grid's own, so they are offered on any axis;
/// moving is asked of the Table Source, and only on a movable axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropTarget {
    /// Request a move of the dragged items to before Position `to` (the
    /// count for the end). `unpin` when the dragged item is pinned: it is
    /// unpinned at once, and stays unpinned in its own place if the move is
    /// refused.
    Move { to: Position, unpin: bool },
    /// Unpin the dragged item, which returns to its own Position `at`: the
    /// axis can't move.
    Unpin { at: Position },
    /// Pin the dragged items to `edge`, drawn just before the pinned item
    /// with Key `before` (after the last for `None`). `reorder` when the
    /// dragged item is pinned there already, so the drop only reorders the
    /// pins.
    Pin {
        edge: Edge,
        before: Option<Key>,
        reorder: bool,
    },
    /// Nothing: unpinned items over the body of an axis that can't move.
    Stay,
}

/// Where headers being dragged would drop, and what that would do, drawn as
/// a line across the viewport at `offset` along `axis` (viewport-relative,
/// like Strip offsets): the middle of the Gap before the item they drop
/// before, as if a Gap also led the first; into a pinned Strip, kept inside
/// the viewport. For [`DropTarget::Unpin`] it marks the item's own Position,
/// kept inside the scrolling region when that is out of view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropIndicator {
    pub axis: Axis,
    pub target: DropTarget,
    pub offset: f32,
}

/// The pin zone at `edge` of `axis` while headers are dragged on it and that
/// edge's pinned Strip is empty: `size` long from `offset` (viewport
/// relative), lit for the host to draw; `hot` while a drop would pin there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinZone {
    pub axis: Axis,
    pub edge: Edge,
    pub offset: f32,
    pub size: f32,
    pub hot: bool,
}

/// What changed since the previous Snapshot, so a host can skip work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Damage {
    /// Geometry: viewport, sizes or item placement.
    pub layout: bool,
    /// Focus or selection.
    pub selection: bool,
    /// Which Key sits at which Position.
    pub positions: bool,
}

impl Damage {
    pub const ALL: Damage = Damage {
        layout: true,
        selection: true,
        positions: true,
    };
    pub const NONE: Damage = Damage {
        layout: false,
        selection: false,
        positions: false,
    };

    pub fn any(self) -> bool {
        self.layout || self.selection || self.positions
    }
}

/// The Grid's read-only description of what to draw this frame. It borrows the
/// Grid's buffers, so it lives until the next mutating call and allocates
/// nothing.
pub struct Snapshot<'a> {
    pub(crate) grid: &'a Grid,
}

impl<'a> Snapshot<'a> {
    pub fn generation(&self) -> Generation {
        self.grid.generation
    }

    pub fn damage(&self) -> Damage {
        self.grid.damage
    }

    pub fn count(&self, axis: Axis) -> u64 {
        self.grid.axis(axis).measure.count()
    }

    /// The viewport's length along `axis`, header band included.
    pub fn extent(&self, axis: Axis) -> f32 {
        self.grid.axis(axis).extent
    }

    /// The thickness of the header band leading `axis`: on [`Axis::Rows`] the
    /// column header band's height, on [`Axis::Cols`] the row header band's width.
    pub fn header(&self, axis: Axis) -> f32 {
        self.grid.axis(axis).header
    }

    pub fn scroll_anchor(&self, axis: Axis) -> ScrollAnchor {
        self.grid.axis(axis).anchor
    }

    pub fn thumb(&self, axis: Axis) -> Thumb {
        self.grid.axis(axis).thumb()
    }

    /// The Gap between items on `axis`.
    pub fn gap(&self, axis: Axis) -> f32 {
        self.grid.axis(axis).measure.gap()
    }

    /// The outline the Table Source sent for the item at `position` on
    /// `axis`, if the axis is grouped and the item is in the Window. A
    /// pinned Grand Total has its outline wherever it is.
    pub fn outline(&self, axis: Axis, position: Position) -> Option<&'a Outline> {
        static GRAND: Outline = Outline::grand_total();
        let st = self.grid.axis(axis);
        let key = st.key_at(position)?;
        let grouped =
            (self.applied_view().grouping.as_ref()).is_some_and(|g| !g.levels(axis).is_empty());
        match st.outlines.get(&key) {
            None if key == GRAND_TOTAL && grouped => Some(&GRAND),
            found => found,
        }
    }

    /// The scrolling Strip of `axis`: the visible items that follow the
    /// Scroll Anchor, pinned items left out. Its items are drawn clipped to
    /// [`Snapshot::scrolling`].
    pub fn strip(&self, axis: Axis) -> &'a [StripItem] {
        &self.grid.axis(axis).strip
    }

    /// The pinned Strip at `edge` of `axis`: the pinned items in pin order,
    /// laid out from the Header Band with a Gap after each, whatever the
    /// scroll. Its items carry their Positions, like the scrolling Strip's.
    pub fn pinned(&self, axis: Axis, edge: Edge) -> &'a [StripItem] {
        &self.grid.axis(axis).pinned[edge.index()]
    }

    /// The Keys pinned to `edge` of `axis`, in pin order, dormant pins
    /// included: what a saved layout keeps, and what `PinsChanged` reports.
    pub fn pins(&self, axis: Axis, edge: Edge) -> impl Iterator<Item = Key> + 'a {
        self.grid.axis(axis).pins[edge.index()]
            .iter()
            .map(|p| p.key)
    }

    /// Where the scrolling Strip of `axis` draws along it: past the Header
    /// Band and the leading pinned Strip (with the Gap after it), to the
    /// viewport's end or the Gap before the trailing pinned Strip. Its
    /// part-scrolled items reach outside it.
    pub fn scrolling(&self, axis: Axis) -> Range<f32> {
        let st = self.grid.axis(axis);
        let end = st.scroll_end().clamp(0.0, st.extent);
        st.scroll_start().min(end)..end
    }

    /// Where the pinned Strip at `edge` of `axis` draws along it: from the
    /// Header Band, or back from the viewport's far end, to where it
    /// clips. Its items are drawn clipped to it; one it clips whole is
    /// still in [`Snapshot::pinned`]. Unclipped, it ends where its items
    /// do (empty if there are none).
    pub fn pinned_region(&self, axis: Axis, edge: Edge) -> Range<f32> {
        self.grid.axis(axis).regions[edge.index()].clone()
    }

    /// Every drawn item of `axis` in drawn order: the leading pinned Strip,
    /// the scrolling Strip, then the trailing pinned Strip.
    pub fn drawn(&self, axis: Axis) -> impl Iterator<Item = &'a StripItem> + 'a {
        let st = self.grid.axis(axis);
        let [leading, trailing] = &st.pinned;
        leading.iter().chain(&st.strip).chain(trailing)
    }

    pub fn rows(&self) -> &'a [StripItem] {
        self.strip(Axis::Rows)
    }

    pub fn cols(&self) -> &'a [StripItem] {
        self.strip(Axis::Cols)
    }

    /// The focused item on `axis`, `None` while focus is unplaced (on both
    /// axes at once).
    pub fn focus(&self, axis: Axis) -> Option<FocusEnd> {
        self.grid.focus.map(|f| f[axis.index()])
    }

    /// Whether the focused Key on `axis` is waiting for `located` after a
    /// Change Notice. Its [`FocusEnd::position`] is then the last known one,
    /// and the focus is drawn only if the Window holds its Key there.
    pub fn locating(&self, axis: Axis) -> bool {
        self.grid.locating.waiting[axis.index()]
    }

    /// The focused cell's item on `axis` in its Strip, if it is visible and its
    /// Key is known.
    fn focused_item(&self, axis: Axis) -> Option<&'a StripItem> {
        let end = self.focus(axis)?;
        end.key?;
        if let Some(item) = (Edge::ALL.iter())
            .flat_map(|&edge| self.pinned(axis, edge))
            .find(|i| i.key == end.key && i.position == end.position)
        {
            return Some(item);
        }
        // The scrolling Strip skips pinned Positions, so it is searched.
        let strip = self.strip(axis);
        let i = strip
            .binary_search_by_key(&end.position, |i| i.position)
            .ok()?;
        let item = &strip[i];
        (item.key == end.key).then_some(item)
    }

    /// Whether focus is in the outline column: on the focused row's cell
    /// of the row Header Band, where the tree keys act, rather than on the
    /// focused column's body cell.
    pub fn outline_focused(&self) -> bool {
        self.grid.in_outline()
    }

    /// Whether the disclosure mark of the Group Item `key` on `axis` is
    /// pending: the user opened or closed it and the Table Source has not
    /// answered yet. Adapters draw it so only once it has been pending a
    /// moment (about 150 ms), so a fast source doesn't flicker.
    pub fn toggle_pending(&self, axis: Axis, key: Key) -> bool {
        self.grid.toggle_pending(axis, key)
    }

    /// The focused cell's rectangle, unclipped, if it is visible and its Keys
    /// are known: in the outline column, the focused row's band cell.
    pub fn focus_rect(&self) -> Option<Rect> {
        let row = self.focused_item(Axis::Rows)?;
        if self.outline_focused() {
            return Some(Rect {
                x: 0.0,
                y: row.offset,
                width: self.header(Axis::Cols),
                height: row.size,
            });
        }
        let col = self.focused_item(Axis::Cols)?;
        Some(Rect {
            x: col.offset,
            y: row.offset,
            width: col.size,
            height: row.size,
        })
    }

    /// Where the host's editor goes while editing: the edited cell's
    /// rectangle, unclipped, if it is visible.
    pub fn editor_rect(&self) -> Option<Rect> {
        let Mode::Editing { row, col } = self.mode() else {
            return None;
        };
        let find = |axis, key| self.drawn(axis).find(|i| i.key == Some(key));
        let (row, col) = (find(Axis::Rows, row)?, find(Axis::Cols, col)?);
        Some(Rect {
            x: col.offset,
            y: row.offset,
            width: col.size,
            height: row.size,
        })
    }

    /// How to draw the cell where `row` and `col` from this Snapshot's Strips
    /// cross.
    pub fn cell_state(&self, row: &StripItem, col: &StripItem) -> CellState {
        let on = |axis, item: &StripItem| {
            self.focus(axis).is_some_and(|f| {
                f.key.is_some() && f.position == item.position && f.key == item.key
            })
        };
        let in_range =
            (self.selection_range()).is_some_and(|r| r.contains(row.position, col.position));
        let picked = |axis, item: &StripItem| {
            item.key
                .is_some_and(|k| self.grid.selection.picked(axis, k))
        };
        let picked = picked(Axis::Rows, row) || picked(Axis::Cols, col);
        let editing = match self.mode() {
            Mode::Editing { row: r, col: c } => row.key == Some(r) && col.key == Some(c),
            _ => false,
        };
        let current_match = self
            .grid
            .find
            .current
            .is_some_and(|m| (row.key, col.key) == (Some(m.row), Some(m.col)));
        CellState {
            focused: on(Axis::Rows, row) && on(Axis::Cols, col) && !self.outline_focused(),
            selected: in_range || picked,
            editing,
            current_match,
        }
    }

    /// The one contiguous range, if any, as the Positions it covers on each
    /// axis: every item drawn between its corners, in drawn order.
    pub fn selection_range(&self) -> Option<&'a CellRange> {
        self.grid.range.as_ref()
    }

    /// The picked Keys of `axis`, in Key order.
    pub fn picked(&self, axis: Axis) -> &'a [Key] {
        &self.grid.selection.picks[axis.index()]
    }

    /// Rects to fill for the selection: the visible part of the range, then
    /// each visible picked row across the visible columns, then each visible
    /// picked column down the visible rows. They may overlap.
    /// Edges are those of the Strip items, unclipped, like [`Self::focus_rect`].
    pub fn selection_rects(&self) -> &'a [Rect] {
        &self.grid.fills
    }

    /// The View Spec the Table Source last applied: what the headers show.
    pub fn applied_view(&self) -> &'a ViewSpec {
        &self.grid.view.applied
    }

    /// The latest requested View Spec while an answer is still due, then
    /// while the applied spec leaves out part of it (see
    /// [`Self::view_pending`]).
    pub fn requested_view(&self) -> Option<&'a ViewSpec> {
        self.grid.view.requested.as_ref()
    }

    /// Whether a `view_applied` is still due: the requested View Spec is
    /// pending while this holds, and what it asks for beyond the applied spec
    /// is not applied once it doesn't.
    pub fn view_pending(&self) -> bool {
        self.grid.view.pending()
    }

    /// The Find Query in force, if find is open.
    pub fn find(&self) -> Option<&'a FindQuery> {
        self.grid.find.query.as_ref()
    }

    /// The match the last find step landed on, until its row or column
    /// leaves, a step for a new Find Query lands, or find closes.
    pub fn find_match(&self) -> Option<FindMatch> {
        self.grid.find.current
    }

    /// How the Table Source finds, if it does.
    pub fn find_support(&self) -> Option<TextSupport> {
        self.grid.find.support
    }

    /// The unfiltered count of `axis`, if the Table Source gave it with the
    /// applied View Spec: shown beside [`Snapshot::count`].
    pub fn total(&self, axis: Axis) -> Option<u64> {
        self.grid.view.totals.axis(axis)
    }

    /// How `axis`'s quick filter matches, if it has one.
    pub fn quick_filter(&self, axis: Axis) -> Option<TextSupport> {
        self.grid.view.quick_filter[axis.index()]
    }

    /// Whether the Table Source hides items of `axis`: adapters offer Hide
    /// only then.
    pub fn hide_support(&self, axis: Axis) -> bool {
        self.grid.view.hides[axis.index()]
    }

    /// The most hidden Keys `axis` takes ([`crate::HIDE_CAP`] unless the
    /// Table Source declared fewer): adapters suggest a filter past it.
    pub fn hide_cap(&self, axis: Axis) -> usize {
        self.grid.view.hide_cap(axis)
    }

    /// The hidden Keys of `axis` in the order they were hidden, as the
    /// latest request has them (else the applied View Spec): what a "Show
    /// hidden" list offers.
    pub fn hidden(&self, axis: Axis) -> &'a [Key] {
        &self.grid.latest_view().axis(axis).hidden
    }

    /// What the Table Source supports when viewing `axis` by `key`, an item
    /// of the other axis: adapters offer only these controls.
    pub fn support(&self, axis: Axis, key: Key) -> Support {
        self.grid.view.support(axis, key)
    }

    /// The conditions of `axis`, as the filter's chips list them: the latest
    /// request's, each applied or being added, then the applied ones being
    /// removed.
    pub fn conditions(&self, axis: Axis) -> Vec<(Condition, ConditionState)> {
        self.grid.view.conditions(axis)
    }

    /// The slicers, as their chips list them: the latest request's, each
    /// applied, being added or not applied, then the applied ones being
    /// removed.
    pub fn slicers(&self) -> Vec<(Condition, ConditionState)> {
        self.grid.view.slicers()
    }

    /// The levels of `axis`, listed as [`Snapshot::slicers`] are: a level
    /// the Table Source could not apply (or not with its Subtotal) shows
    /// not applied.
    pub fn levels(&self, axis: Axis) -> Vec<(Level, ConditionState)> {
        self.grid.view.levels(axis)
    }

    /// The values of the grouping, listed as [`Snapshot::slicers`] are.
    pub fn values(&self) -> Vec<(AggregateValue, ConditionState)> {
        self.grid.view.values()
    }

    /// The aggregates the Table Source offers when grouping `axis` with a
    /// value over `key`, an item of the other axis: adapters offer only
    /// these.
    pub fn aggregates(&self, axis: Axis, key: Key) -> &'a [Aggregate] {
        self.grid.view.aggregates(axis, key)
    }

    /// How the header of column `key` shows the rows' conditions on it.
    pub fn header_filter(&self, key: Key) -> HeaderFilter {
        self.grid.view.header_filter(Axis::Cols, key)
    }

    /// How the header of column `key` shows the rows' sort.
    pub fn header_sort(&self, key: Key) -> HeaderSort {
        self.grid.view.header(Axis::Cols, key)
    }

    /// The headers of the visible items on `axis`, in drawn order (the
    /// pinned Strip, then the scrolling Strip): column headers for
    /// [`Axis::Cols`], row headers for [`Axis::Rows`].
    pub fn headers(&self, axis: Axis) -> impl Iterator<Item = HeaderPlacement> + 'a {
        let grid = self.grid;
        let across = axis.other();
        let thickness = self.header(across);
        let range = self.selection_range();
        let all = self.count(across);
        let whole = range.filter(|r| matches!(r.span(across), [run] if *run == (0..all)));
        let hovered = grid.hovered;
        self.drawn(axis).map(move |item| {
            let (x, y, width, height) = match axis {
                Axis::Rows => (0.0, item.offset, thickness, item.size),
                Axis::Cols => (item.offset, 0.0, item.size, thickness),
            };
            let picked = item.key.is_some_and(|k| grid.selection.picked(axis, k));
            HeaderPlacement {
                axis,
                position: item.position,
                key: item.key,
                rect: Rect {
                    x,
                    y,
                    width,
                    height,
                },
                sort: item
                    .key
                    .map_or(HeaderSort::default(), |key| grid.view.header(axis, key)),
                filter: (item.key).map_or(HeaderFilter::default(), |key| {
                    grid.view.header_filter(axis, key)
                }),
                selected: picked || whole.is_some_and(|w| w.covers(axis, item.position)),
                hovered: hovered == Some((axis, item.position)),
                move_pending: item.key.is_some_and(|k| grid.move_pending(axis, k)),
            }
        })
    }

    pub fn mode(&self) -> Mode {
        self.grid.gesture.mode()
    }

    /// Whether a gesture is following a held button: the host should capture
    /// the pointer so moves and the release arrive even outside the viewport.
    pub fn capturing(&self) -> bool {
        self.grid.gesture.capturing()
    }

    /// The pointer shape for this frame: a resize shape while resizing an
    /// item or band, or while idle over a header border or band edge; the move shape while moving headers.
    pub fn cursor(&self) -> Cursor {
        let axis = match self.mode() {
            Mode::Resizing { axis, .. } | Mode::ResizingBand { axis, .. } => Some(axis),
            Mode::MovingHeaders { .. } => return Cursor::Move,
            _ if self.capturing() => None,
            _ => self.grid.hovered_border,
        };
        match axis {
            Some(Axis::Cols) => Cursor::ResizeCol,
            Some(Axis::Rows) => Cursor::ResizeRow,
            None => Cursor::Default,
        }
    }

    /// The guide line for a resize in progress, while the resized item is in
    /// the Window, or for a band resize: at the band's trailing edge.
    pub fn resize_guide(&self) -> Option<ResizeGuide> {
        let (axis, key) = match self.mode() {
            Mode::Resizing { axis, key, .. } => (axis, key),
            Mode::ResizingBand { axis, .. } => {
                let thickness = self.header(axis);
                return Some(ResizeGuide {
                    axis,
                    key: None,
                    offset: thickness,
                    size: thickness,
                });
            }
            _ => return None,
        };
        let st = self.grid.axis(axis);
        for edge in Edge::ALL {
            if let Some(item) = self.pinned(axis, edge).iter().find(|i| i.key == Some(key)) {
                // A trailing pinned item grows from its far end, so its
                // guide is at its start.
                let offset = match edge {
                    Edge::Leading => item.end(),
                    Edge::Trailing => item.offset,
                };
                return Some(ResizeGuide {
                    axis,
                    key: Some(key),
                    offset,
                    size: item.size,
                });
            }
        }
        let position = st.measure.position_of(key)?;
        let size = st.measure.size_at(position);
        let start = f64::from(st.scroll_start()) - f64::from(st.anchor.offset)
            + st.measure.distance(st.anchor.position, position);
        Some(ResizeGuide {
            axis,
            key: Some(key),
            offset: (start + f64::from(size)) as f32,
            size,
        })
    }

    /// The Keys of the items whose headers are being moved, in Position
    /// order; empty unless the Mode is [`Mode::MovingHeaders`].
    pub fn moving_keys(&self) -> &'a [Key] {
        match &self.grid.gesture {
            crate::gesture::Gesture::MovingHeaders { keys, .. } => keys,
            _ => &[],
        }
    }

    /// Whether the Table Source can move the items of `axis` now (see
    /// [`crate::Grid::set_movable`]).
    pub fn movable(&self, axis: Axis) -> bool {
        self.grid.moves.movable[axis.index()]
    }

    /// The move requested and not yet answered: its axis and the Keys of
    /// the items it moves.
    pub fn pending_move(&self) -> Option<(Axis, &'a [Key])> {
        let (axis, keys) = self.grid.moves.pending.as_ref()?;
        Some((*axis, keys))
    }

    /// The axis of the last move the Table Source refused, and its reason,
    /// until the next press: a short message for the host to show near the
    /// headers or in its status line.
    pub fn move_refusal(&self) -> Option<(Axis, &'a str)> {
        let (axis, reason) = self.grid.moves.refused.as_ref()?;
        Some((*axis, reason))
    }

    /// Where the headers being dragged would drop and what that would do,
    /// while dragging headers; `None` while the drop would do nothing.
    pub fn drop_indicator(&self) -> Option<DropIndicator> {
        let Mode::MovingHeaders { axis, target, .. } = self.mode() else {
            return None;
        };
        let st = self.grid.axis(axis);
        let gap = f64::from(st.measure.gap());
        let in_body = |p: Position| {
            f64::from(st.scroll_start()) - f64::from(st.anchor.offset)
                + st.measure.distance(st.anchor.position, p)
                - gap / 2.0
        };
        let last = f64::from(f32::from_bits(st.extent.to_bits().saturating_sub(1)).max(0.0));
        let offset = match target {
            DropTarget::Stay => return None,
            DropTarget::Move { to, .. } => in_body(to),
            DropTarget::Unpin { at } => in_body(at).clamp(
                f64::from(st.scroll_start()) - gap / 2.0,
                f64::from(st.scroll_end()) - gap / 2.0,
            ),
            DropTarget::Pin { edge, before, .. } => {
                let items: Vec<StripItem> = st.pinned_items(edge).collect();
                let at = before.and_then(|b| items.iter().find(|it| it.key == Some(b)));
                let offset = match (at, items.last(), edge) {
                    (Some(it), _, _) => f64::from(it.offset) - gap / 2.0,
                    (None, Some(end), _) => f64::from(end.end()) + gap / 2.0,
                    (None, None, Edge::Leading) => f64::from(st.header) - gap / 2.0,
                    (None, None, Edge::Trailing) => f64::from(st.extent) - gap / 2.0,
                };
                offset.clamp(0.0, last)
            }
        };
        Some(DropIndicator {
            axis,
            target,
            offset: offset as f32,
        })
    }

    /// The lit pin zones while headers are dragged: one at each edge of the
    /// dragged axis whose pinned Strip is empty, unless the zones there have
    /// no size (see [`crate::Grid::set_pin_zone`]).
    pub fn pin_zones(&self) -> Vec<PinZone> {
        let Mode::MovingHeaders { axis, target, .. } = self.mode() else {
            return Vec::new();
        };
        let st = self.grid.axis(axis);
        let hot = |edge| matches!(target, DropTarget::Pin { edge: e, .. } if e == edge);
        (Edge::ALL.into_iter())
            .filter(|&edge| st.drawn_pins(edge).next().is_none() && st.pin_zone > 0.0)
            .map(|edge| {
                let size = st.pin_zone.min((st.extent - st.header).max(0.0));
                let offset = match edge {
                    Edge::Leading => st.header,
                    Edge::Trailing => st.extent - size,
                };
                PinZone {
                    axis,
                    edge,
                    offset,
                    size,
                    hot: hot(edge),
                }
            })
            .collect()
    }

    /// The header under the pointer: the axis whose item it heads (a row
    /// header is on [`Axis::Rows`]) and that item in its Strip. The Gap after
    /// a header, and a border, is not part of it.
    pub fn hovered_header(&self) -> Option<(Axis, &'a StripItem)> {
        let (axis, position) = self.grid.hovered?;
        let item = self.drawn(axis).find(|i| i.position == position)?;
        Some((axis, item))
    }
}
