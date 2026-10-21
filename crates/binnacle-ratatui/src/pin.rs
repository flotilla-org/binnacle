//! Pinning the focused item, as [`crate::Control::Pin`],
//! [`crate::Control::Unpin`] and [`crate::Control::Freeze`] ask.

use binnacle_core::{Axis, Edge, Grid};

/// Pins the focused item of `axis` to `edge`, or unpins it if it is pinned
/// there already (one pinned at the other edge moves). Does nothing while
/// focus is unplaced or its Key unknown.
pub fn toggle_pin(grid: &mut Grid, axis: Axis, edge: Edge) {
    let snapshot = grid.snapshot();
    let Some(key) = snapshot.focus(axis).and_then(|f| f.key) else {
        return;
    };
    let pinned = (snapshot.pinned(axis, edge).iter()).any(|i| i.key == Some(key));
    if pinned {
        grid.unpin(axis, key);
    } else {
        grid.pin(axis, edge, key);
    }
}

/// Pins every item of `axis` from the start up to the focused one, as many
/// as fit ([`Grid::freeze`]).
pub fn freeze_focused(grid: &mut Grid, axis: Axis) {
    if let Some(key) = grid.snapshot().focus(axis).and_then(|f| f.key) {
        grid.freeze(axis, key);
    }
}

/// Unpins the focused item of `axis`, if it is pinned.
pub fn unpin_focused(grid: &mut Grid, axis: Axis) {
    if let Some(key) = grid.snapshot().focus(axis).and_then(|f| f.key) {
        grid.unpin(axis, key);
    }
}
