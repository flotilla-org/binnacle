//! Drawn order: the order an axis's items are drawn in, the leading pinned
//! Strip in pin order, then the scrolling items by Position, then the
//! trailing pinned Strip. Arrow keys and the selection range follow it,
//! counting only the pins whose Positions the Grid trusts, so every Position
//! has one place in it.

use crate::axis::AxisState;
use crate::focus::FocusEnd;
use std::ops::Range;

use crate::types::{Edge, Key, Position};

/// Where an item sits in drawn order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// The `i`th drawn pin of a pinned Strip.
    Pinned(Edge, usize),
    /// A scrolling item, by its Position.
    Scrolling(Position),
}

impl AxisState {
    fn pinned_len(&self, edge: Edge) -> u64 {
        self.trusted_pins(edge).count() as u64
    }

    fn scrolling_len(&self) -> u64 {
        self.measure.count() - self.measure.skipped_len()
    }

    /// How many items are drawn in drawn order, on screen or not.
    pub fn drawn_len(&self) -> u64 {
        self.pinned_len(Edge::Leading) + self.scrolling_len() + self.pinned_len(Edge::Trailing)
    }

    /// Where `end` sits in drawn order: on its pin if its Key is drawn
    /// pinned, or on the pin drawn at its Position, else in the scrolling
    /// Strip.
    pub fn slot_of(&self, end: FocusEnd) -> Slot {
        for edge in Edge::ALL {
            let mut pins = self.trusted_pins(edge);
            if let Some(i) = pins.position(|(k, _)| Some(k) == end.key) {
                return Slot::Pinned(edge, i);
            }
        }
        if let Some(key) = self.pinned_at(end.position) {
            for edge in Edge::ALL {
                if let Some(i) = self.trusted_pins(edge).position(|(k, _)| k == key) {
                    return Slot::Pinned(edge, i);
                }
            }
        }
        Slot::Scrolling(end.position.min(self.measure.count().saturating_sub(1)))
    }

    /// The index of `slot` in drawn order.
    pub fn drawn_index(&self, slot: Slot) -> u64 {
        let leading = self.pinned_len(Edge::Leading);
        match slot {
            Slot::Pinned(Edge::Leading, i) => i as u64,
            Slot::Scrolling(p) => leading + p - self.measure.skipped_before(p),
            Slot::Pinned(Edge::Trailing, i) => leading + self.scrolling_len() + i as u64,
        }
    }

    /// Where in drawn order an item that has left would be drawn: among its
    /// edge's drawn pins if `key` is pinned, else among the scrolling items
    /// at `suggest`, the Position the Table Source suggests for it. The
    /// item there is the next one drawn after it.
    pub fn drawn_gap(&self, key: Key, suggest: Position) -> u64 {
        for edge in Edge::ALL {
            let pins = &self.pins[edge.index()];
            if let Some(j) = pins.iter().position(|pin| pin.key == key) {
                let before = pins[..j].iter().filter(|pin| self.trusted(pin).is_some());
                let start = match edge {
                    Edge::Leading => 0,
                    Edge::Trailing => self.pinned_len(Edge::Leading) + self.scrolling_len(),
                };
                return start + before.count() as u64;
            }
        }
        let suggest = suggest.min(self.measure.count());
        self.pinned_len(Edge::Leading) + suggest - self.measure.skipped_before(suggest)
    }

    /// The slot at `index` in drawn order, clamped to the last.
    pub fn slot_at(&self, index: u64) -> Slot {
        let index = index.min(self.drawn_len().saturating_sub(1));
        let leading = self.pinned_len(Edge::Leading);
        if index < leading {
            return Slot::Pinned(Edge::Leading, index as usize);
        }
        let scrolling = self.scrolling_len();
        match self.measure.nth_unskipped(index - leading) {
            Some(p) if index - leading < scrolling => Slot::Scrolling(p),
            _ => Slot::Pinned(Edge::Trailing, (index - leading - scrolling) as usize),
        }
    }

    /// The focus end on `slot`: a pin's Key at its Position, or the
    /// scrolling item at its Position with whatever Key the Window holds.
    pub fn end_at(&self, slot: Slot) -> FocusEnd {
        match slot {
            Slot::Pinned(edge, i) => match self.trusted_pins(edge).nth(i) {
                Some((key, position)) => FocusEnd {
                    position,
                    key: Some(key),
                },
                None => FocusEnd::at(self, 0),
            },
            Slot::Scrolling(p) => FocusEnd::at(self, p),
        }
    }

    /// The first item drawn at `indices` (the last if not `forward`) whose
    /// Position is outside `gone`, as it is before `gone` is removed.
    pub fn drawn_survivor(
        &self,
        indices: Range<u64>,
        gone: Range<Position>,
        forward: bool,
    ) -> Option<FocusEnd> {
        let mut index = if forward {
            indices.start
        } else {
            indices.end.checked_sub(1)?
        };
        while indices.contains(&index) {
            let slot = self.slot_at(index);
            let end = self.end_at(slot);
            if !gone.contains(&end.position) {
                return Some(end);
            }
            // Gone scrolling items are drawn together: step over them all.
            index = match slot {
                Slot::Scrolling(_) if forward => self.drawn_index(Slot::Scrolling(gone.end)),
                Slot::Scrolling(_) => self
                    .drawn_index(Slot::Scrolling(gone.start))
                    .checked_sub(1)?,
                Slot::Pinned(..) if forward => index + 1,
                Slot::Pinned(..) => index.checked_sub(1)?,
            };
        }
        None
    }

    /// The scrolling item nearest `p` that is not pinned: `p` itself, else
    /// the next one towards the end if `forward` (the previous otherwise),
    /// else the nearest the other way.
    pub fn unpinned_near(&self, p: Position, forward: bool) -> Option<Position> {
        let n = self.measure.skipped_before(p);
        let shown = self.scrolling_len();
        if shown == 0 {
            return None;
        }
        let at = p - n;
        // `at` counts the unpinned items before `p`; `p` itself is the
        // `at`th unpinned item when it is not skipped.
        let skipped = self.measure.skipped(p);
        let index = if !skipped || forward {
            at
        } else {
            at.saturating_sub(1)
        };
        self.measure.nth_unskipped(index.min(shown - 1))
    }

    /// The Positions of the items at `indices` in drawn order, as sorted
    /// runs apart from each other.
    pub fn drawn_runs(&self, indices: Range<u64>) -> Vec<Range<Position>> {
        let mut runs = self.runs_in_drawn_order(indices);
        runs.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<Position>> = Vec::with_capacity(runs.len());
        for r in runs {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged
    }

    /// The Positions of the items at `indices` in drawn order, as runs in
    /// that order: the leading pins, the scrolling items, the trailing pins.
    pub fn runs_in_drawn_order(&self, indices: Range<u64>) -> Vec<Range<Position>> {
        let indices = indices.start..indices.end.min(self.drawn_len());
        let mut runs: Vec<Range<Position>> = Vec::new();
        let (leading, scrolling) = (self.pinned_len(Edge::Leading), self.scrolling_len());
        let pinned = |edge, from: u64, r: &Range<u64>| {
            let (lo, hi) = (r.start.max(from), r.end.min(from + self.pinned_len(edge)));
            (self.trusted_pins(edge).enumerate())
                .filter(move |&(i, _)| (lo..hi).contains(&(from + i as u64)))
                .map(|(_, (_, p))| p..p + 1)
        };
        runs.extend(pinned(Edge::Leading, 0, &indices));
        let (lo, hi) = (
            indices.start.max(leading),
            indices.end.min(leading + scrolling),
        );
        if lo < hi {
            let first = self.measure.nth_unskipped(lo - leading);
            let last = self.measure.nth_unskipped(hi - 1 - leading);
            if let (Some(first), Some(last)) = (first, last) {
                let mut start = first;
                for &s in self.measure.skipped_in(first..last + 1) {
                    if start < s {
                        runs.push(start..s);
                    }
                    start = s + 1;
                }
                runs.push(start..last + 1);
            }
        }
        runs.extend(pinned(Edge::Trailing, leading + scrolling, &indices));
        let mut joined: Vec<Range<Position>> = Vec::with_capacity(runs.len());
        for r in runs {
            match joined.last_mut() {
                Some(last) if r.start == last.end => last.end = r.end,
                _ => joined.push(r),
            }
        }
        joined
    }
}
