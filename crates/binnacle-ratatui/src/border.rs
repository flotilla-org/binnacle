//! Border Styles: the lines drawn in the Gaps the Grid lays out.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;

use binnacle_core::{Axis, Edge, Snapshot, StripItem};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::{Theme, cell};

/// Which lines are drawn in the Gaps, in what weight, and whether each
/// Header Band ends in a rule. The default is today's look: light handles
/// in the header bands' Gaps only, and no band rules. `Columns` and `Grid`
/// always draw the band rules and close the table after its last column
/// (and, for `Grid`, its last row).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BorderStyle {
    pub extent: BorderExtent,
    pub weight: BorderWeight,
    /// For `Headers`: a rule in the last cell of each Header Band, under the
    /// column headers and beside the row numbers, with the band edge handle
    /// where it crosses the other band. `Columns` and `Grid` draw the rules
    /// whatever this says; `None` never does.
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
    /// Handles, and column lines down the body and after the last column.
    Columns,
    /// Column lines, and rules between rows (where the row Gap is at least
    /// one cell) and under the last row, joined at the crossings.
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

    /// Whether each Header Band ends in a rule (when it is at least two
    /// cells thick): always for `Columns` and `Grid`, for `Headers` only
    /// with `band_rules`.
    pub const fn draws_band_rules(self) -> bool {
        match self.extent {
            BorderExtent::None => false,
            BorderExtent::Headers => self.band_rules,
            BorderExtent::Columns | BorderExtent::Grid => true,
        }
    }
}

impl BorderStyle {
    /// Whether a line closes the table after the last item on `axis`: after
    /// the last column for `Columns` and `Grid`, under the last row for
    /// `Grid`.
    pub const fn closes(self, axis: Axis) -> bool {
        match axis {
            Axis::Rows => matches!(self.extent, BorderExtent::Grid),
            Axis::Cols => matches!(self.extent, BorderExtent::Columns | BorderExtent::Grid),
        }
    }
}

/// Names the style as `<extent> <weight>`, then ` + band rules` for
/// `Headers` with them on.
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
        if self.extent == BorderExtent::Headers && self.band_rules {
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
/// the area's edge is taken to run on past it, unless a closing line ends
/// the table there (`closed`, across then down).
struct Lines {
    size: (i64, i64),
    closed: (bool, bool),
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
            if y + 1 < ys.end || (y + 1 == h && !self.closed.1) {
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
            if x + 1 < xs.end || (x + 1 == w && !self.closed.0) {
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
    // The drawn items not hidden behind the pinned Strips, in drawn order,
    // less the pinned ones whose line their region clips: past a leading
    // Strip's end, or before a trailing Strip's start.
    let drawn = |axis: Axis| -> Vec<StripItem> {
        let span = snapshot.scrolling(axis);
        let scrolling: Vec<StripItem> = (snapshot.strip(axis).iter())
            .filter(|i| i.end() >= span.start && i.offset < span.end)
            .copied()
            .collect();
        // The item before the first scrolling one ends at or before the
        // scrolling Strip's start, so it is not drawn, but its Gap may
        // still start the body: it stands in for its line there.
        let before = (scrolling.first())
            .filter(|f| f.position > 0 && f.offset > span.start)
            .map(|f| StripItem {
                position: f.position - 1,
                key: None,
                offset: f.offset - snapshot.gap(axis),
                size: 0.0,
            })
            .filter(|b| cell(b.end()) >= cell(span.start));
        let [lead, trail] = Edge::ALL.map(|edge| snapshot.pinned_region(axis, edge));
        // A slid Strip's ends are sums, so they may round past its region.
        let within = |end: f32| end - lead.end <= 4.0 * f32::EPSILON * lead.end.abs().max(1.0);
        (snapshot.pinned(axis, Edge::Leading).iter())
            .filter(|i| within(i.end()))
            .copied()
            .chain(before)
            .chain(scrolling)
            .chain(
                (snapshot.pinned(axis, Edge::Trailing).iter())
                    .filter(|i| i.end() > trail.start)
                    .copied(),
            )
            .collect()
    };
    let (rows, cols) = (&drawn(Axis::Rows)[..], &drawn(Axis::Cols)[..]);
    // Where the body's lines stop: the last item's end, or past the Gap
    // after it when more items follow out of view.
    let end = |axis: Axis, strip: &[StripItem], header: i64| {
        let count = snapshot.count(axis);
        strip.last().map_or(header, |it| {
            let more = it.position.saturating_add(1) < count;
            cell(it.end() + if more { snapshot.gap(axis) } else { 0.0 }).max(header)
        })
    };
    let (mut bottom, mut right) = (end(Axis::Rows, rows, top), end(Axis::Cols, cols, left));
    let handles = style.extent != BorderExtent::None;
    let columns = matches!(style.extent, BorderExtent::Columns | BorderExtent::Grid);
    let grid = style.extent == BorderExtent::Grid;
    // A scrolling item's line under the trailing pinned Strip is hidden.
    let hidden = |axis: Axis, at: i64| {
        let pinned = !snapshot.pinned(axis, Edge::Trailing).is_empty();
        pinned && at >= cell(snapshot.pinned_region(axis, Edge::Trailing).start)
    };
    // The cell past the axis's last item, when that item is shown and
    // neither pinned to the leading edge nor followed by trailing pins,
    // where other items follow it.
    let closing = |axis: Axis, strip: &[StripItem], band: f32| {
        let count = snapshot.count(axis);
        let (scrolling, leading) = (snapshot.strip(axis), snapshot.pinned(axis, Edge::Leading));
        let trailing = snapshot.pinned(axis, Edge::Trailing);
        let followed = &trailing[..trailing.len().saturating_sub(1)];
        (strip.iter())
            .filter(|it| !leading.contains(it) && !followed.contains(it))
            .find(|it| it.position.saturating_add(1) >= count && it.end() >= band)
            .map(|it| (it, cell(it.end())))
            .filter(|(it, at)| !(scrolling.contains(it) && hidden(axis, *at)))
            .map(|(_, at)| at)
    };
    // The cells that close the table on `axis`, where its Border Style
    // closes it: past its last item, and past the trailing pinned Strip
    // where that shows, as the table ends there too.
    let closings = |axis: Axis, strip: &[StripItem], band: f32| -> Vec<i64> {
        if !style.closes(axis) {
            return Vec::new();
        }
        let trailing = (snapshot.pinned(axis, Edge::Trailing).last())
            .filter(|&last| strip.last() == Some(last) && last.end() >= band)
            .map(|last| cell(last.end()));
        let mut at: Vec<i64> = closing(axis, strip, band)
            .into_iter()
            .chain(trailing)
            .collect();
        at.dedup();
        at
    };
    let close_x = closings(Axis::Cols, cols, left_f);
    let close_y = closings(Axis::Rows, rows, top_f);
    // The cells in the Edge Gaps beside the pinned Strips of each axis:
    // after the leading one, before the trailing one.
    let edges = |axis: Axis| -> Vec<i64> {
        let span = snapshot.scrolling(axis);
        let [lead, trail] = Edge::ALL.map(|edge| snapshot.pinned_region(axis, edge));
        let leading = (snapshot.pinned(axis, Edge::Leading).last())
            .map(|_| cell(lead.end))
            .filter(|&at| at < cell(span.start));
        let trailing = (snapshot.pinned(axis, Edge::Trailing).first())
            .map(|_| (cell(span.end), cell(trail.start)))
            .filter(|&(at, first)| at < first)
            .map(|(at, _)| at);
        leading.into_iter().chain(trailing).collect()
    };
    let (col_edges, row_edges) = (edges(Axis::Cols), edges(Axis::Rows));
    // The lines a closing line crosses reach it.
    if let Some(&x) = close_x.iter().max() {
        right = right.max(x + 1);
    }
    if let Some(&y) = close_y.iter().max() {
        bottom = bottom.max(y + 1);
    }
    let mut lines = Lines {
        size: (i64::from(area.width), i64::from(area.height)),
        closed: (!close_x.is_empty(), !close_y.is_empty()),
        marks: HashMap::new(),
    };
    if handles {
        let (gap, count) = (snapshot.gap(Axis::Cols), snapshot.count(Axis::Cols));
        let scrolling = snapshot.strip(Axis::Cols);
        for c in cols.iter().filter(|c| c.end() >= left_f) {
            let body = columns && gap_cell(c, gap, count, false).is_some();
            if let Some(x) = gap_cell(c, gap, count, true) {
                if scrolling.contains(c) && hidden(Axis::Cols, x) {
                    continue;
                }
                lines.vertical(x, 0..if body { bottom } else { top }, top);
            }
        }
        let (gap, count) = (snapshot.gap(Axis::Rows), snapshot.count(Axis::Rows));
        let scrolling = snapshot.strip(Axis::Rows);
        for r in rows.iter().filter(|r| r.end() >= top_f) {
            let body = grid && gap_cell(r, gap, count, false).is_some();
            if let Some(y) = gap_cell(r, gap, count, true) {
                if scrolling.contains(r) && hidden(Axis::Rows, y) {
                    continue;
                }
                lines.horizontal(y, 0..if body { right } else { left }, left);
            }
        }
    }
    for &x in &close_x {
        lines.vertical(x, 0..bottom, top);
    }
    for &y in &close_y {
        lines.horizontal(y, 0..right, left);
    }
    // A band too thin for a rule keeps its only cell for its text. A rule
    // reaches the handle after the last item, where there is one.
    if style.draws_band_rules() {
        let handle = |axis: Axis, strip: &[StripItem], band: f32| {
            closing(axis, strip, band).filter(|_| snapshot.gap(axis) > 0.0)
        };
        if left >= 2 {
            let reach = handle(Axis::Rows, rows, top_f).map_or(bottom, |y| bottom.max(y + 1));
            lines.vertical(left - 1, 0..reach, top);
        }
        if top >= 2 {
            let reach = handle(Axis::Cols, cols, left_f).map_or(right, |x| right.max(x + 1));
            lines.horizontal(top - 1, 0..reach, left);
        }
    }
    let (vertical, horizontal, junctions) = glyphs(style.weight);
    // Where the lines between rows cross each column edge, and the column
    // lines cross each row edge.
    let rule_ys: Vec<(i64, i64)> = (lines.marks.iter())
        .filter(|((x, _), m)| col_edges.contains(x) && m.horizontal)
        .map(|(&at, _)| at)
        .collect();
    let line_xs: Vec<(i64, i64)> = (lines.marks.iter())
        .filter(|((_, y), m)| row_edges.contains(y) && m.vertical)
        .map(|(&at, _)| at)
        .collect();
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
    // The pinned edges: double lines across the body whatever the Border
    // Style draws, capped where they meet the band rules and the closing
    // rules and crossing the lines between items and each other. The
    // headers keep their handles. Double lines are already double, so there
    // the edges go heavy.
    let (size_x, size_y) = (i64::from(area.width), i64::from(area.height));
    let mut put = |x: i64, y: i64, ch: char| {
        if (0..size_x).contains(&x)
            && (0..size_y).contains(&y)
            && let Some(c) = buf.cell_mut((area.x + x as u16, area.y + y as u16))
        {
            c.set_char(ch).set_style(theme.pinned_edge);
        }
    };
    let double = style.weight == BorderWeight::Double;
    let (down, cap_top, cap_bottom, cross) = if double {
        ('┃', '┰', '┸', '╂')
    } else {
        ('║', '╥', '╨', '╫')
    };
    let (along, cap_left, cap_right, cross_col) = if double {
        ('━', '┝', '┥', '┿')
    } else {
        ('═', '╞', '╡', '╪')
    };
    let both = if double { '╋' } else { '╬' };
    let rules = style.draws_band_rules();
    let (rule_top, rule_left) = (rules && top >= 2, rules && left >= 2);
    for &x in &col_edges {
        let from = if rule_top { top - 1 } else { top };
        for y in from.max(0)..bottom {
            let ch = if rule_top && y == top - 1 {
                cap_top
            } else if row_edges.contains(&y) {
                both
            } else if close_y.contains(&y) && y + 1 >= bottom {
                cap_bottom
            } else if rule_ys.contains(&(x, y)) {
                cross
            } else {
                down
            };
            put(x, y, ch);
        }
    }
    for &y in &row_edges {
        let from = if rule_left { left - 1 } else { left };
        for x in from.max(0)..right {
            let ch = if col_edges.contains(&x) {
                continue;
            } else if rule_left && x == left - 1 {
                cap_left
            } else if close_x.contains(&x) && x + 1 >= right {
                cap_right
            } else if line_xs.contains(&(x, y)) {
                cross_col
            } else {
                along
            };
            put(x, y, ch);
        }
    }
}
