//! Fixtures shared by the pointer and sorting scenarios.

use binnacle_core::{AxisConfig, Viewport};
use binnacle_testkit::{IdentityTable, Scenario};

/// 20 rows of 1 with no Gap under a 1-high header band; 5 columns of 6 with a
/// Gap of 1 after a 4-wide row header band.
pub fn table() -> Scenario<IdentityTable> {
    Scenario::identity(
        AxisConfig::new(20, 1.0).header(1.0),
        AxisConfig::new(5, 6.0).header(4.0).gap(1.0),
    )
}

pub const V: Viewport = Viewport {
    width: 26.0,
    height: 5.0,
};
