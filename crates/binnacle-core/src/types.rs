/// An index along an axis in the Table Source's current order, from zero to the
/// axis count.
pub type Position = u64;

/// A counter stamped on every Snapshot and Effect so a host can tell that it
/// handled something late. It advances once per [`crate::Grid::begin_frame`].
pub type Generation = u64;

/// An opaque identity the Table Source assigns to a row or column, compared only
/// for equality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub u64);

/// One of the Grid's two symmetric dimensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Rows,
    Cols,
}

impl Axis {
    pub const ALL: [Axis; 2] = [Axis::Rows, Axis::Cols];

    pub fn other(self) -> Axis {
        match self {
            Axis::Rows => Axis::Cols,
            Axis::Cols => Axis::Rows,
        }
    }

    /// 0 for Rows, 1 for Cols: a slot in per-axis pairs.
    pub fn index(self) -> usize {
        match self {
            Axis::Rows => 0,
            Axis::Cols => 1,
        }
    }
}

/// The area the Grid draws into, including its header bands, in the host's unit
/// (terminal cells, pixels, CSS px). Geometry is relative to its top-left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn new(width: f32, height: f32) -> Self {
        Viewport { width, height }
    }

    /// The viewport's length along `axis`: rows stack down its height, columns
    /// across its width. Negative or non-finite lengths count as zero.
    pub fn extent(self, axis: Axis) -> f32 {
        let len = match axis {
            Axis::Rows => self.height,
            Axis::Cols => self.width,
        };
        if len.is_finite() { len.max(0.0) } else { 0.0 }
    }
}
