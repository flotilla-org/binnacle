//! Text placed in a ratatui buffer. Measuring and cutting text is
//! `binnacle_cli::width`.

use ratatui::buffer::Buffer;
use ratatui::style::Style;
use unicode_segmentation::UnicodeSegmentation;

use binnacle_cli::width::grapheme_width;
pub(crate) use binnacle_cli::width::{fit, pad, tail, width};

/// Writes `text` on buffer row `y` from column `x`, one grapheme a step of
/// its width, into the columns `min..max` only, and returns the column after
/// the last grapheme. A zero-width grapheme joins the cell before it. A wide
/// grapheme that is not wholly inside `min..max` draws a space in the columns
/// that are. A zero-width grapheme with no cell before it is dropped.
pub(crate) fn write(
    buf: &mut Buffer,
    (x, y): (i64, u16),
    (min, max): (i64, i64),
    text: &str,
    style: Style,
) -> i64 {
    let mut at = x;
    let mut last = None;
    for g in text.graphemes(true) {
        let w = i64::from(grapheme_width(g));
        if w == 0 {
            if let Some(cell) = last.and_then(|col| buf.cell_mut((col, y))) {
                let joined = format!("{}{g}", cell.symbol());
                cell.set_symbol(&joined);
            }
            continue;
        }
        let whole = at >= min && at + w <= max;
        for col in at.max(min)..(at + w).min(max) {
            let Ok(col) = u16::try_from(col) else {
                continue;
            };
            let Some(cell) = buf.cell_mut((col, y)) else {
                continue;
            };
            if whole && i64::from(col) == at {
                cell.set_symbol(g);
                last = Some(col);
            } else {
                cell.set_symbol(" ");
            }
            cell.set_style(style);
        }
        if !whole {
            last = None;
        }
        at = at.saturating_add(w);
    }
    at
}

/// Writes `text` on row `y` from column `*x` up to `right`, and moves `*x`
/// past it.
pub(crate) fn put_text(
    buf: &mut Buffer,
    x: &mut u16,
    y: u16,
    right: u16,
    text: &str,
    style: Style,
) {
    let next = write(buf, (i64::from(*x), y), (0, i64::from(right)), text, style);
    *x = u16::try_from(next.min(i64::from(right))).unwrap_or(right);
}
