//! Copy: the text of the cells a `CopyRequested` Effect names, as the grid
//! draws them, for the host to put on the clipboard (an OSC 52 sequence,
//! say). The rules are [`binnacle_memory::copy`]'s, shared by every host:
//! the host gathers the Effect's runs through its Table Source with
//! [`gather`], and [`ContentText`] reads the text off its [`Content`]. On
//! grouped rows each row leads with its label from the row Header Band: a
//! Summary Item's [`Content::group_label`] without its indent or disclosure
//! mark, a Leaf's [`Content::row_header`].

use binnacle_core::{Key, Outline, Position};
pub use binnacle_memory::copy::{COPY_LIMIT, Copied, CopiedRow, CopyText, gather};

use crate::Content;

/// A [`Content`] as a copy reads it.
pub struct ContentText<'a, C: ?Sized>(pub &'a C);

impl<C: Content + ?Sized> CopyText for ContentText<'_, C> {
    fn cell(&self, row: Key, col: Key) -> String {
        self.0.cell(row, col)
    }

    fn group_label(&self, row: Key, outline: &Outline) -> String {
        self.0.group_label(row, outline)
    }

    fn row_header(&self, row: Key, position: Position) -> String {
        self.0.row_header(row, position)
    }
}

/// The cells where `rows` cross `cols` as tab-separated lines, each row
/// led by its label when `row_labels` is set. With no Grouping to say where
/// Subtotals are placed, every row copies its cells: a grouped host writes
/// through [`Copied::text`] with its applied Grouping.
pub fn copy_text(
    content: &(impl Content + ?Sized),
    rows: &[CopiedRow],
    cols: &[Key],
    row_labels: bool,
) -> String {
    binnacle_memory::copy::copy_text(&ContentText(content), rows, cols, row_labels, None)
}
