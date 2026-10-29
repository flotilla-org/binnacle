//! Moves: which axes the Table Source can move now, the move requested and
//! not yet answered, those queued behind it, the last refusal, and each
//! axis's arrangement.

use std::collections::{HashSet, VecDeque};

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
    /// Arrangements asked for with [`Grid::arrange`] while a move waited,
    /// requested in turn as each answer comes.
    pub queued: VecDeque<(Axis, Vec<Key>)>,
    /// The last move the Table Source refused and why, until the next press.
    pub refused: Option<(Axis, String)>,
    /// Per axis, the items placed first, in order: see [`Grid::arranged`].
    pub arranged: [Vec<Key>; 2],
    /// The arrangement of the pending move's axis once the move is made.
    pub arranging: Option<Vec<Key>>,
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
        let arranging = self.moves.arranging.take();
        if let (None, Some(after)) = (&refusal, arranging) {
            self.moves.arranged[axis.index()] = after;
        }
        self.moves.refused = refusal.map(|reason| (axis, reason));
        self.pending_damage.selection = true;
        // An arrangement left empty (asked so, or emptied by `forget`) asks nothing.
        while let Some((axis, keys)) = self.moves.queued.pop_front() {
            if keys.is_empty() {
                self.moves.arranged[axis.index()] = keys;
                continue;
            }
            self.ask_move(axis, keys.clone(), 0, keys);
            break;
        }
    }

    /// Places the items with `keys` first on `axis`, in that order, as a
    /// restored Layout's order asks: they become the axis's arrangement
    /// at once if the Window shows them in place already, else once the
    /// Table Source makes the move to Position 0 this asks for, as a header
    /// drop does: a `MoveRequested` now, or once the move already asked for
    /// is answered (then asked whether in place or not). The source may
    /// refuse it, as any move, which leaves the arrangement as it was. No
    /// `keys` empties the arrangement. Returns whether it asks for a move.
    pub fn arrange(&mut self, axis: Axis, keys: Vec<Key>) -> bool {
        let mut seen = HashSet::new();
        let keys: Vec<Key> = keys.into_iter().filter(|k| seen.insert(*k)).collect();
        // The move waiting may change the Window, and its answer the arrangement.
        if self.moves.pending.is_some() {
            self.moves.queued.push_back((axis, keys));
            return true;
        }
        let measure = &self.axis(axis).measure;
        let in_place = (keys.iter().enumerate()).all(|(p, &k)| measure.key_at(p as u64) == Some(k));
        if in_place {
            self.moves.arranged[axis.index()] = keys;
            return false;
        }
        self.ask_move(axis, keys.clone(), 0, keys);
        true
    }

    /// Requests the move of `keys` to before `to` now, to leave `arranging`
    /// as the arrangement of `axis` once the Table Source makes it.
    pub(crate) fn ask_move(
        &mut self,
        axis: Axis,
        keys: Vec<Key>,
        to: Position,
        arranging: Vec<Key>,
    ) {
        self.moves.arranging = Some(arranging);
        self.moves.pending = Some((axis, keys.clone()));
        self.pending_damage.selection = true;
        self.push(Effect::MoveRequested { axis, keys, to });
    }

    /// The items placed first on `axis`, in order, as [`Grid::arrange`] and
    /// the header moves the Table Source made since leave them: what a
    /// Layout saves as the axis's order. A header move to before Position
    /// `to` places the items before `to`, its own, then the rest of the
    /// arrangement. The Grid knows the items before `to` from the Window,
    /// or from the arrangement where it reaches `to`; a move past both only
    /// takes its items out. Empty until something is placed. Change Notices
    /// leave it, and [`Grid::forget`] takes the Keys it forgets out.
    pub fn arranged(&self, axis: Axis) -> &[Key] {
        &self.moves.arranged[axis.index()]
    }

    /// The arrangement of `axis` once `keys` move to before `to`.
    pub(crate) fn arranged_after(&self, axis: Axis, keys: &[Key], to: Position) -> Vec<Key> {
        let was = &self.moves.arranged[axis.index()];
        let moving: HashSet<Key> = keys.iter().copied().collect();
        let measure = &self.axis(axis).measure;
        let before: Option<Vec<Key>> = (0..to.min(measure.count()))
            .map(|p| measure.key_at(p))
            .collect();
        let before = before.or_else(|| was.get(..usize::try_from(to).ok()?).map(<[Key]>::to_vec));
        let Some(before) = before else {
            return (was.iter())
                .filter(|k| !moving.contains(k))
                .copied()
                .collect();
        };
        let before: Vec<Key> = before.into_iter().filter(|k| !moving.contains(k)).collect();
        let placed: HashSet<Key> = before.iter().chain(keys).copied().collect();
        let rest = was.iter().filter(|k| !placed.contains(k)).copied();
        before.iter().chain(keys).copied().chain(rest).collect()
    }

    /// Whether a move of the item with `key` on `axis` is waiting for its
    /// answer.
    pub(crate) fn move_pending(&self, axis: Axis, key: Key) -> bool {
        (self.moves.pending.as_ref()).is_some_and(|(a, keys)| *a == axis && keys.contains(&key))
    }
}
