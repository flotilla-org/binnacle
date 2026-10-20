use crate::find::{FindMatch, FindQuery};
use crate::focus::FocusEnd;
use crate::gesture::Mode;
use crate::grid::Grid;
use crate::selection::CellRange;
use crate::sort::HeaderSort;
use crate::types::{Axis, Generation, Key, Position};
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

/// Where a resize in progress puts the resized item's trailing edge: a line
/// across the viewport at `offset` along `axis` (viewport-relative, like Strip
/// offsets), with the item's Key and current size. Resizing the Header Band
/// leading `axis`, `key` is `None` and `size` is the band's thickness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGuide {
    pub axis: Axis,
    pub key: Option<Key>,
    pub offset: f32,
    pub size: f32,
}

/// Where headers being moved would drop: before the item at Position `to` on
/// `axis` (the count for the end), drawn as a line across the viewport at
/// `offset` along `axis` (viewport-relative, like Strip offsets), the middle
/// of the Gap before that item, as if a Gap also led the first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropIndicator {
    pub axis: Axis,
    pub to: Position,
    pub offset: f32,
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

    pub fn strip(&self, axis: Axis) -> &'a [StripItem] {
        &self.grid.axis(axis).strip
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
        let strip = self.strip(axis);
        let i = end.position.checked_sub(strip.first()?.position)?;
        let item = strip.get(usize::try_from(i).ok()?)?;
        (end.key.is_some() && item.key == end.key).then_some(item)
    }

    /// The focused cell's rectangle, unclipped, if it is visible and its Keys
    /// are known.
    pub fn focus_rect(&self) -> Option<Rect> {
        let row = self.focused_item(Axis::Rows)?;
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
        let find = |axis, key| self.strip(axis).iter().find(|i| i.key == Some(key));
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
        let in_range = self
            .selection_range()
            .is_some_and(|r| r.contains(row.position, col.position));
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
            focused: on(Axis::Rows, row) && on(Axis::Cols, col),
            selected: in_range || picked,
            editing,
            current_match,
        }
    }

    /// The one contiguous range, if any, clamped to the counts.
    pub fn selection_range(&self) -> Option<CellRange> {
        let counts = Axis::ALL.map(|a| self.count(a));
        self.grid.selection.span.map(|s| s.range(counts))
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

    /// How the header of column `key` shows the rows' conditions on it.
    pub fn header_filter(&self, key: Key) -> HeaderFilter {
        self.grid.view.header_filter(Axis::Cols, key)
    }

    /// How the header of column `key` shows the rows' sort.
    pub fn header_sort(&self, key: Key) -> HeaderSort {
        self.grid.view.header(Axis::Cols, key)
    }

    /// The headers of the visible items on `axis`, in Strip order: column
    /// headers for [`Axis::Cols`], row headers for [`Axis::Rows`].
    pub fn headers(&self, axis: Axis) -> impl Iterator<Item = HeaderPlacement> + 'a {
        let grid = self.grid;
        let across = axis.other();
        let thickness = self.header(across);
        let range = self.selection_range();
        let whole = range
            .as_ref()
            .filter(|r| *r.span(across) == (0..self.count(across)))
            .map(|r| r.span(axis).clone());
        let hovered = grid.hovered;
        self.strip(axis).iter().map(move |item| {
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
                selected: picked || whole.as_ref().is_some_and(|w| w.contains(&item.position)),
                hovered: hovered == Some((axis, item.position)),
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
        let position = st.measure.position_of(key)?;
        let size = st.measure.size_at(position);
        let start = f64::from(st.header) - f64::from(st.anchor.offset)
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

    /// Where the headers being moved would drop, while moving headers.
    pub fn drop_indicator(&self) -> Option<DropIndicator> {
        let Mode::MovingHeaders { axis, to, .. } = self.mode() else {
            return None;
        };
        let st = self.grid.axis(axis);
        let start = f64::from(st.header) - f64::from(st.anchor.offset)
            + st.measure.distance(st.anchor.position, to);
        Some(DropIndicator {
            axis,
            to,
            offset: (start - f64::from(st.measure.gap()) / 2.0) as f32,
        })
    }

    /// The header under the pointer: the axis whose item it heads (a row
    /// header is on [`Axis::Rows`]) and that item in its Strip. The Gap after
    /// a header, and a border, is not part of it.
    pub fn hovered_header(&self) -> Option<(Axis, &'a StripItem)> {
        let (axis, position) = self.grid.hovered?;
        let item = self.strip(axis).iter().find(|i| i.position == position)?;
        Some((axis, item))
    }
}
