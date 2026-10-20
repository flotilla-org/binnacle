//! Saving and restoring a layout: the parts of how the Grid presents its
//! table that a host keeps between sessions. A Rust host's Keys are its
//! Table Source's own Ids, so the layout holds Keys. The order of items is
//! not in it: that belongs to the source.

use binnacle_core::{Axis, Edge, Grid, Key, ViewSpec};

/// The format [`save_layout`] writes; [`restore_layout`] restores nothing
/// of another.
pub const LAYOUT_VERSION: u32 = 1;

/// A Grid's layout. Each array is indexed by [`Axis::index`], and `pins`
/// within it by [`Edge::index`].
#[derive(Clone, Debug, PartialEq)]
pub struct SavedLayout {
    pub version: u32,
    /// The View Spec last asked for, else the one applied.
    pub view: ViewSpec,
    /// Each edge's pins in pin order, dormant ones included.
    pub pins: [[Vec<Key>; 2]; 2],
    /// Every Size Override, for items shown or not, in Key order.
    pub sizes: [Vec<(Key, f32)>; 2],
    /// The thickness of the Header Band leading each axis.
    pub bands: [f32; 2],
}

/// The Grid's layout as it stands.
pub fn save_layout(grid: &mut Grid) -> SavedLayout {
    let sizes = Axis::ALL.map(|axis| grid.size_overrides(axis).collect());
    let snapshot = grid.snapshot();
    SavedLayout {
        version: LAYOUT_VERSION,
        view: (snapshot.requested_view())
            .unwrap_or(snapshot.applied_view())
            .clone(),
        pins: Axis::ALL.map(|axis| Edge::ALL.map(|edge| snapshot.pins(axis, edge).collect())),
        sizes,
        bands: Axis::ALL.map(|axis| snapshot.header(axis)),
    }
}

/// Puts `layout` back: pins, Size Overrides and band thicknesses at once
/// (overrides it does not list are cleared), and the View Spec as an
/// ordinary request, pending until the Table Source answers. It never fails
/// on what the source lacks: a missing pinned item sleeps until it appears,
/// the source leaves hidden Keys it does not have out of its answer, a
/// condition on a missing item shows as not applied, and a size waits for
/// its item. Returns `false`, restoring nothing, for a layout of another
/// version.
pub fn restore_layout(grid: &mut Grid, layout: &SavedLayout) -> bool {
    if layout.version != LAYOUT_VERSION {
        return false;
    }
    for axis in Axis::ALL {
        let a = axis.index();
        for edge in Edge::ALL {
            grid.set_pins(axis, edge, &layout.pins[a][edge.index()]);
        }
        let stale: Vec<Key> = (grid.size_overrides(axis))
            .map(|(key, _)| key)
            .filter(|key| !layout.sizes[a].iter().any(|(k, _)| k == key))
            .collect();
        for key in stale {
            grid.clear_size(axis, key);
        }
        for &(key, size) in &layout.sizes[a] {
            grid.set_size(axis, key, size);
        }
        grid.set_header(axis, layout.bands[a]);
    }
    grid.request_view(layout.view.clone());
    true
}
