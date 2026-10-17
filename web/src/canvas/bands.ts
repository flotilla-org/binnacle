// The header bands: column headers along the top, row numbers down the left,
// and the corner where they meet.

import type { HeaderPlacement, Snapshot, SortDirection } from "../snapshot";
import { PAD, clip, type PaintContext } from "./context";
import { paintShimmer } from "./pending";

/** The sort marker's triangle, CSS px. */
const MARKER = 7;
/** Room a sorted header keeps for its marker (and its place in a multi-column sort). */
const MARKED = MARKER + 14;

/** The direction to mark a header with, and whether it is only requested so far. */
function marker(s: Snapshot, h: HeaderPlacement): { direction: SortDirection; place: number; pending: boolean } | null {
  if (h.sort.pending) {
    const index = s.requestedSort?.findIndex((c) => c.key === h.key) ?? -1;
    const requested = s.requestedSort?.[index];
    // A pending removal shows nothing; a pending change shows where it is going.
    return requested ? { direction: requested.direction, place: index, pending: true } : null;
  }
  return h.sort.applied && { direction: h.sort.applied.direction, place: h.sort.applied.index, pending: false };
}

function paintMarker(p: PaintContext, h: HeaderPlacement, mark: NonNullable<ReturnType<typeof marker>>): void {
  const { ctx, theme, s } = p;
  const right = h.rect.x + h.rect.width - PAD;
  const y = h.rect.y + h.rect.height / 2;
  ctx.save();
  if (mark.pending) ctx.globalAlpha = 0.4;
  ctx.fillStyle = theme.sortMarker;
  const multi = (mark.pending ? (s.requestedSort?.length ?? 0) : s.appliedSort.length) > 1;
  const x = multi ? right - 8 - MARKER : right - MARKER;
  const half = MARKER / 2;
  ctx.beginPath();
  if (mark.direction === "ascending") {
    ctx.moveTo(x, y + half * 0.6);
    ctx.lineTo(x + MARKER, y + half * 0.6);
    ctx.lineTo(x + half, y - half * 0.6);
  } else {
    ctx.moveTo(x, y - half * 0.6);
    ctx.lineTo(x + MARKER, y - half * 0.6);
    ctx.lineTo(x + half, y + half * 0.6);
  }
  ctx.fill();
  if (multi) {
    ctx.font = `600 9px ${theme.headerFont.replace(/^.*?px\s*/, "")}`;
    ctx.textAlign = "right";
    ctx.fillText(String(mark.place + 1), right, y);
  }
  ctx.restore();
}

export function paintColumnHeaders(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  if (top <= 0) return;
  ctx.save();
  clip(ctx, left, 0, p.width - left, top);
  ctx.fillStyle = theme.header.background;
  ctx.fillRect(left, 0, p.width - left, top);
  ctx.font = theme.headerFont;
  for (const h of s.cols.headers) {
    const item = { position: h.position, key: h.key, offset: h.rect.x, size: h.rect.width };
    const loading = h.key === null;
    const text = loading ? "…" : p.content.label("cols", item);
    if (h.hovered) {
      ctx.fillStyle = theme.headerHovered.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    if (h.sort.pending) paintShimmer(p, h.rect);
    const mark = marker(s, h);
    const room = h.rect.width - 2 * PAD - (mark ? MARKED : 0);
    ctx.fillStyle = loading ? theme.loading : h.hovered ? theme.headerHovered.color : theme.header.color;
    ctx.fillText(measure.fit(text, theme.headerFont, room), h.rect.x + PAD, top / 2);
    if (mark) paintMarker(p, h, mark);
  }
  ctx.restore();
}

export function paintRowNumbers(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  if (left <= 0) return;
  ctx.save();
  clip(ctx, 0, top, left, p.height - top);
  ctx.fillStyle = theme.rowNumber.background;
  ctx.fillRect(0, top, left, p.height - top);
  ctx.font = theme.font;
  ctx.textAlign = "right";
  for (const h of s.rows.headers) {
    const item = { position: h.position, key: h.key, offset: h.rect.y, size: h.rect.height };
    const text = p.content.label("rows", item);
    if (h.hovered) {
      ctx.fillStyle = theme.headerHovered.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    ctx.fillStyle = h.hovered ? theme.headerHovered.color : theme.rowNumber.color;
    ctx.fillText(measure.fit(text, theme.font, left - 2 * PAD), left - PAD, h.rect.y + h.rect.height / 2);
  }
  ctx.restore();
}

export function paintCorner(p: PaintContext): void {
  const { ctx, theme, left, top } = p;
  ctx.fillStyle = theme.header.background;
  ctx.fillRect(0, 0, left, top);
}
