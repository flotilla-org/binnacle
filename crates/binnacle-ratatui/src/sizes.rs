//! Initial sizes per item, as the web widget's `initialColumnWidth` and
//! `initialRowHeight` give them: chosen by the host per Key, set as Size
//! Overrides once per Key, and never over a size the user set.

use std::collections::HashSet;

use binnacle_core::{Axis, Grid, Key};

use crate::{Content, fit_width};

/// An item's starting size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InitialSize {
    /// The axis's default size.
    Default,
    /// This many cells.
    Cells(f32),
    /// A column's header and the values in view, as a double-click on its
    /// edge fits it. Rows hold one line, so a row's fit is the default.
    Fit,
    /// A share of the room the axis has left, equal among the items that
    /// fill, once every item is laid out. An item that already is as large
    /// as its share keeps its size, and an axis that overflows fills nothing.
    Fill,
}

type Choose = Box<dyn FnMut(Key) -> InitialSize>;

#[derive(Default)]
struct Pending {
    seen: HashSet<Key>,
    fits: Vec<Key>,
    fills: Vec<Key>,
}

/// Seeds each item's size when its Key first appears. Hand it a function
/// from Key to [`InitialSize`] per axis, then call [`InitialSizes::settle`]
/// each frame after the Grid's windows are filled ([`crate::begin_frame`]),
/// laying the frame out again while it returns `true`: fits wait for the
/// values in view, and fills for the fits.
///
/// Each Key is asked once. A Size Override already on the item, set by the
/// user (or restored from a layout) before it was seeded, or after, wins:
/// a later resize is never undone, and an item scrolled out and back keeps
/// its size.
#[derive(Default)]
pub struct InitialSizes {
    choose: [Option<Choose>; 2],
    pending: [Pending; 2],
}

impl InitialSizes {
    pub fn new() -> Self {
        Self::default()
    }

    /// Each column's starting width, by its Key.
    pub fn columns(mut self, choose: impl FnMut(Key) -> InitialSize + 'static) -> Self {
        self.choose[Axis::Cols.index()] = Some(Box::new(choose));
        self
    }

    /// Each row's starting height, by its Key.
    pub fn rows(mut self, choose: impl FnMut(Key) -> InitialSize + 'static) -> Self {
        self.choose[Axis::Rows.index()] = Some(Box::new(choose));
        self
    }

    /// Seeds the sizes the Grid's current Snapshot makes possible, and
    /// returns whether any size changed, so the frame wants laying out again.
    /// `content` measures fits through its Cell Renderers.
    pub fn settle(&mut self, grid: &mut Grid, content: &impl Content) -> bool {
        let mut changed = false;
        for axis in Axis::ALL {
            changed |= self.arrive(grid, axis);
        }
        let shown: Vec<Option<Key>> = grid
            .snapshot()
            .drawn(Axis::Rows)
            .map(|item| item.key)
            .collect();
        let rows: Option<Vec<Key>> = shown.into_iter().collect();
        for axis in Axis::ALL {
            let pending = &mut self.pending[axis.index()];
            let mut fitted = false;
            if axis == Axis::Cols {
                for key in std::mem::take(&mut pending.fits) {
                    if grid.size_override(axis, key).is_some() {
                        continue;
                    }
                    let Some(rows) = &rows else {
                        pending.fits.push(key);
                        continue;
                    };
                    let width = fit_width(content, rows, key).max(1);
                    grid.set_size(axis, key, width as f32);
                    fitted = true;
                }
            }
            if !pending.fits.is_empty() || fitted || pending.fills.is_empty() {
                changed |= fitted;
                continue;
            }
            changed |= fill(grid, axis, &mut pending.fills);
        }
        changed
    }

    /// Asks for the Keys of `axis` that have not been asked: fixed sizes are
    /// set now, fits and fills wait.
    fn arrive(&mut self, grid: &mut Grid, axis: Axis) -> bool {
        let a = axis.index();
        let Some(choose) = self.choose[a].as_mut() else {
            return false;
        };
        let pending = &mut self.pending[a];
        let mut changed = false;
        let keys: Vec<Key> = (grid.snapshot().strip(axis).iter())
            .filter_map(|item| item.key)
            .collect();
        for key in keys {
            if !pending.seen.insert(key) {
                continue;
            }
            match choose(key) {
                InitialSize::Default => {}
                InitialSize::Cells(size) => {
                    if grid.size_override(axis, key).is_none() {
                        grid.set_size(axis, key, size);
                        changed = true;
                    }
                }
                InitialSize::Fit if axis == Axis::Cols => pending.fits.push(key),
                InitialSize::Fit => {}
                InitialSize::Fill => pending.fills.push(key),
            }
        }
        changed
    }
}

/// Shares the room left on `axis` between `fills`, once every item is in the
/// Strip from the first; whether any size changed. Otherwise leaves `fills`
/// for a later frame.
fn fill(grid: &mut Grid, axis: Axis, fills: &mut Vec<Key>) -> bool {
    let sized: HashSet<Key> = grid.size_overrides(axis).map(|(key, _)| key).collect();
    let snapshot = grid.snapshot();
    let strip = snapshot.strip(axis);
    if snapshot.extent(axis) <= snapshot.header(axis) || strip.iter().any(|i| i.key.is_none()) {
        return false;
    }
    let sharing: Vec<(Key, f32)> = strip
        .iter()
        .filter_map(|i| Some((i.key?, i.size)))
        .filter(|(key, _)| fills.contains(key) && !sized.contains(key))
        .collect();
    // Items past the Strip mean the axis already overflows: the fills keep the default.
    let whole = strip
        .first()
        .zip(strip.last())
        .filter(|(first, _)| first.position == 0 && strip.len() as u64 == snapshot.count(axis));
    let room = whole.map_or(0.0, |(first, last)| {
        let taken: f32 = sharing.iter().map(|(_, size)| size).sum();
        snapshot.extent(axis) - snapshot.header(axis) - (last.end() - first.offset) + taken
    });
    fills.clear();
    if sharing.is_empty() {
        return false;
    }
    let share = (room / sharing.len() as f32).floor();
    let mut changed = false;
    for (key, size) in sharing {
        if share > size {
            grid.set_size(axis, key, share);
            changed = true;
        }
    }
    changed
}
