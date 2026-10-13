//! Border Styles: the lines drawn in the Gaps the Grid lays out.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use binnacle_core::{Axis, Snapshot, StripItem};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::{Theme, cell};

/// Which lines are drawn in the Gaps, in what weight, and whether each
/// Header Band ends in a rule. The default is today's look: light handles
/// in the header bands' Gaps only, and no band rules.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderStyle {
    pub extent: BorderExtent,
    pub weight: BorderWeight,
    /// A rule in the last cell of each Header Band, under the column headers
    /// and beside the row numbers, with the band edge handle where it
    /// crosses the other band.
    pub band_rules: bool,
}

/// How far the lines reach.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderExtent {
    /// Gaps stay blank.
    None,
    /// Handles in the header bands' Gaps only.
    #[default]
    Headers,
    /// Handles, and column lines down the body.
    Columns,
    /// Column lines, and rules between rows (where the row Gap is at least
    /// one cell), joined at the crossings.
    Grid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderWeight {
    #[default]
    Light,
    Heavy,
    Double,
}

impl BorderStyle {
    /// Lines in the header band only: the handles a resize grabs.
    pub fn headers() -> Self {
        BorderStyle::default()
    }

    pub const fn new(extent: BorderExtent, weight: BorderWeight) -> Self {
        BorderStyle {
            extent,
            weight,
            band_rules: false,
        }
    }

    pub const fn band_rules(self, band_rules: bool) -> Self {
        BorderStyle {
            extent: self.extent,
            weight: self.weight,
            band_rules,
        }
    }
}

/// Names the style as `<extent> <weight>`, then ` + band rules` when on.
impl fmt::Display for BorderStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let extent = match self.extent {
            BorderExtent::None => "none",
            BorderExtent::Headers => "headers",
            BorderExtent::Columns => "columns",
            BorderExtent::Grid => "grid",
        };
        let weight = match self.weight {
            BorderWeight::Light => "light",
            BorderWeight::Heavy => "heavy",
            BorderWeight::Double => "double",
        };
        write!(f, "{extent} {weight}")?;
        if self.band_rules {
            write!(f, " + band rules")?;
        }
        Ok(())
    }
}

/// The glyphs of a weight: vertical, horizontal, then the junctions indexed
/// by their arms (up 8, down 4, left 2, right 1).
fn glyphs(weight: BorderWeight) -> (char, char, [char; 16]) {
    match weight {
        BorderWeight::Light => (
            '│',
            '─',
            [
                ' ', '─', '─', '─', '│', '┌', '┐', '┬', '│', '└', '┘', '┴', '│', '├', '┤', '┼',
            ],
        ),
        BorderWeight::Heavy => (
            '┃',
            '━',
            [
                ' ', '━', '━', '━', '┃', '┏', '┓', '┳', '┃', '┗', '┛', '┻', '┃', '┣', '┫', '╋',
            ],
        ),
        BorderWeight::Double => (
            '║',
            '═',
            [
                ' ', '═', '═', '═', '║', '╔', '╗', '╦', '║', '╚', '╝', '╩', '║', '╠', '╣', '╬',
            ],
        ),
    }
}

/// One cell's share of the lines: which ways they run and leave it.
#[derive(Clone, Copy, Default)]
struct Mark {
    vertical: bool,
    horizontal: bool,
    arms: u8,
    handle: bool,
}

const UP: u8 = 8;
const DOWN: u8 = 4;
const LEFT: u8 = 2;
const RIGHT: u8 = 1;

/// The lines of one frame, in cells relative to the area. A line reaching
/// the area's edge is taken to run on past it.
struct Lines {
    size: (i64, i64),
    marks: HashMap<(i64, i64), Mark>,
}

impl Lines {
    /// A line down `x` over `ys`; its cells above `top` are handles.
    fn vertical(&mut self, x: i64, ys: Range<i64>, top: i64) {
        let (w, h) = self.size;
        if !(0..w).contains(&x) {
            return;
        }
        for y in ys.start.max(0)..ys.end.min(h) {
            let m = self.marks.entry((x, y)).or_default();
            m.vertical = true;
            m.handle |= y < top;
            if y > ys.start || y == 0 {
                m.arms |= UP;
            }
            if y + 1 < ys.end || y + 1 == h {
                m.arms |= DOWN;
            }
        }
    }

    /// A line along `y` over `xs`; its cells left of `left` are handles.
    fn horizontal(&mut self, y: i64, xs: Range<i64>, left: i64) {
        let (w, h) = self.size;
        if !(0..h).contains(&y) {
            return;
        }
        for x in xs.start.max(0)..xs.end.min(w) {
            let m = self.marks.entry((x, y)).or_default();
            m.horizontal = true;
            m.handle |= x < left;
            if x > xs.start || x == 0 {
                m.arms |= LEFT;
            }
            if x + 1 < xs.end || x + 1 == w {
                m.arms |= RIGHT;
            }
        }
    }
}

/// Where a line sits after `item`: the first cell past its end, if that is
/// a Gap cell. Past the axis's last item there is no Gap, but `header`
/// still gives the cell, for the handle that resizes the last item.
fn gap_cell(item: &StripItem, gap: f32, count: u64, header: bool) -> Option<i64> {
    let at = cell(item.end());
    let last = item.position.saturating_add(1) >= count;
    if gap <= 0.0 {
        None
    } else if last {
        header.then_some(at)
    } else {
        (at < cell(item.end() + gap)).then_some(at)
    }
}

/// Draws `style`'s lines for `snapshot` into `area` of `buf`, in `theme`'s
/// `handle` style within the header bands and `border` style in the body.
pub(crate) fn draw(
    snapshot: &Snapshot,
    style: BorderStyle,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let (top_f, left_f) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    let (top, left) = (cell(top_f), cell(left_f));
    let (rows, cols) = (snapshot.rows(), snapshot.cols());
    // Where the body's lines stop: the last item's end, or past the Gap
    // after it when more items follow out of view.
    let end = |axis: Axis, strip: &[StripItem], header: i64| {
        let count = snapshot.count(axis);
        strip.last().map_or(header, |it| {
            let more = it.position.saturating_add(1) < count;
            cell(it.end() + if more { snapshot.gap(axis) } else { 0.0 }).max(header)
        })
    };
    let (bottom, right) = (end(Axis::Rows, rows, top), end(Axis::Cols, cols, left));
    let mut lines = Lines {
        size: (i64::from(area.width), i64::from(area.height)),
        marks: HashMap::new(),
    };
    let handles = style.extent != BorderExtent::None;
    let columns = matches!(style.extent, BorderExtent::Columns | BorderExtent::Grid);
    let grid = style.extent == BorderExtent::Grid;
    if handles {
        let (gap, count) = (snapshot.gap(Axis::Cols), snapshot.count(Axis::Cols));
        for c in cols.iter().filter(|c| c.end() >= left_f) {
            let body = columns && gap_cell(c, gap, count, false).is_some();
            if let Some(x) = gap_cell(c, gap, count, true) {
                lines.vertical(x, 0..if body { bottom } else { top }, top);
            }
        }
        let (gap, count) = (snapshot.gap(Axis::Rows), snapshot.count(Axis::Rows));
        for r in rows.iter().filter(|r| r.end() >= top_f) {
            let body = grid && gap_cell(r, gap, count, false).is_some();
            if let Some(y) = gap_cell(r, gap, count, true) {
                lines.horizontal(y, 0..if body { right } else { left }, left);
            }
        }
    }
    if style.band_rules {
        if left > 0 {
            lines.vertical(left - 1, 0..bottom, top);
        }
        if top > 0 {
            lines.horizontal(top - 1, 0..right, left);
        }
    }
    let (vertical, horizontal, junctions) = glyphs(style.weight);
    for ((x, y), m) in lines.marks {
        let ch = match (m.vertical, m.horizontal) {
            (true, false) => vertical,
            (false, true) => horizontal,
            _ => {
                let mut arms = m.arms;
                if arms & (UP | DOWN) == 0 {
                    arms |= UP | DOWN;
                }
                if arms & (LEFT | RIGHT) == 0 {
                    arms |= LEFT | RIGHT;
                }
                junctions[usize::from(arms)]
            }
        };
        let style = if m.handle { theme.handle } else { theme.border };
        if let Some(c) = buf.cell_mut((area.x + x as u16, area.y + y as u16)) {
            c.set_char(ch).set_style(style);
        }
    }
}
