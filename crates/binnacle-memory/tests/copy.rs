//! Copy's rules as a host meets them: a cell that holds a tab, a line break
//! or a quote is quoted as a spreadsheet pastes it, grouped rows lead with
//! their labels and leave a closed group's items out, an open Group Item
//! whose Subtotal is placed copies no figures, and a copy past
//! `COPY_LIMIT` cells gives nothing and reads no window.
// Runs of Positions are slices of Ranges, often of one.
#![allow(clippy::single_range_in_vec_init)]

use std::ops::Range;

use binnacle_core::{Axis, Grouping, Key, Level, OpenState, Outline, Placement, Position, Value};
use binnacle_memory::copy::{COPY_LIMIT, CopiedRow, CopyText, copy_text, gather};

const REGION: Key = Key(10);
const PORT: Key = Key(11);

/// Leaves show their Key and column, Summary Items their figure "Σ" and
/// column; labels are a Summary Item's group value and a Leaf's Key.
struct Shipments;

impl CopyText for Shipments {
    fn cell(&self, row: Key, col: Key) -> String {
        match row.0 >= 1000 {
            true => format!("Σ{}", col.0),
            false => format!("s{}c{}", row.0, col.0),
        }
    }

    fn group_label(&self, _: Key, outline: &Outline) -> String {
        match outline.group.as_ref().map(|g| &g.value) {
            Some(Value::Text(t)) => t.clone(),
            _ => "?".into(),
        }
    }

    fn row_header(&self, row: Key, _: Position) -> String {
        format!("#{}", row.0)
    }
}

/// One cell's text, as `copy_text` writes it alone.
struct One(&'static str);

impl CopyText for One {
    fn cell(&self, _: Key, _: Key) -> String {
        self.0.into()
    }

    fn group_label(&self, _: Key, _: &Outline) -> String {
        unreachable!("no labels")
    }

    fn row_header(&self, _: Key, _: Position) -> String {
        unreachable!("no labels")
    }
}

fn one(cell: &'static str) -> String {
    let row = CopiedRow {
        key: Key(0),
        position: 0,
        outline: None,
    };
    copy_text(&One(cell), &[row], &[Key(0)], false, None)
}

#[test]
fn a_cell_with_a_tab_a_line_break_or_a_quote_is_quoted_and_a_quote_doubled() {
    assert_eq!(one("a\tb"), "\"a\tb\"");
    assert_eq!(one("a\nb"), "\"a\nb\"");
    assert_eq!(one("a\rb"), "\"a\rb\"");
    assert_eq!(one("say \"hi\""), "\"say \"\"hi\"\"\"");
    assert_eq!(one("plain, text"), "plain, text");
}

/// North (open: Aberdeen, open, with shipments 0 and 1; Bergen, closed,
/// with shipment 2) and its Subtotal, as the rows' Windows show them.
fn shown() -> Vec<(Key, Outline)> {
    let text = |t: &str| Value::Text(t.into());
    let north = Key(1002);
    vec![
        (
            north,
            Outline::group(vec![], OpenState::Open, REGION, text("North")),
        ),
        (
            Key(1000),
            Outline::group(vec![north], OpenState::Open, PORT, text("Aberdeen")),
        ),
        (Key(0), Outline::leaf(vec![north, Key(1000)])),
        (Key(1), Outline::leaf(vec![north, Key(1000)])),
        (
            Key(1001),
            Outline::group(vec![north], OpenState::Closed, PORT, text("Bergen")),
        ),
        (
            Key(1003),
            Outline::subtotal(vec![north], REGION, text("North total")),
        ),
    ]
}

/// The rows' Windows over [`shown`], the columns' keyed by Position.
fn window(axis: Axis, run: Range<Position>) -> (Vec<Key>, Option<Vec<Outline>>) {
    match axis {
        Axis::Rows => {
            let shown = shown();
            let at = &shown[run.start as usize..(run.end as usize).min(shown.len())];
            let (keys, outlines) = at.iter().cloned().unzip();
            (keys, Some(outlines))
        }
        Axis::Cols => (run.map(Key).collect(), None),
    }
}

/// North and Aberdeen placing their Subtotals, if `placed`.
fn grouping(placed: bool) -> Grouping {
    let subtotal = match placed {
        true => Placement::After,
        false => Placement::Off,
    };
    let mut grouping = Grouping::rows_by(REGION);
    grouping.row_levels = [REGION, PORT]
        .map(|field| Level::Field { field, subtotal })
        .to_vec();
    grouping
}

#[test]
fn grouped_rows_lead_with_their_labels_and_leave_a_closed_groups_items_out() {
    let copied = gather(&[0..2, 3..6], &[1..3], true, window).unwrap();
    let positions: Vec<Position> = copied.rows.iter().map(|r| r.position).collect();
    assert_eq!(positions, [0, 1, 3, 4, 5]);
    assert_eq!(copied.cols, [Key(1), Key(2)]);
    assert_eq!(
        copied.text(&Shipments, Some(&grouping(false))),
        "North\tΣ1\tΣ2\nAberdeen\tΣ1\tΣ2\n#1\ts1c1\ts1c2\nBergen\tΣ1\tΣ2\nNorth total\tΣ1\tΣ2"
    );
    // Shipment 2, in closed Bergen, holds no Position and so is no row.
    assert!(copied.rows.iter().all(|r| r.key != Key(2)));
}

#[test]
fn rows_without_labels_copy_their_cells_alone() {
    let copied = gather(&[2..4], &[0..1], false, window).unwrap();
    assert_eq!(copied.text(&Shipments, None), "s0c0\ns1c0");
}

#[test]
fn an_open_group_item_whose_subtotal_is_placed_copies_no_figures() {
    let copied = gather(&[0..6], &[1..3], true, window).unwrap();
    assert_eq!(
        copied.text(&Shipments, Some(&grouping(true))),
        // North and Aberdeen are open; closed Bergen carries its figures.
        "North\t\t\nAberdeen\t\t\n#0\ts0c1\ts0c2\n#1\ts1c1\ts1c2\nBergen\tΣ1\tΣ2\nNorth total\tΣ1\tΣ2"
    );
}

/// Gathers `rows` by `cols` cells from a window that counts its calls.
fn gather_counting(rows: u64, cols: u64) -> (bool, usize) {
    let mut asked = 0;
    let copied = gather(&[0..rows], &[0..cols], false, |_, run| {
        asked += 1;
        (run.map(Key).collect(), None)
    });
    (copied.is_some(), asked)
}

#[test]
fn a_copy_one_cell_past_the_limit_gives_nothing_and_reads_no_window() {
    assert_eq!(gather_counting(COPY_LIMIT + 1, 1), (false, 0));
    assert_eq!(gather_counting(1, COPY_LIMIT + 1), (false, 0));
    assert_eq!(gather_counting(COPY_LIMIT, 1), (true, 2));
}

#[test]
fn split_runs_count_together_against_the_limit() {
    let mut asked = 0;
    let half = COPY_LIMIT / 2;
    let copied = gather(&[0..half, half..2 * half + 1], &[0..1], false, |_, run| {
        asked += 1;
        (run.map(Key).collect(), None)
    });
    assert_eq!((copied, asked), (None, 0));
}
