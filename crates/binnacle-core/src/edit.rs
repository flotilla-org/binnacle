//! Editing: the host's editor over one cell, entered and left by the host
//! and followed by Key.

use crate::effect::Effect;
use crate::gesture::Gesture;
use crate::grid::Grid;
use crate::sort::Located;
use crate::types::{Axis, Key};

/// How the host's editor ended, for [`Grid::edit_ended`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditEnd {
    /// The host kept the new value.
    Commit,
    /// The host threw the new value away.
    Cancel,
}

impl Grid {
    /// The host has opened its editor on the cell with `row` and `col`
    /// (usually after `EditRequested`, `Activated` or `TextTyped`): the Grid
    /// enters `Editing`, which follows those Keys through Change Notices. A
    /// gesture in progress ends where it is (a resize keeps its size, a
    /// header move is dropped). A host that never calls this never edits.
    pub fn edit_started(&mut self, row: Key, col: Key) {
        if matches!(self.gesture, Gesture::Editing { keys, .. } if keys == [row, col]) {
            return;
        }
        self.end_resize();
        self.gesture = Gesture::Editing {
            keys: [row, col],
            due: [0; 2],
        };
        self.pending_damage.selection = true;
    }

    /// The host closed its editor. The Grid returns to `Idle` and sends
    /// nothing; `end` makes no difference to the Grid in v1.
    pub fn edit_ended(&mut self, _end: EditEnd) {
        if matches!(self.gesture, Gesture::Editing { .. }) {
            self.gesture = Gesture::Idle;
            self.pending_damage.selection = true;
        }
    }

    /// Ends an edit in progress on the Grid's side, telling the host with
    /// `EditCancelled`.
    pub(crate) fn end_edit(&mut self) {
        if let Gesture::Editing {
            keys: [row, col], ..
        } = self.gesture
        {
            self.gesture = Gesture::Idle;
            self.pending_damage.selection = true;
            self.push(Effect::EditCancelled { row, col });
        }
    }

    /// The edited Key on `axis` may have left the Table Source: asks where it
    /// is, unless a `Locate` for it is already queued. If it has left, the
    /// answer cancels the edit.
    pub(crate) fn locate_edit(&mut self, axis: Axis) {
        let i = axis.index();
        let Gesture::Editing { keys, due } = &mut self.gesture else {
            return;
        };
        let locate = Effect::Locate { axis, key: keys[i] };
        if self.effects.iter().any(|s| s.effect == locate) {
            // The queued one answers for the edit too.
            due[i] = due[i].max(1);
        } else {
            due[i] += 1;
            self.push(locate);
        }
    }

    /// A `located` answer about `key` on `axis`: if it is the last one due
    /// for the edited Key and that Key has left, the edit is cancelled.
    pub(crate) fn edit_located(&mut self, axis: Axis, key: Key, answer: &Located) {
        let i = axis.index();
        let count = self.axis(axis).measure.count();
        let Gesture::Editing { keys, due } = &mut self.gesture else {
            return;
        };
        if keys[i] != key || due[i] == 0 {
            return;
        }
        due[i] -= 1;
        let left = match *answer {
            Located::Found(p) => p >= count,
            Located::Gone { .. } => true,
        };
        if due[i] == 0 && left {
            self.end_edit();
        }
    }
}
