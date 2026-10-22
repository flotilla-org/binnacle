//! Pointer gestures: what a press, the moves after it and its release mean,
//! given what they land on.

use crate::effect::Effect;
use crate::focus::FocusEnd;
use crate::grid::Grid;
use crate::grouping::{ItemKind, Outline};
use crate::measure::ItemAt;
use crate::pointer::{Button, Hit, Modifiers, PointerEvent, PointerKind};
use crate::selection::{Cell, HeaderClick, Selection};
use crate::snapshot::{DropTarget, MoveRefusal};
use crate::types::{Axis, Edge, Key, Position};

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
    /// The edge of the Header Band leading `axis` is being dragged: the band,
    /// `start_thickness` thick when the press began, follows the pointer until
    /// release.
    ResizingBand { axis: Axis, start_thickness: f32 },
    /// Headers are being dragged: the header grabbed has `key` on
    /// `axis`, and releasing would do what `target` says (see
    /// [`crate::Snapshot::moving_keys`] for all the dragged items).
    MovingHeaders {
        axis: Axis,
        key: Key,
        target: DropTarget,
    },
    /// The host's editor is open on the cell with these row and column Keys
    /// (see [`crate::Grid::edit_started`]).
    Editing { row: Key, col: Key },
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
    /// coordinate `from` along that axis; `before` when the border is before
    /// the item (a trailing pinned one), which then grows as the pointer
    /// moves back. `start_override` is the item's Size Override before the
    /// press, which `Cancel` puts back.
    Resizing {
        axis: Axis,
        key: Key,
        from: f32,
        before: bool,
        start_size: f32,
        start_override: Option<f32>,
    },
    /// A press on the edge of the Header Band leading `axis`, at coordinate
    /// `from` along that axis, when the band was `start_thickness` thick
    /// (which `Cancel` puts back).
    ResizingBand {
        axis: Axis,
        from: f32,
        start_thickness: f32,
    },
    /// The headers of the items with `keys` on `axis`, at `positions` when
    /// the drag began (both in Position order), are being dragged by the
    /// header of `key`, and releasing would do what `target` says.
    MovingHeaders {
        axis: Axis,
        key: Key,
        keys: Vec<Key>,
        positions: Vec<Position>,
        target: DropTarget,
    },
    /// The host's editor is open on the cell with `keys`. `due` counts, per
    /// axis, the `Locate` answers still to come for its Key.
    Editing {
        keys: [Key; 2],
        due: [u32; 2],
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
            &Gesture::ResizingBand {
                axis,
                start_thickness,
                ..
            } => Mode::ResizingBand {
                axis,
                start_thickness,
            },
            &Gesture::MovingHeaders {
                axis, key, target, ..
            } => Mode::MovingHeaders { axis, key, target },
            &Gesture::Editing {
                keys: [row, col], ..
            } => Mode::Editing { row, col },
            Gesture::Idle | Gesture::Pressed { .. } => Mode::Idle,
        }
    }

    /// Whether the Grid is following a held button, so the host should keep
    /// pointer events coming even outside the viewport.
    pub fn capturing(&self) -> bool {
        !matches!(self, Gesture::Idle | Gesture::Editing { .. })
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
        // keeping a resize made so far. A press ends an edit too: a host that
        // commits on a click elsewhere calls `edit_ended` before passing it on.
        self.end_resize();
        self.end_edit();
        self.gesture = Gesture::Idle;
        if self.moves.refused.take().is_some() {
            self.pending_damage.selection = true;
        }
        if let Some(axis) = self.band_edge_at(x, y) {
            if clicks >= 2 {
                self.push(Effect::BandAutoSizeRequested { axis });
                return;
            }
            self.gesture = Gesture::ResizingBand {
                axis,
                from: along(axis, x, y),
                start_thickness: self.axis(axis).header,
            };
            self.pending_damage.selection = true;
            return;
        }
        if let Some((axis, _, key)) = self.border_at(x, y) {
            if clicks >= 2 {
                self.push(Effect::AutoSizeRequested { axis, key });
                return;
            }
            let st = self.axis(axis);
            let resizing = Gesture::Resizing {
                axis,
                key,
                from: along(axis, x, y),
                before: st.drawn_pins(Edge::Trailing).any(|(k, _)| k == key),
                start_size: st.measure.size_of(key),
                start_override: st.measure.size_override(key),
            };
            self.gesture = resizing;
            self.pending_damage.selection = true;
            return;
        }
        let before = Before {
            focus: self.focus,
            selection: self.selection.clone(),
        };
        let press = match self.hit(x, y) {
            Hit::Cell([r, c]) if !(r.in_gap || c.in_gap) => {
                self.leave_outline();
                // Clicks never scroll, even to a focus move made just before.
                self.reveal = false;
                let cell = self.cell_at(r.position, c.position);
                if modifiers.toggle() {
                    self.toggle_pick(self.pick_axis(), cell);
                    self.set_focus_cell(cell);
                    Press::Pick
                } else if modifiers.shift {
                    self.extend_to(self.focus, cell);
                    Press::Cell
                } else {
                    self.select_cell(cell);
                    if clicks >= 2
                        && let [
                            FocusEnd { key: Some(row), .. },
                            FocusEnd { key: Some(col), .. },
                        ] = cell
                    {
                        self.activate(row, col);
                    }
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
                    Press::Header { axis, position, .. } => self.start_move(axis, position, x, y),
                    _ => {}
                }
            }
            Gesture::MovingHeaders {
                axis, key, keys, ..
            } => {
                let (axis, key, keys) = (*axis, *key, std::mem::take(keys));
                let target = self.drop_target(axis, key, &keys, along(axis, x, y));
                if let Gesture::MovingHeaders { keys: k, .. } = &mut self.gesture {
                    *k = keys;
                }
                if let Gesture::MovingHeaders { target: old, .. } = &mut self.gesture
                    && *old != target
                {
                    *old = target;
                    self.pending_damage.selection = true;
                }
            }
            Gesture::DraggingSelection { .. } => self.drag_selection(x, y),
            &mut Gesture::Resizing {
                axis,
                key,
                from,
                before,
                start_size,
                start_override,
            } => {
                let mut moved = f64::from(along(axis, x, y)) - f64::from(from);
                if before {
                    moved = -moved;
                }
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
            &mut Gesture::ResizingBand {
                axis,
                from,
                start_thickness,
            } => {
                let moved = f64::from(along(axis, x, y)) - f64::from(from);
                let m = &self.axis(axis).measure;
                // A band the host set outside the size range keeps its
                // thickness until the pointer moves along the axis.
                let thickness = if moved == 0.0 {
                    start_thickness
                } else {
                    (f64::from(start_thickness) + moved)
                        .clamp(f64::from(m.min_size()), f64::from(m.max_size()))
                        as f32
                };
                self.put_header(axis, thickness);
            }
            Gesture::Idle | Gesture::Editing { .. } => {}
        }
    }

    /// Restarts a drag of the edge of the band leading `axis`, if one is in
    /// progress, from `thickness` and the pointer's last point.
    pub(crate) fn rebase_band_drag(&mut self, axis: Axis, thickness: f32) {
        let at = self.pointer_at.map(|(x, y)| along(axis, x, y));
        if let Gesture::ResizingBand {
            axis: a,
            from,
            start_thickness,
        } = &mut self.gesture
            && *a == axis
        {
            *start_thickness = thickness;
            *from = at.unwrap_or(*from);
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
    pub(crate) fn end_resize(&mut self) {
        if let Gesture::ResizingBand {
            axis,
            start_thickness,
            ..
        } = self.gesture
        {
            self.gesture = Gesture::Idle;
            self.pending_damage.selection = true;
            let thickness = self.axis(axis).header;
            if thickness != start_thickness {
                self.push(Effect::BandSizeChanged { axis, thickness });
            }
            return;
        }
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

    /// Turns a press on the header at `position` on `axis` into a drag of
    /// that item, or of every selected item if it is in a range spanning
    /// whole items on that axis whose Keys are all known and none pinned. A
    /// pinned item is dragged alone. Header drags are available on any axis,
    /// since pinning is the Grid's own; only a movable one also moves items.
    /// An item without a Key cannot be dragged, nor can any while another
    /// move waits for its answer.
    fn start_move(&mut self, axis: Axis, position: Position, x: f32, y: f32) {
        let i = axis.index();
        if self.moves.pending.is_some() {
            return;
        }
        let st = self.axis(axis);
        let measure = &st.measure;
        let Some(key) = st.key_at(position) else {
            return;
        };
        let pinned = st.pinned_at(position) == Some(key);
        // A range that takes in pinned items moves only the grabbed one.
        let selected = (!pinned)
            .then_some(self.selection.span)
            .flatten()
            .filter(|s| s.whole[axis.other().index()] && !s.whole[i])
            .map(|s| s.range(&self.axes).span(axis).to_vec())
            .and_then(|runs| match runs.as_slice() {
                [r] => Some(r.clone()),
                _ => None,
            })
            .filter(|r| r.contains(&position))
            .filter(|r| measure.skipped_in(r.clone()).is_empty());
        let (keys, positions) = selected
            .and_then(|r| {
                let keys: Option<Vec<Key>> = r.clone().map(|p| measure.key_at(p)).collect();
                Some((keys?, r.collect()))
            })
            .unwrap_or_else(|| (vec![key], vec![position]));
        let target = self.drop_target(axis, key, &keys, along(axis, x, y));
        self.gesture = Gesture::MovingHeaders {
            axis,
            key,
            keys,
            positions,
            target,
        };
        self.pending_damage.selection = true;
    }

    /// What releasing the headers `keys` dragged by the header of `key` at
    /// `v` along `axis` would do: in a pinned Strip, or an empty pin zone,
    /// pin them there; elsewhere move them where the axis can, unpinning a
    /// pinned item first; where it can't, a pinned item is unpinned back to
    /// its own Position and an unpinned one stays. On a grouped axis a move
    /// is refused for Summary Items (a pinned one returns to its place) and
    /// for Leaves dropped outside their group.
    fn drop_target(&self, axis: Axis, key: Key, keys: &[Key], v: f32) -> DropTarget {
        let st = self.axis(axis);
        let from = Edge::ALL
            .into_iter()
            .find(|&e| st.drawn_pins(e).any(|(k, _)| k == key));
        if let Some((edge, before)) = st.pin_slot(v) {
            return DropTarget::Pin {
                edge,
                before,
                reorder: from == Some(edge),
            };
        }
        let movable = self.moves.movable[axis.index()];
        let to = movable.then(|| self.drop_position(axis, v));
        let refusal = to.and_then(|to| self.move_refusal(axis, keys, to));
        let home = |edge| {
            let at = st.drawn_pins(edge).find(|&(k, _)| k == key);
            DropTarget::Unpin {
                at: at.map_or(0, |(_, p)| p),
            }
        };
        match (to, refusal, from) {
            (Some(to), None, _) => DropTarget::Move {
                to,
                unpin: from.is_some(),
            },
            (_, Some(MoveRefusal::Summary), Some(edge)) | (None, _, Some(edge)) => home(edge),
            (Some(to), Some(why), _) => DropTarget::Refused { to, why },
            (None, _, None) => DropTarget::Stay,
        }
    }

    /// Why moving the items `keys` of a grouped `axis` to before Position
    /// `to` is refused, as their outlines and those of the items either side
    /// of `to` say: Summary Items never move, and Leaves move only within
    /// their group. `None` where the Window lacks an outline it needs: the
    /// Table Source decides.
    fn move_refusal(&self, axis: Axis, keys: &[Key], to: Position) -> Option<MoveRefusal> {
        let st = self.axis(axis);
        let outlines: Vec<&Outline> = keys.iter().filter_map(|k| st.outlines.get(k)).collect();
        if outlines.iter().any(|o| o.kind.is_summary()) {
            return Some(MoveRefusal::Summary);
        }
        let parent = outlines.first()?.parent();
        if outlines.iter().any(|o| o.parent() != parent) {
            return Some(MoveRefusal::BetweenGroups);
        }
        let beside = [to.checked_sub(1), Some(to)].map(|p| {
            let key = st.key_at(p?)?;
            st.outlines.get(&key)
        });
        if beside.iter().all(Option::is_none) {
            return None;
        }
        let within = |o: &&Outline| o.kind == ItemKind::Leaf && o.parent() == parent;
        match beside.iter().flatten().any(within) {
            true => None,
            false => Some(MoveRefusal::BetweenGroups),
        }
    }

    /// Where items dragged to `v` along `axis` would drop in the scrolling
    /// Strip: before the item under it if `v` is in its first half, else
    /// after it. A point beyond the scrolling region counts as over the
    /// nearest end of it.
    fn drop_position(&self, axis: Axis, v: f32) -> Position {
        let st = self.axis(axis);
        let body_end = f32::from_bits(st.scroll_end().to_bits().saturating_sub(1));
        match st.hit(v.min(body_end).max(st.scroll_start())) {
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
        if matches!(self.gesture, Gesture::Editing { .. }) {
            return;
        }
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
            } => {
                self.header_click(axis, position, modifiers);
                if axis == Axis::Rows {
                    self.row_header_clicked();
                }
            }
            Gesture::MovingHeaders {
                axis,
                key,
                keys,
                positions,
                target,
            } => match target {
                DropTarget::Pin { edge, before, .. } => self.pin_before(axis, edge, &keys, before),
                DropTarget::Unpin { .. } => self.unpin(axis, key),
                DropTarget::Move { to, unpin } => {
                    if unpin {
                        self.unpin(axis, key);
                    }
                    let (first, last) = (positions[0], positions[positions.len() - 1]);
                    let in_place = last - first + 1 == positions.len() as u64
                        && (first..=last + 1).contains(&to);
                    if !in_place {
                        self.moves.pending = Some((axis, keys.clone()));
                        self.push(Effect::MoveRequested { axis, keys, to });
                    }
                }
                DropTarget::Refused { .. } | DropTarget::Stay => {}
            },
            _ => {}
        }
    }

    /// A click on the header of the item at `position` on `axis`, acted on
    /// by that axis's `header_click` setting; under Sort or Select, Ctrl/Cmd
    /// toggles the item's pick.
    fn header_click(&mut self, axis: Axis, position: Position, modifiers: Modifiers) {
        let click = self.header_click[axis.index()];
        if click == HeaderClick::Sort && !modifiers.toggle() {
            return self.sort_click(axis, position, modifiers);
        }
        // A Reset during the press may have left `position` past the count.
        if click == HeaderClick::None
            || Axis::ALL.iter().any(|&a| self.axis(a).measure.count() == 0)
            || position >= self.axis(axis).measure.count()
        {
            return;
        }
        self.reveal = false;
        let mut cell = self.focus.unwrap_or_else(|| {
            let at = |a: Axis| self.axis(a).anchor.position;
            self.cell_at(at(Axis::Rows), at(Axis::Cols))
        });
        cell[axis.index()] = FocusEnd::at(self.axis(axis), position);
        // The whole of every item across the clicked one.
        let mut whole = [false; 2];
        whole[axis.other().index()] = true;
        if modifiers.toggle() {
            self.toggle_pick(axis, cell);
            self.set_focus_cell(cell);
        } else if self.group_click(axis, position, cell, modifiers) {
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
                picks: Default::default(),
            });
        }
    }

    /// Ends any gesture in progress, restoring focus and selection from before
    /// it, or the resized item's size or band's thickness; an edit is
    /// cancelled with `EditCancelled`. The rest of the press (moves, the
    /// release) then finds the Grid idle and does nothing.
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
            Gesture::ResizingBand {
                axis,
                start_thickness,
                ..
            } => {
                self.pending_damage.selection = true;
                self.put_header(axis, start_thickness);
                return;
            }
            Gesture::MovingHeaders { .. } => {
                self.pending_damage.selection = true;
                return;
            }
            editing @ Gesture::Editing { .. } => {
                self.gesture = editing;
                self.end_edit();
                return;
            }
            Gesture::Idle => return,
        };
        self.pending_damage.selection = true;
        // An end saved without its Key takes it from the Window, as
        // `set_window` would have.
        let focus = before.focus.map(|mut f| {
            for axis in Axis::ALL {
                let end = &mut f[axis.index()];
                end.key = end.key.or(self.axis(axis).key_at(end.position));
            }
            f
        });
        if self.focus != focus {
            self.focus = focus;
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
