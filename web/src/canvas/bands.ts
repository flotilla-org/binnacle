// The header bands: column headers along the top, row numbers down the left,
// and the corner where they meet.

import type { AxisName } from "../codes";
import { itemSpan, type HeaderPlacement, type KeyedOutline, type Rect, type Snapshot, type SortDirection } from "../snapshot";
import { PAD, clip, summaryStyle, type PaintContext } from "./context";
import { markBeat, paintShimmer } from "./pending";

/** The sort marker's triangle, CSS px. */
const MARKER = 7;
/** Room a sorted header keeps for its marker (and its place in a multi-column sort). */
export const MARKED = MARKER + 14;
/** The column menu's button at a header's right end (a row menu's at its left), CSS px square, and the funnel drawn in it. */
const BUTTON = 18;
const FUNNEL = 10;
/** A grouped row band's indent per level, and the room its disclosure mark takes, CSS px. */
const INDENT = 14;
const DISCLOSURE = 14;

/** How far an outlined row's label starts past the band's padding: its indent and any disclosure mark's room. */
export function outlineInset(outline: KeyedOutline): number {
  return outline.depth * INDENT + (outline.kind === "grandTotal" ? 0 : DISCLOSURE);
}

/** Whether a header shows its menu's button: a menu is mounted for its band and its item can be filtered by. */
export function hasFilterButton(filterButtons: boolean, h: HeaderPlacement): boolean {
  return filterButtons && h.key !== null && (h.support.text !== null || h.support.compare !== null);
}

/**
 * Where a header's menu button sits: square, centred across the band, at a
 * column header's right end or a row header's left end (clear of its
 * right-aligned label).
 */
export function filterButtonRect(band: AxisName, h: HeaderPlacement): Rect {
  const size = Math.min(BUTTON, h.rect.height);
  const x = band === "cols" ? h.rect.x + h.rect.width - 3 - size : h.rect.x + 3;
  return { x, y: h.rect.y + (h.rect.height - size) / 2, width: size, height: size };
}

/** The room a filtered row's plain mark takes from its label. */
const MARK_ROOM = FUNNEL + 3;

/**
 * The most room a row header's button or filter mark can take from its
 * label: the button's while hovered, if a row menu is mounted; else the
 * plain mark's, if the columns can be filtered by the row (or already are).
 */
export function rowFilterRoom(filterButtons: boolean, h: HeaderPlacement): number {
  if (hasFilterButton(filterButtons, h)) return Math.min(BUTTON, h.rect.height) + 3;
  const { filtered, notApplied, pending } = h.filter;
  const filterable = h.support.text !== null || h.support.oneOf;
  return filterable || filtered || notApplied || pending ? MARK_ROOM : 0;
}

/**
 * How a header shows the conditions that read its item (the rows' for a
 * column header, the columns' for a row header): filtered (where the latest
 * request leaves it) and whether that is pending.
 */
function filterMark(s: Snapshot, band: AxisName, h: HeaderPlacement): { filtered: boolean; pending: boolean } {
  // A condition asked for and not applied marks the header too.
  if (!h.filter.pending) return { filtered: h.filter.filtered || h.filter.notApplied, pending: false };
  // A pending change shows where it is going, faded.
  const filtered = band === "cols" ? "rows" : "cols";
  const requested = s.requestedView?.[filtered].filter.conditions.some((c) => c.key === h.key) ?? false;
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
 * Source did not apply, in the error colour. A row header, short of room,
 * shows the button only while hovered, else just the plain mark if
 * filtered. Returns the room it took.
 */
function paintFilter(p: PaintContext, band: AxisName, h: HeaderPlacement): number {
  const { ctx, s, theme } = p;
  const mark = filterMark(s, band, h);
  const button = hasFilterButton(p.filterButtons[band], h) && (band === "cols" || h.hovered);
  if (!button && !mark.filtered) return 0;
  const r = filterButtonRect(band, h);
  // A row header's plain mark hugs its left edge.
  const compact = band === "rows" && !button;
  const cx = compact ? r.x + FUNNEL / 2 : r.x + r.width / 2;
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
  return compact ? MARK_ROOM : r.width + 3;
}

/**
 * The direction to mark a header with, and whether it is only requested so
 * far: a column header shows the rows' sort, a row header the columns'.
 */
function marker(
  s: Snapshot,
  h: HeaderPlacement,
  sorted: AxisName,
): { direction: SortDirection; place: number; pending: boolean; multi: boolean } | null {
  if (h.sort.pending) {
    const sort = s.requestedView?.[sorted].sort ?? [];
    const index = sort.findIndex((c) => c.key === h.key);
    const requested = sort[index];
    // A pending removal shows nothing; a pending change shows where it is going.
    return requested ? { direction: requested.direction, place: index, pending: true, multi: sort.length > 1 } : null;
  }
  const multi = s.appliedView[sorted].sort.length > 1;
  return h.sort.applied && { direction: h.sort.applied.direction, place: h.sort.applied.index, pending: false, multi };
}

function paintMarker(
  p: PaintContext,
  h: HeaderPlacement,
  mark: NonNullable<ReturnType<typeof marker>>,
  inset: number,
): void {
  const { ctx, theme } = p;
  const right = h.rect.x + h.rect.width - Math.max(PAD, inset + 2);
  const y = h.rect.y + h.rect.height / 2;
  ctx.save();
  if (mark.pending) ctx.globalAlpha = 0.4;
  ctx.fillStyle = theme.sortMarker;
  const { multi } = mark;
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
  for (const [i, h] of s.cols.headers.entries()) {
    const item = { position: h.position, key: h.key, offset: h.rect.x, size: h.rect.width, outline: null, togglePending: false };
    const loading = h.key === null;
    const text = loading ? "…" : p.content.label("cols", item);
    // A scrolling header slides under the pinned ones.
    const span = itemSpan(s.cols, i);
    ctx.save();
    clip(ctx, span.start, 0, span.end - span.start, top);
    if (h.hovered) {
      ctx.fillStyle = theme.headerHovered.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    if (h.sort.pending || h.movePending) paintShimmer(p, h.rect);
    const mark = marker(s, h, "rows");
    const inset = paintFilter(p, "cols", h);
    const room = h.rect.width - 2 * PAD - (mark ? MARKED : 0) - inset;
    ctx.fillStyle = loading ? theme.loading : h.hovered ? theme.headerHovered.color : theme.header.color;
    ctx.fillText(measure.fit(text, theme.headerFont, room), h.rect.x + PAD, top / 2);
    if (mark) paintMarker(p, h, mark, inset);
    ctx.restore();
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
  for (const [i, h] of s.rows.headers.entries()) {
    const outline = s.rows.strip[i]?.outline ?? null;
    const togglePending = s.rows.strip[i]?.togglePending ?? false;
    const item = { position: h.position, key: h.key, offset: h.rect.y, size: h.rect.height, outline, togglePending };
    const text = p.content.label("rows", item);
    const span = itemSpan(s.rows, i);
    ctx.save();
    clip(ctx, 0, span.start, left, span.end - span.start);
    const role = summaryStyle(theme, outline);
    if (h.hovered || role) {
      ctx.fillStyle = h.hovered ? theme.headerHovered.background : role!.background;
      ctx.fillRect(h.rect.x, h.rect.y, h.rect.width, h.rect.height);
    }
    if (h.sort.pending || h.movePending) paintShimmer(p, h.rect);
    const inset = paintFilter(p, "rows", h);
    // A row the columns sort by ends in its marker, as a column header does.
    const mark = marker(s, h, "cols");
    const room = left - 2 * PAD - (mark ? MARKED : 0) - inset;
    const color = h.hovered ? theme.headerHovered.color : (role?.color ?? theme.rowNumber.color);
    const y = h.rect.y + h.rect.height / 2;
    if (outline) paintOutline(p, outline, text, color, y, room, p.markPending(h.key));
    else {
      ctx.fillStyle = color;
      ctx.fillText(measure.fit(text, theme.font, room), left - PAD - (mark ? MARKED : 0), y);
    }
    if (mark) paintMarker(p, h, mark, 0);
    ctx.restore();
  }
  ctx.restore();
}

/** Where a Group Item's disclosure mark sits in its band cell, which a click toggles; `null` for one that can't open. */
export function disclosureRect(outline: KeyedOutline, rect: Rect): Rect | null {
  if (outline.kind !== "group" || outline.open === "cantOpen") return null;
  return { x: PAD + outline.depth * INDENT - 3, y: rect.y, width: DISCLOSURE, height: rect.height };
}

/**
 * A grouped row's band in outline form, left-aligned: an indent per level,
 * then a Group Item's disclosure mark (none if it can't open; pulsing while
 * a toggle waits for its answer) and its label in the header font, or a
 * Leaf's label past the mark's room. The Grand Total sits flush.
 */
function paintOutline(
  p: PaintContext,
  outline: KeyedOutline,
  text: string,
  color: string,
  y: number,
  room: number,
  pending: boolean,
): void {
  const { ctx, theme, measure } = p;
  ctx.textAlign = "left";
  if (outline.kind === "group" && outline.open !== "cantOpen") {
    ctx.save();
    ctx.fillStyle = theme.disclosure;
    ctx.font = theme.headerFont;
    if (pending) ctx.globalAlpha = markBeat(p);
    ctx.fillText(outline.open === "closed" ? "▸" : "▾", PAD + outline.depth * INDENT, y);
    ctx.restore();
  }
  const x = PAD + outlineInset(outline);
  const font = outline.kind === "leaf" ? theme.font : theme.headerFont;
  ctx.font = font;
  ctx.fillStyle = color;
  ctx.fillText(measure.fit(text, font, room - (x - PAD)), x, y);
  ctx.textAlign = "right";
  ctx.font = theme.font;
}

/** The corner: blank, or the name of the one value grouped rows show. */
export function paintCorner(p: PaintContext): void {
  const { ctx, theme, measure, left, top } = p;
  ctx.fillStyle = theme.header.background;
  ctx.fillRect(0, 0, left, top);
  const caption = p.content.caption();
  if (caption === null || left <= 2 * PAD) return;
  ctx.save();
  clip(ctx, 0, 0, left, top);
  ctx.font = theme.headerFont;
  ctx.textAlign = "left";
  ctx.fillStyle = theme.header.color;
  ctx.fillText(measure.fit(caption, theme.headerFont, left - 2 * PAD), PAD, top / 2);
  ctx.restore();
}
