// Header form: each level of the grouped rows a column of the row band, each
// group one spanning cell over its rows, labelled at the top of the part in
// view, with its disclosure mark, and faded where it goes on out of view.

import type { HeaderSpan, Rect, Snapshot } from "../snapshot";
import { PAD, clip, type PaintContext } from "./context";
import { markBeat } from "./pending";

/** The room a span's disclosure mark takes before its label, CSS px. */
const DISCLOSURE = 14;
/** How far a cut span fades out toward its cut edge, CSS px. */
const FADE = 12;

/** The height of a span's first line in view: that of the row drawn at its top. */
function lineOf(s: Snapshot, span: HeaderSpan): number {
  const top = span.visible.y;
  const row = s.rows.strip.find((i) => i.position >= span.first && i.position <= span.last && i.offset + i.size > top);
  return Math.min(span.visible.height, row?.size ?? span.visible.height);
}

/** Where a span's disclosure mark sits, which a click toggles: at its label's left, on its first line in view. */
export function spanMarkRect(s: Snapshot, span: HeaderSpan): Rect {
  return { x: span.visible.x + PAD - 3, y: span.visible.y, width: DISCLOSURE, height: lineOf(s, span) };
}

/**
 * Each group's span in its level column: tinted, ruled where it starts and
 * ends and at its column's far side, with its disclosure mark (pulsing while
 * a toggle waits for its answer) and label at the top of its part in view.
 * A side the group goes on past fades out instead of closing.
 */
export function paintSpans(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top } = p;
  if (s.rows.spans.length === 0) return;
  ctx.save();
  clip(ctx, 0, top, left, p.height - top);
  for (const span of s.rows.spans) {
    const v = span.visible;
    ctx.save();
    clip(ctx, v.x, v.y, v.width, v.height);
    paintTint(p, span);
    ctx.fillStyle = theme.border;
    if (!span.cutBefore) ctx.fillRect(v.x, span.rect.y, v.width, p.pixel);
    if (!span.cutAfter) ctx.fillRect(v.x, span.rect.y + span.rect.height - p.pixel, v.width, p.pixel);
    ctx.fillRect(v.x + v.width - p.pixel, v.y, p.pixel, v.height);
    const y = v.y + lineOf(s, span) / 2;
    ctx.save();
    ctx.font = theme.headerFont;
    ctx.textAlign = "left";
    ctx.fillStyle = theme.disclosure;
    if (p.markPending(span.key)) ctx.globalAlpha = markBeat(p);
    ctx.fillText(span.open === "closed" ? "▸" : "▾", v.x + PAD, y);
    ctx.restore();
    const room = v.width - 2 * PAD - DISCLOSURE;
    ctx.font = theme.headerFont;
    ctx.textAlign = "left";
    ctx.fillStyle = theme.groupItem.color;
    ctx.fillText(measure.fit(p.content.spanLabel("rows", span), theme.headerFont, room), v.x + PAD + DISCLOSURE, y);
    ctx.restore();
  }
  ctx.restore();
}

/** Fills a span in the Group Item tint, fading to the band's own colour toward a cut side. */
function paintTint(p: PaintContext, span: HeaderSpan): void {
  const { ctx, theme } = p;
  const v = span.visible;
  ctx.fillStyle = theme.rowNumber.background;
  ctx.fillRect(v.x, v.y, v.width, v.height);
  ctx.fillStyle = theme.groupItem.background;
  const start = span.cutBefore ? v.y + FADE : v.y;
  const end = span.cutAfter ? v.y + v.height - FADE : v.y + v.height;
  if (end > start) ctx.fillRect(v.x, start, v.width, end - start);
  // One-pixel steps, so the fade needs no colour with alpha in it.
  for (let i = 0; i < FADE; i++) {
    ctx.globalAlpha = (i + 1) / (FADE + 1);
    if (span.cutBefore) ctx.fillRect(v.x, v.y + i, v.width, 1);
    if (span.cutAfter) ctx.fillRect(v.x, v.y + v.height - 1 - i, v.width, 1);
  }
  ctx.globalAlpha = 1;
}
