//! `Refiltered`: the same Keys, now a subset or superset. The picks and the
//! items that were visible are located by Key, so the picks that left can be
//! dropped and the view can keep its place.

use crate::grid::Grid;
use crate::notice::placement;
use crate::snapshot::ScrollAnchor;
use crate::sort::Located;
use crate::types::{Axis, Key, Position};

/// A Key asked about with `Locate` after a `Refiltered`.
#[derive(Clone, Copy, Debug)]
struct Asked {
    key: Key,
    /// `Locate` answers still to come; only the last one is current.
    due: u32,
    /// Its Position, or `None` if it left, once the last answer is in.
    at: Option<Option<Position>>,
}

/// The items visible on one axis when a `Refiltered` arrived, top first,
/// waiting to learn which of them are still there.
#[derive(Clone, Debug)]
struct Shown {
    asked: Vec<Asked>,
    /// The Key of the anchor's item and the anchor's offset into it, kept if
    /// that item stays on top.
    anchor: Option<(Key, f32)>,
}

/// What a `Refiltered` still waits to hear about, per axis.
#[derive(Clone, Debug, Default)]
pub(crate) struct Refiltering {
    shown: [Option<Shown>; 2],
    /// Picked row Keys not yet known to be still there.
    picks: Vec<Asked>,
}

impl Grid {
    /// The axis holds `count` of the same Keys, filtered differently.
    pub(crate) fn refiltered(&mut self, axis: Axis, count: u64) {
        let i = axis.index();
        let st = &self.axes[i];
        let shown = self.refiltering.shown[i].take().unwrap_or_else(|| Shown {
            asked: st
                .strip
                .iter()
                .filter_map(|item| item.key)
                .map(|key| Asked {
                    key,
                    due: 0,
                    at: None,
                })
                .collect(),
            anchor: st
                .strip
                .iter()
                .find(|item| item.position == st.anchor.position)
                .and_then(|item| item.key)
                .map(|key| (key, st.anchor.offset)),
        });
        self.refiltering.shown[i] = Some(shown);
        if axis == Axis::Rows {
            let tracked: Vec<Key> = self.refiltering.picks.iter().map(|a| a.key).collect();
            for &key in &self.selection.picks {
                if !tracked.contains(&key) {
                    self.refiltering.picks.push(Asked {
                        key,
                        due: 0,
                        at: None,
                    });
                }
            }
        }
        self.reordered_or_counted(axis, Some(count), false);
        if count == 0 {
            if axis == Axis::Rows {
                self.refiltering.picks.clear();
                for key in self.selection.picks.clone() {
                    self.unpick(key);
                }
            }
            self.refiltering.shown[i] = None;
            self.axis_mut(axis).clamp_anchor();
            return;
        }
        self.refilter_reask(axis);
    }

    /// The Keys a `Refiltered` asked about on `axis` may have moved again:
    /// any answers so far are stale, so each is asked again.
    pub(crate) fn refilter_reask(&mut self, axis: Axis) {
        let i = axis.index();
        let mut shown = self.refiltering.shown[i].take();
        let mut picks = if axis == Axis::Rows {
            std::mem::take(&mut self.refiltering.picks)
        } else {
            Vec::new()
        };
        let asked = shown.iter_mut().flat_map(|s| s.asked.iter_mut());
        for a in asked.chain(picks.iter_mut()) {
            a.at = None;
            a.due = self.request_locate(axis, a.key, a.due);
        }
        self.refiltering.shown[i] = shown;
        if axis == Axis::Rows {
            self.refiltering.picks = picks;
        }
    }

    /// A `Reset` on `axis` makes no promise about Keys: nothing waits on a
    /// `Refiltered` there any more.
    pub(crate) fn refilter_forget(&mut self, axis: Axis) {
        self.refiltering.shown[axis.index()] = None;
        if axis == Axis::Rows {
            self.refiltering.picks.clear();
        }
    }

    /// The host scrolled `axis`: a `Refiltered` no longer moves its view.
    pub(crate) fn refilter_scrolled(&mut self, axis: Axis) {
        self.refiltering.shown[axis.index()] = None;
    }

    /// A `located` answer about `key` on `axis`, for the Keys a `Refiltered`
    /// asked about: a pick that left is dropped.
    pub(crate) fn refilter_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let at = placement(answer, self.axis(axis).measure.count()).ok();
        let answered = |a: &mut Asked| {
            if a.key != key || a.due == 0 {
                return false;
            }
            a.due -= 1;
            if a.due == 0 {
                a.at = Some(at);
            }
            a.due == 0
        };
        if let Some(shown) = &mut self.refiltering.shown[i] {
            shown.asked.iter_mut().for_each(|a| {
                answered(a);
            });
        }
        if axis != Axis::Rows {
            return;
        }
        let Some(n) = self.refiltering.picks.iter().position(|a| a.key == key) else {
            return;
        };
        if !answered(&mut self.refiltering.picks[n]) {
            return;
        }
        self.refiltering.picks.remove(n);
        if at.is_none() {
            self.unpick(key);
        }
    }

    /// At the start of a frame, once every visible Key a `Refiltered` asked
    /// about on `axis` has its answer and focus there is placed: the topmost
    /// one still there goes to the top, else focus is shown, else the view
    /// goes to the start. Only then, so the frame's Window follows the move.
    pub(crate) fn refilter_settle(&mut self, axis: Axis) {
        let i = axis.index();
        let Some(shown) = &self.refiltering.shown[i] else {
            return;
        };
        if self.locating.waiting[i] || shown.asked.iter().any(|a| a.at.is_none()) {
            return;
        }
        let shown = self.refiltering.shown[i].take().expect("checked above");
        let top = shown
            .asked
            .iter()
            .find_map(|a| a.at.flatten().map(|p| (a.key, p)));
        let focused = self.focus.map(|f| f[i].position);
        let st = self.axis_mut(axis);
        match (top, focused) {
            (Some((key, position)), _) => {
                let offset = match shown.anchor {
                    Some((k, offset)) if k == key => offset,
                    _ => 0.0,
                };
                st.place_anchor(ScrollAnchor { position, offset });
            }
            (None, Some(p)) => {
                st.reveal(p);
            }
            (None, None) => {
                st.place_anchor(ScrollAnchor::default());
            }
        }
        self.pending_damage.layout = true;
        if self.follow_focus_on_reorder {
            self.reveal_focus();
        }
    }

    /// Drops `key` from the picks, and from those a press would restore.
    fn unpick(&mut self, key: Key) {
        if self.selection.picked(key) {
            let mut selection = self.selection.clone();
            selection.toggle_pick(key);
            self.set_selection(selection);
        }
        self.forget_pick_before(key);
    }
}
