use crate::axis::AxisState;
use crate::measure::ItemAt;
use crate::types::{Axis, Edge, Position};

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
    /// the pinned Strips and the current Scroll Anchor. The Gap belongs to
    /// the item before it, and only the Gap: with Gap 0 nothing is a border.
    pub(crate) fn hit(&self, v: f32) -> AxisHit {
        if !(v >= 0.0 && v < self.extent) {
            return AxisHit::Outside;
        }
        if v < self.header {
            return AxisHit::Header;
        }
        let (start, end) = (self.scroll_start(), self.scroll_end());
        let pinned = |edge| {
            let gap = self.measure.gap();
            (self.pinned_items(edge))
                .find(|it| v >= it.offset && v < it.end() + gap)
                .filter(|it| {
                    it.key
                        .is_some_and(|k| self.pinned_at(it.position) == Some(k))
                })
                .map_or(AxisHit::Outside, |it| {
                    AxisHit::Item(ItemAt {
                        position: it.position,
                        within: f64::from(v - it.offset),
                        in_gap: v >= it.end(),
                    })
                })
        };
        if v < start {
            return pinned(Edge::Leading);
        }
        // The Gap before the trailing Strip belongs to no item.
        if v >= end {
            return pinned(Edge::Trailing);
        }
        let delta = f64::from(self.anchor.offset) + (f64::from(v) - f64::from(start));
        match self.measure.locate(self.anchor.position, delta) {
            Some(at) => AxisHit::Item(at),
            None => AxisHit::Outside,
        }
    }
}

impl AxisState {
    /// The Position of the item whose border `v` is on, if any: the Gap
    /// after the item (before it, in the trailing pinned Strip), widened by
    /// `grab` into the items either side. The
    /// first item's leading edge is no border; the last item's border is the
    /// length of a Gap (or `grab`) past its end.
    pub(crate) fn border(&self, v: f32, grab: f32) -> Option<Position> {
        if !(v >= self.header && v < self.extent) {
            return None;
        }
        let (grab, m) = (f64::from(grab), &self.measure);
        let gap = f64::from(m.gap());
        let start = self.scroll_start();
        // A pinned item's border: its Gap, widened by `grab` either side.
        let pinned = self.pinned_items(Edge::Leading).find(|it| {
            let (v, end) = (f64::from(v), f64::from(it.end()));
            v >= end - grab && v < end + gap + grab
        });
        let pinned = pinned.filter(|it| {
            it.key
                .is_some_and(|k| self.pinned_at(it.position) == Some(k))
        });
        if let Some(it) = pinned {
            return Some(it.position);
        }
        // A trailing pinned item's border is the Gap before it, since the
        // Strip grows from the far end.
        let trailing = self.pinned_items(Edge::Trailing).find(|it| {
            let (v, offset) = (f64::from(v), f64::from(it.offset));
            v >= offset - gap - grab && v < offset + grab
        });
        let trailing = trailing.filter(|it| {
            it.key
                .is_some_and(|k| self.pinned_at(it.position) == Some(k))
        });
        if let Some(it) = trailing {
            return Some(it.position);
        }
        if v < start || v >= self.scroll_end() {
            return None;
        }
        let delta = f64::from(self.anchor.offset) + (f64::from(v) - f64::from(start));
        match m.locate(self.anchor.position, delta) {
            Some(at) => {
                // Past the item's size is its Gap.
                if at.within >= f64::from(m.size_at(at.position)) - grab {
                    Some(at.position)
                } else if at.within < grab {
                    // The scrolling item before it, if one comes first.
                    (0..at.position).rev().find(|&p| !m.skipped(p))
                } else {
                    None
                }
            }
            // Past the last item: `locate` found nothing, so this is beyond
            // its end.
            None if delta >= 0.0 => {
                let last = self.last_shown()?;
                let past =
                    delta - m.distance(self.anchor.position, last) - f64::from(m.size_at(last));
                (past < gap.max(grab)).then_some(last)
            }
            None => None,
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
