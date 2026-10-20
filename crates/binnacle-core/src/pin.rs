//! Pinning: Keys the Grid holds at an edge of an axis, drawn in a pinned
//! Strip of their own while the scrolling Strip skips them.

use crate::axis::AxisState;
use crate::grid::Grid;
use crate::notice::placement;
use crate::snapshot::{ScrollAnchor, StripItem};
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

    /// The pins of `edge` that are drawn, in pin order: dormant pins are
    /// left out, and so are pins whose last known Position the count no
    /// longer reaches.
    pub fn drawn_pins(&self, edge: Edge) -> impl Iterator<Item = (Key, Position)> + '_ {
        let count = self.measure.count();
        (self.pins[edge.index()].iter())
            .filter_map(move |pin| Some((pin.key, pin.at.filter(|&p| p < count)?)))
    }

    /// The pins of `edge` the Grid can act on, in pin order, at their
    /// trusted Positions (see [`AxisState::trusted`]): the Positions the
    /// scrolling Strip skips. Focus and the selection range step through
    /// these, never onto a pin whose `Locate` is still due.
    pub fn trusted_pins(&self, edge: Edge) -> impl Iterator<Item = (Key, Position)> + '_ {
        (self.pins[edge.index()].iter()).filter_map(|pin| Some((pin.key, self.trusted(pin)?)))
    }

    /// The pinned Strip of `edge` in pin order with a Gap between items: the
    /// leading one laid out from the Header Band with a Gap after its last
    /// item, the trailing one laid out back from the viewport's far end, so
    /// its last item ends flush with it.
    pub fn pinned_items(&self, edge: Edge) -> impl Iterator<Item = StripItem> + '_ {
        let gap = self.measure.gap();
        let item = |(key, position)| StripItem {
            position,
            key: Some(key),
            offset: 0.0,
            size: self.measure.size_of(key),
        };
        let mut items: Vec<StripItem> = self.drawn_pins(edge).map(item).collect();
        match edge {
            Edge::Leading => {
                let mut offset = self.header;
                for it in &mut items {
                    it.offset = offset;
                    offset += it.size + gap;
                }
            }
            Edge::Trailing => {
                let mut end = self.extent;
                for it in items.iter_mut().rev() {
                    it.offset = end - it.size;
                    end = it.offset - gap;
                }
            }
        }
        items.into_iter()
    }

    /// The last Position "freeze up to here" on the item at `p` pins: the
    /// items from the start to `p` not pinned already, as far as they fit
    /// whole after the leading pinned Strip and still leave the scrolling
    /// region, past the Edge Gap, room for an item of the default size.
    /// `None` if not even the first fits.
    fn fits_through(&self, p: Position) -> Option<Position> {
        let (m, end) = (&self.measure, self.scroll_end());
        let mut offset =
            (self.pinned_items(Edge::Leading).last()).map_or(self.header, |it| it.end() + m.gap());
        let mut last = None;
        for q in 0..=p.min(m.count().saturating_sub(1)) {
            if m.skipped(q) {
                continue;
            }
            let size = m.size_at(q);
            if offset + size + self.edge_gap() + m.default_size() > end {
                break;
            }
            offset += size + m.gap();
            last = Some(q);
        }
        last
    }

    /// Whether the Window holds every Key a freeze waits for.
    fn freeze_ready(&self) -> bool {
        self.freezing.is_some_and(|last| {
            (0..=last).all(|p| self.measure.skipped(p) || self.measure.key_at(p).is_some())
        })
    }

    /// Where the scrolling Strip starts: past the Header Band and the
    /// leading pinned Strip, with the Edge Gap after its last item.
    pub fn scroll_start(&self) -> f32 {
        self.pinned_items(Edge::Leading)
            .last()
            .map_or(self.header, |it| it.end() + self.edge_gap())
    }

    /// Where the scrolling Strip ends: at the viewport's far end, or the
    /// Edge Gap before the trailing pinned Strip.
    pub fn scroll_end(&self) -> f32 {
        self.pinned_items(Edge::Trailing)
            .next()
            .map_or(self.extent, |it| it.offset - self.edge_gap())
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

    /// "Freeze up to here": pins every item from the start of `axis` to the
    /// one with `key`, in their current order, to the leading edge after the
    /// Keys pinned there already. Items pinned already keep their places,
    /// and only what fits whole in the scrolling region as it stands, less
    /// room for one item of the default size to scroll, is pinned, so on a
    /// long way to `key` the freeze stops short. On a
    /// leading pin it unpins the pins after it instead; on a trailing pin
    /// it does nothing, as on a Key outside the Window. If the Window does
    /// not hold every Key to freeze, the axis scrolls to its start and the
    /// freeze finishes once the next Window does; a Change Notice on the axis
    /// first drops it.
    pub fn freeze(&mut self, axis: Axis, key: Key) {
        let st = self.axis_mut(axis);
        st.freezing = None;
        let leading = &st.pins[Edge::Leading.index()];
        if let Some(i) = leading.iter().position(|p| p.key == key) {
            let after: Vec<Key> = leading[i + 1..].iter().map(|p| p.key).collect();
            for key in after {
                self.unpin(axis, key);
            }
            return;
        }
        if st.pins[Edge::Trailing.index()].iter().any(|p| p.key == key) {
            return;
        }
        let Some(last) = st.measure.position_of(key).and_then(|p| st.fits_through(p)) else {
            return;
        };
        st.freezing = Some(last);
        if !st.freeze_ready() {
            st.place_anchor(ScrollAnchor::default());
            self.pending_damage.layout = true;
            return;
        }
        self.finish_freeze(axis);
    }

    /// Pins what a freeze waits for, once the Window holds every Key of it.
    pub(crate) fn finish_freeze(&mut self, axis: Axis) {
        let st = self.axis(axis);
        if !st.freeze_ready() {
            return;
        }
        let Some(last) = st.freezing else { return };
        let keys: Vec<Key> = (0..=last)
            .filter(|&p| !st.measure.skipped(p))
            .filter_map(|p| st.measure.key_at(p))
            .collect();
        self.axis_mut(axis).freezing = None;
        for key in keys {
            self.pin(axis, Edge::Leading, key);
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

    /// Items were inserted on `axis`: a dormant pin's Key may be among them,
    /// so each dormant pin is asked about again.
    pub(crate) fn locate_dormant_pins(&mut self, axis: Axis) {
        let keys: Vec<Key> = (self.axis(axis).pins.iter().flatten())
            .filter(|p| p.at.is_none())
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
