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
            ChangeNotice::Reordered => self.reordered(axis),
        }
    }

    fn reordered(&mut self, axis: Axis) {
        let i = axis.index();
        if self.gesture.mode() == Mode::DraggingSelection {
            self.cancel();
            self.relocate();
        }
        if self.selection.span.is_some_and(|s| !s.whole[i]) {
            self.set_span(None);
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
        let st = self.axis_mut(axis);
        st.measure.set_keys(0, &[]);
        st.clamp_anchor();
        self.pending_damage.layout = true;
        self.pending_damage.positions = true;
        if let Some(key) = self.focus.and_then(|f| f[i].key) {
            self.locate(axis, key);
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
            Located::Found(_) => self.left(axis, focused, None),
            Located::Gone { suggest } => self.left(axis, focused, suggest),
        }
        if focused {
            self.locating[i] = false;
            self.pending_damage.selection = true;
            self.settled(true);
        }
    }

    /// The Key being located on `axis` has left: forget it, and if it was
    /// focused move focus to `suggest`, else to its last known Position,
    /// clamped to the count.
    fn left(&mut self, axis: Axis, focused: bool, suggest: Option<Position>) {
        self.whereabouts[axis.index()] = None;
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
