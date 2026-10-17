// What a pending state looks like, interpolated by time: a shimmer sweeping
// across the headers of a sort the Table Source has not answered, and a
// pulse on the focused cell while its Locate is pending. With motion
// reduced, both hold still at a static highlight.

import type { Rect, Snapshot } from "../snapshot";
import { clip, type PaintContext } from "./context";

/** How long the shimmer takes to sweep a header, and the pulse to beat once, ms. */
const PERIOD = 1600;

/** Fills a pending sort's header in the shimmer, under its label. */
export function paintShimmer(p: PaintContext, rect: Rect): void {
  const { ctx, theme } = p;
  // The band enters past the header's left edge and leaves past its right,
  // so every pending header shows it, in step; it holds still in the middle
  // with motion reduced.
  const half = Math.max(24, rect.width * 0.35);
  const phase = p.motion ? (p.now % PERIOD) / PERIOD : 0.5;
  const centre = rect.x - half + phase * (rect.width + 2 * half);
  // Past its ends the gradient holds the base colour.
  const band = ctx.createLinearGradient(centre - half, 0, centre + half, 0);
  band.addColorStop(0, theme.shimmerBase);
  band.addColorStop(0.5, theme.shimmerHighlight);
  band.addColorStop(1, theme.shimmerBase);
  ctx.fillStyle = band;
  ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
}

/** Whether any visible column header shows a sort still pending. */
export function sorting(s: Snapshot): boolean {
  return s.cols.headers.some((h) => h.sort.pending);
}

/**
 * Pulses the focused cell while its Locate is pending, where it was last
 * known: the Window may not hold its Key there, so the focus ring is not
 * drawn. Returns whether it drew anything.
 */
export function paintPulse(p: PaintContext): boolean {
  const { ctx, s, theme, left, top } = p;
  if (!(s.rows.locating || s.cols.locating)) return false;
  const row = s.rows.strip.find((i) => i.position === s.rows.focus?.position);
  const col = s.cols.strip.find((i) => i.position === s.cols.focus?.position);
  if (!row || !col) return false;
  // 0 to 1 and back once a period; a still midpoint with motion reduced.
  const beat = p.motion ? 0.5 - 0.5 * Math.cos((2 * Math.PI * (p.now % PERIOD)) / PERIOD) : 0.5;
  const line = theme.focus.width;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.globalAlpha = 0.25 + 0.45 * beat;
  ctx.fillStyle = theme.shimmerBase;
  ctx.fillRect(col.offset, row.offset, col.size, row.size);
  ctx.globalAlpha = 0.35 + 0.65 * beat;
  ctx.strokeStyle = theme.focus.color;
  ctx.lineWidth = line;
  ctx.strokeRect(col.offset + line / 2, row.offset + line / 2, col.size - line, row.size - line);
  ctx.restore();
  return true;
}
