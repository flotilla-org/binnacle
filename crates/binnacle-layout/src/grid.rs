//! A Layout read from a Grid, and restored onto one, through the host's
//! [`IdMap`].

use binnacle_core as core;
use binnacle_core::{Axis, Edge, Grid, Key, Snapshot};

use crate::view::{Level, Searched, ViewSpec};
use crate::{Bands, EdgePins, IdMap, Layout, Order, Pins, Sizes, Version};

impl<RowId: Clone, ColId: Clone> Layout<RowId, ColId> {
    /// The layout of `grid` as it stands, every part set: the View Spec
    /// last asked for (else the one applied), the pins, dormant or not,
    /// every Size Override and the band thicknesses. An axis's order is
    /// the IDs of its items in Position order, written while the Snapshot
    /// draws every one of them and neither a sort nor a grouping orders
    /// them, and empty otherwise. Whatever names a Key `ids` does not know
    /// is left out; [`Layout::keeping`] puts back what a restored layout
    /// held for those.
    pub fn read(grid: &mut Grid, ids: &impl IdMap<RowId, ColId>) -> Self {
        let sizes = Axis::ALL.map(|axis| grid.size_overrides(axis).collect::<Vec<_>>());
        let s = grid.snapshot();
        let spec = s.requested_view().unwrap_or(s.applied_view());
        let row = |k: Key| ids.row_id(k);
        let col = |k: Key| ids.col_id(k);
        let order = |axis: Axis| match sorted_or_grouped(spec, axis) {
            true => Vec::new(),
            false => order_of(&s, axis).unwrap_or_default(),
        };
        let pins = |axis: Axis| Edge::ALL.map(|edge| s.pins(axis, edge).collect::<Vec<_>>());
        let [rows_pins, cols_pins] = Axis::ALL.map(pins);
        let [rows_sizes, cols_sizes] = sizes;
        Layout {
            version: Version,
            view: Some(ViewSpec::from_keys(spec, ids)),
            order: Some(Order {
                rows: named(order(Axis::Rows), row),
                cols: named(order(Axis::Cols), col),
            }),
            pins: Some(Pins {
                rows: edge_pins(&rows_pins, row),
                cols: edge_pins(&cols_pins, col),
            }),
            sizes: Some(Sizes {
                rows: sized(&rows_sizes, row),
                cols: sized(&cols_sizes, col),
            }),
            bands: Some(Bands {
                rows: f64::from(s.header(Axis::Rows)),
                cols: f64::from(s.header(Axis::Cols)),
            }),
        }
    }

    /// Puts this layout back on `grid`, each part it sets: the pins and
    /// the band thicknesses at once, the Size Overrides at once (clearing
    /// those it does not list), the View Spec as an ordinary request, and
    /// the order as a move of the listed items to the front, through the
    /// Table Source's move path ([`Grid::request_move`]), unless they are
    /// in place already. Whatever names an ID `ids` does not know is
    /// skipped, and this document keeps it.
    ///
    /// Returns the IDs of the order it could not place: those `ids` does
    /// not know, and every one on an axis the View Spec sorts or groups,
    /// which a move would undo. The Table Source may still refuse the move
    /// it asks for, as it answers the `MoveRequested`. Drain the Grid's
    /// Effects afterwards.
    pub fn restore(&self, grid: &mut Grid, ids: &impl IdMap<RowId, ColId>) -> Order<RowId, ColId> {
        let row = |id: &RowId| ids.row_key(id);
        let col = |id: &ColId| ids.col_key(id);
        if let Some(pins) = &self.pins {
            for edge in Edge::ALL {
                grid.set_pins(Axis::Rows, edge, &keys(pins.rows.edge(edge), row));
                grid.set_pins(Axis::Cols, edge, &keys(pins.cols.edge(edge), col));
            }
        }
        if let Some(sizes) = &self.sizes {
            set_sizes(grid, Axis::Rows, &sizes.rows, row);
            set_sizes(grid, Axis::Cols, &sizes.cols, col);
        }
        if let Some(bands) = &self.bands {
            grid.set_header(Axis::Rows, bands.rows as f32);
            grid.set_header(Axis::Cols, bands.cols as f32);
        }
        if let Some(view) = &self.view {
            grid.request_view(view.keys(ids));
        }
        let mut unplaced = Order::default();
        let Some(order) = &self.order else {
            return unplaced;
        };
        let spec = match &self.view {
            Some(view) => view.keys(ids),
            None => {
                let s = grid.snapshot();
                s.requested_view().unwrap_or(s.applied_view()).clone()
            }
        };
        unplaced.rows = place(grid, &spec, Axis::Rows, &order.rows, row);
        unplaced.cols = place(grid, &spec, Axis::Cols, &order.cols, col);
        unplaced
    }
}

impl<RowId: Clone + PartialEq, ColId: Clone + PartialEq> Layout<RowId, ColId> {
    /// This layout, read from a Grid, with what `before` (the layout
    /// restored onto it) held for IDs `ids` does not know put back, since
    /// the Grid never held them. A list the user left as it was restored
    /// (a sort, the conditions, the hidden items, a quick filter's items,
    /// the levels, values, slicers and open groups, each edge's pins) comes
    /// back whole; one they changed stays as it is now. Sizes for unknown
    /// IDs come back, and so do unknown IDs in an order that still starts
    /// with the known ones `before` listed, or that is empty now.
    pub fn keeping(mut self, before: &Self, ids: &impl IdMap<RowId, ColId>) -> Self {
        let row = |id: &RowId| ids.row_key(id).is_some();
        let col = |id: &ColId| ids.col_key(id).is_some();
        if let (Some(now), Some(was)) = (&mut self.view, &before.view) {
            keep_axis(&mut now.rows, &was.rows, col, row);
            keep_axis(&mut now.cols, &was.cols, row, col);
            if let (Some(now), Some(was)) = (&mut now.grouping, &was.grouping) {
                let level = |l: &Level<ColId>| match l {
                    Level::Field { field, .. } => col(field),
                    Level::Values { .. } => true,
                };
                keep(&mut now.row_levels, &was.row_levels, level);
                keep(&mut now.col_levels, &was.col_levels, level);
                keep(&mut now.values, &was.values, |v| {
                    v.field.as_ref().is_none_or(col)
                });
                keep(&mut now.slicers, &was.slicers, |c| col(&c.id));
                keep(
                    &mut now.row_axis.expand.except,
                    &was.row_axis.expand.except,
                    row,
                );
                keep(
                    &mut now.col_axis.expand.except,
                    &was.col_axis.expand.except,
                    col,
                );
            }
        }
        if let (Some(now), Some(was)) = (&mut self.order, &before.order) {
            keep_order(&mut now.rows, &was.rows, row);
            keep_order(&mut now.cols, &was.cols, col);
        }
        if let (Some(now), Some(was)) = (&mut self.pins, &before.pins) {
            keep_pins(&mut now.rows, &was.rows, row);
            keep_pins(&mut now.cols, &was.cols, col);
        }
        if let (Some(now), Some(was)) = (&mut self.sizes, &before.sizes) {
            keep_sizes(&mut now.rows, &was.rows, row);
            keep_sizes(&mut now.cols, &was.cols, col);
        }
        self
    }
}

impl<Id> EdgePins<Id> {
    pub fn edge(&self, edge: Edge) -> &[Id] {
        match edge {
            Edge::Leading => &self.leading,
            Edge::Trailing => &self.trailing,
        }
    }

    fn edge_mut(&mut self, edge: Edge) -> &mut Vec<Id> {
        match edge {
            Edge::Leading => &mut self.leading,
            Edge::Trailing => &mut self.trailing,
        }
    }
}

/// Whether `spec` orders the items of `axis` itself, by a sort or a
/// grouping (a pivot's columns are its groups), so that a Layout's order
/// does not apply to them.
fn sorted_or_grouped(spec: &core::ViewSpec, axis: Axis) -> bool {
    let grouped = spec.grouping.as_ref().is_some_and(|g| match axis {
        Axis::Rows => !g.row_levels.is_empty() || g.pivot(),
        Axis::Cols => g.pivot(),
    });
    grouped || !spec.axis(axis).sort.is_empty()
}

/// The Keys of every item of `axis` in Position order, if the Snapshot
/// draws them all.
fn order_of(s: &Snapshot, axis: Axis) -> Option<Vec<Key>> {
    let mut drawn: Vec<(u64, Option<Key>)> = s.drawn(axis).map(|i| (i.position, i.key)).collect();
    drawn.sort_by_key(|&(p, _)| p);
    drawn.dedup_by_key(|&mut (p, _)| p);
    let all = drawn.len() as u64 == s.count(axis)
        && drawn.iter().enumerate().all(|(i, &(p, _))| p == i as u64);
    all.then(|| drawn.into_iter().map(|(_, k)| k).collect())?
}

/// Moves the items `list` names to the front of `axis` unless they are
/// there already, returning those it can't place.
fn place<Id: Clone>(
    grid: &mut Grid,
    spec: &core::ViewSpec,
    axis: Axis,
    list: &[Id],
    key: impl Fn(&Id) -> Option<Key>,
) -> Vec<Id> {
    if sorted_or_grouped(spec, axis) {
        return list.to_vec();
    }
    let unknown = list
        .iter()
        .filter(|id| key(id).is_none())
        .cloned()
        .collect();
    let keys: Vec<Key> = list.iter().filter_map(key).collect();
    let s = grid.snapshot();
    let at = |p: u64| s.drawn(axis).find(|i| i.position == p).and_then(|i| i.key);
    let in_place = (keys.iter().enumerate()).all(|(p, &k)| at(p as u64) == Some(k));
    if !in_place {
        grid.request_move(axis, keys, 0);
    }
    unknown
}

fn set_sizes<Id>(
    grid: &mut Grid,
    axis: Axis,
    sizes: &[(Id, f64)],
    key: impl Fn(&Id) -> Option<Key>,
) {
    let listed: Vec<(Key, f32)> = (sizes.iter())
        .filter_map(|(id, size)| Some((key(id)?, *size as f32)))
        .collect();
    let stale: Vec<Key> = (grid.size_overrides(axis))
        .map(|(k, _)| k)
        .filter(|k| !listed.iter().any(|(l, _)| l == k))
        .collect();
    for k in stale {
        grid.clear_size(axis, k);
    }
    for (k, size) in listed {
        grid.set_size(axis, k, size);
    }
}

fn keys<Id>(ids: &[Id], key: impl Fn(&Id) -> Option<Key>) -> Vec<Key> {
    ids.iter().filter_map(key).collect()
}

fn named<Id>(keys: Vec<Key>, id: impl Fn(Key) -> Option<Id>) -> Vec<Id> {
    keys.into_iter().filter_map(id).collect()
}

fn edge_pins<Id>(pins: &[Vec<Key>; 2], id: impl Fn(Key) -> Option<Id> + Copy) -> EdgePins<Id> {
    let [leading, trailing] = pins.clone();
    EdgePins {
        leading: named(leading, id),
        trailing: named(trailing, id),
    }
}

fn sized<Id>(sizes: &[(Key, f32)], id: impl Fn(Key) -> Option<Id>) -> Vec<(Id, f64)> {
    (sizes.iter())
        .filter_map(|&(k, size)| Some((id(k)?, f64::from(size))))
        .collect()
}

/// `now` becomes `before` if it is `before` less the items `known` does
/// not know.
fn keep<T: Clone + PartialEq>(now: &mut Vec<T>, before: &[T], known: impl Fn(&T) -> bool) {
    let mut known_before = before.iter().filter(|t| known(t));
    let same = now.len() == known_before.clone().count()
        && now.iter().all(|t| known_before.next() == Some(t));
    if same {
        *now = before.to_vec();
    }
}

fn keep_axis<CrossId: Clone + PartialEq, Id: Clone + PartialEq>(
    now: &mut crate::view::AxisView<CrossId, Id>,
    before: &crate::view::AxisView<CrossId, Id>,
    cross: impl Fn(&CrossId) -> bool + Copy,
    own: impl Fn(&Id) -> bool,
) {
    keep(&mut now.sort, &before.sort, |c| cross(&c.id));
    keep(&mut now.filter.conditions, &before.filter.conditions, |c| {
        cross(&c.id)
    });
    keep(&mut now.hidden, &before.hidden, own);
    if let (Some(now), Some(was)) = (&mut now.filter.quick, &before.filter.quick)
        && (&now.text, now.case_sensitive, now.regex) == (&was.text, was.case_sensitive, was.regex)
        && let (Searched::Items(now), Searched::Items(was)) = (&mut now.searches, &was.searches)
    {
        keep(now, was, cross);
    }
}

fn keep_pins<Id: Clone + PartialEq>(
    now: &mut EdgePins<Id>,
    before: &EdgePins<Id>,
    known: impl Fn(&Id) -> bool + Copy,
) {
    for edge in Edge::ALL {
        keep(now.edge_mut(edge), before.edge(edge), known);
    }
}

fn keep_sizes<Id: Clone + PartialEq>(
    now: &mut Vec<(Id, f64)>,
    before: &[(Id, f64)],
    known: impl Fn(&Id) -> bool,
) {
    for (id, size) in before {
        if !known(id) && !now.iter().any(|(n, _)| n == id) {
            now.push((id.clone(), *size));
        }
    }
}

/// `now`, an order read from the Grid, with `before`'s unknown IDs back:
/// `before` itself while `now` is empty, else `before` followed by the
/// rest of `now` while `now` starts with `before`'s known IDs.
fn keep_order<Id: Clone + PartialEq>(
    now: &mut Vec<Id>,
    before: &[Id],
    known: impl Fn(&Id) -> bool,
) {
    if now.is_empty() {
        *now = before.to_vec();
        return;
    }
    let listed: Vec<&Id> = before.iter().filter(|id| known(id)).collect();
    if now.len() >= listed.len() && now.iter().zip(&listed).all(|(a, b)| a == *b) {
        let rest = now.split_off(listed.len());
        *now = before.to_vec();
        now.extend(rest);
    }
}
