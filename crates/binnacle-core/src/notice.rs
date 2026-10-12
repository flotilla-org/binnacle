//! Change Notices: the Table Source telling the Grid how an axis changed, and
//! keyed state following its Keys through the change.

use std::ops::Range;

use crate::effect::Effect;
use crate::focus::FocusEnd;
use crate::gesture::{Before, Gesture, Mode, Press};
use crate::grid::Grid;
use crate::measure::AxisMeasure;
use crate::selection::Span;
use crate::snapshot::ScrollAnchor;
use crate::sort::Located;
use crate::types::{Axis, Key, Position};

/// A message from the Table Source telling the Grid how an axis changed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ChangeNotice {
    /// The items are in a new order and the count is unchanged. Focus and
    /// picks follow their Keys (the Grid sends `Locate` for the focused Key),
    /// Size Overrides follow their items, the range is dropped unless it spans
    /// the whole axis, the Scroll Anchor keeps its Position, and a header
    /// move on that axis is cancelled.
    Reordered,
    /// Anything may have changed, and the axis now holds `count` items:
    /// always safe to send. Focus follows its Key (the Grid sends `Locate`)
    /// and falls back to its last Position clamped to the count; the range is
    /// dropped unless it spans the whole axis; row Size Overrides are dropped
    /// (column ones kept); the Scroll Anchor keeps its Position, clamped.
    Reset(u64),
    /// `n` items were inserted before the item at Position `at` (the count
    /// for the end). Everything from `at` on moves `n` later: focus, the
    /// range and the Scroll Anchor move with their items, so the view stays
    /// steady.
    Inserted { at: Position, n: u64 },
    /// The `n` items from Position `at` were removed and those after them
    /// move `n` earlier. Focus on a removed item falls back as for a `Gone`
    /// answer, to the item that took its place; removed Keys the Grid can
    /// see leave the picks and, on the rows, lose their Size Overrides.
    Removed { at: Position, n: u64 },
    /// The content of the items at these Positions changed; their Keys and
    /// order did not. Their Keys and content sizes are fetched again.
    ContentChanged(Range<Position>),
}

/// Where a focused Key went after a Change Notice, as far as the Grid knows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Whereabouts {
    pub key: Key,
    /// Its Position since the notice, once `located` answers.
    pub at: Option<Position>,
    /// `Locate` answers still to come; only the last one is current.
    due: u32,
}

impl Grid {
    /// Tells the Grid how `axis` changed in the Table Source.
    pub fn notify(&mut self, axis: Axis, notice: ChangeNotice) {
        match notice {
            ChangeNotice::Reordered => self.changed(axis, None),
            ChangeNotice::Reset(count) => self.changed(axis, Some(count)),
            ChangeNotice::Inserted { at, n } => {
                let at = at.min(self.axis(axis).measure.count());
                if n > 0 {
                    self.shifted(axis, Shift::Inserted { at, n });
                }
            }
            ChangeNotice::Removed { at, n } => {
                let count = self.axis(axis).measure.count();
                let at = at.min(count);
                let n = n.min(count - at);
                if n > 0 {
                    self.shifted(axis, Shift::Removed { at, n });
                }
            }
            ChangeNotice::ContentChanged(range) => self.content_changed(axis, range),
        }
    }

    /// Ends the gestures a change on `axis` invalidates: a range drag, and a
    /// header move on that axis, whose Positions are stale.
    fn cancel_drags(&mut self, axis: Axis) {
        if self.gesture.mode() == Mode::DraggingSelection {
            self.cancel();
            self.relocate();
        }
        if matches!(self.gesture.mode(), Mode::MovingHeaders { axis: a, .. } if a == axis) {
            self.cancel();
        }
    }

    /// A press in progress can no longer restore Positions from before a
    /// notice: Cancel restores the state as it is now.
    fn rebase_press(&mut self) {
        let now = Before {
            focus: self.focus,
            selection: self.selection.clone(),
        };
        if let Gesture::Pressed { before, .. } = &mut self.gesture {
            *before = now;
        }
    }

    /// Items were inserted or removed on `axis`: everything keyed or placed
    /// by Position moves with its item.
    fn shifted(&mut self, axis: Axis, shift: Shift) {
        let i = axis.index();
        self.cancel_drags(axis);
        if let Gesture::Pressed {
            press: Press::Header {
                axis: a, position, ..
            },
            ..
        } = &mut self.gesture
            && *a == axis
        {
            match shift.map(*position) {
                Some(p) => *position = p,
                // Its header is gone: the release clicks nothing. A header
                // press changed no focus or selection, so nothing to restore.
                None => {
                    self.gesture = Gesture::Idle;
                    self.pending_damage.selection = true;
                }
            }
        }
        let gone = match shift {
            Shift::Inserted { at, n } => {
                self.axis_mut(axis).measure.insert(at, n);
                Vec::new()
            }
            Shift::Removed { at, n } => self.axis_mut(axis).measure.remove(at, n),
        };
        let count = self.axis(axis).measure.count();
        let mut selection = self.selection.clone();
        if axis == Axis::Rows {
            for &key in &gone {
                self.axis_mut(axis).measure.clear_size(key);
            }
            selection.picks.retain(|k| !gone.contains(k));
        }
        selection.span = selection
            .span
            .and_then(|span| shift_span(span, axis, shift, &self.axis(axis).measure));
        if count == 0 {
            selection.span = None;
        }
        self.set_selection(selection);
        if let Some(w) = &mut self.whereabouts[i] {
            match w.at.map(|p| shift.map(p)) {
                Some(Some(p)) => w.at = Some(p),
                Some(None) => self.whereabouts[i] = None,
                None => {}
            }
        }
        if let Shift::Removed { .. } = shift {
            self.edit_removed(axis, &gone);
        }
        self.shift_focus(axis, shift, count);
        self.rebase_press();
        let st = self.axis_mut(axis);
        st.anchor = match shift.map(st.anchor.position) {
            Some(position) if position == st.anchor.position => st.anchor,
            Some(position) => ScrollAnchor {
                position,
                ..st.anchor
            },
            None => ScrollAnchor {
                position: shift.at(),
                offset: 0.0,
            },
        };
        st.clamp_anchor();
        self.pending_damage.layout = true;
        self.pending_damage.positions = true;
    }

    /// Items on `axis` were removed, `gone` being the Keys the Window held:
    /// an edit whose Key went is cancelled, one whose Key the Window cannot
    /// vouch for asks where it is.
    fn edit_removed(&mut self, axis: Axis, gone: &[Key]) {
        let Gesture::Editing { keys, .. } = &self.gesture else {
            return;
        };
        let key = keys[axis.index()];
        let measure = &self.axis(axis).measure;
        if measure.count() == 0 || gone.contains(&key) {
            self.end_edit();
        } else if measure.position_of(key).is_none() {
            self.locate_edit(axis);
        }
    }

    /// Moves the focused end on `axis` with its item. If its item was
    /// removed it falls back to the item that took its place; while its
    /// Position is being located, the fallback moves and the Key is located
    /// again, since an answer on its way may predate the change.
    fn shift_focus(&mut self, axis: Axis, shift: Shift, count: u64) {
        let i = axis.index();
        if count == 0 {
            return self.clamp_positions(axis, 0);
        }
        let fallback = FocusEnd::at(self.axis(axis), shift.at().min(count - 1));
        let Some(f) = &mut self.focus else { return };
        let end = f[i];
        let moved = shift.map(end.position);
        if self.locating[i] {
            f[i].position = moved.unwrap_or(shift.at()).min(count - 1);
            if let Some(key) = end.key {
                self.locate(axis, key);
            }
            return;
        }
        match (moved, end.key) {
            (Some(p), _) => f[i].position = p,
            (None, Some(key)) => {
                self.pending_damage.selection = true;
                self.left(axis, key, true, Some(shift.at()));
            }
            (None, None) => {
                f[i] = fallback;
                self.pending_damage.selection = true;
                self.announce_focus();
            }
        }
    }

    /// The content of the items at `range` on `axis` changed: any of them in
    /// the Window are fetched again, with their content sizes.
    fn content_changed(&mut self, axis: Axis, range: Range<Position>) {
        let m = &self.axis(axis).measure;
        let start = m.window_start();
        let end = start + m.window_len() as u64;
        if range.start < end && start < range.end {
            self.axis_mut(axis).measure.set_keys(0, &[]);
            self.pending_damage.positions = true;
        }
    }

    /// The items on `axis` moved, and if `count` is given the count changed
    /// too (a `Reset`).
    fn changed(&mut self, axis: Axis, count: Option<u64>) {
        let i = axis.index();
        self.cancel_drags(axis);
        if self.selection.span.is_some_and(|s| !s.whole[i]) || count == Some(0) {
            self.set_span(None);
        }
        let st = self.axis_mut(axis);
        match count {
            None => {
                st.measure.set_keys(0, &[]);
            }
            Some(count) => {
                let resized = st.measure.count() != count;
                st.measure.set_count(count);
                if axis == Axis::Rows {
                    st.measure.clear_sizes();
                }
                self.clamp_positions(axis, count);
                // A range whole on this axis now spans a different count.
                if resized && self.selection.span.is_some() {
                    self.announce_selection();
                }
            }
        }
        self.rebase_press();
        self.axis_mut(axis).clamp_anchor();
        self.pending_damage.layout = true;
        self.pending_damage.positions = true;
        if let Some(key) = self.focus.and_then(|f| f[i].key) {
            self.locate(axis, key);
        }
        // An edit follows its Key; only a count change can take the Key away.
        match count {
            Some(0) => self.end_edit(),
            Some(_) => self.locate_edit(axis),
            None => {}
        }
    }

    /// Keeps focus below a new `count` on `axis`: with no items it is
    /// unplaced, otherwise it keeps its Key and its last known Position is
    /// clamped, the fallback if that Key has left. (A range survives only if
    /// whole on `axis`, where its corners do not count.)
    fn clamp_positions(&mut self, axis: Axis, count: u64) {
        let i = axis.index();
        if count == 0 {
            if self.focus.take().is_some() {
                self.pending_damage.selection = true;
                self.announce_focus();
            }
            self.locating = [false; 2];
            self.whereabouts = [None; 2];
            self.moves.clear();
            return;
        }
        if let Some(f) = &mut self.focus {
            f[i].position = f[i].position.min(count - 1);
        }
    }

    /// Asks the Table Source where `key`, focused on `axis`, is now. Focus is
    /// drawn nowhere until it answers.
    pub(crate) fn locate(&mut self, axis: Axis, key: Key) {
        let i = axis.index();
        self.locating[i] = true;
        self.pending_damage.selection = true;
        let mut w = match self.whereabouts[i] {
            Some(w) if w.key == key => w,
            _ => Whereabouts {
                key,
                at: None,
                due: 0,
            },
        };
        w.at = None;
        let locate = Effect::Locate { axis, key };
        if !self.effects.iter().any(|s| s.effect == locate) {
            self.push(locate);
            w.due += 1;
        } else if w.due == 0 {
            // Queued for an edit, or for this Key before focus left it and
            // came back: its answer is this one's.
            w.due = 1;
        }
        self.whereabouts[i] = Some(w);
    }

    /// The Table Source's answer to `Locate { axis, key }`. An answer that is
    /// not the last one due, or names a Key the Grid no longer asks about, is
    /// ignored. Once every focused Key is placed, focus moves made meanwhile
    /// are applied. An edit whose Key has left is cancelled (`EditCancelled`).
    pub fn located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let count = self.axis(axis).measure.count();
        self.edit_located(axis, key, &answer);
        let focused = self.locating[i] && self.focus.and_then(|f| f[i].key) == Some(key);
        let Some(w) = &mut self.whereabouts[i] else {
            return;
        };
        if w.key != key || w.due == 0 {
            return;
        }
        w.due -= 1;
        if w.due > 0 || w.at.is_some() {
            return;
        }
        match answer {
            Located::Found(p) if p < count => {
                w.at = Some(p);
                if focused && let Some(f) = &mut self.focus {
                    f[i].position = p;
                }
            }
            Located::Found(_) => self.left(axis, key, focused, None),
            Located::Gone { suggest } => self.left(axis, key, focused, suggest),
        }
        if focused {
            self.locating[i] = false;
            self.pending_damage.selection = true;
            self.settled(true);
        }
    }

    /// The Key being located on `axis` has left: forget it, drop it from the
    /// picks, and if it was focused move focus to `suggest`, else to its last
    /// known Position, clamped to the count.
    fn left(&mut self, axis: Axis, key: Key, focused: bool, suggest: Option<Position>) {
        self.whereabouts[axis.index()] = None;
        if axis == Axis::Rows && self.selection.picked(key) {
            let mut selection = self.selection.clone();
            selection.toggle_pick(key);
            self.set_selection(selection);
        }
        if !focused {
            return;
        }
        let st = self.axis(axis);
        let count = st.measure.count();
        let Some(mut f) = self.focus else { return };
        if count == 0 {
            self.focus = None;
        } else {
            let p = suggest.unwrap_or(f[axis.index()].position).min(count - 1);
            f[axis.index()] = FocusEnd::at(st, p);
            self.focus = Some(f);
        }
        self.announce_focus();
    }

    /// Called once a `located` answer placed the focus: if no axis is still
    /// waiting, applies the focus moves buffered meanwhile and, if `follow`
    /// and the `follow_focus_on_reorder` setting is on, reveals the focus.
    pub(crate) fn settled(&mut self, follow: bool) {
        if self.locating.iter().any(|&l| l) {
            return;
        }
        if std::mem::take(&mut self.goto)
            && let Some(f) = self.focus
        {
            self.set_span(self.unit_span(f));
            self.reveal = true;
            self.reveal_focus();
        }
        for m in std::mem::take(&mut self.moves) {
            self.move_and_select(m);
        }
        if follow && self.follow_focus_on_reorder {
            self.reveal = true;
            self.reveal_focus();
        }
    }

    /// After `Cancel` restored an earlier focus: an end whose Key a Change
    /// Notice moved takes the Position learned since, or waits for it again.
    pub(crate) fn relocate(&mut self) {
        let Some(mut f) = self.focus else {
            self.locating = [false; 2];
            return;
        };
        for axis in Axis::ALL {
            let i = axis.index();
            self.locating[i] = false;
            match self.whereabouts[i] {
                Some(w) if f[i].key == Some(w.key) => match w.at {
                    Some(p) => f[i].position = p,
                    None => self.locating[i] = true,
                },
                _ => {}
            }
        }
        self.focus = Some(f);
        self.settled(false);
    }

    /// Focus is about to be set to `cell`, by direct input or a move made
    /// now: buffered moves are dropped, and an end that changes is no longer
    /// waiting for `located`.
    pub(crate) fn before_focus_set(&mut self, cell: [FocusEnd; 2]) {
        self.moves.clear();
        self.goto = false;
        for axis in Axis::ALL {
            let i = axis.index();
            if self.focus.map(|f| f[i]) != Some(cell[i]) {
                self.locating[i] = false;
            }
        }
    }
}

/// Items inserted or removed on an axis, as a map of old Positions to new.
#[derive(Clone, Copy, Debug)]
enum Shift {
    Inserted { at: Position, n: u64 },
    Removed { at: Position, n: u64 },
}

impl Shift {
    fn at(self) -> Position {
        match self {
            Shift::Inserted { at, .. } | Shift::Removed { at, .. } => at,
        }
    }

    /// Where the item at `p` is now, or `None` if it was removed.
    fn map(self, p: Position) -> Option<Position> {
        match self {
            Shift::Inserted { at, n } if p >= at => Some(p.saturating_add(n)),
            Shift::Removed { at, n } if p >= at + n => Some(p - n),
            Shift::Removed { at, .. } if p >= at => None,
            _ => Some(p),
        }
    }
}

/// The range after `shift` on `axis`. A corner whose item was removed moves
/// to the nearest survivor inside the range; a range whose items on `axis`
/// were all removed is dropped, unless it spans the whole axis.
fn shift_span(mut span: Span, axis: Axis, shift: Shift, measure: &AxisMeasure) -> Option<Span> {
    let i = axis.index();
    let (a, h) = (span.anchor[i].position, span.head[i].position);
    let (lo, hi) = (a.min(h), a.max(h));
    if shift.map(lo).is_none() && shift.map(hi).is_none() && !span.whole[i] {
        return None;
    }
    let last = measure.count().saturating_sub(1);
    for corner in [&mut span.anchor, &mut span.head] {
        let end = &mut corner[i];
        match shift.map(end.position) {
            Some(p) => end.position = p,
            None => {
                let low = end.position == lo;
                let p = if low {
                    shift.at()
                } else {
                    shift.at().saturating_sub(1)
                };
                *end = FocusEnd {
                    position: p.min(last),
                    key: measure.key_at(p.min(last)),
                };
            }
        }
    }
    Some(span)
}
