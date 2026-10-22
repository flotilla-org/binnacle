//! The corner grid: where the tiers of the two Header Bands cross, the
//! corner names the levels each tier groups by.

use std::ops::Range;

use crate::grid::Grid;
use crate::snapshot::Rect;
use crate::span::header_form;
use crate::types::{Axis, Key};

/// The corner where the Header Bands meet, from [`crate::Snapshot::corner`],
/// as a grid: one row per tier of the column Header Band, one column per
/// tier of the row Header Band, each band's own headers taking the last.
/// Adapters name the levels in it through a corner renderer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CornerGrid {
    tiers: [Vec<CornerTier>; 2],
}

/// One tier of a Header Band where it crosses the corner.
#[derive(Clone, Debug, PartialEq)]
pub struct CornerTier {
    /// Where it lies across its band, from the band's outer edge: a row
    /// tier's columns of the corner, a column tier's lines.
    pub across: Range<f32>,
    /// The levels it names, outermost first. In header form a level's tier
    /// names that level and the items' own tier none; in outline form the
    /// one tier names every level of its axis.
    pub levels: Vec<CornerLevel>,
}

/// A level named in the corner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CornerLevel {
    /// Its place among its axis's levels, 0 for the outermost.
    pub index: u32,
    /// The field it groups by; `None` marks the values pseudo-level.
    pub field: Option<Key>,
}

impl CornerLevel {
    /// Whether it is the values pseudo-level.
    pub fn is_values(&self) -> bool {
        self.field.is_none()
    }
}

impl CornerGrid {
    /// The tiers of the Header Band heading `axis`'s items where it crosses
    /// the corner, outermost first, its items' own headers last: on
    /// [`Axis::Rows`] the corner's columns, on [`Axis::Cols`] its rows.
    pub fn tiers(&self, axis: Axis) -> &[CornerTier] {
        &self.tiers[axis.index()]
    }

    /// The cell where column tier `col_tier` crosses row tier `row_tier`,
    /// unclipped; `None` past either band's tiers.
    pub fn cell(&self, col_tier: usize, row_tier: usize) -> Option<Rect> {
        let x = &self.tiers(Axis::Rows).get(row_tier)?.across;
        let y = &self.tiers(Axis::Cols).get(col_tier)?.across;
        Some(Rect {
            x: x.start,
            y: y.start,
            width: x.end - x.start,
            height: y.end - y.start,
        })
    }
}

impl Grid {
    /// Lays out the corner grid from the bands and their tiers, once the
    /// spans are laid out.
    pub(crate) fn lay_out_corner(&mut self) {
        let grouping = self.view.applied.grouping.as_ref();
        let tiers = Axis::ALL.map(|axis| {
            let thickness = self.axis(axis.other()).header;
            let levels = grouping.map_or(&[][..], |g| g.levels(axis));
            let named = |i: usize| CornerLevel {
                index: i as u32,
                field: levels[i].field_key(),
            };
            let laid = &self.tiers[axis.index()];
            match header_form(grouping, axis) {
                Some(_) if !laid.is_empty() => {
                    let end = laid.last().map_or(0.0, |t| t.end);
                    (laid.iter().enumerate())
                        .map(|(i, t)| CornerTier {
                            across: t.clone(),
                            levels: vec![named(i)],
                        })
                        .chain([CornerTier {
                            across: end..thickness.max(end),
                            levels: Vec::new(),
                        }])
                        .collect()
                }
                _ => vec![CornerTier {
                    across: 0.0..thickness,
                    levels: (0..levels.len()).map(named).collect(),
                }],
            }
        });
        self.corner = CornerGrid { tiers };
    }
}
