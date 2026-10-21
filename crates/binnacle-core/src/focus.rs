use crate::axis::AxisState;
use crate::drawn::Slot;
use crate::effect::FocusStep;
use crate::types::{Axis, Edge, Key, Position};

/// One axis's half of the focused cell: the focused item's Key, which the
/// focus follows, and its last known Position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusEnd {
    pub position: Position,
    /// `None` until the Window holds `position`; until then the focus is drawn
    /// nowhere.
    pub key: Option<Key>,
}

impl FocusEnd {
    pub(crate) fn at(st: &AxisState, position: Position) -> Self {
        FocusEnd {
            position,
            key: st.key_at(position),
        }
    }
}

/// A focus move made while a focused Key's Position is unknown, kept until
/// `located` answers. Runs of the same move are summed into `times`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub axis: Axis,
    pub by: FocusStep,
    pub forward: bool,
    pub extend: bool,
    pub times: u64,
}

/// Where a focus move on one axis lands, from `from` (`None` while focus is
/// unplaced: the first move then lands on the Scroll Anchor's item, the first
/// visible scrolling one, unless it goes to an edge), made `times` times
/// over. Items and edges are counted in drawn order, so focus steps through
/// the leading pinned Strip, the scrolling items and the trailing pinned
/// Strip as they are drawn. A page is a body's length of scrolling items; a
/// page out of a pinned Strip lands on the scrolling item beside it, a page
/// towards the outside edge of one on the first or last drawn item. The axis
/// must not be empty.
pub(crate) fn target(
    st: &AxisState,
    from: Option<FocusEnd>,
    by: FocusStep,
    forward: bool,
    times: u64,
) -> FocusEnd {
    let last = st.drawn_len().saturating_sub(1);
    let edge = |forward: bool| st.end_at(st.slot_at(if forward { last } else { 0 }));
    let Some(from) = from else {
        return match by {
            FocusStep::Edge => edge(forward),
            _ => {
                let p = st.anchor.position.min(st.measure.count() - 1);
                match st.unpinned_near(p, true) {
                    Some(p) => FocusEnd::at(st, p),
                    None => edge(false),
                }
            }
        };
    };
    let slot = st.slot_of(from);
    match by {
        FocusStep::Item => {
            let at = st.drawn_index(slot);
            let to = if forward {
                at.saturating_add(times).min(last)
            } else {
                at.saturating_sub(times)
            };
            st.end_at(st.slot_at(to))
        }
        FocusStep::Edge => edge(forward),
        FocusStep::Page => {
            let p = match (slot, forward) {
                (Slot::Scrolling(p), _) => p,
                (Slot::Pinned(Edge::Leading, _), false)
                | (Slot::Pinned(Edge::Trailing, _), true) => {
                    return edge(forward);
                }
                (Slot::Pinned(..), _) => {
                    return match st.unpinned_near(if forward { 0 } else { u64::MAX }, forward) {
                        Some(p) => FocusEnd::at(st, p),
                        None => edge(forward),
                    };
                }
            };
            let mut p = p;
            for _ in 0..times {
                let next = st.page_from(p, forward);
                if next == p {
                    break;
                }
                p = next;
            }
            FocusEnd::at(st, st.unpinned_near(p, forward).unwrap_or(p))
        }
    }
}
