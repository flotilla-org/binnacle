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
        self.grid.axis(axis).count
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
