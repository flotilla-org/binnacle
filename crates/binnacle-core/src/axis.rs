use std::collections::HashMap;
use std::ops::Range;

use crate::grouping::Outline;
use crate::measure::{AxisMeasure, ItemAt};
use crate::pin::Pin;
use crate::snapshot::{ScrollAnchor, StripItem, Thumb};
use crate::types::{Edge, Key, Position};

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
    /// The scrolling Strip.
    pub strip: Vec<StripItem>,
    /// The pinned Keys per edge (leading, trailing), in pin order.
    pub pins: [Vec<Pin>; 2],
    /// The pinned Strips per edge, as last laid out.
    pub pinned: [Vec<StripItem>; 2],
    /// A "freeze up to here" waiting for the Window: the last Position it
    /// pins.
    pub freezing: Option<Position>,
    /// The least space between a pinned Strip and the scrolling Strip (see
    /// [`AxisState::edge_gap`]).
    pub edge_gap: f32,
    /// The room kept past the last item at the end of the axis (see
    /// [`crate::Grid::set_end_gap`]).
    pub end_gap: f32,
    /// How far each end's pin zone reaches (see [`crate::Grid::set_pin_zone`]).
    pub pin_zone: f32,
    /// The least length the pinned Strips leave the scrolling region.
    pub min_scrolling: f32,
    /// The focused Key on this axis, as of the last Snapshot: a clipped
    /// pinned item with it is shown.
    pub focused: Option<Key>,
    /// How far each pinned Strip has slid (towards its outside edge) to
    /// show its focused item, as last laid out.
    pub slide: [f32; 2],
    /// Where the pinned Strips draw, as last laid out.
    pub regions: [Range<f32>; 2],
    /// The outlines the Table Source sent with the Window, by Key.
    pub outlines: HashMap<Key, Outline>,
    /// The sizes of the tiers of the Header Band heading this axis's items
    /// in header form, as the host set them (see [`crate::Grid::set_tiers`]).
    pub tier_sizes: Vec<f32>,
}

impl AxisState {
    pub fn new(measure: AxisMeasure, header: f32) -> Self {
        let measure_default = measure.default_size();
        AxisState {
            measure,
            header,
            extent: 0.0,
            anchor: ScrollAnchor::default(),
            at_end: false,
            strip: Vec::new(),
            pins: [Vec::new(), Vec::new()],
            pinned: [Vec::new(), Vec::new()],
            freezing: None,
            edge_gap: 0.0,
            end_gap: 0.0,
            pin_zone: measure_default / 4.0,
            min_scrolling: 0.0,
            focused: None,
            slide: [0.0; 2],
            regions: [0.0..0.0, 0.0..0.0],
            outlines: HashMap::new(),
            tier_sizes: Vec::new(),
        }
    }

    /// The space between a pinned Strip and the scrolling Strip: the Edge
    /// Gap, or the Gap if that is larger.
    pub fn edge_gap(&self) -> f32 {
        self.measure.gap().max(self.edge_gap)
    }

    /// Sets the Edge Gap, keeping the Scroll Anchor as [`AxisState::set_gap`]
    /// does. Returns whether anything changed.
    pub fn set_edge_gap(&mut self, gap: f32) -> bool {
        if !(gap.is_finite() && gap >= 0.0) || gap == self.edge_gap {
            return false;
        }
        self.edge_gap = gap;
        self.at_end = false;
        self.clamp_anchor();
        true
    }

    /// Sets the End Gap, keeping the Scroll Anchor as [`AxisState::set_gap`]
    /// does, except that an anchor at the end follows the end. Returns
    /// whether anything changed.
    pub fn set_end_gap(&mut self, gap: f32) -> bool {
        if !(gap.is_finite() && gap >= 0.0) || gap == self.end_gap {
            return false;
        }
        self.end_gap = gap;
        self.clamp_anchor();
        true
    }

    /// Where the axis's content may end: the End Gap before the viewport's
    /// far end, though never before the Header Band.
    pub fn far_end(&self) -> f32 {
        (self.extent - self.end_gap).max(self.header.min(self.extent))
    }

    /// Sets the least length the pinned Strips leave to scroll, keeping
    /// the Scroll Anchor as [`AxisState::set_gap`] does. Returns whether
    /// anything changed.
    pub fn set_min_scrolling(&mut self, length: f32) -> bool {
        if !(length.is_finite() && length >= 0.0) || length == self.min_scrolling {
            return false;
        }
        self.min_scrolling = length;
        self.at_end = false;
        self.clamp_anchor();
        true
    }

    /// The length the scrolling Strip shows: between the pinned Strips,
    /// inside the header band.
    /// Measured in f64, where the difference of the edges is exact.
    fn body(&self) -> f64 {
        (f64::from(self.scroll_end()) - f64::from(self.scroll_start())).max(0.0)
    }

    /// The length the scrolling Strip's last item may end in: its body,
    /// less the End Gap unless a trailing pinned Strip follows it. Measured
    /// in f64, as [`AxisState::far_end`] rounded to f32 could make a body of
    /// one f32 step vanish, or one of nothing appear.
    fn end_body(&self) -> f64 {
        if self.drawn_pins(Edge::Trailing).next().is_some() {
            return self.body();
        }
        let header = f64::from(self.header.min(self.extent));
        let far_end = (f64::from(self.extent) - f64::from(self.end_gap)).max(header);
        (far_end - f64::from(self.scroll_start())).max(0.0)
    }

    /// The anchor at the start of the scrolling Strip: its first item, past
    /// any pinned ones.
    fn start(&self) -> ScrollAnchor {
        let position = (0..self.measure.count())
            .find(|&p| !self.measure.skipped(p))
            .unwrap_or(0);
        ScrollAnchor {
            position,
            offset: 0.0,
        }
    }

    /// The last Position the scrolling Strip can show, if any.
    pub fn last_shown(&self) -> Option<Position> {
        let mut p = self.measure.count().checked_sub(1)?;
        while self.measure.skipped(p) {
            p = p.checked_sub(1)?;
        }
        Some(p)
    }

    /// The Window this axis needs for the current viewport: the visible items
    /// plus overscan either side. Counted by stepping with the smallest size an
    /// item can have, so it holds every item a real layout can reach.
    ///
    /// When the last item can show, the end clamp may pull the anchor back once
    /// the Window's sizes are known, so the Window also reaches back a body's
    /// length of smallest items from the end. Both ends reach one Position
    /// further per pin, since the scrolling Strip skips pinned Keys it holds,
    /// wherever a pin still waiting for `Locate` turns out to be.
    pub fn needed(&self) -> Range<Position> {
        let count = self.measure.count();
        let visible_end = self.visible_end();
        let pins = self.pins.iter().flatten().count() as u64;
        let mut first = self.anchor.position;
        if visible_end == count {
            let stride = f64::from(self.measure.smallest_size()) + f64::from(self.measure.gap());
            let back = (self.body() / stride).ceil() as u64;
            first = first.min(count.saturating_sub(back.saturating_add(1 + pins)));
        }
        let start = first.saturating_sub(OVERSCAN);
        let end = (visible_end.saturating_add(OVERSCAN + pins)).min(count);
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

    /// The Position, offset and size of each visible item of the scrolling
    /// Strip from the Scroll Anchor's Position, `anchor_offset` into it, in
    /// order, skipping pinned items. Stops early if an item is too small to
    /// advance an f32 offset, rather than spin.
    fn placements(
        &self,
        anchor_offset: f32,
        size_at: impl Fn(Position) -> f32,
    ) -> impl Iterator<Item = (Position, f32, f32)> {
        let (count, header, extent) =
            (self.measure.count(), self.scroll_start(), self.scroll_end());
        let (gap, m) = (self.measure.gap(), &self.measure);
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
                if position < count && m.skipped(position) {
                    next = Some((position + 1, offset));
                    continue;
                }
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

    /// Lays out the pinned Strips, each slid to show its focused item, then
    /// the visible items into `strip`, reusing their buffers. Never past
    /// what the Window bound reaches, so a Strip item lacks a Key only if
    /// the Table Source left it unfilled. Returns whether a pinned Strip
    /// slid or its region changed.
    pub fn layout(&mut self) -> bool {
        let slide = Edge::ALL.map(|edge| self.slide_for(edge));
        let regions = self.pinned_regions();
        let slid = slide != self.slide || regions != self.regions;
        self.slide = slide;
        self.regions = regions;
        for edge in Edge::ALL {
            let mut pinned = std::mem::take(&mut self.pinned[edge.index()]);
            pinned.clear();
            pinned.extend(self.pinned_items(edge));
            self.pinned[edge.index()] = pinned;
        }
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
        slid
    }

    /// The anchor for a located offset, rounded toward the axis start so a
    /// clamped view never shows space past the last item.
    fn anchor_at(&self, l: ItemAt) -> ScrollAnchor {
        let mut offset = l.within as f32;
        if f64::from(offset) > l.within {
            offset = f32::from_bits(offset.to_bits() - 1);
        }
        let offset = offset.min(self.below_stride(l.position));
        ScrollAnchor {
            position: l.position,
            offset,
        }
    }

    /// The largest anchor offset inside the item at `p`: the f32 just below
    /// its size plus the Gap, summed in f64. Rounding that sum to f32 first
    /// can land below it by a step, which a huge Gap makes visible.
    fn below_stride(&self, p: Position) -> f32 {
        let stride = f64::from(self.measure.size_at(p)) + f64::from(self.measure.gap());
        let below = stride as f32;
        if f64::from(below) < stride {
            below
        } else {
            f32::from_bits(below.to_bits() - 1)
        }
    }

    /// The furthest the anchor may go: where the last item ends the viewport,
    /// or the End Gap before its end.
    fn max_anchor(&self) -> ScrollAnchor {
        let body = self.end_body();
        let Some(last) = self.last_shown() else {
            return ScrollAnchor::default();
        };
        if body <= 0.0 {
            return ScrollAnchor {
                position: last,
                offset: 0.0,
            };
        }
        let size = self.measure.size_at(last);
        let back = f64::from(size) - body;
        match self.measure.locate(last, back) {
            Some(l) => self.anchor_at(l),
            // The body is shorter than the whole axis.
            None if back < 0.0 => self.start(),
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
            None if offset < 0.0 => self.start(),
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
        self.at_end = to == max && max != self.start();
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
    /// end clamp pulls the anchor back, even one following the end, only once
    /// the Window holds the items it is measured over. An empty axis has no item to wait for: its anchor
    /// goes back to the start.
    pub fn clamp_anchor(&mut self) -> bool {
        // An item pinned since takes the anchor to the next scrolling one.
        if self.measure.skipped(self.anchor.position)
            && let Some(at) = self.measure.locate(self.anchor.position, 0.0)
        {
            self.anchor = ScrollAnchor {
                position: at.position,
                offset: 0.0,
            };
        }
        let count = self.measure.count();
        let position = self.anchor.position.min(count.saturating_sub(1));
        let max = self.max_anchor();
        // The end clamp is exact only once the Window holds every item from
        // it to the end; until then the anchor waits for the Window the next
        // frame asks for, which reaches the end.
        let exact = self.window_covers(&(max.position..count));
        let held = ScrollAnchor {
            position,
            ..self.anchor
        };
        // An end estimated short of an anchor following it may be wrong.
        let waiting = self.at_end
            && count > 0
            && !exact
            && (held.position, held.offset) > (max.position, max.offset);
        let to = if (self.at_end && !waiting) || count == 0 {
            max
        } else if !self.window_covers(&(position..position + 1)) {
            held
        } else {
            let offset = self.anchor.offset.min(self.below_stride(position));
            if (position, offset) > (max.position, max.offset) && exact {
                max
            } else {
                ScrollAnchor { position, offset }
            }
        };
        let following = self.at_end;
        let moved = self.settle(to, max);
        if waiting {
            self.at_end = true;
        }
        // An anchor left where it was follows the end only if it already
        // did: the end may be an estimate the next Window's sizes move.
        if !moved && !following {
            self.at_end = false;
        }
        moved
    }

    /// Puts the anchor at `to`, then re-applies its bounds as
    /// [`Self::clamp_anchor`] does.
    pub fn place_anchor(&mut self, to: ScrollAnchor) {
        self.anchor = to;
        self.at_end = false;
        self.clamp_anchor();
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
        let body = self.body();
        let delta = if forward { body } else { -body };
        let landed = self.measure.locate(p, delta).map(|at| at.position);
        if forward {
            landed.unwrap_or(last).max(p.saturating_add(1)).min(last)
        } else {
            landed.unwrap_or(0).min(p.saturating_sub(1))
        }
    }

    /// Scrolls the least distance that shows all of item `p`, or its start if
    /// it is longer than the body. A pinned item always shows. With no body (no viewport yet) nothing can
    /// show, so the anchor stays. Returns whether the anchor moved.
    pub fn reveal(&mut self, p: Position) -> bool {
        if self.body() <= 0.0 || self.pinned_at(p).is_some() {
            return false;
        }
        // The last item shows with the End Gap after it.
        let body = if Some(p) == self.last_shown() {
            self.end_body()
        } else {
            self.body()
        };
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

    /// Scrolls so the item at `p` starts the body, clamped at the end.
    pub fn scroll_to_item(&mut self, p: Position) -> bool {
        self.move_to(p, 0.0)
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
        // The room the End Gap keeps at the end scrolls like content.
        let extent = self.measure.extent() + (self.body() - self.end_body());
        Thumb {
            fraction: if scrollable > 0.0 {
                (scrolled / scrollable).clamp(0.0, 1.0)
            } else {
                0.0
            },
            size: if extent > 0.0 {
                (self.body() / extent).min(1.0)
            } else {
                1.0
            },
        }
    }
}
