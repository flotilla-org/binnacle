//! Pinning: Keys the Grid holds at an edge of an axis, drawn in a pinned
//! Strip of their own while the scrolling Strip skips them.

use crate::axis::AxisState;
use crate::grid::Grid;
use crate::notice::placement;
use crate::snapshot::StripItem;
use crate::sort::Located;
use crate::types::{Axis, Edge, Key, Position};

/// A pinned Key and what the Grid knows of its Position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pin {
    pub key: Key,
    /// Its last known Position, or `None` while it is dormant: its Key has
    /// left the axis, and the pin waits for it to come back.
    pub at: Option<Position>,
    /// `Locate` answers still to come; only the last one is current.
    pub due: u32,
}

impl AxisState {
    /// Pins `key` at the inside end of `edge`'s Strip, at `at` if known,
    /// moving it there if it was pinned already. Returns whether anything
    /// changed.
    pub fn pin(&mut self, edge: Edge, key: Key) -> bool {
        let list = &self.pins[edge.index()];
        if list.last().is_some_and(|p| p.key == key) {
            return false;
        }
        let kept = self.take_pin(key);
        let pin = kept.unwrap_or(Pin {
            key,
            at: self.measure.position_of(key),
            due: 0,
        });
        self.pins[edge.index()].push(pin);
        self.sync_pins();
        true
    }

    /// Unpins `key`. Returns whether it was pinned.
    pub fn unpin(&mut self, key: Key) -> bool {
        let gone = self.take_pin(key).is_some();
        self.sync_pins();
        gone
    }

    fn take_pin(&mut self, key: Key) -> Option<Pin> {
        self.pins.iter_mut().find_map(|list| {
            let i = list.iter().position(|p| p.key == key)?;
            Some(list.remove(i))
        })
    }

    pub fn pin_mut(&mut self, key: Key) -> Option<&mut Pin> {
        self.pins.iter_mut().flatten().find(|p| p.key == key)
    }

    /// The Key of the pinned item at Position `p`, if any.
    pub fn pinned_at(&self, p: Position) -> Option<Key> {
        let mut pins = self.pins.iter().flatten();
        pins.find(|pin| self.trusted(pin) == Some(p))
            .map(|pin| pin.key)
    }

    /// The pin's Position, when the Grid can act on it: the Window shows
    /// its Key there, or no `Locate` about it is due and the Window shows
    /// no other Key there. A pin waiting for `Locate` is still drawn at its
    /// last known Position, but nothing is done at a Position it may have
    /// left.
    pub fn trusted(&self, pin: &Pin) -> Option<Position> {
        let m = &self.measure;
        let p = pin.at.filter(|&p| p < m.count())?;
        match m.key_at(p) {
            Some(k) => (k == pin.key).then_some(p),
            None => (pin.due == 0).then_some(p),
        }
    }

    /// The Key at `p`: from the Window, else from a pin drawn there.
    pub fn key_at(&self, p: Position) -> Option<Key> {
        self.measure.key_at(p).or_else(|| self.pinned_at(p))
    }

    /// Brings the pins up to date with the Window, which counts as an
    /// answer for any pinned Key it holds, and tells the measure which
    /// Positions the scrolling Strip skips: a pin's, unless the Window
    /// shows another Key there.
    pub fn sync_pins(&mut self) {
        let m = &self.measure;
        for pin in self.pins.iter_mut().flatten() {
            if let Some(p) = m.position_of(pin.key) {
                pin.at = Some(p);
            }
        }
        let skipped: Vec<Position> = (self.pins.iter().flatten())
            .filter_map(|pin| self.trusted(pin))
            .collect();
        self.measure.set_skipped(skipped);
    }

    /// The pinned Strip of `edge`, laid out from the Header Band in pin
    /// order with a Gap after each item. Dormant pins are left out, and so
    /// are pins whose last known Position the count no longer reaches.
    pub fn pinned_items(&self, edge: Edge) -> impl Iterator<Item = StripItem> + '_ {
        let (gap, count) = (self.measure.gap(), self.measure.count());
        let mut offset = self.header;
        self.pins[edge.index()].iter().filter_map(move |pin| {
            let position = pin.at.filter(|&p| p < count)?;
            let size = self.measure.size_of(pin.key);
            let item = StripItem {
                position,
                key: Some(pin.key),
                offset,
                size,
            };
            offset += size + gap;
            Some(item)
        })
    }

    /// Where the scrolling Strip starts: past the Header Band and the
    /// leading pinned Strip, with the Gap after its last item.
    pub fn scroll_start(&self) -> f32 {
        self.pinned_items(Edge::Leading)
            .last()
            .map_or(self.header, |it| it.end() + self.measure.gap())
    }
}

impl Grid {
    /// Pins the item with `key` on `axis` to `edge`: it is drawn in that
    /// edge's pinned Strip, after the Keys pinned there before, however the
    /// axis is sorted or scrolled. A Key pinned already moves to the end of
    /// `edge`'s list. A Key outside the Window is found with `Locate`, and
    /// drawn once `located` answers.
    pub fn pin(&mut self, axis: Axis, edge: Edge, key: Key) {
        if !self.axis_mut(axis).pin(edge, key) {
            return;
        }
        if self.axis(axis).measure.position_of(key).is_none() {
            self.locate_pin(axis, key);
        }
        self.pending_damage.layout = true;
    }

    /// Unpins the item with `key` on `axis`: it goes back to its Position in
    /// the scrolling Strip, and the Scroll Anchor stays where it is.
    pub fn unpin(&mut self, axis: Axis, key: Key) {
        if self.axis_mut(axis).unpin(key) {
            self.pending_damage.layout = true;
        }
    }

    /// Asks where the pinned `key` on `axis` is now.
    fn locate_pin(&mut self, axis: Axis, key: Key) {
        let due = self.axis(axis).pins.iter().flatten().find(|p| p.key == key);
        let Some(due) = due.map(|p| p.due) else {
            return;
        };
        let due = self.request_locate(axis, key, due);
        if let Some(pin) = self.axis_mut(axis).pin_mut(key) {
            pin.due = due;
        }
    }

    /// The pinned Keys on `axis` may have moved or left: each is asked
    /// about, and drawn at its last known Position until the answer.
    pub(crate) fn locate_pins(&mut self, axis: Axis) {
        let keys: Vec<Key> = self
            .axis(axis)
            .pins
            .iter()
            .flatten()
            .map(|p| p.key)
            .collect();
        for key in keys {
            self.locate_pin(axis, key);
        }
    }

    /// A `located` answer about a pinned Key: found, it is drawn at its new
    /// Position; gone, the pin goes dormant until its Key comes back.
    pub(crate) fn pin_located(&mut self, axis: Axis, key: Key, answer: Located) {
        let count = self.axis(axis).measure.count();
        let st = self.axis_mut(axis);
        let Some(pin) = st.pin_mut(key) else {
            return;
        };
        if pin.due == 0 {
            return;
        }
        pin.due -= 1;
        if pin.due > 0 {
            return;
        }
        pin.at = placement(answer, count).ok();
        st.sync_pins();
        self.pending_damage.layout = true;
    }

    /// Items were inserted or removed on `axis`: each pin moves with its
    /// item, and one whose item was removed goes dormant.
    pub(crate) fn shift_pins(&mut self, axis: Axis, map: impl Fn(Position) -> Option<Position>) {
        let st = self.axis_mut(axis);
        for pin in st.pins.iter_mut().flatten() {
            pin.at = pin.at.and_then(&map);
        }
        st.sync_pins();
    }
}
