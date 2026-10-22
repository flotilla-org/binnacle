// What a pending state looks like, interpolated by time: a shimmer sweeping
// across the headers of a sort or move the Table Source has not answered,
// and a pulse on the focused cell while its Locate is pending. With motion
// reduced, both hold still at a static highlight. A refused move's reason
// shows where it was dropped.

import type { AxisName } from "../codes";
import type { Rect, Snapshot } from "../snapshot";
import { clipCell, PAD, type PaintContext } from "./context";

/** How long the shimmer takes to sweep a header, and the pulse to beat once, ms. */
const PERIOD = 1600;

/** How long a toggled group's disclosure mark waits before it draws pending, so a fast source never flickers, ms. */
export const MARK_DELAY = 150;

/**
 * When each Group Item's disclosure mark went pending, by Key. Each frame
 * `seen` brings it up to date; `pending` says which have waited long
 * enough to draw so.
 */
export interface PendingMarks {
  seen(s: Snapshot, now: number): void;
  pending(key: bigint | null, now: number): boolean;
  /** Whether any mark is pending: another frame is wanted to draw it, or to pulse it. */
  readonly waiting: boolean;
}

export function createPendingMarks(): PendingMarks {
  const since = new Map<bigint, number>();
  return {
    seen(s, now) {
      const keys = new Set(s.rows.strip.flatMap((i) => (i.togglePending && i.key !== null ? [i.key] : [])));
      for (const key of since.keys()) if (!keys.has(key)) since.delete(key);
      for (const key of keys) if (!since.has(key)) since.set(key, now);
    },
    pending: (key, now) => key !== null && now - (since.get(key) ?? Infinity) >= MARK_DELAY,
    get waiting() {
      return since.size > 0;
    },
  };
}

/** How opaque a pending disclosure mark draws now: pulsing, or still at half with motion reduced. */
export function markBeat(p: PaintContext): number {
  return p.motion ? 0.3 + 0.5 * (0.5 - 0.5 * Math.cos((2 * Math.PI * (p.now % PERIOD)) / PERIOD)) : 0.5;
}

/** Fills a pending sort's or move's header in the shimmer, under its label. */
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

/** Whether any visible header shows a sort or move still pending. */
export function shimmering(s: Snapshot): boolean {
  const pending = (h: Snapshot["rows"]["headers"][number]) => h.sort.pending || h.movePending;
  return s.cols.headers.some(pending) || s.rows.headers.some(pending);
}

/**
 * The reason the Table Source refused the last move, in a box at the edge
 * of the header band where the move was dropped (`drop`, the last drop
 * line drawn on the refusal's axis), else at the band's start.
 */
export function paintRefusal(p: PaintContext, drop: { axis: AxisName; offset: number } | null): void {
  const refusal = p.s.moveRefusal;
  if (!refusal) return;
  const { ctx, theme, measure, left, top } = p;
  const at = drop?.axis === refusal.axis ? drop.offset : refusal.axis === "cols" ? left : top;
  const text = measure.fit(refusal.reason, theme.font, p.width - left - 4 * PAD);
  if (text === "") return;
  const width = measure.width(text, theme.font) + 2 * PAD;
  const height = 2 * PAD + 12;
  const x = refusal.axis === "cols" ? Math.min(Math.max(at - width / 2, left), p.width - width) : left;
  const y = refusal.axis === "cols" ? top : Math.min(Math.max(at - height / 2, top), p.height - height);
  ctx.save();
  ctx.fillStyle = theme.header.background;
  ctx.fillRect(x, y, width, height);
  ctx.strokeStyle = theme.error;
  ctx.lineWidth = 1;
  ctx.strokeRect(x + 0.5, y + 0.5, width - 1, height - 1);
  ctx.font = theme.font;
  ctx.fillStyle = theme.error;
  ctx.fillText(text, x + PAD, y + height / 2);
  ctx.restore();
}

/**
 * Pulses the focused cell while its Locate is pending, where it was last
 * known: the Window may not hold its Key there, so the focus ring is not
 * drawn. Returns whether it drew anything.
 */
export function paintPulse(p: PaintContext): boolean {
  const { ctx, s, theme } = p;
  if (!(s.rows.locating || s.cols.locating)) return false;
  const i = s.rows.strip.findIndex((item) => item.position === s.rows.focus?.position);
  const j = s.cols.strip.findIndex((item) => item.position === s.cols.focus?.position);
  const row = s.rows.strip[i];
  const col = s.cols.strip[j];
  if (!row || !col) return false;
  // 0 to 1 and back once a period; a still midpoint with motion reduced.
  const beat = p.motion ? 0.5 - 0.5 * Math.cos((2 * Math.PI * (p.now % PERIOD)) / PERIOD) : 0.5;
  const line = theme.focus.width;
  ctx.save();
  clipCell(p, i, j);
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
