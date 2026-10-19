//! PROTOTYPE: the pinned-Strips model for issue #137, with no terminal code.
//!
//! Question: how do Key-pinned Strips work in the core? Each axis holds two
//! pinned Key lists (leading and trailing) in pin order. Pinned Strips draw
//! from their Keys; the scrolling Strip walks Positions from the Scroll Anchor
//! and skips pinned Keys using the Window. After a `Refiltered` every pin is
//! asked about with `Locate`: pending pins stay drawn, gone pins go dormant,
//! found pins come back. Pinned Strips too wide for the viewport clip from the
//! inside edge and never unpin.
//!
//! Shortcuts a real core would not take, all marked `SHORTCUT`:
//! - sizes are whole terminal cells and there is no Gap;
//! - where the core would request a Window and wait a frame (a focus move to
//!   an unseen Position, the end clamp, "freeze up to here"), this model reads
//!   the source's order directly;
//! - focus and the selection corners are relocated at once after a
//!   `Refiltered`; only pins go through a visible pending `Locate`.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

pub type Position = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Rows,
    Cols,
}

impl Axis {
    pub fn index(self) -> usize {
        match self {
            Axis::Rows => 0,
            Axis::Cols => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Leading,
    Trailing,
}

impl Side {
    pub fn index(self) -> usize {
        match self {
            Side::Leading => 0,
            Side::Trailing => 1,
        }
    }
}

/// What the Grid knows of a pinned Key's place on the axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinState {
    /// On the axis at this Position.
    Found(Position),
    /// A `Locate` is out after a `Refiltered`; still drawn, at its last known
    /// Position if it had one.
    Pending(Option<Position>),
    /// `Locate` answered gone: not drawn, Key kept, back when found.
    Dormant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pin {
    pub key: Key,
    pub state: PinState,
}

impl Pin {
    pub fn drawn(&self) -> bool {
        self.state != PinState::Dormant
    }

    pub fn position(&self) -> Option<Position> {
        match self.state {
            PinState::Found(p) => Some(p),
            PinState::Pending(p) => p,
            PinState::Dormant => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScrollAnchor {
    pub position: Position,
    pub offset: i32,
}

/// The order arrow keys follow along an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusOrder {
    /// Leading pins, then the scrolling items by Position, then trailing pins.
    Drawn,
    /// By Position in the source's order, wherever each item is drawn.
    Source,
}

/// What the one contiguous range is contiguous in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeOrder {
    /// Positions between the corners (today's `CellRange`); may draw as
    /// several runs once pins are involved.
    Source,
    /// Drawn indices between the corners; always one run on screen.
    Drawn,
}

/// Which pinned Strip gives up space first when both do not fit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipFirst {
    Leading,
    Trailing,
}

/// What revealing a focused pinned item that is clipped does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClippedReveal {
    /// Nothing: focus sits on an item that is not visible.
    Nothing,
    /// The pinned Strip slides inside its region to show it.
    Slide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub focus_order: FocusOrder,
    pub range_order: RangeOrder,
    pub clip_first: ClipFirst,
    pub clipped_reveal: ClippedReveal,
}

/// One drawn item: its Key, the Position the Grid knows for it, where it sits
/// unclipped along the axis and the part of that left visible in its region.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub key: Key,
    pub position: Option<Position>,
    pub pending: bool,
    pub offset: i32,
    pub size: i32,
    pub shown: Range<i32>,
}

/// Which of an axis's regions a coordinate is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Header,
    Leading,
    Body,
    Trailing,
    Outside,
}

/// One axis's three Strips and the regions they draw into.
#[derive(Clone, Debug, Default)]
pub struct Strips {
    pub leading: Vec<Placed>,
    pub body: Vec<Placed>,
    pub trailing: Vec<Placed>,
    /// Leading, body and trailing regions, viewport-relative.
    pub spans: [Range<i32>; 3],
    /// What each pinned Strip would take unclipped.
    pub wants: [i32; 2],
}

/// A slot in an axis's drawn order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Pin(Side, usize),
    Body(Position),
}

/// The Grid's view of one axis.
pub struct AxisGrid {
    pub axis: Axis,
    pub count: u64,
    pub default_size: i32,
    pub overrides: HashMap<Key, i32>,
    pub header: i32,
    pub extent: i32,
    /// The scrolling region the pinned Strips must leave.
    pub min_scroll: i32,
    pub anchor: ScrollAnchor,
    pub pins: [Vec<Pin>; 2],
    /// How far each pinned Strip has slid to reveal a clipped item.
    pub slide: [i32; 2],
    pub window_start: Position,
    pub window: Vec<Key>,
    pub needed: Range<Position>,
    pub strips: Strips,
}

const OVERSCAN: u64 = 2;

impl AxisGrid {
    pub fn new(axis: Axis, count: u64, default_size: i32, header: i32, min_scroll: i32) -> Self {
        AxisGrid {
            axis,
            count,
            default_size,
            overrides: HashMap::new(),
            header,
            extent: 0,
            min_scroll,
            anchor: ScrollAnchor::default(),
            pins: [Vec::new(), Vec::new()],
            slide: [0, 0],
            window_start: 0,
            window: Vec::new(),
            needed: 0..0,
            strips: Strips::default(),
        }
    }

    pub fn size(&self, key: Key) -> i32 {
        *self.overrides.get(&key).unwrap_or(&self.default_size)
    }

    pub fn pin_of(&self, key: Key) -> Option<(Side, usize)> {
        for side in [Side::Leading, Side::Trailing] {
            if let Some(i) = self.pins[side.index()].iter().position(|p| p.key == key) {
                return Some((side, i));
            }
        }
        None
    }

    pub fn drawn_pins(&self, side: Side) -> Vec<Pin> {
        self.pins[side.index()]
            .iter()
            .copied()
            .filter(Pin::drawn)
            .collect()
    }

    /// Positions the Grid believes pinned items hold: what the body skips
    /// when it reasons by Position rather than by Window Key.
    pub fn pinned_positions(&self) -> BTreeSet<Position> {
        self.pins
            .iter()
            .flatten()
            .filter_map(Pin::position)
            .collect()
    }

    pub fn window_key(&self, p: Position) -> Option<Key> {
        let i = p.checked_sub(self.window_start)? as usize;
        self.window.get(i).copied()
    }

    /// The Window this axis needs: from the anchor, a body's length of the
    /// smallest items, plus one slot per drawn pin (the body skips those Keys
    /// as it walks, so each costs a Position), plus overscan.
    pub fn compute_needed(&mut self) {
        let smallest = self
            .overrides
            .values()
            .copied()
            .chain([self.default_size])
            .min()
            .unwrap_or(1)
            .max(1);
        let body = (self.extent - self.header).max(0);
        let visible = (body / smallest) as u64 + 1;
        let pins = self.pins.iter().flatten().filter(|p| p.drawn()).count() as u64;
        let start = self.anchor.position.saturating_sub(OVERSCAN);
        let end = (self.anchor.position + visible + pins + OVERSCAN).min(self.count);
        self.needed = start.min(end)..end;
    }

    /// The host fills the Window from the source's order.
    pub fn fill_window(&mut self, order: &[Key]) {
        self.window_start = self.needed.start;
        self.window = order[self.needed.start as usize..self.needed.end as usize].to_vec();
    }

    /// How the viewport splits between the pinned Strips and the body.
    fn allot(&self, clip_first: ClipFirst) -> (i32, i32, [i32; 2]) {
        let wants = [Side::Leading, Side::Trailing]
            .map(|s| self.drawn_pins(s).iter().map(|p| self.size(p.key)).sum());
        let avail = (self.extent - self.header - self.min_scroll).max(0);
        if wants[0] + wants[1] <= avail {
            return (wants[0], wants[1], wants);
        }
        let (l, t) = match clip_first {
            ClipFirst::Leading => {
                let t = wants[1].min(avail);
                (avail - t, t)
            }
            ClipFirst::Trailing => {
                let l = wants[0].min(avail);
                (l, avail - l)
            }
        };
        (l, t, wants)
    }

    pub fn layout(&mut self, clip_first: ClipFirst) {
        let (l, t, wants) = self.allot(clip_first);
        let lead = self.header..self.header + l;
        let trail = self.extent - t..self.extent;
        let body = lead.end..trail.start;
        let clip = |off: i32, size: i32, span: &Range<i32>| {
            let s = off.max(span.start);
            let e = (off + size).min(span.end);
            s..e.max(s)
        };
        // Leading: from the header outwards-in, clipped at the inside edge
        // (its trailing end).
        let mut off = self.header - self.slide[0];
        let mut leading = Vec::new();
        for pin in self.drawn_pins(Side::Leading) {
            let size = self.size(pin.key);
            leading.push(Placed {
                key: pin.key,
                position: pin.position(),
                pending: matches!(pin.state, PinState::Pending(_)),
                offset: off,
                size,
                shown: clip(off, size, &lead),
            });
            off += size;
        }
        // Trailing: flush with the far edge, in pin order left to right,
        // clipped at the inside edge (its leading end).
        let mut off = self.extent - wants[1] + self.slide[1];
        let mut trailing = Vec::new();
        for pin in self.drawn_pins(Side::Trailing) {
            let size = self.size(pin.key);
            trailing.push(Placed {
                key: pin.key,
                position: pin.position(),
                pending: matches!(pin.state, PinState::Pending(_)),
                offset: off,
                size,
                shown: clip(off, size, &trail),
            });
            off += size;
        }
        // Body: walk Positions from the anchor, skipping pinned Keys by
        // looking each Position up in the Window.
        let mut body_items = Vec::new();
        let mut p = self.anchor.position;
        let mut off = body.start - self.anchor.offset;
        while p < self.count && off < body.end {
            let Some(key) = self.window_key(p) else {
                break;
            };
            if self.pin_of(key).is_none() {
                let size = self.size(key);
                body_items.push(Placed {
                    key,
                    position: Some(p),
                    pending: false,
                    offset: off,
                    size,
                    shown: clip(off, size, &body),
                });
                off += size;
            }
            p += 1;
        }
        self.strips = Strips {
            leading,
            body: body_items,
            trailing,
            spans: [lead, body, trail],
            wants,
        };
    }

    pub fn hit(&self, v: i32) -> (Region, Option<&Placed>) {
        if v < 0 || v >= self.extent {
            return (Region::Outside, None);
        }
        if v < self.header {
            return (Region::Header, None);
        }
        let s = &self.strips;
        for (region, span, items) in [
            (Region::Leading, &s.spans[0], &s.leading),
            (Region::Body, &s.spans[1], &s.body),
            (Region::Trailing, &s.spans[2], &s.trailing),
        ] {
            if span.contains(&v) {
                let item = items.iter().find(|i| i.shown.contains(&v));
                return (
                    if item.is_some() {
                        region
                    } else {
                        Region::Outside
                    },
                    item,
                );
            }
        }
        (Region::Outside, None)
    }

    /// The scrolling items' total length: every item on the axis less the
    /// pinned ones the Grid believes are still on it (found or pending). A
    /// pending pin that has in fact gone is subtracted anyway: off by one item
    /// until its answer comes.
    pub fn scroll_extent(&self, order: &[Key]) -> i32 {
        // SHORTCUT: sums real sizes; the core estimates unseen ones.
        let all: i32 = order.iter().map(|&k| self.size(k)).sum();
        let pinned: i32 = self
            .pins
            .iter()
            .flatten()
            .filter(|p| p.drawn())
            .map(|p| self.size(p.key))
            .sum();
        all - pinned
    }

    pub fn body_len(&self) -> i32 {
        let s = &self.strips.spans[1];
        s.end - s.start
    }

    /// Whether Position `p` holds a pinned item: by its Key where the Window
    /// has it, else by the Positions pins were last known at (stale while a
    /// `Locate` is pending after a reorder).
    pub fn pinned_at(&self, p: Position, pinned: &BTreeSet<Position>) -> bool {
        match self.window_key(p) {
            Some(k) => self.pin_of(k).is_some(),
            None => pinned.contains(&p),
        }
    }

    fn next_body(&self, from: Option<Position>, forward: bool) -> Option<Position> {
        let pinned = self.pinned_positions();
        let mut p = from;
        loop {
            let q = match (p, forward) {
                (None, true) => 0,
                (None, false) => self.count.checked_sub(1)?,
                (Some(p), true) => p + 1,
                (Some(p), false) => p.checked_sub(1)?,
            };
            if q >= self.count {
                return None;
            }
            if !self.pinned_at(q, &pinned) {
                return Some(q);
            }
            p = Some(q);
        }
    }

    /// The anchor must sit on a scrolling item: step it off a pinned one.
    pub fn settle_anchor(&mut self, order: &[Key]) {
        let pinned = |p: Position| {
            order
                .get(p as usize)
                .is_some_and(|&k| self.pin_of(k).is_some())
        };
        if pinned(self.anchor.position) {
            let next = self
                .next_body(Some(self.anchor.position), true)
                .or_else(|| self.next_body(Some(self.anchor.position), false));
            self.anchor = ScrollAnchor {
                position: next.unwrap_or(0),
                offset: 0,
            };
        }
        self.anchor.position = self.anchor.position.min(self.count.saturating_sub(1));
    }

    /// Scrolls the body by `delta` cells. Only the anchor moves; it steps over
    /// pinned Positions. Clamped so the last scrolling item ends at the body's
    /// end.
    pub fn scroll(&mut self, delta: i32, order: &[Key]) {
        let size_at = |p: Position| self.size(order[p as usize]);
        let mut a = self.anchor;
        a.offset += delta;
        loop {
            if a.offset < 0 {
                match self.next_body(Some(a.position), false) {
                    Some(p) => {
                        a.offset += size_at(p);
                        a.position = p;
                    }
                    None => {
                        a.offset = 0;
                        break;
                    }
                }
            } else if a.offset >= size_at(a.position) {
                match self.next_body(Some(a.position), true) {
                    Some(p) => {
                        a.offset -= size_at(a.position);
                        a.position = p;
                    }
                    None => {
                        a.offset = size_at(a.position) - 1;
                        break;
                    }
                }
            } else {
                break;
            }
        }
        self.anchor = a;
        self.clamp_end(order);
    }

    /// SHORTCUT: walks back from the end with the source's order.
    pub fn clamp_end(&mut self, order: &[Key]) {
        let body = self.body_len();
        let mut acc = 0;
        let mut p = None;
        let mut max = ScrollAnchor::default();
        while let Some(q) = self.next_body(p, false) {
            acc += self.size(order[q as usize]);
            p = Some(q);
            if acc >= body {
                max = ScrollAnchor {
                    position: q,
                    offset: acc - body,
                };
                break;
            }
        }
        if (self.anchor.position, self.anchor.offset) > (max.position, max.offset) {
            self.anchor = max;
        }
    }

    /// Brings Position `p` of a scrolling item fully into the body.
    pub fn reveal_body(&mut self, p: Position, order: &[Key]) {
        if (p, 0) < (self.anchor.position, self.anchor.offset) {
            self.anchor = ScrollAnchor {
                position: p,
                offset: 0,
            };
            return;
        }
        let body = self.body_len();
        // Walk back from p's end; if the anchor is reached first it fits.
        let mut acc = 0;
        let mut q = Some(p);
        while let Some(at) = q {
            acc += self.size(order[at as usize]);
            if at == self.anchor.position {
                if acc - self.anchor.offset <= body {
                    return;
                }
                break;
            }
            if acc >= body {
                break;
            }
            q = self.next_body(Some(at), false);
        }
        // Put p's end at the body's end.
        let mut acc = 0;
        let mut q = Some(p);
        while let Some(at) = q {
            acc += self.size(order[at as usize]);
            if acc >= body {
                self.anchor = ScrollAnchor {
                    position: at,
                    offset: acc - body,
                };
                return;
            }
            q = self.next_body(Some(at), false);
        }
        self.anchor = ScrollAnchor::default();
    }

    /// Slides a pinned Strip so its item `i` is fully inside the region.
    pub fn reveal_pin(&mut self, side: Side, i: usize, clip_first: ClipFirst) {
        self.slide = [0, 0];
        let (l, t, wants) = self.allot(clip_first);
        let pins = self.drawn_pins(side);
        let before: i32 = pins[..i].iter().map(|p| self.size(p.key)).sum();
        let size = self.size(pins[i].key);
        match side {
            // Leading clips at its end: slide back until item i ends inside.
            Side::Leading => self.slide[0] = (before + size - l).max(0).min(before),
            // Trailing clips at its start: slide on until item i starts inside.
            Side::Trailing => self.slide[1] = (wants[1] - t - before).max(0),
        }
    }

    /// The order of slots as drawn: leading pins, scrolling Positions,
    /// trailing pins. SHORTCUT: lists every Position.
    pub fn drawn_order(&self) -> Vec<Slot> {
        let mut out: Vec<Slot> = (0..self.drawn_pins(Side::Leading).len())
            .map(|i| Slot::Pin(Side::Leading, i))
            .collect();
        let pinned = self.pinned_positions();
        out.extend(
            (0..self.count)
                .filter(|&p| !self.pinned_at(p, &pinned))
                .map(Slot::Body),
        );
        out.extend(
            (0..self.drawn_pins(Side::Trailing).len()).map(|i| Slot::Pin(Side::Trailing, i)),
        );
        out
    }

    /// A slot's index in drawn order, worked out from counts rather than a
    /// list, as the core would on a huge axis.
    pub fn drawn_index(&self, slot: Slot) -> u64 {
        let nl = self.drawn_pins(Side::Leading).len() as u64;
        let pinned = self.pinned_positions();
        match slot {
            Slot::Pin(Side::Leading, i) => i as u64,
            Slot::Body(p) => nl + p - pinned.range(..p).count() as u64,
            Slot::Pin(Side::Trailing, i) => nl + self.count - pinned.len() as u64 + i as u64,
        }
    }

    pub fn slot_key(&self, slot: Slot, order: &[Key]) -> Option<Key> {
        match slot {
            Slot::Pin(side, i) => self.drawn_pins(side).get(i).map(|p| p.key),
            Slot::Body(p) => order.get(p as usize).copied(),
        }
    }

    pub fn slot_of(&self, key: Key, order: &[Key]) -> Option<Slot> {
        if let Some((side, _)) = self.pin_of(key) {
            let i = self.drawn_pins(side).iter().position(|p| p.key == key)?;
            return Some(Slot::Pin(side, i));
        }
        self.position_of(key, order).map(Slot::Body)
    }

    /// Where the Grid believes `key` sits: a pin's known Position, else the
    /// Window, else (SHORTCUT) the source's order.
    pub fn position_of(&self, key: Key, order: &[Key]) -> Option<Position> {
        if let Some((side, i)) = self.pin_of(key) {
            return self.pins[side.index()][i].position();
        }
        if let Some(i) = self.window.iter().position(|&k| k == key) {
            return Some(self.window_start + i as u64);
        }
        order.iter().position(|&k| k == key).map(|p| p as u64)
    }

    /// One arrow-key step from `key`.
    pub fn step(&self, key: Key, forward: bool, order_kind: FocusOrder, order: &[Key]) -> Key {
        match order_kind {
            FocusOrder::Source => {
                let Some(p) = self.position_of(key, order) else {
                    return key;
                };
                let q = if forward { p + 1 } else { p.wrapping_sub(1) };
                order.get(q as usize).copied().unwrap_or(key)
            }
            FocusOrder::Drawn => {
                let Some(slot) = self.slot_of(key, order) else {
                    return key;
                };
                let nl = self.drawn_pins(Side::Leading).len();
                let nt = self.drawn_pins(Side::Trailing).len();
                let first_body = self.next_body(None, true).map(Slot::Body);
                let last_body = self.next_body(None, false).map(Slot::Body);
                let next = match (slot, forward) {
                    (Slot::Pin(Side::Leading, i), true) if i + 1 < nl => {
                        Some(Slot::Pin(Side::Leading, i + 1))
                    }
                    (Slot::Pin(Side::Leading, _), true) => {
                        first_body.or((nt > 0).then_some(Slot::Pin(Side::Trailing, 0)))
                    }
                    (Slot::Pin(Side::Leading, i), false) => {
                        i.checked_sub(1).map(|i| Slot::Pin(Side::Leading, i))
                    }
                    (Slot::Body(p), true) => self
                        .next_body(Some(p), true)
                        .map(Slot::Body)
                        .or((nt > 0).then_some(Slot::Pin(Side::Trailing, 0))),
                    (Slot::Body(p), false) => self
                        .next_body(Some(p), false)
                        .map(Slot::Body)
                        .or((nl > 0).then(|| Slot::Pin(Side::Leading, nl - 1))),
                    (Slot::Pin(Side::Trailing, i), true) => {
                        (i + 1 < nt).then_some(Slot::Pin(Side::Trailing, i + 1))
                    }
                    (Slot::Pin(Side::Trailing, 0), false) => {
                        last_body.or((nl > 0).then(|| Slot::Pin(Side::Leading, nl - 1)))
                    }
                    (Slot::Pin(Side::Trailing, i), false) => Some(Slot::Pin(Side::Trailing, i - 1)),
                };
                next.and_then(|s| self.slot_key(s, order)).unwrap_or(key)
            }
        }
    }
}

/// The one contiguous range: Key corners, as in the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range2 {
    pub anchor: [Key; 2],
    pub head: [Key; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Locate { axis: Axis, key: Key },
}

pub struct Grid {
    pub axes: [AxisGrid; 2],
    pub focus: Option<[Key; 2]>,
    pub range: Option<Range2>,
    pub options: Options,
    /// `Locate`s sent and not yet answered, oldest first.
    pub outbox: Vec<Effect>,
}

impl Grid {
    pub fn axis(&self, axis: Axis) -> &AxisGrid {
        &self.axes[axis.index()]
    }

    pub fn axis_mut(&mut self, axis: Axis) -> &mut AxisGrid {
        &mut self.axes[axis.index()]
    }

    /// The frame: settle anchors, size the Window, have the host fill it,
    /// lay out the Strips.
    pub fn frame(&mut self, src: &Source) {
        let clip = self.options.clip_first;
        for ax in &mut self.axes {
            let order = &src.axes[ax.axis.index()].order;
            ax.count = order.len() as u64;
            // A pinned Strip slides only while focus is on one of its
            // clipped items, recomputed every frame as sizes change.
            ax.slide = [0, 0];
            let focused = self.focus.map(|f| f[ax.axis.index()]);
            if self.options.clipped_reveal == ClippedReveal::Slide {
                if let Some(Slot::Pin(side, n)) = focused.and_then(|k| ax.slot_of(k, order)) {
                    ax.reveal_pin(side, n, clip);
                }
            }
            ax.settle_anchor(order);
            ax.layout(clip);
            ax.clamp_end(order);
            ax.compute_needed();
            ax.fill_window(order);
            ax.layout(clip);
        }
    }

    pub fn pin(&mut self, axis: Axis, key: Key, side: Side, src: &Source) {
        let order = &src.axes[axis.index()].order;
        let ax = self.axis_mut(axis);
        let Some(p) = ax.position_of(key, order) else {
            return;
        };
        if let Some((s, i)) = ax.pin_of(key) {
            ax.pins[s.index()].remove(i);
        }
        ax.pins[side.index()].push(Pin {
            key,
            state: PinState::Found(p),
        });
    }

    pub fn unpin(&mut self, axis: Axis, key: Key) {
        let ax = self.axis_mut(axis);
        if let Some((s, i)) = ax.pin_of(key) {
            ax.pins[s.index()].remove(i);
            ax.slide = [0, 0];
        }
    }

    /// Moves a pin to index `to` within its own pinned Strip.
    pub fn move_pin(&mut self, axis: Axis, key: Key, to: usize) {
        let ax = self.axis_mut(axis);
        if let Some((s, i)) = ax.pin_of(key) {
            let list = &mut ax.pins[s.index()];
            let pin = list.remove(i);
            list.insert(to.min(list.len()), pin);
        }
    }

    /// "Freeze up to here": on a scrolling item, pins every scrolling Key from
    /// the start up to it, in their current order, after any leading pins. On
    /// a leading pin, unpins the leading pins after it.
    pub fn freeze_up_to(&mut self, axis: Axis, key: Key, src: &Source) {
        let order = &src.axes[axis.index()].order;
        let ax = self.axis_mut(axis);
        match ax.pin_of(key) {
            Some((Side::Leading, i)) => ax.pins[0].truncate(i + 1),
            Some((Side::Trailing, _)) => {}
            None => {
                let Some(p) = ax.position_of(key, order) else {
                    return;
                };
                // SHORTCUT: the Keys before p come from a Window request.
                for q in 0..=p {
                    let k = order[q as usize];
                    if ax.pin_of(k).is_none() {
                        ax.pins[0].push(Pin {
                            key: k,
                            state: PinState::Found(q),
                        });
                    }
                }
            }
        }
    }

    pub fn scroll(&mut self, axis: Axis, delta: i32, src: &Source) {
        let order = &src.axes[axis.index()].order;
        self.axis_mut(axis).scroll(delta, order);
    }

    pub fn resize(&mut self, axis: Axis, key: Key, size: i32) {
        self.axis_mut(axis).overrides.insert(key, size.max(1));
    }

    pub fn set_focus(&mut self, row: Key, col: Key, src: &Source) {
        self.focus = Some([row, col]);
        self.reveal_focus(src);
    }

    /// Moves focus one step along `axis`; `extend` grows the range.
    pub fn move_focus(&mut self, axis: Axis, forward: bool, extend: bool, src: &Source) {
        let Some(mut f) = self.focus else {
            return;
        };
        let i = axis.index();
        let order = &src.axes[i].order;
        let before = f;
        f[i] = self.axes[i].step(f[i], forward, self.options.focus_order, order);
        self.focus = Some(f);
        self.range = if extend {
            Some(Range2 {
                anchor: self.range.map_or(before, |r| r.anchor),
                head: f,
            })
        } else {
            None
        };
        self.reveal_focus(src);
    }

    pub fn reveal_focus(&mut self, src: &Source) {
        let Some(f) = self.focus else {
            return;
        };
        let clip = self.options.clip_first;
        for i in 0..2 {
            let order = &src.axes[i].order;
            let ax = &mut self.axes[i];
            match ax.slot_of(f[i], order) {
                Some(Slot::Body(p)) => {
                    ax.layout(clip);
                    ax.reveal_body(p, order);
                }
                // A clipped pin is revealed by `frame`, if at all.
                _ => {}
            }
        }
    }

    /// Whether `key` on `axis` is inside the range along that axis.
    pub fn in_range(&self, axis: Axis, key: Key, src: &Source) -> bool {
        let Some(r) = self.range else {
            return false;
        };
        let i = axis.index();
        let ax = &self.axes[i];
        let order = &src.axes[i].order;
        match self.options.range_order {
            RangeOrder::Source => {
                let (Some(a), Some(b), Some(k)) = (
                    ax.position_of(r.anchor[i], order),
                    ax.position_of(r.head[i], order),
                    ax.position_of(key, order),
                ) else {
                    return false;
                };
                a.min(b) <= k && k <= a.max(b)
            }
            RangeOrder::Drawn => {
                let idx = |k| ax.slot_of(k, order).map(|s| ax.drawn_index(s));
                let (Some(a), Some(b), Some(k)) = (idx(r.anchor[i]), idx(r.head[i]), idx(key))
                else {
                    return false;
                };
                a.min(b) <= k && k <= a.max(b)
            }
        }
    }

    /// The source applied a new View Spec on `axis` and answered `Refiltered`.
    pub fn refiltered(&mut self, axis: Axis, src: &Source) {
        let i = axis.index();
        let order = &src.axes[i].order;
        let ax = &mut self.axes[i];
        ax.count = order.len() as u64;
        // Every pin, dormant ones too, is asked about again.
        let mut asks = Vec::new();
        for pin in ax.pins.iter_mut().flatten() {
            let last = pin.position();
            pin.state = PinState::Pending(last);
            asks.push(Effect::Locate { axis, key: pin.key });
        }
        // Drop older asks about the same Keys: only the last answer counts.
        self.outbox.retain(|e| !asks.contains(e));
        self.outbox.extend(asks);
        // SHORTCUT: focus, range corners and the anchor's item relocate at once.
        if let Some(mut f) = self.focus {
            if !order.contains(&f[i]) {
                match src.axes[i].suggest(f[i]) {
                    Some(k) => f[i] = k,
                    None => self.focus = None,
                }
                if let Some(ff) = &mut self.focus {
                    ff[i] = f[i];
                }
            }
        }
        if let Some(mut r) = self.range {
            for c in [&mut r.anchor, &mut r.head] {
                if !order.contains(&c[i]) {
                    match src.axes[i].suggest(c[i]) {
                        Some(k) => c[i] = k,
                        None => {
                            self.range = None;
                            break;
                        }
                    }
                }
            }
            if self.range.is_some() {
                self.range = Some(r);
            }
        }
        let anchor_key = src.axes[i]
            .prev_order
            .get(self.axes[i].anchor.position as usize)
            .copied();
        let ax = &mut self.axes[i];
        ax.anchor = match anchor_key.and_then(|k| order.iter().position(|&o| o == k)) {
            Some(p) => ScrollAnchor {
                position: p as u64,
                offset: ax.anchor.offset,
            },
            None => ScrollAnchor::default(),
        };
        ax.slide = [0, 0];
    }

    /// The source answers the oldest outstanding `Locate`.
    pub fn answer_one(&mut self, src: &Source) -> Option<String> {
        if self.outbox.is_empty() {
            return None;
        }
        let Effect::Locate { axis, key } = self.outbox.remove(0);
        let found = src.axes[axis.index()].locate(key);
        let ax = self.axis_mut(axis);
        let Some((s, n)) = ax.pin_of(key) else {
            return Some(format!(
                "Locate {} ignored: no longer pinned",
                src.name(key)
            ));
        };
        ax.pins[s.index()][n].state = match found {
            Some(p) => PinState::Found(p),
            None => PinState::Dormant,
        };
        Some(match found {
            Some(p) => format!("Locate {} -> Found({p})", src.name(key)),
            None => format!("Locate {} -> Gone: pin dormant", src.name(key)),
        })
    }
}

/// A stand-in Table Source: base Keys per axis, a sort, a filter and hidden
/// Keys (the View Spec), applied into an order.
pub struct SourceAxis {
    pub base: Vec<Key>,
    pub order: Vec<Key>,
    pub prev_order: Vec<Key>,
    pub sort: SortKind,
    pub filter: bool,
    pub hidden: BTreeSet<Key>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKind {
    Natural,
    Reverse,
    ByName,
}

impl SourceAxis {
    pub fn locate(&self, key: Key) -> Option<Position> {
        self.order.iter().position(|&k| k == key).map(|p| p as u64)
    }

    /// `Gone { suggest }`: the next item in the old order still here, else
    /// the previous.
    pub fn suggest(&self, key: Key) -> Option<Key> {
        let at = self.prev_order.iter().position(|&k| k == key)?;
        let here = |k: &&Key| self.order.contains(k);
        self.prev_order[at + 1..]
            .iter()
            .find(here)
            .or_else(|| self.prev_order[..at].iter().rev().find(here))
            .copied()
    }
}

pub struct Source {
    pub axes: [SourceAxis; 2],
    pub names: HashMap<Key, String>,
}

impl Source {
    pub fn name(&self, key: Key) -> &str {
        self.names.get(&key).map_or("?", String::as_str)
    }

    pub fn key(&self, name: &str) -> Option<(Axis, Key)> {
        let (&k, _) = self.names.iter().find(|(_, n)| n.as_str() == name)?;
        let axis = if self.axes[0].base.contains(&k) {
            Axis::Rows
        } else {
            Axis::Cols
        };
        Some((axis, k))
    }

    /// The filter keeps odd rows, or columns without an "o" in their name.
    pub fn apply(&mut self, axis: Axis) {
        let names = &self.names;
        let a = &mut self.axes[axis.index()];
        let mut order: Vec<Key> = a
            .base
            .iter()
            .copied()
            .filter(|k| !a.hidden.contains(k))
            .filter(|k| {
                !a.filter
                    || match axis {
                        Axis::Rows => k.0 % 2 == 1,
                        Axis::Cols => !names[k].contains('o'),
                    }
            })
            .collect();
        match a.sort {
            SortKind::Natural => {}
            SortKind::Reverse => order.reverse(),
            SortKind::ByName => order.sort_by_key(|k| names[k].clone()),
        }
        a.prev_order = std::mem::replace(&mut a.order, order);
    }
}
