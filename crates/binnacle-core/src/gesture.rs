//! Pointer gestures: what a press, the moves after it and its release mean,
//! given what they land on.

use crate::focus::FocusEnd;
use crate::grid::Grid;
use crate::measure::ItemAt;
use crate::pointer::{Button, Hit, Modifiers, PointerEvent, PointerKind};
use crate::selection::{Cell, HeaderClick, Selection};
use crate::types::{Axis, Position};

/// The single interaction the Grid is in the middle of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mode {
    #[default]
    Idle,
    /// A press on a cell moved past the drag threshold; the range follows the
    /// pointer until release.
    DraggingSelection,
}

/// Focus and selection from before a gesture, which `Cancel` restores.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Before {
    focus: Option<Cell>,
    selection: Selection,
}

/// What a press landed on, deciding what moving and releasing do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Press {
    /// A cell: dragging past the threshold selects a range.
    Cell,
    /// A cell under Ctrl/Cmd: the pick was toggled on press; dragging does
    /// nothing.
    Pick,
    /// The header of the item at `position` on `axis`: releasing it without
    /// dragging is a header click.
    Header {
        axis: Axis,
        position: Position,
        modifiers: Modifiers,
    },
}

/// The Grid's pointer state between a press and its release.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum Gesture {
    #[default]
    Idle,
    /// The primary button is down at (`x`, `y`) on `press`. `dragged` once
    /// the pointer moved past the drag threshold.
    Pressed {
        x: f32,
        y: f32,
        press: Press,
        dragged: bool,
        before: Before,
    },
    DraggingSelection {
        before: Before,
    },
}

impl Gesture {
    pub fn mode(&self) -> Mode {
        match self {
            Gesture::DraggingSelection { .. } => Mode::DraggingSelection,
            _ => Mode::Idle,
        }
    }

    /// Whether the Grid is following a held button, so the host should keep
    /// pointer events coming even outside the viewport.
    pub fn capturing(&self) -> bool {
        matches!(
            self,
            Gesture::Pressed { .. } | Gesture::DraggingSelection { .. }
        )
    }
}

impl Grid {
    /// Takes a raw pointer event: the Grid hit tests it against the current
    /// Scroll Anchors and recognises the gesture. Only the primary button
    /// starts gestures in v1. A press or move at a non-finite point is
    /// ignored; a release ends the gesture wherever it is.
    pub fn pointer(&mut self, event: PointerEvent) {
        let PointerEvent { x, y, .. } = event;
        let finite = x.is_finite() && y.is_finite();
        self.pointer_at = finite.then_some((x, y));
        match event.kind {
            PointerKind::Up if event.button == Button::Primary => self.release(),
            _ if !finite => {}
            PointerKind::Down if event.button == Button::Primary => {
                self.press(x, y, event.modifiers)
            }
            PointerKind::Move => self.drag(x, y),
            _ => {}
        }
    }

    fn press(&mut self, x: f32, y: f32, modifiers: Modifiers) {
        // A press while another is held (a lost release) ends the old one.
        self.gesture = Gesture::Idle;
        let before = Before {
            focus: self.focus,
            selection: self.selection.clone(),
        };
        let press = match self.hit(x, y) {
            Hit::Cell([r, c]) if !(r.in_gap || c.in_gap) => {
                let cell = self.cell_at(r.position, c.position);
                if modifiers.toggle() {
                    self.toggle_pick(cell);
                    self.set_focus_cell(cell);
                    Press::Pick
                } else if modifiers.shift {
                    self.extend_to(self.focus, cell);
                    Press::Cell
                } else {
                    self.select_cell(cell);
                    Press::Cell
                }
            }
            Hit::Header { axis, at } if !at.in_gap => Press::Header {
                axis,
                position: at.position,
                modifiers,
            },
            _ => return,
        };
        self.gesture = Gesture::Pressed {
            x,
            y,
            press,
            dragged: false,
            before,
        };
    }

    fn drag(&mut self, x: f32, y: f32) {
        match &mut self.gesture {
            Gesture::Pressed {
                x: x0,
                y: y0,
                press,
                dragged,
                before,
            } => {
                let moved = f64::from(x - *x0).hypot(f64::from(y - *y0));
                if *dragged || moved < f64::from(self.drag_threshold) {
                    return;
                }
                *dragged = true;
                if *press == Press::Cell {
                    let before = before.clone();
                    self.gesture = Gesture::DraggingSelection { before };
                    self.pending_damage.selection = true;
                    self.drag_selection(x, y);
                }
            }
            Gesture::DraggingSelection { .. } => self.drag_selection(x, y),
            _ => {}
        }
    }

    /// Stretches the range to the cell nearest (`x`, `y`): a pointer beyond
    /// the body on an axis counts as over the nearest visible item there.
    fn drag_selection(&mut self, x: f32, y: f32) {
        let nearest = |st: &crate::axis::AxisState, v: f32| -> Option<ItemAt> {
            let count = st.measure.count();
            if count == 0 {
                return None;
            }
            let body_end = f32::from_bits(st.extent.to_bits().saturating_sub(1));
            let v = v.min(body_end).max(st.header);
            match st.hit(v) {
                crate::pointer::AxisHit::Item(at) => Some(at),
                _ => st.measure.locate(count - 1, 0.0),
            }
        };
        let (Some(r), Some(c)) = (
            nearest(self.axis(Axis::Rows), y),
            nearest(self.axis(Axis::Cols), x),
        ) else {
            return;
        };
        let cell = self.cell_at(r.position, c.position);
        self.extend_to(self.focus, cell);
    }

    fn release(&mut self) {
        let gesture = std::mem::take(&mut self.gesture);
        if gesture.capturing() {
            self.pending_damage.selection = true;
        }
        if let Gesture::Pressed {
            press:
                Press::Header {
                    axis,
                    position,
                    modifiers,
                },
            dragged: false,
            ..
        } = gesture
        {
            self.header_click(axis, position, modifiers);
        }
    }

    /// A click on the header of the item at `position` on `axis`, acted on
    /// by that axis's `header_click` setting.
    fn header_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        if self.header_click[axis.index()] != HeaderClick::Select
            || Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0)
        {
            return;
        }
        let mut cell = self.focus.unwrap_or_else(|| {
            let at = |a: Axis| self.axis(a).anchor.position;
            self.cell_at(at(Axis::Rows), at(Axis::Cols))
        });
        cell[axis.index()] = FocusEnd::at(self.axis(axis), position);
        // The whole of every item across the clicked one.
        let mut whole = [false; 2];
        whole[axis.other().index()] = true;
        if modifiers.toggle() && axis == Axis::Rows {
            self.toggle_pick(cell);
            self.set_focus_cell(cell);
        } else if modifiers.shift {
            self.extend_with(self.focus, cell, whole);
        } else {
            self.set_focus_cell(cell);
            self.set_selection(Selection {
                span: Some(crate::selection::Span {
                    anchor: cell,
                    head: cell,
                    whole,
                }),
                picks: Vec::new(),
            });
        }
    }

    /// Ends any gesture in progress, restoring focus and selection from before
    /// it. The rest of the press (moves, the release) then finds the Grid idle
    /// and does nothing.
    pub(crate) fn cancel(&mut self) {
        let before = match std::mem::take(&mut self.gesture) {
            Gesture::Pressed { before, .. } | Gesture::DraggingSelection { before } => before,
            other => {
                self.gesture = other;
                return;
            }
        };
        self.pending_damage.selection = true;
        if self.focus != before.focus {
            self.focus = before.focus;
            self.announce_focus();
        }
        self.set_selection(before.selection);
    }
}
