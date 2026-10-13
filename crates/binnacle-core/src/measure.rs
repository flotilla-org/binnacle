use std::collections::BTreeMap;
use std::ops::Range;

use crate::types::{Key, Position};

/// Where an offset along an axis lands: the item at `position`, `within` its
/// start. Past the item's size it is in the Gap that follows (`in_gap`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemAt {
    pub position: Position,
    pub within: f64,
    pub in_gap: bool,
}

/// What replacing the Window changed: which Key sits at which Position, and
/// the size of any item in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowChange {
    pub keys: bool,
    pub sizes: bool,
}

/// One axis's sizes: a default size, a Gap between items, Size Overrides by Key
/// and the Keys (with optional content sizes) of the current Window.
///
/// Only the Window's Positions have known Keys, so only they can take a Size
/// Override or content size; every other Position is assumed to be the default
/// size. Memory is proportional to the overrides plus the Window, never the
/// count. Distances are in `f64` and taken between Positions, so a query near a
/// Scroll Anchor stays exact however far down a huge axis it is: default-sized
/// runs are crossed by arithmetic, Window items one by one from the query's
/// start (O(Window), never O(count) or O(overrides)).
///
/// Every size it uses lies in its size range `[min_size, max_size]`: the
/// default size, Size Overrides, content sizes and the minimum-size hint are
/// clamped into it, and the Gap is capped at `max_size`. The range keeps f32
/// offsets precise and bounds the Window at about a viewport's length over
/// `min_size` items.
#[derive(Clone, Debug)]
pub struct AxisMeasure {
    count: u64,
    default_size: f32,
    gap: f32,
    min_size: f32,
    max_size: f32,
    overrides: BTreeMap<Key, f32>,
    /// How many overrides have each size, keyed by its bits (positive floats
    /// order like their bits), so the smallest is found in O(log n).
    override_sizes: BTreeMap<u32, usize>,
    content_min: Option<f32>,
    window_start: Position,
    keys: Vec<Key>,
    /// Content sizes parallel to `keys`, NaN where unknown; empty if none given.
    content: Vec<f32>,
    /// Effective size of each Window item.
    sizes: Vec<f32>,
}

impl AxisMeasure {
    /// The size range a measure uses unless given another.
    pub const DEFAULT_MIN_SIZE: f32 = 1.0;
    pub const DEFAULT_MAX_SIZE: f32 = 1_048_576.0;

    /// A measure with the default size range. `default_size` must be positive
    /// and finite, `gap` finite and not negative; both are then clamped as
    /// the range says.
    pub fn new(count: u64, default_size: f32, gap: f32) -> Self {
        Self::with_size_range(
            count,
            default_size,
            gap,
            Self::DEFAULT_MIN_SIZE,
            Self::DEFAULT_MAX_SIZE,
        )
    }

    /// Like [`AxisMeasure::new`] with the size range `[min_size, max_size]`.
    /// `min_size` must be positive and finite, `max_size` finite and at least
    /// `min_size`.
    pub fn with_size_range(
        count: u64,
        default_size: f32,
        gap: f32,
        min_size: f32,
        max_size: f32,
    ) -> Self {
        assert!(
            default_size.is_finite() && default_size > 0.0,
            "default size must be positive and finite, got {default_size}"
        );
        assert!(
            gap.is_finite() && gap >= 0.0,
            "Gap must be finite and not negative, got {gap}"
        );
        assert!(
            min_size.is_finite() && min_size > 0.0,
            "min size must be positive and finite, got {min_size}"
        );
        assert!(
            max_size.is_finite() && max_size >= min_size,
            "max size must be finite and at least the min size {min_size}, got {max_size}"
        );
        AxisMeasure {
            count,
            default_size: default_size.clamp(min_size, max_size),
            gap: gap.min(max_size),
            min_size,
            max_size,
            overrides: BTreeMap::new(),
            override_sizes: BTreeMap::new(),
            content_min: None,
            window_start: 0,
            keys: Vec::new(),
            content: Vec::new(),
            sizes: Vec::new(),
        }
    }

    pub fn count(&self) -> u64 {
        self.count
    }

    pub fn default_size(&self) -> f32 {
        self.default_size
    }

    pub fn gap(&self) -> f32 {
        self.gap
    }

    /// The smallest size any item may have.
    pub fn min_size(&self) -> f32 {
        self.min_size
    }

    /// The largest size any item may have.
    pub fn max_size(&self) -> f32 {
        self.max_size
    }

    /// `size` clamped into the size range, or `None` if it is not positive and
    /// finite.
    fn in_range(&self, size: f32) -> Option<f32> {
        (size.is_finite() && size > 0.0).then(|| size.clamp(self.min_size, self.max_size))
    }

    /// Sets the Size Override for `key`, clamped into the size range. Sizes
    /// that are not positive and finite are ignored.
    pub fn set_size(&mut self, key: Key, size: f32) {
        let Some(size) = self.in_range(size) else {
            return;
        };
        if let Some(old) = self.overrides.insert(key, size) {
            self.forget_size(old);
        }
        *self.override_sizes.entry(size.to_bits()).or_default() += 1;
        if self.keys.contains(&key) {
            self.rebuild();
        }
    }

    /// Removes the Size Override for `key`, if any, so its content size or the
    /// default applies again.
    pub fn clear_size(&mut self, key: Key) {
        let Some(old) = self.overrides.remove(&key) else {
            return;
        };
        self.forget_size(old);
        if self.keys.contains(&key) {
            self.rebuild();
        }
    }

    fn forget_size(&mut self, size: f32) {
        let bits = size.to_bits();
        if let Some(n) = self.override_sizes.get_mut(&bits) {
            *n -= 1;
            if *n == 0 {
                self.override_sizes.remove(&bits);
            }
        }
    }

    /// Drops every Size Override.
    pub fn clear_sizes(&mut self) {
        self.overrides.clear();
        self.override_sizes.clear();
        self.rebuild();
    }

    /// Sets the count and empties the Window, whose Keys may now sit anywhere.
    pub fn set_count(&mut self, count: u64) {
        self.count = count;
        self.set_keys(0, &[]);
    }

    /// `n` items were inserted before Position `at` (at most the count): the
    /// count grows, and the Window keeps the Keys whose Positions it still
    /// knows: all of them, `n` later, if `at` is at or before its start,
    /// else those before `at`. Items that would take the count past
    /// `u64::MAX` are ignored.
    pub fn insert(&mut self, at: Position, n: u64) {
        let at = at.min(self.count);
        let n = n.min(u64::MAX - self.count);
        self.count += n;
        if at <= self.window_start {
            self.window_start += n;
        } else if let Some(i) = self.index(at) {
            self.keys.truncate(i);
            self.content.truncate(i);
        }
        self.rebuild();
    }

    /// The `n` items from Position `at` were removed (both clipped to the
    /// count): the count shrinks, and the Window drops their Keys and closes
    /// up around them. Returns the Keys the Window dropped.
    pub fn remove(&mut self, at: Position, n: u64) -> Vec<Key> {
        let at = at.min(self.count);
        let n = n.min(self.count - at);
        let w = self.window();
        let (lo, hi) = (at.max(w.start), (at + n).min(w.end));
        let mut gone = Vec::new();
        if lo < hi {
            let r = (lo - w.start) as usize..(hi - w.start) as usize;
            gone = self.keys.drain(r.clone()).collect();
            self.content.drain(r);
        }
        self.window_start = if w.start >= at + n {
            w.start - n
        } else {
            w.start.min(at)
        };
        self.count -= n;
        self.rebuild();
        gone
    }

    /// The Size Override set for `key`, if any.
    pub fn size_override(&self, key: Key) -> Option<f32> {
        self.overrides.get(&key).copied()
    }

    /// The smallest size any item can have: the default, the smallest Size
    /// Override and the content size hint. Bounds how many items a length can
    /// hold before their Keys are known.
    pub fn smallest_size(&self) -> f32 {
        let smallest_override = self
            .override_sizes
            .keys()
            .next()
            .map_or(f32::INFINITY, |&b| f32::from_bits(b));
        self.default_size
            .min(smallest_override)
            .min(self.content_min.unwrap_or(f32::INFINITY))
    }

    /// Replaces the Window's Keys, dropping any past the count and forgetting
    /// content sizes.
    pub fn set_keys(&mut self, start: Position, keys: &[Key]) -> WindowChange {
        self.replace(start, keys, |_| None)
    }

    /// Like [`AxisMeasure::set_keys`], with the host's content size for each Key
    /// and a hint that no content size on this axis is below `min_size`. Sizes
    /// below the hint are raised to it; sizes that are not positive and finite
    /// count as unknown. The hint stays until replaced. Sizes and hint are
    /// clamped into the size range.
    pub fn set_keys_sized(
        &mut self,
        start: Position,
        keys: &[Key],
        sizes: &[f32],
        min_size: f32,
    ) -> WindowChange {
        if let Some(hint) = self.in_range(min_size) {
            self.content_min = Some(hint);
        }
        let hint = self.content_min.unwrap_or(self.min_size);
        let max = self.max_size;
        self.replace(start, keys, |i| {
            sizes
                .get(i)
                .filter(|s| s.is_finite() && **s > 0.0)
                .map(|s| s.clamp(hint, max))
        })
    }

    fn replace(
        &mut self,
        start: Position,
        keys: &[Key],
        content: impl Fn(usize) -> Option<f32>,
    ) -> WindowChange {
        let fit = usize::try_from(self.count.saturating_sub(start)).unwrap_or(usize::MAX);
        let keys = &keys[..keys.len().min(fit)];
        let moved = start != self.window_start;
        let keys_changed = moved || keys != self.keys.as_slice();
        self.window_start = start;
        self.keys.clear();
        self.keys.extend_from_slice(keys);
        self.content.clear();
        self.content
            .extend((0..keys.len()).map(|i| content(i).unwrap_or(f32::NAN)));
        let resized = self.rebuild();
        WindowChange {
            keys: keys_changed,
            sizes: moved || resized,
        }
    }

    /// Recomputes the Window's sizes. Returns whether a size changed.
    fn rebuild(&mut self) -> bool {
        let old = std::mem::take(&mut self.sizes);
        for (i, key) in self.keys.iter().enumerate() {
            let content = self.content.get(i).copied().filter(|s| !s.is_nan());
            let size = self
                .overrides
                .get(key)
                .copied()
                .or(content)
                .unwrap_or(self.default_size);
            self.sizes.push(size);
        }
        old != self.sizes
    }

    fn window(&self) -> Range<Position> {
        self.window_start..self.window_start + self.keys.len() as u64
    }

    fn index(&self, pos: Position) -> Option<usize> {
        let i = pos.checked_sub(self.window_start)?;
        let i = usize::try_from(i).ok()?;
        (i < self.keys.len()).then_some(i)
    }

    pub fn window_start(&self) -> Position {
        self.window_start
    }

    /// How many Keys the Window holds.
    pub fn window_len(&self) -> usize {
        self.keys.len()
    }

    /// The Position of `key` if the Window holds it.
    pub fn position_of(&self, key: Key) -> Option<Position> {
        let i = self.keys.iter().position(|&k| k == key)?;
        Some(self.window_start + i as u64)
    }

    /// The Key the Window holds at `pos`, if any.
    pub fn key_at(&self, pos: Position) -> Option<Key> {
        self.index(pos).map(|i| self.keys[i])
    }

    /// The size of the item at `pos`: its Size Override, else its content size,
    /// else the default.
    pub fn size_at(&self, pos: Position) -> f32 {
        self.index(pos).map_or(self.default_size, |i| self.sizes[i])
    }

    fn stride(&self, size: f32) -> f64 {
        f64::from(size) + f64::from(self.gap)
    }

    fn default_stride(&self) -> f64 {
        self.stride(self.default_size)
    }

    /// The distance from the start of item `from` to the start of item `to`
    /// (negative if `to` comes first). Positions are clamped to the count; the
    /// "start" of Position `count` is one Gap past the last item's end.
    pub fn distance(&self, from: Position, to: Position) -> f64 {
        let (from, to) = (from.min(self.count), to.min(self.count));
        if from > to {
            return -self.distance(to, from);
        }
        let w = self.window();
        let (lo, hi) = (from.max(w.start), to.min(w.end));
        let (known, inside) = if lo < hi {
            let (a, b) = ((lo - w.start) as usize, (hi - w.start) as usize);
            let known: f64 = self.sizes[a..b].iter().map(|&s| self.stride(s)).sum();
            (known, hi - lo)
        } else {
            (0.0, 0)
        };
        known + (to - from - inside) as f64 * self.default_stride()
    }

    /// The length of the whole axis: every item plus the Gaps between them.
    pub fn extent(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.distance(0, self.count) - f64::from(self.gap)
        }
    }

    /// The item at `delta` past the start of item `from` (before it if
    /// negative), or `None` if that is outside the axis. The Gap after the last
    /// item is not part of the axis.
    pub fn locate(&self, from: Position, delta: f64) -> Option<ItemAt> {
        if self.count == 0 || !delta.is_finite() {
            return None;
        }
        let mut p = from.min(self.count - 1);
        let mut rem = delta;
        let w = self.window();
        while rem < 0.0 {
            if p == 0 {
                return None;
            }
            if w.start < p && p <= w.end {
                p -= 1;
                rem += self.stride(self.sizes[(p - w.start) as usize]);
                continue;
            }
            let floor = if p > w.end { w.end } else { 0 };
            let stride = self.default_stride();
            let n = (-rem / stride).ceil();
            if n <= (p - floor) as f64 {
                p -= n as u64;
                rem = (rem + n * stride).max(0.0);
            } else {
                rem += (p - floor) as f64 * stride;
                p = floor;
            }
        }
        loop {
            if p >= self.count {
                return None;
            }
            if let Some(i) = self.index(p) {
                let stride = self.stride(self.sizes[i]);
                if rem < stride {
                    return self.landed(p, rem);
                }
                rem -= stride;
                p += 1;
                continue;
            }
            let end = if p < w.start { w.start } else { self.count };
            let stride = self.default_stride();
            let k = (rem / stride).floor();
            if k < (end - p) as f64 {
                let k = k as u64;
                return self.landed(p + k, rem - k as f64 * stride);
            }
            rem -= (end - p) as f64 * stride;
            p = end;
        }
    }

    fn landed(&self, position: Position, within: f64) -> Option<ItemAt> {
        let size = f64::from(self.size_at(position));
        let stride = size + f64::from(self.gap);
        let within = within.clamp(0.0, stride);
        if within >= stride {
            return self.locate(position + 1, 0.0);
        }
        let in_gap = within >= size;
        if in_gap && position + 1 == self.count {
            return None;
        }
        Some(ItemAt {
            position,
            within,
            in_gap,
        })
    }
}
