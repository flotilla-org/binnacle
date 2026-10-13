//! The naive model of the spec's sort and filter that the property files
//! check the In-Memory Table Source against.

use binnacle_core::{SortDirection, SortSpec};

/// Base rows `0..rows` in the spec's order: sorted stably by their tuple of
/// ranks in Sort Spec order, so ties keep base order, then the rows `keep`
/// drops skipped. `rank(column, row)` ranks row `row`'s value in the column
/// with Logical Index `column` by class (0 for a value, 1 for NaN, 2 for
/// null), then by order within the class; a descending column reverses only
/// the order within the class, so nulls and NaNs go last either way.
pub fn model(
    rows: usize,
    spec: &SortSpec,
    rank: impl Fn(usize, usize) -> (u8, i128),
    keep: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let ranks: Vec<Vec<(u8, i128)>> = (0..rows)
        .map(|i| {
            spec.columns()
                .iter()
                .map(|c| {
                    let (class, rank) = rank(c.key.0 as usize, i);
                    match c.direction {
                        SortDirection::Ascending => (class, rank),
                        SortDirection::Descending => (class, -rank),
                    }
                })
                .collect()
        })
        .collect();
    let mut order: Vec<usize> = (0..rows).collect();
    order.sort_by_key(|&i| ranks[i].clone());
    order.retain(|&i| keep(i));
    order
}
