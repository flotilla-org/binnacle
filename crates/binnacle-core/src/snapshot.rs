use crate::grid::Grid;
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
}
