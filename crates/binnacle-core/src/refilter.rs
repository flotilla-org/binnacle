//! `Refiltered`: the same Keys, now a subset or superset. The picks, the
//! range's corners and the items that were visible are located by Key, so
//! the picks that left can be dropped, the range can shrink around the items
//! that left and the view can keep its place.

use crate::focus::FocusEnd;
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

/// A range's corner on one axis, asked about with `Locate` after a
/// `Refiltered`.
#[derive(Clone, Copy, Debug)]
struct Corner {
    key: Key,
    due: u32,
    answer: Option<Located>,
}

/// The corners of a range not whole on one axis, anchor then head, waiting
/// to learn where they went. The range is drawn nowhere until both answer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Corners {
    ends: [Corner; 2],
    /// Whether the anchor came first on the axis.
    anchor_first: bool,
}

/// What a `Refiltered` still waits to hear about, per axis.
#[derive(Clone, Debug, Default)]
pub(crate) struct Refiltering {
    shown: [Option<Shown>; 2],
    /// Per axis, picked Keys not yet known to be still there.
    picks: [Vec<Asked>; 2],
    pub(crate) corners: [Option<Corners>; 2],
}

impl Refiltering {
    /// Whether the range waits to learn where its corners went.
    pub(crate) fn locating_corners(&self) -> bool {
        self.corners.iter().any(Option::is_some)
    }
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
        // A range drag is cancelled first: the picks it brings back are
        // located too.
        self.cancel_drags(axis);
        let tracked: Vec<Key> = self.refiltering.picks[i].iter().map(|a| a.key).collect();
        for &key in &self.selection.picks[i] {
            if !tracked.contains(&key) {
                self.refiltering.picks[i].push(Asked {
                    key,
                    due: 0,
                    at: None,
                });
            }
        }
        self.track_corners(axis);
        self.reordered_or_counted(axis, Some(count), true);
        if count == 0 {
            self.refiltering.picks[i].clear();
            for key in self.selection.picks[i].clone() {
                self.unpick(axis, key);
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
        let mut picks = std::mem::take(&mut self.refiltering.picks[i]);
        let asked = shown.iter_mut().flat_map(|s| s.asked.iter_mut());
        for a in asked.chain(picks.iter_mut()) {
            a.at = None;
            a.due = self.request_locate(axis, a.key, a.due);
        }
        self.refiltering.shown[i] = shown;
        self.refiltering.picks[i] = picks;
        if let Some(mut corners) = self.refiltering.corners[i] {
            for end in &mut corners.ends {
                end.answer = None;
                end.due = self.request_locate(axis, end.key, end.due);
            }
            self.refiltering.corners[i] = Some(corners);
        }
    }

    /// A `Reset` on `axis` makes no promise about Keys: nothing waits on a
    /// `Refiltered` there any more.
    pub(crate) fn refilter_forget(&mut self, axis: Axis) {
        self.refiltering.shown[axis.index()] = None;
        self.refiltering.picks[axis.index()].clear();
        self.refiltering.corners[axis.index()] = None;
    }

    /// The host scrolled `axis`: a `Refiltered` no longer moves its view.
    pub(crate) fn refilter_scrolled(&mut self, axis: Axis) {
        self.refiltering.shown[axis.index()] = None;
    }

    /// A `located` answer about `key` on `axis`, for the Keys a `Refiltered`
    /// asked about: a pick that left is dropped, and a range whose corners
    /// have both answered shrinks.
    pub(crate) fn refilter_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        self.corner_located(axis, key, answer);
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
        let Some(n) = self.refiltering.picks[i].iter().position(|a| a.key == key) else {
            return;
        };
        if !answered(&mut self.refiltering.picks[i][n]) {
            return;
        }
        self.refiltering.picks[i].remove(n);
        if at.is_none() {
            self.unpick(axis, key);
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

    /// A range not whole on `axis` is kept through a `Refiltered` only if
    /// both its corners there have Keys to locate; it waits for their
    /// answers (asked with the other Keys).
    fn track_corners(&mut self, axis: Axis) {
        let i = axis.index();
        let Some(span) = self.selection.span.filter(|s| !s.whole[i]) else {
            return;
        };
        let (anchor, head) = (span.anchor[i], span.head[i]);
        let (Some(a), Some(h)) = (anchor.key, head.key) else {
            return self.set_span(None);
        };
        let corner = |key| Corner {
            key,
            due: 0,
            answer: None,
        };
        self.refiltering.corners[i].get_or_insert(Corners {
            ends: [corner(a), corner(h)],
            anchor_first: anchor.position <= head.position,
        });
        self.pending_damage.selection = true;
    }

    /// A `located` answer about `key`, a corner of the range on `axis`. Once
    /// both corners have answered, the range covers the items still between
    /// them: a corner that left pulls in to the nearest item inside, and the
    /// range clears if none is left (or a corner cannot be placed).
    fn corner_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let i = axis.index();
        let Some(corners) = &mut self.refiltering.corners[i] else {
            return;
        };
        for end in &mut corners.ends {
            if end.key == key && end.due > 0 {
                end.due -= 1;
                if end.due == 0 {
                    end.answer = Some(answer);
                }
            }
        }
        if corners.ends.iter().any(|e| e.answer.is_none()) {
            return;
        }
        let corners = self.refiltering.corners[i].take().expect("checked above");
        self.pending_damage.selection = true;
        let Some(mut span) = self.selection.span else {
            return;
        };
        let st = self.axis(axis);
        let count = st.measure.count();
        let first = usize::from(!corners.anchor_first);
        let mut placed = [None; 2];
        for (n, end) in corners.ends.iter().enumerate() {
            placed[n] = match end.answer.expect("checked above") {
                Located::Found(p) if p < count => Some(FocusEnd {
                    position: p,
                    key: Some(end.key),
                }),
                // Where the Key would sit: the next item's Position.
                Located::Gone { suggest: Some(s) } if s <= count => {
                    let p = if n == first {
                        Some(s).filter(|&s| s < count)
                    } else {
                        s.checked_sub(1)
                    };
                    p.map(|p| FocusEnd::at(st, p))
                }
                _ => None,
            };
        }
        // A notice since may have moved a corner to another item.
        let same = [span.anchor[i].key, span.head[i].key] == corners.ends.map(|e| Some(e.key));
        let span = match placed {
            [Some(a), Some(h)] if same && [a, h][first].position <= [a, h][1 - first].position => {
                span.anchor[i] = a;
                span.head[i] = h;
                Some(span)
            }
            _ => None,
        };
        self.set_span(span);
    }

    /// Drops `key` from the picks of `axis`, and from those a press would
    /// restore.
    fn unpick(&mut self, axis: Axis, key: Key) {
        if self.selection.picked(axis, key) {
            let mut selection = self.selection.clone();
            selection.unpick(axis, key);
            self.set_selection(selection);
        }
        self.forget_pick_before(axis, key);
    }
}
