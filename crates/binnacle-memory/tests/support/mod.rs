//! The naive model of the spec's sort and filter that the property files
//! check the In-Memory Table Source against.

use binnacle_core::{Axis, ChangeNotice, SortDirection, SortSpec};

/// Base rows `0..rows` in the spec's order: sorted by their tuple of ranks in
/// Sort Spec order, then by `key` ascending whichever way the columns run,
/// then the rows `keep` drops skipped (an empty spec keeps the base order). `rank(column, row)` ranks row `row`'s
/// value in the column with Logical Index `column` by class (0 for null, 1
/// for a value, 2 for NaN), then by order within the class; a descending
/// column reverses the whole order, nulls included.
// Not every property file that shares this module uses both.
#[allow(dead_code)]
pub fn model(
    rows: usize,
    spec: &SortSpec,
    rank: impl Fn(usize, usize) -> (u8, i128),
    key: impl Fn(usize) -> u64,
    keep: impl Fn(usize) -> bool,
) -> Vec<usize> {
    model_from((0..rows).collect(), spec, rank, key, keep)
}

/// [`model`] from the base order `base` (Logical Indices, all of them) rather
/// than Logical Index order.
pub fn model_from(
    base: Vec<usize>,
    spec: &SortSpec,
    rank: impl Fn(usize, usize) -> (u8, i128),
    key: impl Fn(usize) -> u64,
    keep: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let ranks: Vec<Vec<(i16, i128)>> = (0..base.len())
        .map(|i| {
            spec.columns()
                .iter()
                .map(|c| {
                    let (class, rank) = rank(c.key.0 as usize, i);
                    match c.direction {
                        SortDirection::Ascending => (i16::from(class), rank),
                        SortDirection::Descending => (-i16::from(class), -rank),
                    }
                })
                .collect()
        })
        .collect();
    let mut order = base;
    // An empty spec keeps the base order.
    if !spec.is_empty() {
        order.sort_by_key(|&i| (ranks[i].clone(), key(i)));
    }
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
