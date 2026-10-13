use crate::focus::FocusEnd;
use crate::gesture::Mode;
use crate::grid::Grid;
use crate::selection::CellRange;
use crate::sort::{HeaderSort, SortSpec};
use crate::types::{Axis, Generation, Key, Position};

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
    /// Inside the range, or in a picked row.
    pub selected: bool,
    /// The host's editor is open on this cell.
    pub editing: bool,
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
    /// How it shows the sort; always unsorted on row headers.
    pub sort: HeaderSort,
    /// Its whole item is selected: the range spans every item across it, or
    /// (on a row header) its row is picked.
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
/// offsets), with the item's Key and current size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGuide {
    pub axis: Axis,
    pub key: Key,
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
        let picked = row.key.is_some_and(|k| self.grid.selection.picked(k));
        let editing = match self.mode() {
            Mode::Editing { row: r, col: c } => row.key == Some(r) && col.key == Some(c),
            _ => false,
        };
        CellState {
            focused: on(Axis::Rows, row) && on(Axis::Cols, col),
            selected: in_range || picked,
            editing,
        }
    }

    /// The one contiguous range, if any, clamped to the counts.
    pub fn selection_range(&self) -> Option<CellRange> {
        let counts = Axis::ALL.map(|a| self.count(a));
        self.grid.selection.span.map(|s| s.range(counts))
    }

    /// The picked row Keys, in Key order.
    pub fn picked(&self) -> &'a [Key] {
        &self.grid.selection.picks
    }

    /// Rects to fill for the selection: the visible part of the range, then
    /// each visible picked row across the visible columns. They may overlap.
    /// Edges are those of the Strip items, unclipped, like [`Self::focus_rect`].
    pub fn selection_rects(&self) -> &'a [Rect] {
        &self.grid.fills
    }

    /// The Sort Spec the Table Source last applied: what the headers show.
    pub fn applied_sort(&self) -> &'a SortSpec {
        &self.grid.sort.applied
    }

    /// The latest requested Sort Spec while an answer is still due.
    pub fn requested_sort(&self) -> Option<&'a SortSpec> {
        self.grid.sort.requested.as_ref()
    }

    /// How the header of column `key` shows the sort.
    pub fn header_sort(&self, key: Key) -> HeaderSort {
        self.grid.sort.header(key)
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
            let picked = axis == Axis::Rows && item.key.is_some_and(|k| grid.selection.picked(k));
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
                sort: match (axis, item.key) {
                    (Axis::Cols, Some(key)) => grid.sort.header(key),
                    _ => HeaderSort::default(),
                },
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

    /// The pointer shape for this frame: a resize shape while resizing, or
    /// while idle over a header border; the move shape while moving headers.
    pub fn cursor(&self) -> Cursor {
        let axis = match self.mode() {
            Mode::Resizing { axis, .. } => Some(axis),
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
    /// the Window.
    pub fn resize_guide(&self) -> Option<ResizeGuide> {
        let Mode::Resizing { axis, key, .. } = self.mode() else {
            return None;
        };
        let st = self.grid.axis(axis);
        let position = st.measure.position_of(key)?;
        let size = st.measure.size_at(position);
        let start = f64::from(st.header) - f64::from(st.anchor.offset)
            + st.measure.distance(st.anchor.position, position);
        Some(ResizeGuide {
            axis,
            key,
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
