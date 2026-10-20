// What the Grid's focus, selection and pointer gestures look like: selection
// fills, picked rows, selected and moving headers, the focus ring, the resize
// guide and the drop indicator. The resize handles are the Border Style's.

import type { Line } from "../theme";
import { clip, type PaintContext } from "./context";

/** Laid over the body after its cells: the selection range and picked rows. */
export function paintSelection(p: PaintContext): void {
  const { ctx, s, theme, left, top } = p;
  if (s.selectionRects.length === 0) return;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.fillStyle = theme.selection.background;
  for (const r of s.selectionRects) ctx.fillRect(r.x, r.y, r.width, r.height);
  ctx.restore();
}

/** Laid over the header bands: selected and moving headers, and a mark beside picked rows and under picked columns. */
export function paintHeaderStates(p: PaintContext): void {
  const { ctx, s, theme, left, top } = p;
  const moving = new Set(s.movingKeys);
  const picked = new Set(s.picked.rows);
  const pickedCols = new Set(s.picked.cols);
  ctx.save();
  clip(ctx, left, 0, p.width - left, top);
  for (const h of s.cols.headers) {
    if (h.selected || (h.key !== null && moving.has(h.key))) {
      ctx.fillStyle = theme.selection.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    if (h.key !== null && pickedCols.has(h.key)) {
      ctx.fillStyle = theme.picks.color;
      ctx.fillRect(h.rect.x, top - 3, h.rect.width, 3);
    }
  }
  ctx.restore();
  ctx.save();
  clip(ctx, 0, top, left, p.height - top);
  for (const h of s.rows.headers) {
    if (h.selected) {
      ctx.fillStyle = theme.selection.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    if (h.key !== null && picked.has(h.key)) {
      ctx.fillStyle = theme.picks.color;
      ctx.fillRect(0, h.rect.y, 3, h.rect.height);
    }
  }
  ctx.restore();
}

/** The ring inside the focused cell, only while the widget has DOM focus. */
export function paintFocus(p: PaintContext): void {
  const { ctx, s, theme, left, top, pixel } = p;
  const r = s.focusRect;
  if (!p.focused || !r) return;
  const line = theme.focus.width;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.strokeStyle = theme.focus.color;
  ctx.lineWidth = line;
  const x = snap(r.x, pixel) + line / 2;
  const y = snap(r.y, pixel) + line / 2;
  ctx.strokeRect(x, y, Math.max(0, snap(r.width, pixel) - line), Math.max(0, snap(r.height, pixel) - line));
  ctx.restore();
}

/** The resize guide and the drop indicator, across the whole grid. */
export function paintGuides(p: PaintContext): void {
  const { ctx, s, theme } = p;
  const across = (axis: "rows" | "cols", offset: number, { color, width: line }: Line) => {
    ctx.fillStyle = color;
    const at = snap(offset - line / 2, p.pixel);
    if (axis === "cols") ctx.fillRect(at, 0, line, p.height);
    else ctx.fillRect(0, at, p.width, line);
  };
  if (s.resizeGuide) across(s.resizeGuide.axis, s.resizeGuide.offset, theme.guide);
  if (s.dropIndicator) across(s.dropIndicator.axis, s.dropIndicator.offset, theme.dropIndicator);
}

function snap(v: number, pixel: number): number {
  return Math.round(v / pixel) * pixel;
}
