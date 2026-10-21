// What the Grid's focus, selection and pointer gestures look like: selection
// fills, picked rows, selected and moving headers, the focus ring, the resize
// guide and the drop indicator. The resize handles are the Border Style's.

import type { Line } from "../theme";
import { focusIndex, itemSpan } from "../snapshot";
import { clip, clipBody, clipCell, type PaintContext } from "./context";

/** Laid over the body after its cells: the selection range and picked rows. */
export function paintSelection(p: PaintContext): void {
  const { ctx, s, theme, left, top } = p;
  if (s.selectionRects.length === 0) return;
  ctx.fillStyle = theme.selection.background;
  for (const r of s.selectionRects) {
    ctx.save();
    clipBody(p, r.x, r.y, r.width, r.height);
    ctx.fillRect(r.x, r.y, r.width, r.height);
    ctx.restore();
  }
}

/** Laid over the header bands: selected and moving headers, and a mark beside picked rows and under picked columns. */
export function paintHeaderStates(p: PaintContext): void {
  const { ctx, s, theme, left, top } = p;
  const moving = new Set(s.movingKeys);
  const movingOn = s.mode.type === "MovingHeaders" ? s.mode.axis : null;
  const picked = new Set(s.picked.rows);
  const pickedCols = new Set(s.picked.cols);
  ctx.save();
  clip(ctx, left, 0, p.width - left, top);
  for (const [i, h] of s.cols.headers.entries()) {
    // A scrolling header's states slide under the pinned headers too.
    const span = itemSpan(s.cols, i);
    const x = Math.max(h.rect.x, span.start);
    const width = Math.min(h.rect.x + h.rect.width, span.end) - x;
    if (h.selected || (movingOn === "cols" && h.key !== null && moving.has(h.key))) {
      ctx.fillStyle = theme.selection.background;
      ctx.fillRect(x, h.rect.y, width, h.rect.height);
    }
    if (h.key !== null && pickedCols.has(h.key)) {
      ctx.fillStyle = theme.picks.color;
      ctx.fillRect(x, top - 3, width, 3);
    }
  }
  ctx.restore();
  ctx.save();
  clip(ctx, 0, top, left, p.height - top);
  for (const [i, h] of s.rows.headers.entries()) {
    const span = itemSpan(s.rows, i);
    const y = Math.max(h.rect.y, span.start);
    const height = Math.min(h.rect.y + h.rect.height, span.end) - y;
    if (h.selected || (movingOn === "rows" && h.key !== null && moving.has(h.key))) {
      ctx.fillStyle = theme.selection.background;
      ctx.fillRect(h.rect.x, y, h.rect.width, height);
    }
    if (h.key !== null && picked.has(h.key)) {
      ctx.fillStyle = theme.picks.color;
      ctx.fillRect(0, y, 3, height);
    }
  }
  ctx.restore();
}

/** The ring inside the focused cell, only while the widget has DOM focus. */
export function paintFocus(p: PaintContext): void {
  const { ctx, s, theme, pixel } = p;
  const r = s.focusRect;
  if (!p.focused || !r) return;
  const line = theme.focus.width;
  ctx.save();
  clipCell(p, focusIndex(s.rows), focusIndex(s.cols));
  ctx.strokeStyle = theme.focus.color;
  ctx.lineWidth = line;
  const x = snap(r.x, pixel) + line / 2;
  const y = snap(r.y, pixel) + line / 2;
  ctx.strokeRect(x, y, Math.max(0, snap(r.width, pixel) - line), Math.max(0, snap(r.height, pixel) - line));
  ctx.restore();
}

/** The resize guide and the drop indicator, across the whole grid; the drop line is dashed where an unpinned item returns to its own place. */
export function paintGuides(p: PaintContext): void {
  const { ctx, s, theme } = p;
  const across = (axis: "rows" | "cols", offset: number, { color, width: line }: Line, dashed = false) => {
    ctx.fillStyle = color;
    const at = snap(offset - line / 2, p.pixel);
    const length = axis === "cols" ? p.height : p.width;
    // Dashes of 6 with gaps of 4, as the prototype drew a return line.
    for (let from = 0; from < length; from += dashed ? 10 : length) {
      const run = dashed ? Math.min(6, length - from) : length;
      if (axis === "cols") ctx.fillRect(at, from, line, run);
      else ctx.fillRect(from, at, run, line);
    }
  };
  if (s.resizeGuide) across(s.resizeGuide.axis, s.resizeGuide.offset, theme.guide);
  const drop = s.dropIndicator;
  // Dashed where an unpinned item returns to its own place.
  if (drop) across(drop.axis, drop.offset, theme.dropIndicator, drop.target.type === "Unpin");
}

function snap(v: number, pixel: number): number {
  return Math.round(v / pixel) * pixel;
}
