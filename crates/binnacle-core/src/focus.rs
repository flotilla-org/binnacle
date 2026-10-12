use crate::axis::AxisState;
use crate::effect::FocusStep;
use crate::types::{Key, Position};

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
            key: st.measure.key_at(position),
        }
    }
}

/// Where a focus move on one axis lands, from `from` (`None` while focus is
/// unplaced: the first move then lands on the Scroll Anchor's item, the first
/// visible one, unless it goes to an edge). The axis must not be empty.
pub(crate) fn target(
    st: &AxisState,
    from: Option<Position>,
    by: FocusStep,
    forward: bool,
) -> Position {
    let last = st.measure.count() - 1;
    let Some(p) = from else {
        return match (by, forward) {
            (FocusStep::Edge, true) => last,
            (FocusStep::Edge, false) => 0,
            _ => st.anchor.position.min(last),
        };
    };
    match (by, forward) {
        (FocusStep::Item, true) => p.saturating_add(1).min(last),
        (FocusStep::Item, false) => p.saturating_sub(1),
        (FocusStep::Page, _) => st.page_from(p, forward),
        (FocusStep::Edge, true) => last,
        (FocusStep::Edge, false) => 0,
    }
}
