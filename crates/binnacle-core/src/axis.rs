use std::ops::Range;

use crate::snapshot::StripItem;
use crate::types::{Key, Position};

/// Positions fetched beyond the visible ones, so small scrolls need no new Window.
const OVERSCAN: u64 = 2;

/// One axis's layout and Window. Holds nothing proportional to the count.
pub(crate) struct AxisState {
    pub count: u64,
    pub default_size: f32,
    pub header: f32,
    /// The viewport's length along this axis, header band included.
    pub extent: f32,
    window_start: Position,
    window_keys: Vec<Key>,
    pub strip: Vec<StripItem>,
}

impl AxisState {
    pub fn new(count: u64, default_size: f32, header: f32) -> Self {
        AxisState {
            count,
            default_size,
            header,
            extent: 0.0,
            window_start: 0,
            window_keys: Vec::new(),
            strip: Vec::new(),
        }
    }

    /// The Window this axis needs for the current viewport: the visible items
    /// plus overscan. Counted by the same stepping as [`AxisState::layout`], so
    /// the two agree even where f32 rounding drifts.
    pub fn needed(&self) -> Range<Position> {
        let visible = self.placements().last().map_or(0, |(p, _)| p + 1);
        0..visible.saturating_add(OVERSCAN).min(self.count)
    }

    /// The Position and offset of each visible item, in order. Stops early if an
    /// item is too small to advance an f32 offset, rather than spin.
    fn placements(&self) -> impl Iterator<Item = (Position, f32)> + '_ {
        let size = self.default_size;
        let mut next = Some((0, self.header));
        std::iter::from_fn(move || {
            let (position, offset) = next?;
            let end = offset + size;
            if position >= self.count || offset >= self.extent || end <= offset {
                next = None;
                return None;
            }
            next = Some((position + 1, end));
            Some((position, offset))
        })
    }

    pub fn window_covers(&self, range: &Range<Position>) -> bool {
        range.is_empty()
            || (self.window_start <= range.start
                && range.end <= self.window_start + self.window_keys.len() as u64)
    }

    /// Replaces the Window, returning whether any Key at any Position changed.
    pub fn set_window(&mut self, start: Position, keys: &[Key]) -> bool {
        let fit = self.count.saturating_sub(start);
        let keys = &keys[..keys.len().min(usize::try_from(fit).unwrap_or(usize::MAX))];
        if start == self.window_start && keys == self.window_keys.as_slice() {
            return false;
        }
        self.window_start = start;
        self.window_keys.clear();
        self.window_keys.extend_from_slice(keys);
        true
    }

    fn key_at(&self, pos: Position) -> Option<Key> {
        let i = pos.checked_sub(self.window_start)?;
        self.window_keys.get(usize::try_from(i).ok()?).copied()
    }

    /// Lays out the visible items into `strip`, reusing its buffer.
    pub fn layout(&mut self) {
        let mut strip = std::mem::take(&mut self.strip);
        strip.clear();
        let size = self.default_size;
        strip.extend(self.placements().map(|(position, offset)| StripItem {
            position,
            key: self.key_at(position),
            offset,
            size,
        }));
        self.strip = strip;
    }
}
