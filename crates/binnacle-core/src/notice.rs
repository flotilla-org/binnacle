//! Change Notices: the Table Source telling the Grid how an axis changed, and
//! keyed state following its Keys through the change.

use std::ops::Range;

use crate::axis::AxisState;
use crate::effect::Effect;
use crate::focus::{FocusEnd, Move};
use crate::gesture::{Before, Gesture, Mode, Press};
use crate::grid::Grid;
use crate::grouping::ItemKind;
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
    /// dropped unless it spans the whole axis; Size Overrides stay with their
    /// Keys, so an item that comes back keeps its size; the Scroll Anchor
    /// keeps its Position, clamped.
    Reset(u64),
    /// The axis holds `count` of the same Keys as before, some left out or
    /// brought back and possibly reordered: a filter changed. Focus follows
    /// its Key (the Grid sends `Locate`) and falls back as for a `Gone`
    /// answer; picks are located too, and those that left are dropped; the
    /// range is dropped unless it spans the whole axis; Size Overrides stay
    /// with their Keys; the topmost visible item that is still there stays at
    /// the top, else focus is shown, else the view goes to the start. An edit
    /// whose Key left is cancelled.
    Refiltered(u64),
    /// `n` items were inserted before the item at Position `at` (the count
    /// for the end). Everything from `at` on moves `n` later: focus, the
    /// range and the Scroll Anchor move with their items, so the view stays
    /// steady. Items that would take the count past `u64::MAX` are ignored.
    Inserted { at: Position, n: u64 },
    /// The `n` items from Position `at` were removed and those after them
    /// move `n` earlier. Focus on a removed item falls back as for a `Gone`
    /// answer, to the item that took its place; removed Keys the Grid can
    /// see leave the picks and lose their Size Overrides.
    Removed { at: Position, n: u64 },
    /// The content of the items at these Positions changed; their Keys and
    /// order did not. Their Keys and content sizes are fetched again.
    ContentChanged(Range<Position>),
    /// The group headed by the Group Item at `at - 1` closed, hiding the `n`
    /// items from Position `at`; those after them move `n` earlier. The
    /// hidden items stay alive: their Size Overrides are kept and pins on
    /// them go dormant until they show again. Focus, a range corner or the
    /// Scroll Anchor on a hidden item moves to the Group Item; picks of
    /// hidden items the Grid can see are dropped.
    Collapsed { at: Position, n: u64 },
    /// The group headed by the Group Item at `at - 1` opened, showing `n`
    /// items before the item at Position `at`. Everything from `at` on moves
    /// `n` later, as for `Inserted`, and dormant pins are asked about again.
    Expanded { at: Position, n: u64 },
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

/// Focus waiting for `located`: which axes wait, where their Keys went, and
/// the input held until every axis is placed.
#[derive(Clone, Debug, Default)]
pub(crate) struct Locating {
    /// Per axis, the focused end's Key is known but its Position is not: a
    /// Change Notice moved it, or `set_focus` named a Key outside the Window,
    /// and `located` has not answered yet.
    pub waiting: [bool; 2],
    pub whereabouts: [Option<Whereabouts>; 2],
    /// Focus moves made while waiting, applied once every axis is placed.
    pub moves: Vec<Move>,
    /// A `set_focus` is waiting: once placed, the focus is selected as a
    /// focus move would and revealed.
    pub select_when_placed: bool,
    /// A find step is waiting: once placed, the focus is revealed.
    pub reveal_when_placed: bool,
}

impl Locating {
    pub fn any(&self) -> bool {
        self.waiting.contains(&true)
    }

    /// Nothing is focused any more: nothing waits and no moves are held.
    fn forget(&mut self) {
        self.waiting = [false; 2];
        self.whereabouts = [None; 2];
        self.moves.clear();
    }
}

/// Where a `located` answer puts the Key on an axis of `count` items:
/// `Ok(position)`, or `Err(suggest)` if it has left (a `Found` past the count
/// has left too, with no suggestion).
pub(crate) fn placement(answer: Located, count: u64) -> Result<Position, Option<Position>> {
    match answer {
        Located::Found(p) if p < count => Ok(p),
        Located::Found(_) => Err(None),
        Located::Gone { suggest } => Err(suggest),
    }
}

/// Where a focus end on `st`'s axis goes when its Key has left: `suggest`,
/// else its last known `position`, clamped to the count; `None` when the
/// axis is empty.
fn fallback_end(st: &AxisState, position: Position, suggest: Option<Position>) -> Option<FocusEnd> {
    let last = st.measure.count().checked_sub(1)?;
    Some(FocusEnd::at(st, suggest.unwrap_or(position).min(last)))
}

impl Grid {
    /// Tells the Grid how `axis` changed in the Table Source. The current
    /// find match is located after a change that may take its Key away. A
    /// change that may show or hide items requests the View Spec again
    /// ([`crate::ViewEdit::ShownChanged`]) when a quick filter searches the
    /// shown items of `axis`.
    pub fn notify(&mut self, axis: Axis, notice: ChangeNotice) {
        // A freeze waiting for the Window named Positions that may have moved.
        self.axis_mut(axis).freezing = None;
        // Items may have been shown or hidden.
        let shown = !matches!(
            notice,
            ChangeNotice::Reordered | ChangeNotice::ContentChanged(_)
        );
        // Keys may have left.
        let folding = matches!(
            notice,
            ChangeNotice::Collapsed { .. } | ChangeNotice::Expanded { .. }
        );
        let left = matches!(
            notice,
            ChangeNotice::Reset(_)
                | ChangeNotice::Refiltered(_)
                | ChangeNotice::Removed { .. }
                | ChangeNotice::Collapsed { .. }
        );
        // The notice answering a toggle on `axis`, whichever it is.
        let toggled = match self.toggled {
            Some((a, key)) if a == axis && !matches!(notice, ChangeNotice::ContentChanged(_)) => {
                self.toggled = None;
                Some(key)
            }
            _ => None,
        };
        match notice {
            ChangeNotice::Reordered => {
                self.reordered_or_counted(axis, None, false);
                self.refilter_reask(axis);
            }
            ChangeNotice::Reset(count) => {
                self.reordered_or_counted(axis, Some(count), false);
                self.refilter_forget(axis);
            }
            ChangeNotice::Refiltered(count) => self.refiltered(axis, count, toggled),
            ChangeNotice::Inserted { at, n } | ChangeNotice::Expanded { at, n } => {
                let count = self.axis(axis).measure.count();
                let at = at.min(count);
                let n = n.min(u64::MAX - count);
                if n > 0 {
                    // In header form a group opening before its Subtotal at
                    // the Scroll Anchor shows what it opened.
                    let anchor = self.axis(axis).anchor;
                    let reveal = folding
                        && self.header_levels(axis) > 0
                        && anchor.position == at
                        && (self.outline_at(axis, at))
                            .is_some_and(|o| o.kind == ItemKind::Subtotal);
                    self.inserted_or_removed(axis, Shift::Inserted { at, n });
                    if reveal {
                        self.axis_mut(axis).anchor = anchor;
                        self.axis_mut(axis).clamp_anchor();
                    }
                    if folding {
                        self.refetch(axis);
                    }
                    self.refilter_reask(axis);
                }
            }
            ChangeNotice::Removed { at, n } | ChangeNotice::Collapsed { at, n } => {
                let count = self.axis(axis).measure.count();
                let at = at.min(count);
                let n = n.min(count - at);
                if n > 0 {
                    let shift = match folding {
                        false => Shift::Removed { at, n },
                        true => Shift::Collapsed {
                            at,
                            n,
                            group: self.collapsed_group(axis, at, n),
                        },
                    };
                    self.inserted_or_removed(axis, shift);
                    if folding {
                        self.refetch(axis);
                    }
                    self.refilter_reask(axis);
                }
            }
            ChangeNotice::ContentChanged(range) => self.content_changed(axis, range),
        }
        if left {
            self.find_locate(axis);
        }
        if shown {
            self.shown_changed(axis);
        }
    }

    /// Ends the gestures a change on `axis` invalidates: a range drag, and a
    /// header move on that axis, whose Positions are stale.
    pub(crate) fn cancel_drags(&mut self, axis: Axis) {
        if self.gesture.mode() == Mode::DraggingSelection {
            self.cancel();
            self.relocate();
        }
        if matches!(self.gesture.mode(), Mode::MovingHeaders { axis: a, .. } if a == axis) {
            self.cancel();
        }
    }

    /// A gesture in progress can no longer restore the state from before a
    /// notice (its Positions are stale) or a `set_focus` (the host moved
    /// focus; Cancel undoes only the gesture): Cancel restores the state as it
    /// is now.
    pub(crate) fn rebase_gesture(&mut self) {
        let now = Before {
            focus: self.focus,
            selection: self.selection.clone(),
        };
        if let Gesture::Pressed { before, .. } | Gesture::DraggingSelection { before } =
            &mut self.gesture
        {
            *before = now;
        }
    }

    /// Items were inserted or removed on `axis`: everything keyed or placed
    /// by Position moves with its item.
    fn inserted_or_removed(&mut self, axis: Axis, shift: Shift) {
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
        let span =
            (self.selection.span).and_then(|span| shift_span(span, axis, shift, self.axis(axis)));
        let gone = match shift {
            Shift::Inserted { at, n } => {
                self.axis_mut(axis).measure.insert(at, n);
                Vec::new()
            }
            Shift::Removed { at, n } | Shift::Collapsed { at, n, .. } => {
                self.axis_mut(axis).measure.remove(at, n)
            }
        };
        self.shift_pins(axis, |p| shift.map(p));
        if matches!(shift, Shift::Inserted { .. }) {
            self.locate_dormant_pins(axis);
        }
        let count = self.axis(axis).measure.count();
        let mut selection = self.selection.clone();
        selection.span = span;
        if let Shift::Removed { .. } = shift {
            for &key in &gone {
                self.axis_mut(axis).measure.clear_size(key);
            }
        }
        selection.picks[i].retain(|k| !gone.contains(k));
        if count == 0 {
            selection.span = None;
        }
        // Corners a `Refiltered` still locates are asked about again.
        let corners = self.refiltering.corners;
        self.set_selection(selection);
        if self.selection.span.is_some() {
            self.refiltering.corners = corners;
        }
        let whereabouts = &mut self.locating.whereabouts[i];
        if let Some(w) = whereabouts {
            match w.at.map(|p| shift.map(p)) {
                Some(Some(p)) => w.at = Some(p),
                Some(None) => *whereabouts = None,
                None => {}
            }
        }
        if !matches!(shift, Shift::Inserted { .. }) {
            self.edit_removed(axis, &gone);
        }
        self.shift_focus(axis, shift, count);
        self.rebase_gesture();
        let st = self.axis_mut(axis);
        st.anchor = match shift.map(st.anchor.position) {
            Some(position) if position == st.anchor.position => st.anchor,
            Some(position) => ScrollAnchor {
                position,
                ..st.anchor
            },
            None => ScrollAnchor {
                position: shift.landing(),
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
        let fallback = fallback_end(self.axis(axis), shift.landing(), None);
        let Some(f) = &mut self.focus else { return };
        let end = f[i];
        let moved = shift.map(end.position);
        if self.locating.waiting[i] {
            f[i].position = moved.unwrap_or(shift.landing()).min(count - 1);
            if let Some(key) = end.key {
                self.locate(axis, key);
            }
            return;
        }
        match (moved, end.key) {
            (Some(p), _) => f[i].position = p,
            (None, _) if matches!(shift, Shift::Collapsed { .. }) => {
                f[i] = fallback.expect("the axis is not empty");
                self.pending_damage.selection = true;
                self.announce_focus();
            }
            (None, Some(key)) => {
                self.pending_damage.selection = true;
                self.key_departed(axis, key, true, Some(shift.at()));
            }
            (None, None) => {
                f[i] = fallback.expect("the axis is not empty");
                self.pending_damage.selection = true;
                self.announce_focus();
            }
        }
    }

    /// Where what stands for a group whose `n` items from `at` close will
    /// be: its Group Item just before them, or in header form its Subtotal
    /// on either side, the one of the deepest group over the first of
    /// them.
    fn collapsed_group(&self, axis: Axis, at: Position, n: u64) -> Option<Position> {
        let before = at.checked_sub(1);
        if self.header_levels(axis) == 0 {
            return before;
        }
        let Some(first) = self.outline_at(axis, at) else {
            return before;
        };
        // How deep the group whose Subtotal is at `p` lies over `first`.
        let depth = |p: Position| {
            let o = self.outline_at(axis, p)?;
            let group = o.parent().filter(|_| o.kind == ItemKind::Subtotal)?;
            first.ancestors.iter().position(|&a| a == group)
        };
        match (before.and_then(depth), depth(at + n)) {
            (Some(b), Some(a)) if a > b => Some(at),
            (None, Some(_)) => Some(at),
            _ => before,
        }
    }

    /// The outlines of `axis` changed with a group opening or closing: the
    /// Window is fetched again.
    fn refetch(&mut self, axis: Axis) {
        self.axis_mut(axis).measure.set_keys(0, &[]);
        self.axis_mut(axis).outlines.clear();
        self.axis_mut(axis).sync_pins();
        self.pending_damage.positions = true;
    }

    /// The content of the items at `range` on `axis` changed: any of them in
    /// the Window are fetched again, with their content sizes.
    fn content_changed(&mut self, axis: Axis, range: Range<Position>) {
        let m = &self.axis(axis).measure;
        let start = m.window_start();
        let end = start + m.window_len() as u64;
        if range.start < end && start < range.end {
            self.axis_mut(axis).measure.set_keys(0, &[]);
            self.axis_mut(axis).sync_pins();
            self.pending_damage.positions = true;
        }
    }

    /// The items on `axis` moved, and if `count` is given the count changed
    /// too (a `Reset` or a `Refiltered`). A range not whole on `axis` goes,
    /// unless `refiltered` (its corners are located instead).
    pub(crate) fn reordered_or_counted(
        &mut self,
        axis: Axis,
        count: Option<u64>,
        refiltered: bool,
    ) {
        let i = axis.index();
        self.cancel_drags(axis);
        if self.selection.span.is_some_and(|s| !s.whole[i]) && !refiltered || count == Some(0) {
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
                self.clamp_positions(axis, count);
                // A range whole on this axis now spans a different count.
                if resized && self.selection.span.is_some() {
                    self.announce_selection();
                }
            }
        }
        // Each pin is asked about before the Positions skipped are worked
        // out, so none is skipped at a Position it may have left.
        self.locate_pins(axis);
        self.axis_mut(axis).sync_pins();
        self.rebase_gesture();
        self.axis_mut(axis).clamp_anchor();
        self.pending_damage.layout = true;
        self.pending_damage.positions = true;
        match self.focus.and_then(|f| f[i].key) {
            Some(key) => self.locate(axis, key),
            // A Position learned before the change is stale.
            None => self.locating.whereabouts[i] = None,
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
    /// clamped, the fallback if that Key has left. So are the range's
    /// corners, until located.
    fn clamp_positions(&mut self, axis: Axis, count: u64) {
        let i = axis.index();
        if count == 0 {
            if self.focus.take().is_some() {
                self.pending_damage.selection = true;
                self.announce_focus();
            }
            self.locating.forget();
            return;
        }
        if let Some(f) = &mut self.focus {
            f[i].position = f[i].position.min(count - 1);
        }
        if let Some(span) = &mut self.selection.span {
            for end in [&mut span.anchor, &mut span.head] {
                end[i].position = end[i].position.min(count - 1);
            }
        }
    }

    /// Asks the Table Source where `key`, focused on `axis`, is now. Focus is
    /// drawn nowhere until it answers.
    pub(crate) fn locate(&mut self, axis: Axis, key: Key) {
        let i = axis.index();
        self.locating.waiting[i] = true;
        self.pending_damage.selection = true;
        let mut w = match self.locating.whereabouts[i] {
            Some(w) if w.key == key => w,
            _ => Whereabouts {
                key,
                at: None,
                due: 0,
            },
        };
        w.at = None;
        w.due = self.request_locate(axis, key, w.due);
        self.locating.whereabouts[i] = Some(w);
    }

    /// Queues `Locate { axis, key }` unless it is already queued, and
    /// returns how many answers a requester with `due` answers outstanding
    /// now waits for: one more for a new request, at least one for a queued
    /// one (queued for an edit, or for this Key before focus left it and came
    /// back), whose answer is this one's too.
    pub(crate) fn request_locate(&mut self, axis: Axis, key: Key, due: u32) -> u32 {
        let locate = Effect::Locate { axis, key };
        if self.effects.iter().any(|s| s.effect == locate) {
            due.max(1)
        } else {
            self.push(locate);
            due + 1
        }
    }

    /// The Table Source's answer to `Locate { axis, key }`. An answer that is
    /// not the last one due, or names a Key the Grid no longer asks about, is
    /// ignored. Once every focused Key is placed, focus moves made meanwhile
    /// are applied. An edit whose Key has left is cancelled (`EditCancelled`).
    pub fn located(&mut self, axis: Axis, key: Key, answer: Located) {
        self.edit_located(axis, key, &answer);
        self.pin_located(axis, key, answer);
        self.refilter_located(axis, key, answer);
        self.find_located(axis, key, answer);
        self.focus_located(axis, key, answer);
    }

    /// A `located` answer about the focused Key on `axis`.
    fn focus_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let count = self.axis(axis).measure.count();
        let focused = self.locating.waiting[i] && self.focus.and_then(|f| f[i].key) == Some(key);
        let Some(w) = &mut self.locating.whereabouts[i] else {
            return;
        };
        if w.key != key || w.due == 0 {
            return;
        }
        w.due -= 1;
        if w.due > 0 || w.at.is_some() {
            return;
        }
        match placement(answer, count) {
            Ok(p) => {
                w.at = Some(p);
                if focused && let Some(f) = &mut self.focus {
                    f[i].position = p;
                }
            }
            Err(suggest) => self.key_departed(axis, key, focused, suggest),
        }
        if focused {
            self.locating.waiting[i] = false;
            self.pending_damage.selection = true;
            if self.resume_focus() && self.follow_focus_on_reorder {
                self.reveal = true;
                self.reveal_focus();
            }
        }
    }

    /// The Key being located on `axis` has left: forget it, drop it from the
    /// picks, and if it was focused move focus to `suggest`, else to its last
    /// known Position, clamped to the count.
    fn key_departed(&mut self, axis: Axis, key: Key, focused: bool, suggest: Option<Position>) {
        self.locating.whereabouts[axis.index()] = None;
        if self.selection.picked(axis, key) {
            let mut selection = self.selection.clone();
            selection.unpick(axis, key);
            self.set_selection(selection);
        }
        self.forget_before(axis, key, suggest);
        if !focused {
            return;
        }
        let Some(mut f) = self.focus else { return };
        let i = axis.index();
        self.focus = fallback_end(self.axis(axis), f[i].position, suggest).map(|end| {
            f[i] = end;
            f
        });
        self.announce_focus();
    }

    /// The state a press would restore on `Cancel` must not bring back a Key
    /// that has left: its focus end falls back as the focus would, and the
    /// Key leaves its picks.
    fn forget_before(&mut self, axis: Axis, key: Key, suggest: Option<Position>) {
        let i = axis.index();
        let st = &self.axes[i];
        let (Gesture::Pressed { before, .. } | Gesture::DraggingSelection { before }) =
            &mut self.gesture
        else {
            return;
        };
        before.selection.unpick(axis, key);
        if let Some(mut f) = before.focus
            && f[i].key == Some(key)
        {
            before.focus = fallback_end(st, f[i].position, suggest).map(|end| {
                f[i] = end;
                f
            });
        }
    }

    /// The state a press would restore on `Cancel` must not bring back a
    /// pick of an item on `axis` that has left.
    pub(crate) fn forget_pick_before(&mut self, axis: Axis, key: Key) {
        if let Gesture::Pressed { before, .. } | Gesture::DraggingSelection { before } =
            &mut self.gesture
        {
            before.selection.unpick(axis, key);
        }
    }

    /// Called when an axis stops waiting for `located`: once no axis
    /// waits, finishes a waiting `set_focus`, and once no pin waits either,
    /// applies the focus moves held meanwhile. Returns false while an axis
    /// still waits.
    pub(crate) fn resume_focus(&mut self) -> bool {
        if self.locating.any() {
            return false;
        }
        if std::mem::take(&mut self.locating.select_when_placed)
            && let Some(f) = self.focus
        {
            self.set_span(self.unit_span(f));
            self.reveal = true;
            self.reveal_focus();
        }
        if std::mem::take(&mut self.locating.reveal_when_placed) && self.focus.is_some() {
            self.reveal = true;
            self.reveal_focus();
        }
        if !self.pins_waiting() {
            for m in std::mem::take(&mut self.locating.moves) {
                self.move_and_select(m);
            }
        }
        true
    }

    /// After `Cancel` restored an earlier focus: an end whose Key a Change
    /// Notice moved takes the Position learned since, or waits for it again.
    pub(crate) fn relocate(&mut self) {
        let Some(mut f) = self.focus else {
            self.locating.waiting = [false; 2];
            return;
        };
        for axis in Axis::ALL {
            let i = axis.index();
            self.locating.waiting[i] = false;
            match self.locating.whereabouts[i] {
                Some(w) if f[i].key == Some(w.key) => match w.at {
                    Some(p) => f[i].position = p,
                    None => self.locating.waiting[i] = true,
                },
                _ => {}
            }
        }
        self.focus = Some(f);
        self.resume_focus();
    }

    /// Focus is about to be set to `cell`, by direct input or a move made
    /// now: buffered moves are dropped, and an end that changes is no longer
    /// waiting for `located`.
    pub(crate) fn before_focus_set(&mut self, cell: [FocusEnd; 2]) {
        self.locating.moves.clear();
        self.locating.select_when_placed = false;
        self.locating.reveal_when_placed = false;
        for axis in Axis::ALL {
            let i = axis.index();
            if self.focus.map(|f| f[i]) != Some(cell[i]) {
                self.locating.waiting[i] = false;
            }
        }
    }
}

/// Items inserted, removed or hidden under a closed group on an axis, as a
/// map of old Positions to new.
#[derive(Clone, Copy, Debug)]
enum Shift {
    Inserted {
        at: Position,
        n: u64,
    },
    Removed {
        at: Position,
        n: u64,
    },
    /// `group` is where what stands for the closed group is after the
    /// shift, if anything does: its Group Item, or in header form its
    /// Subtotal.
    Collapsed {
        at: Position,
        n: u64,
        group: Option<Position>,
    },
}

impl Shift {
    fn at(self) -> Position {
        match self {
            Shift::Inserted { at, .. }
            | Shift::Removed { at, .. }
            | Shift::Collapsed { at, .. } => at,
        }
    }

    /// Where what stood on an item that went now stands, as a Position
    /// after the shift: the item that took its place, or a closed group's
    /// Group Item.
    fn landing(self) -> Position {
        match self {
            Shift::Collapsed { at, group, .. } => group.unwrap_or(at.saturating_sub(1)),
            _ => self.at(),
        }
    }

    /// Where the item at `p` is now, or `None` if it was removed or hidden.
    fn map(self, p: Position) -> Option<Position> {
        match self {
            Shift::Inserted { at, n } if p >= at => Some(p.saturating_add(n)),
            Shift::Removed { at, n } | Shift::Collapsed { at, n, .. } if p >= at + n => Some(p - n),
            Shift::Removed { at, .. } | Shift::Collapsed { at, .. } if p >= at => None,
            _ => Some(p),
        }
    }
}

/// The range after `shift` on `axis`, given the axis as it was before. A
/// corner whose item was removed pulls in to the nearest surviving item
/// drawn inside the range; a range with no item left on `axis` is dropped,
/// unless it spans the whole axis.
fn shift_span(mut span: Span, axis: Axis, shift: Shift, st: &AxisState) -> Option<Span> {
    let i = axis.index();
    let (at, n) = match shift {
        Shift::Inserted { .. } => {
            for corner in [&mut span.anchor, &mut span.head] {
                corner[i].position = shift.map(corner[i].position).expect("nothing removed");
            }
            return Some(span);
        }
        // A corner on a hidden item pulls to the closed group's Group Item.
        Shift::Collapsed {
            at,
            n,
            group: Some(group),
        } => {
            // The item standing for the group, as it was before the shift.
            let was = if group < at { group } else { group + n };
            let group = FocusEnd {
                position: group,
                key: st.key_at(was),
            };
            for corner in [&mut span.anchor, &mut span.head] {
                corner[i] = match shift.map(corner[i].position) {
                    Some(p) => FocusEnd {
                        position: p,
                        ..corner[i]
                    },
                    None => group,
                };
            }
            return Some(span);
        }
        Shift::Removed { at, n } | Shift::Collapsed { at, n, .. } => (at, n),
    };
    let drawn = [span.anchor[i], span.head[i]].map(|e| st.drawn_index(st.slot_of(e)));
    let (lo, hi) = (drawn[0].min(drawn[1]), drawn[0].max(drawn[1]));
    let inside = lo..hi + 1;
    let ends = st
        .drawn_survivor(inside.clone(), at..at + n, true)
        .zip(st.drawn_survivor(inside, at..at + n, false));
    for (corner, d) in [&mut span.anchor, &mut span.head].into_iter().zip(drawn) {
        let end = &mut corner[i];
        *end = match (shift.map(end.position), ends) {
            (Some(p), _) => FocusEnd {
                position: p,
                ..*end
            },
            (None, Some((low, high))) => {
                let end = if d == lo { low } else { high };
                FocusEnd {
                    position: shift.map(end.position).expect("it survived"),
                    ..end
                }
            }
            // A whole axis keeps its corners on the item that took their
            // place, or the last.
            (None, None) if span.whole[i] => {
                let p = if at + n < st.measure.count() {
                    at + n
                } else {
                    at.saturating_sub(1)
                };
                FocusEnd {
                    position: shift.map(p).unwrap_or(0),
                    key: st.key_at(p),
                }
            }
            (None, None) => return None,
        };
    }
    Some(span)
}
