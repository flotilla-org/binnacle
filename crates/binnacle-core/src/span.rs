//! Header form: each level of a grouped axis is a tier of the Header Band
//! that heads its items, and each group one spanning cell over the items
//! under it, merged by ancestor Key and broken at pinned edges.

use std::ops::Range;

use crate::axis::AxisState;
use crate::grid::Grid;
use crate::grouping::{Form, Grouping, ItemKind, OpenState, Outline};
use crate::snapshot::{Rect, StripItem};
use crate::types::{Axis, Edge, Key, Position};

/// One spanning cell of a tier, from [`crate::Snapshot::spans`]: a group of
/// `axis` drawn in header form over the run of items under it that one
/// Strip draws side by side. A group whose items fall in several Strips has
/// a span in each, each with its own label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeaderSpan {
    /// The axis whose items it spans (a row span is on [`Axis::Rows`]).
    pub axis: Axis,
    /// Its tier, 0 for the outermost level.
    pub tier: u32,
    /// The group's Group Item Key: an ancestor Key of every item it spans.
    pub key: Key,
    /// The field of its level.
    pub field: Option<Key>,
    /// Open, or closed with its Subtotal alone under it.
    pub open: OpenState,
    /// The Positions of the first and last items it spans, in drawn order.
    pub first: Position,
    pub last: Position,
    /// Its rectangle in the Header Band, unclipped like the Strip items.
    pub rect: Rect,
    /// The part of `rect` its Strip draws: where its label goes, so the
    /// label stays in view while the span scrolls.
    pub visible: Rect,
    /// The group goes on before or after this span: past the viewport, the
    /// Window or a pinned edge. Adapters mark the side that continues.
    pub cut_before: bool,
    pub cut_after: bool,
}

/// The grouping of `axis` if it is drawn in header form.
pub(crate) fn header_form(grouping: Option<&Grouping>, axis: Axis) -> Option<&Grouping> {
    grouping.filter(|g| !g.levels(axis).is_empty() && g.axis(axis).form == Form::Header)
}

/// The group whose span holds an item with `outline` at `tier`, given
/// whether the applied spec opens a group: its ancestor there, except an
/// open group's own Subtotal, which sits outside its span with its label.
pub(crate) fn spanned_by(
    outline: &Outline,
    tier: usize,
    opens: impl Fn(u32, Key) -> bool,
) -> Option<Key> {
    let key = *outline.ancestors.get(tier)?;
    let own = outline.kind == ItemKind::Subtotal && tier + 1 == outline.ancestors.len();
    match own && opens(tier as u32, key) {
        true => None,
        false => Some(key),
    }
}

/// Splits a band `thickness` long into `n` tiers sized as `sizes` says,
/// from its outer edge; tiers past `sizes` share what is left. Tiers past
/// the band's end are clipped to it.
pub(crate) fn tier_ranges(sizes: &[f32], n: usize, thickness: f32) -> Vec<Range<f32>> {
    let given: f32 = sizes.iter().take(n).sum();
    let missing = n.saturating_sub(sizes.len());
    let share = match missing {
        0 => 0.0,
        m => (thickness - given).max(0.0) / m as f32,
    };
    let mut at = 0.0f32;
    (0..n)
        .map(|i| {
            let size = sizes.get(i).copied().unwrap_or(share);
            let start = at.min(thickness);
            at += size;
            start..at.min(thickness)
        })
        .collect()
}

impl Grid {
    /// The outline of the item at `position` on `axis`, if the Window holds
    /// it; a pinned Grand Total of a grouped axis has its own.
    pub(crate) fn outline_at(&self, axis: Axis, position: Position) -> Option<&Outline> {
        let key = self.axis(axis).key_at(position)?;
        self.outline_by_key(axis, key)
    }

    /// Whether the applied spec opens the group `key` at `level` of `axis`.
    pub(crate) fn applied_opens(&self, axis: Axis, level: u32, key: Key) -> bool {
        (self.view.applied.grouping.as_ref()).is_some_and(|g| g.axis(axis).expand.opens(level, key))
    }

    /// The group whose span holds the item at `position` on `axis` at
    /// `tier`, in header form.
    pub(crate) fn span_key(&self, axis: Axis, position: Position, tier: usize) -> Option<Key> {
        let outline = self.outline_at(axis, position)?;
        spanned_by(outline, tier, |l, k| self.applied_opens(axis, l, k))
    }

    /// How many tiers, from the outermost, spans take on the item at
    /// `position` of `axis` in header form: its own header starts past them.
    pub(crate) fn spanned_tiers(&self, axis: Axis, position: Position) -> usize {
        let Some(outline) = self.outline_at(axis, position) else {
            return 0;
        };
        let opens = |l, k| self.applied_opens(axis, l, k);
        (0..outline.ancestors.len())
            .take_while(|&t| spanned_by(outline, t, opens).is_some())
            .count()
    }

    /// Lays out the tiers and spans of each axis drawn in header form, for
    /// the Snapshot.
    pub(crate) fn lay_out_spans(&mut self) {
        for axis in Axis::ALL {
            let grouping = header_form(self.view.applied.grouping.as_ref(), axis);
            let levels = grouping.map_or(0, |g| g.levels(axis).len());
            let thickness = self.axis(axis.other()).header;
            let tiers = tier_ranges(&self.axis(axis).tier_sizes, levels, thickness);
            let fields: Vec<Option<Key>> = (grouping.map_or(&[][..], |g| g.levels(axis)).iter())
                .map(|l| l.field_key())
                .collect();
            let mut spans = Vec::new();
            if levels > 0 {
                let st = self.axis(axis);
                let strips: [(&[StripItem], Range<f32>); 3] = [
                    (
                        &st.pinned[Edge::Leading.index()],
                        st.regions[Edge::Leading.index()].clone(),
                    ),
                    (&st.strip, scrolling(st)),
                    (
                        &st.pinned[Edge::Trailing.index()],
                        st.regions[Edge::Trailing.index()].clone(),
                    ),
                ];
                for (items, region) in strips {
                    for (tier, across) in tiers.iter().enumerate() {
                        self.strip_spans(
                            axis,
                            items,
                            &region,
                            tier,
                            across,
                            fields[tier],
                            &mut spans,
                        );
                    }
                }
            }
            self.tiers[axis.index()] = tiers;
            self.spans[axis.index()] = spans;
        }
    }

    /// The spans of one Strip at `tier`: runs of neighbouring items under
    /// one group, drawn within `region` along `axis` and `across` the band.
    #[allow(clippy::too_many_arguments)]
    fn strip_spans(
        &self,
        axis: Axis,
        items: &[StripItem],
        region: &Range<f32>,
        tier: usize,
        across: &Range<f32>,
        field: Option<Key>,
        out: &mut Vec<HeaderSpan>,
    ) {
        let mut i = 0;
        while i < items.len() {
            let Some(key) = self.span_key(axis, items[i].position, tier) else {
                i += 1;
                continue;
            };
            let mut j = i + 1;
            while j < items.len() && self.span_key(axis, items[j].position, tier) == Some(key) {
                j += 1;
            }
            let (first, last) = (&items[i], &items[j - 1]);
            let (start, end) = (first.offset, last.end());
            let (lo, hi) = (start.max(region.start), end.min(region.end));
            if lo < hi {
                let continues = |p: Option<Position>| {
                    p.is_some_and(|p| {
                        p < self.axis(axis).measure.count()
                            && self.span_key(axis, p, tier) == Some(key)
                    })
                };
                let open = match self.applied_opens(axis, tier as u32, key) {
                    true => OpenState::Open,
                    false => OpenState::Closed,
                };
                out.push(HeaderSpan {
                    axis,
                    tier: tier as u32,
                    key,
                    field,
                    open,
                    first: first.position,
                    last: last.position,
                    rect: band_rect(axis, start..end, across.clone()),
                    visible: band_rect(axis, lo..hi, across.clone()),
                    cut_before: start < region.start || continues(first.position.checked_sub(1)),
                    cut_after: end > region.end || continues(last.position.checked_add(1)),
                });
            }
            i = j;
        }
    }
}

/// Where the scrolling Strip of `st` draws, as [`crate::Snapshot::scrolling`] says.
fn scrolling(st: &AxisState) -> Range<f32> {
    let end = st.scroll_end().clamp(0.0, st.extent);
    st.scroll_start().min(end)..end
}

/// A rectangle in the Header Band heading `axis`'s items: `along` the axis
/// and `across` the band.
pub(crate) fn band_rect(axis: Axis, along: Range<f32>, across: Range<f32>) -> Rect {
    let (along, across) = (
        along.start..along.end.max(along.start),
        across.start..across.end.max(across.start),
    );
    match axis {
        Axis::Rows => Rect {
            x: across.start,
            y: along.start,
            width: across.end - across.start,
            height: along.end - along.start,
        },
        Axis::Cols => Rect {
            x: along.start,
            y: across.start,
            width: along.end - along.start,
            height: across.end - across.start,
        },
    }
}
