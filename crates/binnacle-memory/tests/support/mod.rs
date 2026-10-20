//! The naive model of the spec's sort and filter that the property files
//! check the In-Memory Table Source against.

use binnacle_core::{Axis, ChangeNotice, SortDirection, SortSpec};

/// Base rows `0..rows` in the spec's order: sorted stably by their tuple of
/// ranks in Sort Spec order, so ties keep base order, then the rows `keep`
/// drops skipped. `rank(column, row)` ranks row `row`'s value in the column
/// with Logical Index `column` by class (0 for a value, 1 for NaN, 2 for
/// null), then by order within the class; a descending column reverses only
/// the order within the class, so nulls and NaNs go last either way.
// Not every property file that shares this module uses both.
#[allow(dead_code)]
pub fn model(
    rows: usize,
    spec: &SortSpec,
    rank: impl Fn(usize, usize) -> (u8, i128),
    keep: impl Fn(usize) -> bool,
) -> Vec<usize> {
    model_from((0..rows).collect(), spec, rank, keep)
}

/// [`model`] from the base order `base` (Logical Indices, all of them, in
/// the order moves left them) rather than Logical Index order: ties keep
/// `base`'s order.
pub fn model_from(
    base: Vec<usize>,
    spec: &SortSpec,
    rank: impl Fn(usize, usize) -> (u8, i128),
    keep: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let ranks: Vec<Vec<(u8, i128)>> = (0..base.len())
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
    let mut order = base;
    order.sort_by_key(|&i| ranks[i].clone());
    order.retain(|&i| keep(i));
    order
}

/// The rows' Change Notice among `notices`, from a source with no filter on
/// its columns, which tells the columns nothing.
#[allow(dead_code)]
pub fn rows_only(notices: Vec<(Axis, ChangeNotice)>) -> Option<ChangeNotice> {
    assert!(notices.len() <= 1, "one notice at most: {notices:?}");
    notices
        .into_iter()
        .map(|(axis, notice)| {
            assert_eq!(axis, Axis::Rows);
            notice
        })
        .next()
}
