//! Pinning: Keys the Grid holds at an edge of an axis, drawn in a pinned
//! Strip of their own while the scrolling Strip skips them.

use crate::axis::AxisState;
use crate::effect::Effect;
use crate::grid::Grid;
use crate::notice::placement;
use crate::snapshot::{ScrollAnchor, StripItem};
use crate::sort::Located;
use crate::types::{Axis, Edge, Key, Position};
use std::ops::Range;

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
    /// Pins `key` to `edge`, drawn just before the pinned `before` (at the
    /// end of the list for `None`, or if `before` is not pinned there),
    /// moving it there if it was pinned already. Returns whether anything
    /// changed.
    pub fn pin_before(&mut self, edge: Edge, key: Key, before: Option<Key>) -> bool {
        if before == Some(key) {
            return false;
        }
        let was = self.pins.clone();
        let pin = self.take_pin(key).unwrap_or(Pin {
            key,
            at: self.measure.position_of(key),
            due: 0,
        });
        let list = &mut self.pins[edge.index()];
        let at = (before.and_then(|b| list.iter().position(|p| p.key == b))).unwrap_or(list.len());
        list.insert(at, pin);
        if self.pins == was {
            return false;
        }
        self.sync_pins();
        true
    }

    /// Where a header dragged to `v` would be pinned: into the pinned Strip
    /// of the edge `v` is over, drawn before the pinned item whose first
    /// half `v` is in (`None` past the last), or into an edge whose Strip is
    /// empty while `v` is in its pin zone or past it, out of the scrolling
    /// region. `None` over the scrolling region.
    pub fn pin_slot(&self, v: f32) -> Option<(Edge, Option<Key>)> {
        let edge = Edge::ALL.into_iter().find(|&edge| {
            let empty = self.drawn_pins(edge).next().is_none();
            let zone = self.pin_zone > 0.0;
            match (edge, empty) {
                (Edge::Leading, true) => zone && v < self.header + self.pin_zone,
                (Edge::Leading, false) => v < self.scroll_start(),
                (Edge::Trailing, true) => zone && v >= self.extent - self.pin_zone,
                (Edge::Trailing, false) => v >= self.scroll_end(),
            }
        })?;
        let before = (self.pinned_items(edge))
            .find(|it| v < it.offset + it.size / 2.0)
            .and_then(|it| it.key);
        Some((edge, before))
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
    /// longer reaches, unless a `Locate` about them is due: until it
    /// answers they are drawn as they were.
    pub fn drawn_pins(&self, edge: Edge) -> impl Iterator<Item = (Key, Position)> + '_ {
        let count = self.measure.count();
        (self.pins[edge.index()].iter())
            .filter_map(move |pin| Some((pin.key, pin.at.filter(|&p| p < count || pin.due > 0)?)))
    }

    /// The pins of `edge` the Grid can act on, in pin order, at their
    /// trusted Positions (see [`AxisState::trusted`]): the Positions the
    /// scrolling Strip skips. Focus and the selection range step through
    /// these, never onto a pin whose `Locate` is still due.
    pub fn trusted_pins(&self, edge: Edge) -> impl Iterator<Item = (Key, Position)> + '_ {
        (self.pins[edge.index()].iter()).filter_map(|pin| Some((pin.key, self.trusted(pin)?)))
    }

    /// The pinned Strip of `edge` in pin order with a Gap between items, as
    /// it would draw unclipped: the leading one laid out from the Header
    /// Band, the trailing one laid out back from the viewport's far end, so
    /// its last item ends flush with it.
    fn laid_out(&self, edge: Edge) -> Vec<StripItem> {
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
        items
    }

    /// The pinned Strip of `edge` as drawn: laid out as [`Self::laid_out`]
    /// does, slid to show its focused item. Its items are clipped to
    /// [`Self::pinned_region`].
    pub fn pinned_items(&self, edge: Edge) -> impl Iterator<Item = StripItem> + '_ {
        let mut items = self.laid_out(edge);
        let slide = self.slide[edge.index()];
        for it in &mut items {
            match edge {
                Edge::Leading => it.offset -= slide,
                Edge::Trailing => it.offset += slide,
            }
        }
        items.into_iter()
    }

    /// Where the pinned Strips of both edges may draw. Together with their
    /// Edge Gaps they take at most the viewport past the Header Band less
    /// the minimum scrolling region: the leading Strip first, then the
    /// trailing one, unless the focused item is in the trailing one, which
    /// then keeps room for it. Each clips at its inside end, and a Strip
    /// clipped whole keeps its Edge Gap.
    pub fn pinned_regions(&self) -> [Range<f32>; 2] {
        let strips = Edge::ALL.map(|edge| self.laid_out(edge));
        let [leading, trailing] = &strips;
        let want = [
            leading.last().map_or(0.0, |it| it.end() - self.header),
            trailing.first().map_or(0.0, |it| self.extent - it.offset),
        ];
        let edge_gap = strips
            .clone()
            .map(|s| if s.is_empty() { 0.0 } else { self.edge_gap() });
        let focused = self
            .focused
            .and_then(|key| trailing.iter().find(|it| it.key == Some(key)));
        let room = (self.extent - self.header - self.min_scrolling).max(0.0);
        // The trailing Strip keeps its Edge Gap, so its edge still shows.
        let kept = focused.map_or(0.0, |it| it.size.min(want[1])) + edge_gap[1];
        let lead = (room - edge_gap[0] - kept).max(0.0);
        // A leading Strip that clips leaves the trailing one only what was
        // kept for its focused item: anything more is rounding.
        let trail = if want[0] > lead {
            kept - edge_gap[1]
        } else {
            (room - (want[0] + edge_gap[0]) - edge_gap[1]).max(0.0)
        };
        // Unclipped, each ends where its items do.
        let lead_end = match leading.last() {
            Some(it) if want[0] <= lead => it.end(),
            _ => self.header + lead.min(want[0]),
        };
        let trail_start = match trailing.first() {
            Some(it) if want[1] <= trail => it.offset,
            _ => self.extent - trail.min(want[1]),
        };
        // Sums round, so the regions are kept apart and inside the viewport.
        let lead_end = lead_end.min(self.extent).max(self.header);
        let trail_start = (trail_start.max(lead_end + edge_gap[0] + edge_gap[1])).min(self.extent);
        [self.header..lead_end, trail_start..self.extent]
    }

    /// Where the pinned Strip of `edge` may draw.
    pub fn pinned_region(&self, edge: Edge) -> Range<f32> {
        self.pinned_regions()[edge.index()].clone()
    }

    /// How far the pinned Strip of `edge` slides to show the focused item,
    /// if that item is in it and clipped: so the item ends at the inside
    /// end of the region, or starts at its start if it is longer.
    pub fn slide_for(&self, edge: Edge) -> f32 {
        let Some(key) = self.focused else {
            return 0.0;
        };
        let items = self.laid_out(edge);
        let Some(it) = items.iter().find(|it| it.key == Some(key)) else {
            return 0.0;
        };
        let region = self.pinned_region(edge);
        match edge {
            Edge::Leading => (it.end() - region.end)
                .min(it.offset - region.start)
                .max(0.0),
            Edge::Trailing => (region.start - it.offset).max(0.0),
        }
    }

    /// The last Position "freeze up to here" on the item at `p` pins: the
    /// items from the start to `p` not pinned already, as far as they fit
    /// whole after the leading pinned Strip and still leave the scrolling
    /// region, past the Edge Gap, room for an item of the default size and
    /// the minimum scrolling region.
    /// `None` if not even the first fits.
    fn fits_through(&self, p: Position) -> Option<Position> {
        let (m, end) = (&self.measure, self.scroll_end());
        let mut offset =
            (self.laid_out(Edge::Leading).last()).map_or(self.header, |it| it.end() + m.gap());
        let scrolls = m.default_size().max(self.min_scrolling);
        let mut last = None;
        for q in 0..=p.min(m.count().saturating_sub(1)) {
            if m.skipped(q) {
                continue;
            }
            let size = m.size_at(q);
            if offset + size + self.edge_gap() + scrolls > end {
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
    /// leading pinned Strip's region, with the Edge Gap after it.
    pub fn scroll_start(&self) -> f32 {
        if self.drawn_pins(Edge::Leading).next().is_none() {
            return self.header;
        }
        self.pinned_region(Edge::Leading).end + self.edge_gap()
    }

    /// Where the scrolling Strip ends: at the viewport's far end, or the
    /// Edge Gap before the trailing pinned Strip's region.
    pub fn scroll_end(&self) -> f32 {
        if self.drawn_pins(Edge::Trailing).next().is_none() {
            return self.extent;
        }
        self.pinned_region(Edge::Trailing).start - self.edge_gap()
    }
}

impl Grid {
    /// Pins the items with `keys` on `axis` to `edge`, in that order, drawn
    /// just before the pinned `before` (after the last for `None`).
    pub(crate) fn pin_before(&mut self, axis: Axis, edge: Edge, keys: &[Key], before: Option<Key>) {
        let was = self.pin_lists(axis);
        for &key in keys {
            if self.axis_mut(axis).pin_before(edge, key, before) {
                if self.axis(axis).measure.position_of(key).is_none() {
                    self.locate_pin(axis, key);
                }
                self.pending_damage.layout = true;
            }
        }
        self.announce_pins(axis, was);
    }

    /// Pins the item with `key` on `axis` to `edge`: it is drawn in that
    /// edge's pinned Strip, after the Keys pinned there before, however the
    /// axis is sorted or scrolled. A Key pinned already moves to the end of
    /// `edge`'s list. A Key outside the Window is found with `Locate`, and
    /// drawn once `located` answers.
    pub fn pin(&mut self, axis: Axis, edge: Edge, key: Key) {
        self.pin_before(axis, edge, &[key], None);
    }

    /// How far, along `axis`, the zone at each end of its scrolling region
    /// reaches while headers are dragged: a drop there, or past it over the
    /// Header Band or beyond the viewport, pins to that edge when its pinned
    /// Strip is empty; 0 turns that off. Default a quarter of the axis's
    /// default item size, which leaves the first item's first half room to
    /// drop before it. Values that are not finite and non-negative are
    /// ignored.
    pub fn set_pin_zone(&mut self, axis: Axis, size: f32) {
        let zone = &mut self.axis_mut(axis).pin_zone;
        if size.is_finite() && size >= 0.0 && *zone != size {
            *zone = size;
            self.pending_damage.selection = true;
        }
    }

    /// Unpins the item with `key` on `axis`: it goes back to its Position in
    /// the scrolling Strip, and the Scroll Anchor stays where it is.
    pub fn unpin(&mut self, axis: Axis, key: Key) {
        let before = self.pin_lists(axis);
        if self.axis_mut(axis).unpin(key) {
            self.pending_damage.layout = true;
            self.resume_focus();
            self.announce_pins(axis, before);
        }
    }

    /// Makes `keys` the pins of `edge` on `axis`, in that order, say to
    /// restore a saved layout: Keys pinned there before and not listed are
    /// unpinned, a listed Key pinned to the other edge moves here, and a
    /// Key listed twice counts once. A Key outside the Window is found with
    /// `Locate`, and one the Table Source does not have sleeps as a dormant
    /// pin until it comes back.
    pub fn set_pins(&mut self, axis: Axis, edge: Edge, keys: &[Key]) {
        let before = self.pin_lists(axis);
        let st = self.axis_mut(axis);
        let mut listed: Vec<Key> = Vec::new();
        for &key in keys {
            if !listed.contains(&key) {
                listed.push(key);
            }
        }
        let unlisted: Vec<Key> = (st.pins[edge.index()].iter())
            .map(|p| p.key)
            .filter(|k| !listed.contains(k))
            .collect();
        let unpinned = !unlisted.is_empty();
        for key in unlisted {
            st.unpin(key);
        }
        let mut new = Vec::new();
        for &key in &listed {
            let pinned = st.pins.iter().flatten().any(|p| p.key == key);
            st.pin_before(edge, key, None);
            if !pinned && st.measure.position_of(key).is_none() {
                new.push(key);
            }
        }
        for key in new {
            self.locate_pin(axis, key);
        }
        if self.pin_lists(axis) != before {
            self.pending_damage.layout = true;
            if unpinned {
                self.resume_focus();
            }
            self.announce_pins(axis, before);
        }
    }

    /// The Keys pinned to each edge of `axis`, in pin order.
    fn pin_lists(&self, axis: Axis) -> [Vec<Key>; 2] {
        let pins = &self.axis(axis).pins;
        Edge::ALL.map(|edge| pins[edge.index()].iter().map(|p| p.key).collect())
    }

    /// Queues `PinsChanged` for each edge of `axis` whose pins differ from
    /// `before`, in place of one queued for it already.
    fn announce_pins(&mut self, axis: Axis, before: [Vec<Key>; 2]) {
        let now = self.pin_lists(axis);
        for (edge, keys) in Edge::ALL.into_iter().zip(now) {
            if keys == before[edge.index()] {
                continue;
            }
            self.effects.retain(|s| {
                !matches!(&s.effect, Effect::PinsChanged { axis: a, edge: e, .. } if (*a, *e) == (axis, edge))
            });
            self.push(Effect::PinsChanged { axis, edge, keys });
        }
    }

    /// Whether a pin on either axis waits for `Locate`: focus moves wait
    /// too, since the order items are drawn in, which they step through,
    /// is not known until it answers.
    pub(crate) fn pins_waiting(&self) -> bool {
        (self.axes.iter()).any(|st| st.pins.iter().flatten().any(|p| p.due > 0))
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
        self.resume_focus();
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
