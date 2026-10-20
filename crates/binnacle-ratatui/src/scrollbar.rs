//! Scrollbars: where [`render_with`](crate::render_with) puts them, how it
//! draws them, and the mouse gestures on them.

use binnacle_core::{Action, Axis, Grid, Snapshot, Thumb, Viewport};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::{RenderContext, Theme};

/// How [`render_with`](crate::render_with) divides its area: the Grid, a
/// one-column scrollbar for the rows down the right edge and, when the
/// columns overflow, a one-line scrollbar for the columns along the bottom.
/// Each bar's Rect is its track, beside the body only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub grid: Rect,
    rows: Option<Rect>,
    cols: Option<Rect>,
}

impl Layout {
    /// The track of the scrollbar for `axis`, if one is drawn.
    pub fn bar(&self, axis: Axis) -> Option<Rect> {
        let track = match axis {
            Axis::Rows => self.rows,
            Axis::Cols => self.cols,
        };
        track.filter(|t| !t.is_empty())
    }

    /// The Viewport the Grid drawn in this layout takes.
    pub fn viewport(&self) -> Viewport {
        Viewport::new(self.grid.width.into(), self.grid.height.into())
    }
}

/// How `snapshot` divides `area` as `ctx` says. The columns overflow when
/// their thumb is shorter than its track, so `snapshot` must be laid out at
/// this layout's width: [`begin_frame`] sees to that.
pub fn layout(snapshot: &Snapshot, ctx: &RenderContext, area: Rect) -> Layout {
    if !ctx.scrollbars || area.width < 2 || area.height < 2 {
        return Layout {
            grid: area,
            ..Layout::default()
        };
    }
    let mut grid = Rect {
        width: area.width - 1,
        ..area
    };
    let overflows = snapshot.thumb(Axis::Cols).size < 1.0;
    let cols = overflows.then(|| {
        grid.height -= 1;
        let left = band(snapshot.header(Axis::Cols), grid.width);
        Rect::new(grid.x + left, grid.bottom(), grid.width - left, 1)
    });
    let top = band(snapshot.header(Axis::Rows), grid.height);
    let rows = Rect::new(grid.right(), grid.y + top, 1, grid.height - top);
    Layout {
        grid,
        rows: Some(rows),
        cols,
    }
}

/// A header band's thickness in whole cells, at most `max`.
fn band(thickness: f32, max: u16) -> u16 {
    (thickness.max(0.0).ceil().min(f32::from(max))) as u16
}

/// Begins a frame of `grid` to be drawn at `area` with `ctx`, leaving the
/// scrollbars their share: `fill` begins the Grid's frame at the Viewport it
/// is given and fills the Windows the Grid asks for. It runs once, or again
/// when the columns' scrollbar comes or goes. Returns the layout
/// [`render_with`](crate::render_with) will draw the Grid's Snapshot in.
/// It also gives each axis an Edge Gap of one cell, so a pinned edge has a
/// cell to be drawn in even where the axis has no Gap (rows, usually), and
/// pin zones of one cell.
pub fn begin_frame(
    grid: &mut Grid,
    ctx: &RenderContext,
    area: Rect,
    mut fill: impl FnMut(&mut Grid, Viewport),
) -> Layout {
    for axis in Axis::ALL {
        grid.set_edge_gap(axis, 1.0);
        grid.set_pin_zone(axis, 1.0);
    }
    let mut viewport = layout(&grid.snapshot(), ctx, area).viewport();
    // The width never depends on the Snapshot, so the second pass settles it.
    for _ in 0..3 {
        fill(grid, viewport);
        let next = layout(&grid.snapshot(), ctx, area);
        if next.viewport() == viewport {
            return next;
        }
        viewport = next.viewport();
    }
    layout(&grid.snapshot(), ctx, area)
}

/// The thumb's first cell and length along a track `track` cells long (at
/// least one): at least one cell, however long the axis.
pub(crate) fn thumb_cells(thumb: Thumb, track: u16) -> (u16, u16) {
    let len = (thumb.size * f64::from(track)).round().max(1.0) as u16;
    let len = len.min(track);
    let travel = f64::from(track - len);
    let start = (thumb.fraction.clamp(0.0, 1.0) * travel).round() as u16;
    (start, len)
}

/// The thumb on `axis` along a track `track` cells long: none (a bare
/// track) while the axis has no items.
fn thumb(snapshot: &Snapshot, axis: Axis, track: u16) -> (u16, u16) {
    if snapshot.count(axis) == 0 {
        return (0, 0);
    }
    thumb_cells(snapshot.thumb(axis), track)
}

/// Draws `layout`'s scrollbars for `snapshot` in `theme`, with the corners
/// beside the header bands and between the bars left blank.
pub(crate) fn draw(snapshot: &Snapshot, layout: &Layout, theme: &Theme, buf: &mut Buffer) {
    for axis in Axis::ALL {
        let Some(track) = layout.bar(axis) else {
            continue;
        };
        let (glyph, length) = match axis {
            Axis::Rows => ('│', track.height),
            Axis::Cols => ('─', track.width),
        };
        let (start, len) = thumb(snapshot, axis, length);
        for (i, p) in track.positions().enumerate() {
            let on = (start..start + len).contains(&(i as u16));
            let (ch, style) = if on {
                ('█', theme.scrollbar_thumb)
            } else {
                (glyph, theme.scrollbar_track)
            };
            if let Some(c) = buf.cell_mut(p) {
                c.set_char(ch).set_style(style);
            }
        }
    }
    let grid = layout.grid;
    let mut blank = |r: Rect, style: Style| {
        for p in r.intersection(buf.area).positions() {
            if let Some(c) = buf.cell_mut(p) {
                c.set_char(' ').set_style(style);
            }
        }
    };
    if let Some(rows) = layout.rows {
        blank(Rect::new(rows.x, grid.y, 1, rows.y - grid.y), theme.header);
    }
    if let Some(cols) = layout.cols {
        blank(Rect::new(grid.x, cols.y, cols.x - grid.x, 1), theme.body);
        blank(Rect::new(grid.right(), cols.y, 1, 1), theme.body);
    }
}

/// What [`Mouse::scrollbar`](crate::Mouse::scrollbar) made of a mouse event.
#[derive(Clone, Debug, PartialEq)]
pub enum BarEvent {
    /// Not the scrollbars' business: hand it to
    /// [`Mouse::event`](crate::Mouse::event).
    Missed,
    /// A scrollbar took it, with the Action to send the Grid, if any.
    Took(Option<Action>),
}

/// A thumb held by the pointer: its axis and the cell of it grabbed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub axis: Axis,
    pub grab: u16,
}

/// How far along `axis`'s track `(column, row)` is, in cells from its start
/// (negative before it).
pub(crate) fn along(track: Rect, axis: Axis, (column, row): (u16, u16)) -> i32 {
    match axis {
        Axis::Rows => i32::from(row) - i32::from(track.y),
        Axis::Cols => i32::from(column) - i32::from(track.x),
    }
}

/// A press at `at` on a scrollbar: a page towards it on the track, or a
/// grab of the thumb.
pub(crate) fn press(
    snapshot: &Snapshot,
    layout: &Layout,
    at: (u16, u16),
) -> Option<Result<Action, Held>> {
    let axis = Axis::ALL
        .into_iter()
        .find(|&a| layout.bar(a).is_some_and(|t| t.contains(at.into())))?;
    let track = layout.bar(axis)?;
    let length = track.width.max(track.height);
    let (start, len) = thumb(snapshot, axis, length);
    let i = along(track, axis, at) as u16;
    let page = snapshot.extent(axis) - snapshot.header(axis);
    let by = if i < start {
        -page
    } else if i >= start + len {
        page
    } else {
        return Some(Err(Held {
            axis,
            grab: i - start,
        }));
    };
    Some(Ok(match axis {
        Axis::Rows => Action::Scroll { dx: 0.0, dy: by },
        Axis::Cols => Action::Scroll { dx: by, dy: 0.0 },
    }))
}

/// The scroll that puts `held`'s grabbed cell under `at`, as far as the
/// track allows.
pub(crate) fn drag(snapshot: &Snapshot, layout: &Layout, held: Held, at: (u16, u16)) -> Action {
    let axis = held.axis;
    let fraction = layout.bar(axis).map_or(0.0, |track| {
        let length = track.width.max(track.height);
        let (_, len) = thumb(snapshot, axis, length);
        let travel = f64::from(length - len);
        let start = f64::from(along(track, axis, at) - i32::from(held.grab));
        if travel > 0.0 {
            (start / travel).clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    Action::ScrollTo { axis, fraction }
}
