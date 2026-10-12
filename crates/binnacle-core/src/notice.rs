//! Change Notices: the Table Source telling the Grid how an axis changed, and
//! keyed state following its Keys through the change.

use crate::effect::Effect;
use crate::focus::FocusEnd;
use crate::gesture::{Before, Gesture, Mode};
use crate::grid::Grid;
use crate::sort::Located;
use crate::types::{Axis, Key, Position};

/// A message from the Table Source telling the Grid how an axis changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChangeNotice {
    /// The items are in a new order and the count is unchanged. Focus and
    /// picks follow their Keys (the Grid sends `Locate` for the focused Key),
    /// Size Overrides follow their items, the range is dropped unless it spans
    /// the whole axis, and the Scroll Anchor keeps its Position.
    Reordered,
    /// Anything may have changed, and the axis now holds `count` items:
    /// always safe to send. Focus follows its Key (the Grid sends `Locate`)
    /// and falls back to its last Position clamped to the count; the range is
    /// dropped unless it spans the whole axis; row Size Overrides are dropped
    /// (column ones kept); the Scroll Anchor keeps its Position, clamped.
    Reset(u64),
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
        }
    }

    /// The items on `axis` moved, and if `count` is given the count changed
    /// too (a `Reset`).
    fn changed(&mut self, axis: Axis, count: Option<u64>) {
        let i = axis.index();
        if self.gesture.mode() == Mode::DraggingSelection {
            self.cancel();
            self.relocate();
        }
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
        // A press in progress can no longer restore Positions from before the
        // notice: Cancel restores the state as it is now.
        let now = Before {
            focus: self.focus,
            selection: self.selection.clone(),
        };
        if let Gesture::Pressed { before, .. } = &mut self.gesture {
            *before = now;
        }
        self.axis_mut(axis).clamp_anchor();
        self.pending_damage.layout = true;
        self.pending_damage.positions = true;
        if let Some(key) = self.focus.and_then(|f| f[i].key) {
            self.locate(axis, key);
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
    fn locate(&mut self, axis: Axis, key: Key) {
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
        }
        self.whereabouts[i] = Some(w);
    }

    /// The Table Source's answer to `Locate { axis, key }`. An answer that is
    /// not the last one due, or names a Key the Grid no longer asks about, is
    /// ignored. Once every focused Key is placed, focus moves made meanwhile
    /// are applied.
    pub fn located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let count = self.axis(axis).measure.count();
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
        for axis in Axis::ALL {
            let i = axis.index();
            if self.focus.map(|f| f[i]) != Some(cell[i]) {
                self.locating[i] = false;
            }
        }
    }
}
