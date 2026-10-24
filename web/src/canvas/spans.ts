// Header form: each level of a grouped axis a tier of its Header Band (a
// column of the row band, a row of the column band), each group one
// spanning cell over its items, labelled at the start of the part in view,
// and faded where it goes on out of view, after its disclosure mark.

import type { HeaderSpan, Rect, Snapshot } from "../snapshot";
import { PAD, clip, type PaintContext } from "./context";
import { markBeat } from "./pending";

/** The room a span's disclosure mark takes before its label, CSS px. */
const DISCLOSURE = 14;
/** How far a cut span fades out toward its cut edge, CSS px. */
const FADE = 12;

/** The height of a span's first line in view: that of the row drawn at its top, or a column span's whole tier. */
function lineOf(s: Snapshot, span: HeaderSpan): number {
  if (span.axis === "cols") return span.visible.height;
  const top = span.visible.y;
  const row = s.rows.strip.find((i) => i.position >= span.first && i.position <= span.last && i.offset + i.size > top);
  return Math.min(span.visible.height, row?.size ?? span.visible.height);
}

/** Where a span's disclosure mark sits, which a click toggles: at its label's left, on its first line in view. */
export function spanMarkRect(s: Snapshot, span: HeaderSpan): Rect {
  return { x: span.visible.x + PAD - 3, y: span.visible.y, width: DISCLOSURE, height: lineOf(s, span) };
}

/**
 * Each group's span in its tier: tinted, ruled where it starts and ends and
 * at its tier's far side, with its label at the start of its part in view
 * after its disclosure mark, which pulses while a toggle waits for its
 * answer. A side the group goes on past fades out instead of closing.
 */
export function paintSpans(p: PaintContext): void {
  paintRowSpans(p);
  paintColumnSpans(p);
}

function paintRowSpans(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  if (s.rows.spans.length === 0) return;
  ctx.save();
  clip(ctx, 0, top, left, p.height - top);
  // The row a group's label that is the current match rides, as a cell would be.
  const found = s.findMatch;
  const at = found ? (s.rows.headers.find((h) => h.key === found.row)?.position ?? null) : null;
  for (const span of s.rows.spans) {
    const v = span.visible;
    ctx.save();
    clip(ctx, v.x, v.y, v.width, v.height);
    paintTint(p, span);
    if (found && at !== null && span.field === found.col && span.first <= at && at <= span.last) {
      ctx.fillStyle = theme.findCurrent.background;
      ctx.fillRect(v.x, v.y, v.width, v.height);
    }
    ctx.fillStyle = theme.border;
    if (!span.cutBefore) ctx.fillRect(v.x, span.rect.y, v.width, p.pixel);
    if (!span.cutAfter) ctx.fillRect(v.x, span.rect.y + span.rect.height - p.pixel, v.width, p.pixel);
    ctx.fillRect(v.x + v.width - p.pixel, v.y, p.pixel, v.height);
    const y = v.y + lineOf(s, span) / 2;
    paintMark(p, span, v.x + PAD, y);
    const room = v.width - 2 * PAD - DISCLOSURE;
    ctx.font = theme.headerFont;
    ctx.textAlign = "left";
    ctx.fillStyle = theme.groupItem.color;
    ctx.fillText(measure.fit(p.content.spanLabel("rows", span), theme.headerFont, room), v.x + PAD + DISCLOSURE, y);
    ctx.restore();
  }
  ctx.restore();
}

function paintColumnSpans(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  if (s.cols.spans.length === 0) return;
  ctx.save();
  clip(ctx, left, 0, p.width - left, top);
  for (const span of s.cols.spans) {
    const v = span.visible;
    ctx.save();
    clip(ctx, v.x, v.y, v.width, v.height);
    paintTint(p, span);
    ctx.fillStyle = theme.border;
    if (!span.cutBefore) ctx.fillRect(span.rect.x, v.y, p.pixel, v.height);
    if (!span.cutAfter) ctx.fillRect(span.rect.x + span.rect.width - p.pixel, v.y, p.pixel, v.height);
    ctx.fillRect(v.x, v.y + v.height - p.pixel, v.width, p.pixel);
    const y = v.y + v.height / 2;
    paintMark(p, span, v.x + PAD, y);
    ctx.font = theme.headerFont;
    ctx.textAlign = "left";
    ctx.fillStyle = theme.groupItem.color;
    const room = v.width - 2 * PAD - DISCLOSURE;
    const label = measure.fit(p.content.spanLabel("cols", span), theme.headerFont, room);
    ctx.fillText(label, v.x + PAD + DISCLOSURE, y);
    ctx.restore();
  }
  ctx.restore();
}

/** A span's disclosure mark at (`x`, `y`): ▸ closed, ▾ open, pulsing while its toggle is pending. */
function paintMark(p: PaintContext, span: HeaderSpan, x: number, y: number): void {
  const { ctx, theme } = p;
  ctx.save();
  ctx.font = theme.headerFont;
  ctx.textAlign = "left";
  ctx.fillStyle = theme.disclosure;
  if (p.markPending(span.key)) ctx.globalAlpha = markBeat(p);
  ctx.fillText(span.open === "closed" ? "▸" : "▾", x, y);
  ctx.restore();
}

/** Fills a span in the Group Item tint, fading to the band's own colour toward a cut side. */
function paintTint(p: PaintContext, span: HeaderSpan): void {
  const { ctx, theme } = p;
  const v = span.visible;
  const rows = span.axis === "rows";
  const band = rows ? theme.rowNumber : theme.header;
  // Along the axis: where the span starts and how long it is, and a strip one pixel long `i` in from either end.
  const [start, length] = rows ? [v.y, v.height] : [v.x, v.width];
  const strip = (at: number) => (rows ? ctx.fillRect(v.x, at, v.width, 1) : ctx.fillRect(at, v.y, 1, v.height));
  ctx.fillStyle = band.background;
  ctx.fillRect(v.x, v.y, v.width, v.height);
  ctx.fillStyle = theme.groupItem.background;
  const from = span.cutBefore ? start + FADE : start;
  const to = span.cutAfter ? start + length - FADE : start + length;
  if (to > from) {
    if (rows) ctx.fillRect(v.x, from, v.width, to - from);
    else ctx.fillRect(from, v.y, to - from, v.height);
  }
  // One-pixel steps, so the fade needs no colour with alpha in it: the
  // tint fades in from the cut edge, under a shade of the band's text
  // colour that fades out from it.
  for (let i = 0; i < FADE; i++) {
    ctx.globalAlpha = (i + 1) / (FADE + 1);
    if (span.cutBefore) strip(start + i);
    if (span.cutAfter) strip(start + length - 1 - i);
  }
  ctx.fillStyle = band.color;
  for (let i = 0; i < FADE; i++) {
    ctx.globalAlpha = 0.3 * (1 - i / FADE) ** 2;
    if (span.cutBefore) strip(start + i);
    if (span.cutAfter) strip(start + length - 1 - i);
  }
  ctx.globalAlpha = 1;
}
