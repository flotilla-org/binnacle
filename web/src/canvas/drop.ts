// What dragging headers across the pinned edge looks like: an empty pinned
// Strip's pin zone lit at each end of the axis, and a caption on the drop
// line saying what a drop does (the line itself is a guide).

import type { AxisName } from "../codes";
import type { DropTarget } from "../snapshot";
import { PAD, type PaintContext } from "./context";

const CAPTION_FONT = `600 11px system-ui, -apple-system, "Segoe UI", Roboto, sans-serif`;

/** What a drop does, as the drop line's caption says it; `null` for nothing. */
export function dropCaption(axis: AxisName, target: DropTarget): string | null {
  switch (target.type) {
    case "Move":
      return target.unpin ? "Unpin and move here" : "Move here";
    case "Unpin":
      return "Unpin: returns here";
    case "Pin": {
      if (target.reorder) return "Reorder the pins";
      const lead = target.edge === "leading";
      if (axis === "cols") return lead ? "Pin left" : "Pin right";
      return lead ? "Pin top" : "Pin bottom";
    }
    case "Stay":
      return null;
  }
}

/**
 * Fills the lit pin zones, twice over and outlined where a drop would pin. A
 * leading zone reaches back over the header band, since a drop there pins too.
 */
export function paintPinZones(p: PaintContext): void {
  const { ctx, s, theme } = p;
  for (const z of s.pinZones) {
    const start = z.edge === "leading" ? 0 : z.offset;
    const size = z.offset + z.size - start;
    const [x, y, w, h] = z.axis === "cols" ? [start, 0, size, p.height] : [0, start, p.width, size];
    ctx.fillStyle = theme.pinZone;
    ctx.fillRect(x, y, w, h);
    if (!z.hot) continue;
    ctx.fillRect(x, y, w, h);
    ctx.strokeStyle = theme.dropIndicator.color;
    ctx.lineWidth = 1;
    ctx.strokeRect(x + 0.5, y + 0.5, Math.max(0, w - 1), Math.max(0, h - 1));
  }
}

/**
 * The caption beside the drop line: just under the column header band, right
 * of a column drop line (left of it near the far end), or just past the
 * row-number band, under a row drop line.
 */
export function paintDropCaption(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  const drop = s.dropIndicator;
  if (!drop) return;
  const text = dropCaption(drop.axis, drop.target);
  if (text === null) return;
  const width = measure.width(text, CAPTION_FONT) + 2 * PAD;
  const height = 18;
  const gap = 6;
  let x: number;
  let y: number;
  if (drop.axis === "cols") {
    x = drop.offset + gap;
    if (x + width > p.width) x = drop.offset - gap - width;
    y = top + gap;
  } else {
    x = left + gap;
    y = drop.offset + gap;
    if (y + height > p.height) y = drop.offset - gap - height;
  }
  x = Math.max(0, x);
  y = Math.max(0, y);
  ctx.save();
  ctx.fillStyle = theme.dropIndicator.color;
  ctx.beginPath();
  ctx.roundRect(x, y, width, height, height / 2);
  ctx.fill();
  ctx.font = CAPTION_FONT;
  ctx.fillStyle = theme.body.background;
  ctx.fillText(text, x + PAD, y + height / 2);
  ctx.restore();
}
