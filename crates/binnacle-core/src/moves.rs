//! Moves: which axes the Table Source can move now, the move requested and
//! not yet answered, those queued behind it, and the last refusal.

use std::collections::VecDeque;

use crate::effect::Effect;
use crate::gesture::Mode;
use crate::grid::Grid;
use crate::types::{Axis, Key, Position};

/// The Grid's move state.
#[derive(Clone, Debug, Default)]
pub(crate) struct MoveState {
    /// Per axis, whether the Table Source can move its items now.
    pub movable: [bool; 2],
    /// The move requested and not yet answered: its axis and Keys. One at a
    /// time; no other move starts until it is answered.
    pub pending: Option<(Axis, Vec<Key>)>,
    /// Moves asked for with [`Grid::request_move`] while another waited,
    /// requested in turn as each answer comes.
    pub queued: VecDeque<(Axis, Vec<Key>, Position)>,
    /// The last move the Table Source refused and why, until the next press.
    pub refused: Option<(Axis, String)>,
}

impl Grid {
    /// Whether the Table Source can move the items of `axis` now, as it
    /// declares and may change at any time (a source that won't move a
    /// sorted axis withdraws it while sorted). Header drags move items only
    /// on a movable axis: elsewhere no drop line shows and nothing is
    /// requested. Withdrawing it ends a move being dragged on that axis. No
    /// axis is movable until declared.
    pub fn set_movable(&mut self, axis: Axis, movable: bool) {
        let was = std::mem::replace(&mut self.moves.movable[axis.index()], movable);
        if was == movable {
            return;
        }
        self.pending_damage.selection = true;
        if !movable
            && matches!(self.gesture.mode(), Mode::MovingHeaders { axis: a, .. } if a == axis)
        {
            self.cancel();
        }
    }

    /// The Table Source's answer to a `MoveRequested`: `None` once it has
    /// moved the items (its Change Notices may come before or after), or
    /// the reason it refused, which the Snapshot carries until the next
    /// press for the host to show. Either way the moved headers stop showing
    /// pending and the next move can start.
    pub fn move_answered(&mut self, refusal: Option<String>) {
        let Some((axis, _)) = self.moves.pending.take() else {
            return;
        };
        self.moves.refused = refusal.map(|reason| (axis, reason));
        self.pending_damage.selection = true;
        if let Some((axis, keys, to)) = self.moves.queued.pop_front() {
            self.request_move(axis, keys, to);
        }
    }

    /// Asks the Table Source to move the items with `keys`, in that order,
    /// to before Position `to` on `axis`, as a header drop does: a
    /// `MoveRequested` now, or once the moves already asked for are
    /// answered. The source may refuse it, as any move. A restored Layout's
    /// order comes this way.
    pub fn request_move(&mut self, axis: Axis, keys: Vec<Key>, to: Position) {
        if keys.is_empty() {
            return;
        }
        if self.moves.pending.is_some() {
            self.moves.queued.push_back((axis, keys, to));
            return;
        }
        self.moves.pending = Some((axis, keys.clone()));
        self.pending_damage.selection = true;
        self.push(Effect::MoveRequested { axis, keys, to });
    }

    /// Whether a move of the item with `key` on `axis` is waiting for its
    /// answer.
    pub(crate) fn move_pending(&self, axis: Axis, key: Key) -> bool {
        (self.moves.pending.as_ref()).is_some_and(|(a, keys)| *a == axis && keys.contains(&key))
    }
}
