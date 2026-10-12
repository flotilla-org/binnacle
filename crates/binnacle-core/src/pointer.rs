use crate::axis::AxisState;
use crate::measure::ItemAt;
use crate::types::Axis;

/// A pointer button. v1 gestures use only [`Button::Primary`]; the others are
/// ignored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Button {
    #[default]
    Primary,
    Secondary,
    Middle,
}

/// The modifier keys held during a pointer event. Ctrl and Meta (Cmd) both
/// toggle picks, so a host need not know the platform's convention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    pub const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    pub const ALT: Modifiers = Modifiers {
        alt: true,
        ..Modifiers::NONE
    };
    pub const META: Modifiers = Modifiers {
        meta: true,
        ..Modifiers::NONE
    };

    /// Ctrl or Meta: the platform's "add to the selection" modifier.
    pub fn toggle(self) -> bool {
        self.ctrl || self.meta
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Modifiers;

    fn bitor(self, rhs: Modifiers) -> Modifiers {
        Modifiers {
            shift: self.shift || rhs.shift,
            ctrl: self.ctrl || rhs.ctrl,
            alt: self.alt || rhs.alt,
            meta: self.meta || rhs.meta,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PointerKind {
    Down,
    Move,
    Up,
}

/// A raw pointer event in the viewport's coordinates (the host's unit, origin
/// at the top-left of the area including the header bands). The Grid does all
/// hit testing and gesture recognition from these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub x: f32,
    pub y: f32,
    /// The button pressed or released; ignored on [`PointerKind::Move`].
    pub button: Button,
    pub modifiers: Modifiers,
    /// The host's click count for a press: 1 for a single click, 2 for a
    /// double click, and so on. Ignored except on [`PointerKind::Down`].
    pub clicks: u32,
}

impl PointerEvent {
    fn new(kind: PointerKind, x: f32, y: f32) -> Self {
        PointerEvent {
            kind,
            x,
            y,
            button: Button::Primary,
            modifiers: Modifiers::NONE,
            clicks: 1,
        }
    }

    /// The primary button pressed once at (`x`, `y`) with no modifiers.
    pub fn down(x: f32, y: f32) -> Self {
        PointerEvent::new(PointerKind::Down, x, y)
    }

    /// The pointer moved to (`x`, `y`), with or without a button held. A host
    /// that sees the pointer leave sends a move to a point outside the
    /// viewport.
    pub fn moved(x: f32, y: f32) -> Self {
        PointerEvent::new(PointerKind::Move, x, y)
    }

    /// The primary button released at (`x`, `y`).
    pub fn up(x: f32, y: f32) -> Self {
        PointerEvent::new(PointerKind::Up, x, y)
    }

    pub fn button(self, button: Button) -> Self {
        PointerEvent { button, ..self }
    }

    pub fn modifiers(self, modifiers: Modifiers) -> Self {
        PointerEvent { modifiers, ..self }
    }

    pub fn clicks(self, clicks: u32) -> Self {
        PointerEvent { clicks, ..self }
    }
}

/// Where a coordinate along one axis lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AxisHit {
    /// The header band leading the axis.
    Header,
    /// An item, or the Gap after it (`in_gap`).
    Item(ItemAt),
    /// Before the viewport, past it, or past the last item.
    Outside,
}

impl AxisState {
    /// Hit tests `v`, a viewport-relative coordinate along this axis, against
    /// the current Scroll Anchor. The Gap belongs to the item before it, and
    /// only the Gap: with Gap 0 nothing is a border.
    pub(crate) fn hit(&self, v: f32) -> AxisHit {
        if !(v >= 0.0 && v < self.extent) {
            return AxisHit::Outside;
        }
        if v < self.header {
            return AxisHit::Header;
        }
        let delta = f64::from(self.anchor.offset) + (f64::from(v) - f64::from(self.header));
        match self.measure.locate(self.anchor.position, delta) {
            Some(at) => AxisHit::Item(at),
            None => AxisHit::Outside,
        }
    }
}

/// What a viewport point lands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Hit {
    /// A cell, or a Gap beside it: the row and column items, in that order.
    Cell([ItemAt; 2]),
    /// The header of an item on `axis` (a row header for [`Axis::Rows`]), or
    /// the Gap after it.
    Header {
        axis: Axis,
        at: ItemAt,
    },
    /// Where the two header bands meet.
    Corner,
    Nothing,
}

impl Hit {
    pub(crate) fn of(rows: AxisHit, cols: AxisHit) -> Hit {
        match (rows, cols) {
            (AxisHit::Item(r), AxisHit::Item(c)) => Hit::Cell([r, c]),
            (AxisHit::Item(at), AxisHit::Header) => Hit::Header {
                axis: Axis::Rows,
                at,
            },
            (AxisHit::Header, AxisHit::Item(at)) => Hit::Header {
                axis: Axis::Cols,
                at,
            },
            (AxisHit::Header, AxisHit::Header) => Hit::Corner,
            _ => Hit::Nothing,
        }
    }
}
