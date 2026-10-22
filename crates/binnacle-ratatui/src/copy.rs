//! Copy: the text of the cells a `CopyRequested` Effect names, as the grid
//! draws them, for the host to put on the clipboard (an OSC 52 sequence,
//! say). The host resolves the Effect's runs of Positions through its Table
//! Source, so a collapsed group's items, which hold no Position, are left
//! out. On grouped rows each row leads with its label from the row Header
//! Band: a Summary Item's [`Content::group_label`] without its indent or
//! disclosure mark, a Leaf's [`Content::row_header`].

use binnacle_core::{Key, Outline, Position};

use crate::Content;

/// One copied row: its Key, its Position and, on a grouped axis, its outline.
#[derive(Clone, Debug, PartialEq)]
pub struct CopiedRow {
    pub key: Key,
    pub position: Position,
    pub outline: Option<Outline>,
}

/// A field as a spreadsheet pastes it: quoted when it holds a tab, a line
/// break or a quote.
fn field(s: String) -> String {
    match s.contains(['\t', '\n', '\r', '"']) {
        true => format!("\"{}\"", s.replace('"', "\"\"")),
        false => s,
    }
}

/// The cells where `rows` cross `cols` as tab-separated lines, each row
/// led by its label when `row_labels` is set.
pub fn copy_text(
    content: &(impl Content + ?Sized),
    rows: &[CopiedRow],
    cols: &[Key],
    row_labels: bool,
) -> String {
    let line = |row: &CopiedRow| {
        let label = row_labels.then(|| match &row.outline {
            Some(o) if o.kind.is_summary() => content.group_label(row.key, o),
            _ => content.row_header(row.key, row.position),
        });
        let cells = cols.iter().map(|&col| content.cell(row.key, col));
        label
            .into_iter()
            .chain(cells)
            .map(field)
            .collect::<Vec<_>>()
            .join("\t")
    };
    rows.iter().map(line).collect::<Vec<_>>().join("\n")
}
