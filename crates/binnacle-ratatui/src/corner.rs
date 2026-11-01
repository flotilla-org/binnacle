//! The corner grid where the Header Bands meet, in the Tiers layout: the
//! values caption and the values pseudo-level's name on its top line, each
//! column level's name right-aligned in its own tier with an arrow, and the
//! row levels' names along its bottom line, one per level column in header
//! form or joined with `›` in outline form.

use binnacle_core::{Axis, CornerLevel, Snapshot};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::width::width as text_width;
use crate::{Content, Theme, cell, put};

/// What a piece of corner text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Level(Axis, CornerLevel),
    Caption,
    Join,
}

/// A piece of text the corner draws, from column `x` of line `y` (in the
/// Grid's area), clipped at `end`.
#[derive(Clone, Debug, PartialEq)]
struct Piece {
    kind: Kind,
    text: String,
    x: f32,
    y: f32,
    end: f32,
}

impl Piece {
    fn width(&self) -> f32 {
        text_width(&self.text) as f32
    }
}

/// The name of `level` of `axis` with its sort mark: `▲` or `▼` while its
/// field sorts the axis, numbered in a sort by several.
fn level_text(
    snapshot: &Snapshot,
    content: &impl Content,
    axis: Axis,
    level: CornerLevel,
) -> String {
    let name = content.level_name(axis, level.field);
    let sort = snapshot.applied_view().axis(axis).sort.columns();
    let at = level
        .field
        .and_then(|f| sort.iter().position(|c| c.key == f));
    match at {
        None => name,
        Some(i) => {
            let arrow = match sort[i].direction {
                binnacle_core::SortDirection::Ascending => '▲',
                binnacle_core::SortDirection::Descending => '▼',
            };
            match sort.len() {
                1 => format!("{name} {arrow}"),
                _ => format!("{name} {arrow}{}", i + 1),
            }
        }
    }
}

/// Lays `pieces` on the line from `y` within `from..to`: from its start, or
/// ending at its end if `right`.
fn lay(
    out: &mut Vec<Piece>,
    y: f32,
    pieces: Vec<(Kind, String)>,
    (from, to): (f32, f32),
    right: bool,
) {
    let total: f32 = pieces.iter().map(|(_, t)| text_width(t) as f32).sum();
    // Right-aligned text keeps a cell clear before the band's edge.
    let mut x = match right {
        true => (to - 1.0 - total).max(from),
        false => from,
    };
    for (kind, text) in pieces {
        let width = text_width(&text) as f32;
        out.push(Piece {
            kind,
            text,
            x,
            y,
            end: to,
        });
        x += width;
    }
}

/// Where each piece of text goes in the corner, in the Tiers layout.
fn lay_out(snapshot: &Snapshot, content: &impl Content) -> Vec<Piece> {
    let corner = snapshot.corner();
    let (lines, columns) = (corner.tiers(Axis::Cols), corner.tiers(Axis::Rows));
    let (Some(first), Some(last)) = (columns.first(), columns.last()) else {
        return Vec::new();
    };
    let Some(bottom) = lines.len().checked_sub(1) else {
        return Vec::new();
    };
    let (left, right) = (first.across.start, last.across.end);
    let level = |axis, l| (Kind::Level(axis, l), level_text(snapshot, content, axis, l));
    let named = |axis, levels: &[CornerLevel]| -> Vec<(Kind, String)> {
        let mut out = Vec::new();
        for (i, &l) in levels.iter().enumerate() {
            if i > 0 {
                out.push((Kind::Join, " › ".into()));
            }
            out.push(level(axis, l));
        }
        out
    };
    let mut out: Vec<Piece> = Vec::new();
    // Column levels, right-aligned in their own tiers with an arrow; the
    // values pseudo-level's name goes on the top line instead.
    for line in lines {
        let fields: Vec<CornerLevel> = (line.levels.iter().copied())
            .filter(|l| !l.is_values())
            .collect();
        if !fields.is_empty() {
            let mut pieces = named(Axis::Cols, &fields);
            pieces.push((Kind::Join, " →".into()));
            lay(&mut out, line.across.start, pieces, (left, right), true);
        }
    }
    // Row levels along the bottom line: each in its level column, or joined
    // in the outline column.
    for column in columns {
        let pieces = named(Axis::Rows, &column.levels);
        if !pieces.is_empty() {
            let (from, to) = (column.across.start, column.across.end);
            lay(
                &mut out,
                lines[bottom].across.start,
                pieces,
                (from, to),
                false,
            );
        }
    }
    // The caption and the values pseudo-level's name on the top line: at
    // its start above a column level, or after the row names on one line.
    let grouping = snapshot.applied_view().grouping.as_ref();
    let mut head: Vec<(Kind, String)> = Vec::new();
    if let Some(value) = grouping.and_then(|g| g.caption()) {
        head.push((Kind::Caption, content.value_name(value)));
    }
    let values = (lines.iter().flat_map(|l| &l.levels)).find(|l| l.is_values());
    if let Some(&v) = values {
        if !head.is_empty() {
            head.push((Kind::Join, " ".into()));
        }
        head.push(level(Axis::Cols, v));
    }
    if !head.is_empty() {
        let top = lines[0].across.start;
        let on_top = out.iter().filter(|p| p.y == top);
        if bottom == 0 {
            // Only where it fits whole: the row names come first.
            let after = on_top.map(|p| p.x + p.width() + 2.0).fold(left, f32::max);
            let width: f32 = head.iter().map(|(_, t)| text_width(t) as f32).sum();
            if after + width < right {
                lay(&mut out, top, head, (after, right), true);
            }
        } else {
            let before = on_top.map(|p| p.x - 1.0).fold(right, f32::min);
            lay(&mut out, top, head, (left, before), false);
        }
    }
    out
}

/// Draws the corner of a grouped table in the header style: its level
/// names, caption and sort marks. A plain table's corner stays blank.
pub(crate) fn render(
    snapshot: &Snapshot,
    content: &impl Content,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let pieces = lay_out(snapshot, content);
    if pieces.is_empty() {
        return;
    }
    let (top, left) = (snapshot.header(Axis::Rows), snapshot.header(Axis::Cols));
    let mut style = theme.header;
    if theme.invert_headers {
        style = style.add_modifier(Modifier::REVERSED);
    }
    let blank = " ".repeat(usize::try_from(cell(left)).unwrap_or(0));
    for y in 0..cell(top) {
        put(
            buf,
            area,
            (0.0, y as f32),
            (0.0, left, 0.0, top),
            &blank,
            style,
        );
    }
    for p in pieces {
        let piece_style = match p.kind {
            Kind::Caption => style.add_modifier(Modifier::ITALIC),
            Kind::Join => style.patch(theme.handle),
            Kind::Level(_, l) if l.is_values() => style.patch(theme.sort_marker),
            Kind::Level(..) => style,
        };
        let clip = (0.0, p.end.min(left), 0.0, top);
        put(buf, area, (p.x, p.y), clip, &p.text, piece_style);
    }
}

/// The level whose name the corner draws at (`x`, `y`) in the Grid's area,
/// as [`render_with`](crate::render_with) lays it out: a click there sorts
/// by its field with [`Grid::cycle_sort`](binnacle_core::Grid::cycle_sort),
/// and a secondary click opens its menu (say [`crate::ColumnFilter::open_on`]).
/// `None` elsewhere.
pub fn corner_level_at(
    snapshot: &Snapshot,
    content: &impl Content,
    x: u16,
    y: u16,
) -> Option<(Axis, CornerLevel)> {
    let (x, y) = (f32::from(x), f32::from(y));
    if x >= snapshot.header(Axis::Cols) || y >= snapshot.header(Axis::Rows) {
        return None;
    }
    lay_out(snapshot, content).into_iter().find_map(|p| {
        let on = cell(p.y) == cell(y) && p.x <= x && x < (p.x + p.width()).min(p.end);
        match p.kind {
            Kind::Level(axis, level) if on => Some((axis, level)),
            _ => None,
        }
    })
}
