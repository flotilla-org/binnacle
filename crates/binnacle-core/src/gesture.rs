//! Pointer gestures: what a press, the moves after it and its release mean,
//! given what they land on.

use crate::effect::Effect;
use crate::focus::FocusEnd;
use crate::grid::Grid;
use crate::measure::ItemAt;
use crate::pointer::{Button, Hit, Modifiers, PointerEvent, PointerKind};
use crate::selection::{Cell, HeaderClick, Selection};
use crate::types::{Axis, Key, Position};

/// The single interaction the Grid is in the middle of.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mode {
    #[default]
    Idle,
    /// A press on a cell moved past the drag threshold; the range follows the
    /// pointer until release.
    DraggingSelection,
    /// A header border is being dragged: the item with `key` on `axis`, which
    /// was `start_size` when the press began, follows the pointer until
    /// release.
    Resizing {
        axis: Axis,
        key: Key,
        start_size: f32,
    },
    /// Column headers are being dragged: the header grabbed has `key` on
    /// `axis`, and the dragged items would drop before Position `to` (see
    /// [`crate::Snapshot::moving_keys`] for all of them).
    MovingHeaders { axis: Axis, key: Key, to: Position },
}

/// Focus and selection from before a gesture, which `Cancel` restores.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Before {
    pub focus: Option<Cell>,
    pub selection: Selection,
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
    /// A press on the border after the item with `key` on `axis`, at
    /// coordinate `from` along that axis. `start_override` is the item's Size
    /// Override before the press, which `Cancel` puts back.
    Resizing {
        axis: Axis,
        key: Key,
        from: f32,
        start_size: f32,
        start_override: Option<f32>,
    },
    /// The headers of the items with `keys` on `axis`, at `positions` when
    /// the drag began (both in Position order), are being dragged by the
    /// header of `key` and would drop before Position `to`.
    MovingHeaders {
        axis: Axis,
        key: Key,
        keys: Vec<Key>,
        positions: Vec<Position>,
        to: Position,
    },
}

impl Gesture {
    pub fn mode(&self) -> Mode {
        match self {
            Gesture::DraggingSelection { .. } => Mode::DraggingSelection,
            &Gesture::Resizing {
                axis,
                key,
                start_size,
                ..
            } => Mode::Resizing {
                axis,
                key,
                start_size,
            },
            &Gesture::MovingHeaders { axis, key, to, .. } => Mode::MovingHeaders { axis, key, to },
            Gesture::Idle | Gesture::Pressed { .. } => Mode::Idle,
        }
    }

    /// Whether the Grid is following a held button, so the host should keep
    /// pointer events coming even outside the viewport.
    pub fn capturing(&self) -> bool {
        !matches!(self, Gesture::Idle)
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
                self.press(x, y, event.modifiers, event.clicks)
            }
            PointerKind::Move => self.drag(x, y),
            _ => {}
        }
    }

    fn press(&mut self, x: f32, y: f32, modifiers: Modifiers, clicks: u32) {
        // A press while another is held (a lost release) ends the old one,
        // keeping a resize made so far.
        self.end_resize();
        self.gesture = Gesture::Idle;
        if let Some((axis, position, key)) = self.border_at(x, y) {
            if clicks >= 2 {
                self.push(Effect::AutoSizeRequested { axis, key });
                return;
            }
            let measure = &self.axis(axis).measure;
            self.gesture = Gesture::Resizing {
                axis,
                key,
                from: along(axis, x, y),
                start_size: measure.size_at(position),
                start_override: measure.size_override(key),
            };
            self.pending_damage.selection = true;
            return;
        }
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
                match *press {
                    Press::Cell => {
                        let before = before.clone();
                        self.gesture = Gesture::DraggingSelection { before };
                        self.pending_damage.selection = true;
                        self.drag_selection(x, y);
                    }
                    Press::Header {
                        axis: Axis::Cols,
                        position,
                        ..
                    } => self.start_move(Axis::Cols, position, x, y),
                    _ => {}
                }
            }
            Gesture::MovingHeaders { axis, .. } => {
                let axis = *axis;
                let to = self.drop_position(axis, along(axis, x, y));
                if let Gesture::MovingHeaders { to: old, .. } = &mut self.gesture
                    && *old != to
                {
                    *old = to;
                    self.pending_damage.selection = true;
                }
            }
            Gesture::DraggingSelection { .. } => self.drag_selection(x, y),
            &mut Gesture::Resizing {
                axis,
                key,
                from,
                start_size,
                start_override,
            } => {
                let moved = f64::from(along(axis, x, y)) - f64::from(from);
                // Clamped here as `set_size` would, so a drag pinned at the
                // range's edge compares equal to an item that started there.
                let m = &self.axis(axis).measure;
                let size = (f64::from(start_size) + moved)
                    .clamp(f64::from(m.min_size()), f64::from(m.max_size()))
                    as f32;
                if size == start_size {
                    self.restore_size(axis, key, start_override);
                } else {
                    self.set_size(axis, key, size);
                }
            }
            Gesture::Idle => {}
        }
    }

    /// Puts back the Size Override `size` (or none) for `key` on `axis`.
    fn restore_size(&mut self, axis: Axis, key: Key, size: Option<f32>) {
        match size {
            Some(size) => self.set_size(axis, key, size),
            None => self.clear_size(axis, key),
        }
    }

    /// Ends a resize in progress where it is, reporting the new size if it
    /// changed.
    fn end_resize(&mut self) {
        let Gesture::Resizing {
            axis,
            key,
            start_override,
            ..
        } = self.gesture
        else {
            return;
        };
        self.gesture = Gesture::Idle;
        self.pending_damage.selection = true;
        let size = self.size_override(axis, key);
        if let Some(size) = size.filter(|_| size != start_override) {
            self.push(Effect::SizeChanged { axis, key, size });
        }
    }

    /// Turns a press on the header at `position` on `axis` into a move of
    /// that item, or of every selected item if it is in a range spanning
    /// whole items on that axis whose Keys are all known. An item without a
    /// Key cannot be moved.
    fn start_move(&mut self, axis: Axis, position: Position, x: f32, y: f32) {
        let i = axis.index();
        let measure = &self.axis(axis).measure;
        let Some(key) = measure.key_at(position) else {
            return;
        };
        let counts = Axis::ALL.map(|a| self.axis(a).measure.count());
        let selected = self
            .selection
            .span
            .filter(|s| s.whole[axis.other().index()] && !s.whole[i])
            .map(|s| s.range(counts).span(axis).clone())
            .filter(|r| r.contains(&position));
        let (keys, positions) = selected
            .and_then(|r| {
                let keys: Option<Vec<Key>> = r.clone().map(|p| measure.key_at(p)).collect();
                Some((keys?, r.collect()))
            })
            .unwrap_or_else(|| (vec![key], vec![position]));
        self.gesture = Gesture::MovingHeaders {
            axis,
            key,
            keys,
            positions,
            to: self.drop_position(axis, along(axis, x, y)),
        };
        self.pending_damage.selection = true;
    }

    /// Where items dragged to `v` along `axis` would drop: before the item
    /// under it if `v` is in its first half, else after it. A point beyond
    /// the body counts as over the nearest end of it.
    fn drop_position(&self, axis: Axis, v: f32) -> Position {
        let st = self.axis(axis);
        let body_end = f32::from_bits(st.extent.to_bits().saturating_sub(1));
        match st.hit(v.min(body_end).max(st.header)) {
            crate::pointer::AxisHit::Item(at) => {
                let after =
                    at.in_gap || at.within >= f64::from(st.measure.size_at(at.position)) / 2.0;
                at.position + u64::from(after)
            }
            _ => st.measure.count(),
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
        self.end_resize();
        let gesture = std::mem::take(&mut self.gesture);
        if gesture.capturing() {
            self.pending_damage.selection = true;
        }
        match gesture {
            Gesture::Pressed {
                press:
                    Press::Header {
                        axis,
                        position,
                        modifiers,
                    },
                dragged: false,
                ..
            } => self.header_click(axis, position, modifiers),
            Gesture::MovingHeaders {
                axis,
                keys,
                positions,
                to,
                ..
            } => {
                let (first, last) = (positions[0], positions[positions.len() - 1]);
                let in_place =
                    last - first + 1 == positions.len() as u64 && (first..=last + 1).contains(&to);
                if !in_place {
                    self.push(Effect::MoveRequested { axis, keys, to });
                }
            }
            _ => {}
        }
    }

    /// A click on the header of the item at `position` on `axis`, acted on
    /// by that axis's `header_click` setting.
    fn header_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        if self.header_click[axis.index()] == HeaderClick::Sort {
            return self.sort_click(axis, position, modifiers);
        }
        // A Reset during the press may have left `position` past the count.
        if self.header_click[axis.index()] != HeaderClick::Select
            || Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0)
            || position >= self.axis(axis).measure.count()
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
    /// it, or the resized item's size. The rest of the press (moves, the release) then finds the Grid idle
    /// and does nothing.
    pub(crate) fn cancel(&mut self) {
        let before = match std::mem::take(&mut self.gesture) {
            Gesture::Pressed { before, .. } | Gesture::DraggingSelection { before } => before,
            Gesture::Resizing {
                axis,
                key,
                start_override,
                ..
            } => {
                self.pending_damage.selection = true;
                self.restore_size(axis, key, start_override);
                return;
            }
            Gesture::MovingHeaders { .. } => {
                self.pending_damage.selection = true;
                return;
            }
            Gesture::Idle => return,
        };
        self.pending_damage.selection = true;
        if self.focus != before.focus {
            self.focus = before.focus;
            self.announce_focus();
        }
        self.set_selection(before.selection);
    }
}

/// The coordinate of (`x`, `y`) along `axis`.
fn along(axis: Axis, x: f32, y: f32) -> f32 {
    match axis {
        Axis::Rows => y,
        Axis::Cols => x,
    }
}
