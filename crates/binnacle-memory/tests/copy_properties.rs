//! Seam 2 property for copy: runs of Positions over the In-Memory Table
//! Source, both axes filtered, gathered and written as text, against the
//! cells and labels a naive model reads off the source's Windows. Cells
//! hold tabs, line breaks and quotes, so the text must be unquoted to
//! give them back.

use std::ops::Range;

use binnacle_core::{Axis, Key, Outline, Position};
use binnacle_memory::InMemoryTableSource;
use binnacle_memory::copy::{CopyText, gather};
use hegel::TestCase;
use hegel::generators as gs;

const MAX: usize = 6;

#[hegel::composite]
fn text(tc: &TestCase) -> String {
    let chars = gs::sampled_from(vec!['a', 'b', ' ', ',', '\t', '\n', '\r', '"']);
    tc.draw(gs::vecs(chars).max_size(4)).into_iter().collect()
}

/// Cells and row headers by Logical Index.
struct Table {
    cells: Vec<Vec<String>>,
    headers: Vec<String>,
}

impl CopyText for Table {
    fn cell(&self, row: Key, col: Key) -> String {
        self.cells[row.0 as usize][col.0 as usize].clone()
    }

    fn group_label(&self, _: Key, _: &Outline) -> String {
        unreachable!("the In-Memory Table Source is not grouped")
    }

    fn row_header(&self, row: Key, _: Position) -> String {
        self.headers[row.0 as usize].clone()
    }
}

/// Runs of Positions on an axis of `count` items, some reaching past it.
#[hegel::composite]
fn runs(tc: &TestCase, count: usize) -> Vec<Range<Position>> {
    let bound = || gs::integers::<u64>().max_value(count as u64 + 2);
    let ends = tc.draw(gs::vecs(hegel::tuples!(bound(), bound())).max_size(3));
    ends.into_iter().map(|(a, b)| a.min(b)..a.max(b)).collect()
}

/// The fields of tab-separated `text` as a spreadsheet reads them: a field
/// opening with a quote runs to the next quote not doubled, and a line
/// breaks at a line feed or a carriage return.
fn unquote(text: &str) -> Vec<Vec<String>> {
    let mut lines = vec![vec![String::new()]];
    let mut chars = text.chars().peekable();
    let mut quoted = false;
    while let Some(c) = chars.next() {
        let field = lines.last_mut().unwrap().last_mut().unwrap();
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            '\t' if !quoted => lines.last_mut().unwrap().push(String::new()),
            '\n' | '\r' if !quoted => lines.push(vec![String::new()]),
            c => field.push(c),
        }
    }
    lines
}

#[hegel::test(test_cases = 1000)]
fn copied_text_unquotes_to_the_cells_of_the_rows_and_columns_gathered(tc: TestCase) {
    let rows = tc.draw(gs::integers::<usize>().max_value(MAX));
    let cols = tc.draw(gs::integers::<usize>().max_value(MAX));
    let table = Table {
        cells: (0..rows)
            .map(|_| tc.draw(gs::vecs(text()).min_size(cols).max_size(cols)))
            .collect(),
        headers: tc.draw(gs::vecs(text()).min_size(rows).max_size(rows)),
    };
    let mut source = InMemoryTableSource::new(rows, cols);
    for (axis, n) in [(Axis::Rows, rows), (Axis::Cols, cols)] {
        let keep = tc.draw(gs::vecs(gs::booleans()).min_size(n).max_size(n));
        source.apply_filter(axis, keep);
    }
    let row_runs = tc.draw(runs(source.count(Axis::Rows) as usize));
    let col_runs = tc.draw(runs(source.count(Axis::Cols) as usize));
    let labels = tc.draw(gs::booleans());
    let copied = gather(&row_runs, &col_runs, labels, |axis, run| {
        (source.window(axis, run), None)
    })
    .expect("within the cap");

    // The model: each run's Window, in order.
    let shown = |axis, runs: &[Range<Position>]| -> Vec<(Key, Position)> {
        (runs.iter())
            .flat_map(|r| source.window(axis, r.clone()).into_iter().zip(r.clone()))
            .collect()
    };
    let want_rows = shown(Axis::Rows, &row_runs);
    let want_cols: Vec<Key> = (shown(Axis::Cols, &col_runs).into_iter())
        .map(|(k, _)| k)
        .collect();
    let got_rows: Vec<(Key, Position)> =
        (copied.rows.iter()).map(|r| (r.key, r.position)).collect();
    assert_eq!(got_rows, want_rows);
    assert_eq!(copied.cols, want_cols);

    let want: Vec<Vec<String>> = (want_rows.iter())
        .map(|&(row, _)| {
            let label = labels.then(|| table.headers[row.0 as usize].clone());
            let cells = want_cols.iter().map(|&col| table.cell(row, col));
            label.into_iter().chain(cells).collect()
        })
        .collect();
    let text = copied.text(&table, None);
    let fields = want_cols.len() + usize::from(labels);
    if want.is_empty() || fields == 0 {
        // No fields to read back: as many empty lines as rows.
        assert_eq!(text, "\n".repeat(want.len().saturating_sub(1)));
        return;
    }
    let got = unquote(&text);
    assert_eq!(got.len(), want_rows.len(), "{text:?}");
    assert!(got.iter().all(|line| line.len() == fields), "{text:?}");
    assert_eq!(got, want, "{text:?}");
}
