//! The rules a copy follows, shared by every host so each copies the same
//! text: what a `CopyRequested` Effect names, gathered from the Table Source
//! as the grid shows it ([`gather`]) and written as tab-separated lines
//! ([`copy_text`]). Each copied row is the item its Position holds, so a
//! collapsed group's items, which hold no Position, are left out. On grouped
//! rows each row leads with its label from the row Header Band: a Summary
//! Item's group label without its indent or disclosure mark, else its row
//! header. An open Group Item whose Subtotal is placed copies no figures, as
//! it draws none.
//!
//! The text is the cells as drawn, not raw values: the host says how each
//! reads through [`CopyText`]. The Effect's `col_labels` is not written yet,
//! as no host copies column labels.

use std::ops::Range;

use binnacle_core::{Axis, Grouping, Key, Outline, Position};

/// The most cells one copy gathers; a bigger selection copies nothing.
pub const COPY_LIMIT: u64 = 1_000_000;

/// One copied row: its Key, its Position and, on a grouped axis, its outline.
#[derive(Clone, Debug, PartialEq)]
pub struct CopiedRow {
    pub key: Key,
    pub position: Position,
    pub outline: Option<Outline>,
}

/// What a copy gathered: the rows and column Keys its runs hold, and whether
/// each row leads with its label.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Copied {
    pub rows: Vec<CopiedRow>,
    pub cols: Vec<Key>,
    pub row_labels: bool,
}

impl Copied {
    /// The copy as tab-separated lines; see [`copy_text`].
    pub fn text(&self, text: &(impl CopyText + ?Sized), grouping: Option<&Grouping>) -> String {
        copy_text(text, &self.rows, &self.cols, self.row_labels, grouping)
    }
}

/// How the host draws what a copy writes.
pub trait CopyText {
    /// The text of the cell where `row` crosses `col`, as drawn.
    fn cell(&self, row: Key, col: Key) -> String;

    /// A Summary Item's label in the row Header Band, without its indent or
    /// disclosure mark.
    fn group_label(&self, row: Key, outline: &Outline) -> String;

    /// A Leaf's label in the row Header Band, or a plain row's.
    fn row_header(&self, row: Key, position: Position) -> String;
}

/// How many Positions `runs` hold.
fn count(runs: &[Range<Position>]) -> u64 {
    runs.iter().map(|r| r.end.saturating_sub(r.start)).sum()
}

/// Gathers the rows and columns at runs of Positions `rows` and `cols`
/// through `window`, which answers a run of an axis with the Keys there and,
/// on a grouped axis, their outlines. `None` past [`COPY_LIMIT`] cells, with
/// no window asked for. A run's Keys take its Positions in order, so a
/// Window shorter than its run (one reaching past the count) gives fewer
/// rows or columns.
pub fn gather(
    rows: &[Range<Position>],
    cols: &[Range<Position>],
    row_labels: bool,
    mut window: impl FnMut(Axis, Range<Position>) -> (Vec<Key>, Option<Vec<Outline>>),
) -> Option<Copied> {
    if count(rows).saturating_mul(count(cols)) > COPY_LIMIT {
        return None;
    }
    let mut copied_rows = Vec::new();
    for run in rows {
        let (keys, outlines) = window(Axis::Rows, run.clone());
        let mut outlines = outlines.map(Vec::into_iter);
        for (key, position) in keys.into_iter().zip(run.clone()) {
            let outline = outlines.as_mut().and_then(Iterator::next);
            copied_rows.push(CopiedRow {
                key,
                position,
                outline,
            });
        }
    }
    let cols = (cols.iter())
        .flat_map(|run| window(Axis::Cols, run.clone()).0)
        .collect();
    Some(Copied {
        rows: copied_rows,
        cols,
        row_labels,
    })
}

/// A field as a spreadsheet pastes it: quoted when it holds a tab, a line
/// break or a quote, with each quote doubled.
fn field(s: String) -> String {
    match s.contains(['\t', '\n', '\r', '"']) {
        true => format!("\"{}\"", s.replace('"', "\"\"")),
        false => s,
    }
}

/// The cells where `rows` cross `cols` as tab-separated lines, each row led
/// by its label when `row_labels` is set. Under `grouping`, an open Group
/// Item whose level places a Subtotal copies empty cells.
pub fn copy_text(
    text: &(impl CopyText + ?Sized),
    rows: &[CopiedRow],
    cols: &[Key],
    row_labels: bool,
    grouping: Option<&Grouping>,
) -> String {
    let line = |row: &CopiedRow| {
        let label = row_labels.then(|| match &row.outline {
            Some(o) if o.kind.is_summary() => text.group_label(row.key, o),
            _ => text.row_header(row.key, row.position),
        });
        let figureless = (row.outline.as_ref())
            .is_some_and(|o| grouping.is_some_and(|g| g.blank(Axis::Rows, o)));
        let cells = cols.iter().map(|&col| match figureless {
            true => String::new(),
            false => text.cell(row.key, col),
        });
        label
            .into_iter()
            .chain(cells)
            .map(field)
            .collect::<Vec<_>>()
            .join("\t")
    };
    rows.iter().map(line).collect::<Vec<_>>().join("\n")
}
