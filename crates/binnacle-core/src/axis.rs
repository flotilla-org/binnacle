use std::ops::Range;

use crate::measure::{AxisMeasure, ItemAt};
use crate::snapshot::{ScrollAnchor, StripItem, Thumb};
use crate::types::Position;

/// Positions fetched either side of the visible ones, so small scrolls need no
/// new Window.
const OVERSCAN: u64 = 2;

/// One axis's layout, Scroll Anchor and Window. Holds nothing proportional to
/// the count.
pub(crate) struct AxisState {
    pub measure: AxisMeasure,
    pub header: f32,
    /// The viewport's length along this axis, header band included.
    pub extent: f32,
    pub anchor: ScrollAnchor,
    /// The last scroll reached the end clamp. Re-clamping then follows the end
    /// rather than keep an anchor computed from older sizes: the end is found
    /// in f64 along whatever path the Window allows, and paths round apart.
    at_end: bool,
    pub strip: Vec<StripItem>,
}

impl AxisState {
    pub fn new(measure: AxisMeasure, header: f32) -> Self {
        AxisState {
            measure,
            header,
            extent: 0.0,
            anchor: ScrollAnchor::default(),
            at_end: false,
            strip: Vec::new(),
        }
    }

    /// The length past the header band where items show.
    fn body(&self) -> f32 {
        (self.extent - self.header).max(0.0)
    }

    /// The Window this axis needs for the current viewport: the visible items
    /// plus overscan either side. Counted by stepping with the smallest size an
    /// item can have, so it holds every item a real layout can reach.
    ///
    /// When the last item can show, the end clamp may pull the anchor back once
    /// the Window's sizes are known, so the Window also reaches back a body's
    /// length of smallest items from the end.
    pub fn needed(&self) -> Range<Position> {
        let count = self.measure.count();
        let visible_end = self.visible_end();
        let mut first = self.anchor.position;
        if visible_end == count {
            let stride = f64::from(self.measure.smallest_size()) + f64::from(self.measure.gap());
            let back = (f64::from(self.body()) / stride).ceil() as u64;
            first = first.min(count.saturating_sub(back.saturating_add(1)));
        }
        let start = first.saturating_sub(OVERSCAN);
        let end = visible_end.saturating_add(OVERSCAN).min(count);
        start.min(end)..end
    }

    /// One past the last Position a layout can reach, stepping with the smallest
    /// size. The anchor's own item may be larger than that, so its offset is
    /// capped at the smallest stride: the next item then starts no later than
    /// it really does. The anchor's item always counts, as it may be the one
    /// larger item.
    fn visible_end(&self) -> Position {
        let smallest = self.measure.smallest_size();
        let offset = self.anchor.offset.min(smallest + self.measure.gap());
        let anchor_end = (self.anchor.position + 1).min(self.measure.count());
        self.placements(offset, |_| smallest)
            .last()
            .map_or(anchor_end, |(p, _, _)| p + 1)
    }

    /// The Position, offset and size of each visible item from the Scroll
    /// Anchor's Position, `anchor_offset` into it, in order. Stops early if an
    /// item is too small to advance an f32 offset, rather than spin.
    fn placements(
        &self,
        anchor_offset: f32,
        size_at: impl Fn(Position) -> f32,
    ) -> impl Iterator<Item = (Position, f32, f32)> {
        let (count, header, extent) = (self.measure.count(), self.header, self.extent);
        let gap = self.measure.gap();
        let (first, off) = (self.anchor.position, f64::from(anchor_offset));
        let first_size = f64::from(size_at(first));
        // An anchor in the Gap after its item starts at the next item, placed
        // straight from the anchor so a tiny item cannot stall the stepping.
        let start = if off >= first_size {
            (
                first + 1,
                f64::from(header) + first_size + f64::from(gap) - off,
            )
        } else {
            (first, f64::from(header) - off)
        };
        let mut next = Some((start.0, start.1 as f32));
        std::iter::from_fn(move || {
            loop {
                let (position, offset) = next?;
                let size = size_at(position);
                let end = offset + size;
                if position >= count || offset.max(header) >= extent || end <= offset {
                    next = None;
                    return None;
                }
                next = Some((position + 1, end + gap));
                if end > header {
                    return Some((position, offset, size));
                }
            }
        })
    }

    pub fn window_covers(&self, range: &Range<Position>) -> bool {
        let start = self.measure.window_start();
        let end = start + self.measure.window_len() as u64;
        range.is_empty() || (start <= range.start && range.end <= end)
    }

    /// Lays out the visible items into `strip`, reusing its buffer. Never past
    /// what the Window bound reaches, so a Strip item lacks a Key only if the
    /// Table Source left it unfilled.
    pub fn layout(&mut self) {
        let mut strip = std::mem::take(&mut self.strip);
        strip.clear();
        let bound = self.visible_end();
        let m = &self.measure;
        strip.extend(
            self.placements(self.anchor.offset, |p| m.size_at(p))
                .take_while(|&(p, _, _)| p < bound)
                .map(|(position, offset, size)| StripItem {
                    position,
                    key: m.key_at(position),
                    offset,
                    size,
                }),
        );
        self.strip = strip;
    }

    /// The anchor for a located offset, rounded toward the axis start so a
    /// clamped view never shows space past the last item.
    fn anchor_at(&self, l: ItemAt) -> ScrollAnchor {
        let stride = self.measure.size_at(l.position) + self.measure.gap();
        let mut offset = l.within as f32;
        if f64::from(offset) > l.within {
            offset = f32::from_bits(offset.to_bits() - 1);
        }
        if offset >= stride {
            offset = f32::from_bits(stride.to_bits() - 1);
        }
        ScrollAnchor {
            position: l.position,
            offset,
        }
    }

    /// The furthest the anchor may go: where the last item ends the viewport.
    fn max_anchor(&self) -> ScrollAnchor {
        let count = self.measure.count();
        let body = self.body();
        if count == 0 {
            return ScrollAnchor::default();
        }
        let last = count - 1;
        if body <= 0.0 {
            return ScrollAnchor {
                position: last,
                offset: 0.0,
            };
        }
        let size = self.measure.size_at(last);
        let back = f64::from(size) - f64::from(body);
        match self.measure.locate(last, back) {
            Some(l) => self.anchor_at(l),
            // The body is shorter than the whole axis.
            None if back < 0.0 => ScrollAnchor::default(),
            // A body too thin to register against the last item's size: go as
            // deep into that item as an anchor can.
            None => ScrollAnchor {
                position: last,
                offset: f32::from_bits(size.to_bits() - 1),
            },
        }
    }

    /// Moves the anchor `offset` past the start of item `position`, kept
    /// between the start and [`Self::max_anchor`] with its offset inside its
    /// item. Returns whether it moved.
    fn move_to(&mut self, position: Position, offset: f64) -> bool {
        let max = self.max_anchor();
        let to = match self.measure.locate(position, offset) {
            Some(l) => self.anchor_at(l),
            None if offset < 0.0 => ScrollAnchor::default(),
            None => max,
        };
        let to = if (to.position, to.offset) > (max.position, max.offset) {
            max
        } else {
            to
        };
        self.settle(to, max)
    }

    /// Sets the anchor, noting whether it is at a reachable end. Returns
    /// whether it moved.
    fn settle(&mut self, to: ScrollAnchor, max: ScrollAnchor) -> bool {
        self.at_end = to == max && max != ScrollAnchor::default();
        let moved = to != self.anchor;
        self.anchor = to;
        moved
    }

    /// Re-applies the anchor's bounds after the viewport or sizes changed. The
    /// anchor keeps its Position: an offset its item no longer reaches is
    /// pulled back inside the item, and only the end clamp moves it further. An
    /// anchor at the end stays at the end. While the Window does not hold the
    /// anchor's item (a notice emptied it) its size is only the default, so
    /// the offset is kept until the Window brings the real one; likewise the
    /// end clamp pulls the anchor back only once the Window holds the items
    /// it is measured over. An empty axis has no item to wait for: its anchor
    /// goes back to the start.
    pub fn clamp_anchor(&mut self) -> bool {
        let count = self.measure.count();
        let position = self.anchor.position.min(count.saturating_sub(1));
        let max = self.max_anchor();
        let to = if self.at_end || count == 0 {
            max
        } else if !self.window_covers(&(position..position + 1)) {
            ScrollAnchor {
                position,
                ..self.anchor
            }
        } else {
            let stride = self.measure.size_at(position) + self.measure.gap();
            let offset = self.anchor.offset.min(f32::from_bits(stride.to_bits() - 1));
            // The end clamp is exact only once the Window holds every item
            // from it to the end; until then the anchor waits for the Window
            // the next frame asks for, which reaches the end.
            let exact = self.window_covers(&(max.position..count));
            if (position, offset) > (max.position, max.offset) && exact {
                max
            } else {
                ScrollAnchor { position, offset }
            }
        };
        let following = self.at_end;
        let moved = self.settle(to, max);
        // An anchor left where it was follows the end only if it already
        // did: the end may be an estimate the next Window's sizes move.
        if !moved && !following {
            self.at_end = false;
        }
        moved
    }

    /// Sets the Gap, keeping the anchor's Position and offset as far as the
    /// new Gap and end clamp allow. An anchor that was following the end
    /// stops following it, so its item stays put. Returns whether anything
    /// changed.
    pub fn set_gap(&mut self, gap: f32) -> bool {
        if !self.measure.set_gap(gap) {
            return false;
        }
        self.at_end = false;
        self.clamp_anchor();
        true
    }

    /// The item a body's length before or after the start of item `p`, at
    /// least one item away and inside the axis.
    pub fn page_from(&self, p: Position, forward: bool) -> Position {
        let last = self.measure.count().saturating_sub(1);
        let body = f64::from(self.body());
        let delta = if forward { body } else { -body };
        let landed = self.measure.locate(p, delta).map(|at| at.position);
        if forward {
            landed.unwrap_or(last).max(p.saturating_add(1)).min(last)
        } else {
            landed.unwrap_or(0).min(p.saturating_sub(1))
        }
    }

    /// Scrolls the least distance that shows all of item `p`, or its start if
    /// it is longer than the body. With no body (no viewport yet) nothing can
    /// show, so the anchor stays. Returns whether the anchor moved.
    pub fn reveal(&mut self, p: Position) -> bool {
        let body = f64::from(self.body());
        if body <= 0.0 {
            return false;
        }
        let start = self.measure.distance(self.anchor.position, p) - f64::from(self.anchor.offset);
        let size = f64::from(self.measure.size_at(p));
        if start < 0.0 || size >= body {
            self.move_to(p, 0.0)
        } else if start + size > body {
            self.move_to(p, size - body)
        } else {
            false
        }
    }

    /// Scrolls by `delta` in the host's unit. Non-finite deltas are ignored.
    pub fn scroll(&mut self, delta: f32) -> bool {
        if !delta.is_finite() || delta == 0.0 {
            return false;
        }
        let offset = f64::from(self.anchor.offset) + f64::from(delta);
        self.move_to(self.anchor.position, offset)
    }

    /// Scrolls by whole items from the anchor's item, landing on an item start.
    pub fn scroll_items(&mut self, items: i64) -> bool {
        let count = self.measure.count();
        if count == 0 {
            return false;
        }
        let target =
            (i128::from(self.anchor.position) + i128::from(items)).clamp(0, i128::from(count - 1));
        self.move_to(target as Position, 0.0)
    }

    /// The anchor's distance from the axis start and the most it can be.
    fn scrolled(&self) -> (f64, f64) {
        let at = |a: ScrollAnchor| self.measure.distance(0, a.position) + f64::from(a.offset);
        (at(self.anchor), at(self.max_anchor()))
    }

    /// Scrolls so the thumb sits at `fraction` (clamped to 0..=1). NaN is
    /// ignored.
    pub fn scroll_to(&mut self, fraction: f64) -> bool {
        if fraction.is_nan() {
            return false;
        }
        if fraction >= 1.0 {
            let max = self.max_anchor();
            return self.move_to(max.position, f64::from(max.offset));
        }
        let (_, scrollable) = self.scrolled();
        self.move_to(0, fraction.max(0.0) * scrollable)
    }

    /// The scrollbar thumb, estimated from the measure: sizes outside the
    /// Window are assumed default.
    pub fn thumb(&self) -> Thumb {
        let (scrolled, scrollable) = self.scrolled();
        let extent = self.measure.extent();
        Thumb {
            fraction: if scrollable > 0.0 {
                (scrolled / scrollable).clamp(0.0, 1.0)
            } else {
                0.0
            },
            size: if extent > 0.0 {
                (f64::from(self.body()) / extent).min(1.0)
            } else {
                1.0
            },
        }
    }
}
