// The header bands: column headers along the top, row numbers down the left,
// and the corner where they meet.

import type { HeaderPlacement, Rect, Snapshot, SortDirection } from "../snapshot";
import { PAD, clip, type PaintContext } from "./context";
import { paintShimmer } from "./pending";

/** The sort marker's triangle, CSS px. */
const MARKER = 7;
/** Room a sorted header keeps for its marker (and its place in a multi-column sort). */
const MARKED = MARKER + 14;
/** The column menu's button at a header's right end, CSS px square, and the funnel drawn in it. */
const BUTTON = 18;
const FUNNEL = 10;

/** Whether a column header shows the column menu's button: a menu is mounted and the column can be filtered. */
export function hasFilterButton(filterButtons: boolean, h: HeaderPlacement): boolean {
  return filterButtons && h.key !== null && h.support.text !== null;
}

/** Where a column header's menu button sits: square, at its right end, centred across the band. */
export function filterButtonRect(h: HeaderPlacement): Rect {
  const size = Math.min(BUTTON, h.rect.height);
  return { x: h.rect.x + h.rect.width - 3 - size, y: h.rect.y + (h.rect.height - size) / 2, width: size, height: size };
}

/** How a header shows its column's conditions: filtered (where the latest request leaves it) and whether that is pending. */
function filterMark(s: Snapshot, h: HeaderPlacement): { filtered: boolean; pending: boolean } {
  // A condition asked for and not applied marks the header too.
  if (!h.filter.pending) return { filtered: h.filter.filtered || h.filter.notApplied, pending: false };
  // A pending change shows where it is going, faded.
  const requested = s.requestedView?.rows.filter.conditions.some((c) => c.key === h.key) ?? false;
  return { filtered: requested, pending: true };
}

function funnel(ctx: CanvasRenderingContext2D, cx: number, cy: number, size: number): void {
  const half = size / 2;
  ctx.beginPath();
  ctx.moveTo(cx - half, cy - half * 0.8);
  ctx.lineTo(cx + half, cy - half * 0.8);
  ctx.lineTo(cx + half * 0.2, cy + half * 0.1);
  ctx.lineTo(cx + half * 0.2, cy + half * 0.9);
  ctx.lineTo(cx - half * 0.2, cy + half * 0.6);
  ctx.lineTo(cx - half * 0.2, cy + half * 0.1);
  ctx.closePath();
}

/**
 * The column menu's button, or (with no menu mounted) a plain mark on a
 * filtered column. With a menu, an unfiltered column's funnel is a faint
 * outline, brighter while hovered, and a filtered one is solid on a tinted
 * square; without one, a filtered column's funnel is small and solid in the
 * header's text colour. A pending change shows faded; a condition the Table
 * Source did not apply, in the error colour. Returns the room it took.
 */
function paintFilter(p: PaintContext, h: HeaderPlacement): number {
  const { ctx, s, theme } = p;
  const mark = filterMark(s, h);
  const button = hasFilterButton(p.filterButtons, h);
  if (!button && !mark.filtered) return 0;
  const r = filterButtonRect(h);
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  ctx.save();
  if (mark.pending) ctx.globalAlpha = 0.4;
  const colour = h.filter.notApplied ? theme.error : theme.sortMarker;
  if (!button) {
    ctx.fillStyle = h.filter.notApplied ? theme.error : theme.header.color;
    funnel(ctx, cx, cy, FUNNEL - 2);
    ctx.fill();
  } else if (mark.filtered) {
    ctx.fillStyle = colour;
    ctx.globalAlpha *= 0.16;
    ctx.beginPath();
    ctx.roundRect(r.x, r.y, r.width, r.height, 4);
    ctx.fill();
    ctx.globalAlpha /= 0.16;
    funnel(ctx, cx, cy, FUNNEL);
    ctx.fill();
  } else {
    ctx.strokeStyle = h.hovered ? theme.headerHovered.color : theme.loading;
    ctx.lineWidth = 1.2;
    ctx.lineJoin = "round";
    funnel(ctx, cx, cy, FUNNEL);
    ctx.stroke();
  }
  ctx.restore();
  return r.width + 3;
}

/** The direction to mark a header with, and whether it is only requested so far. */
function marker(s: Snapshot, h: HeaderPlacement): { direction: SortDirection; place: number; pending: boolean } | null {
  if (h.sort.pending) {
    const index = s.requestedView?.rows.sort.findIndex((c) => c.key === h.key) ?? -1;
    const requested = s.requestedView?.rows.sort[index];
    // A pending removal shows nothing; a pending change shows where it is going.
    return requested ? { direction: requested.direction, place: index, pending: true } : null;
  }
  return h.sort.applied && { direction: h.sort.applied.direction, place: h.sort.applied.index, pending: false };
}

function paintMarker(
  p: PaintContext,
  h: HeaderPlacement,
  mark: NonNullable<ReturnType<typeof marker>>,
  inset: number,
): void {
  const { ctx, theme, s } = p;
  const right = h.rect.x + h.rect.width - Math.max(PAD, inset + 2);
  const y = h.rect.y + h.rect.height / 2;
  ctx.save();
  if (mark.pending) ctx.globalAlpha = 0.4;
  ctx.fillStyle = theme.sortMarker;
  const multi = (mark.pending ? (s.requestedView?.rows.sort.length ?? 0) : s.appliedView.rows.sort.length) > 1;
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
    const inset = paintFilter(p, h);
    const room = h.rect.width - 2 * PAD - (mark ? MARKED : 0) - inset;
    ctx.fillStyle = loading ? theme.loading : h.hovered ? theme.headerHovered.color : theme.header.color;
    ctx.fillText(measure.fit(text, theme.headerFont, room), h.rect.x + PAD, top / 2);
    if (mark) paintMarker(p, h, mark, inset);
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
