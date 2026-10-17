// The header bands: column headers along the top, row numbers down the left,
// and the corner where they meet.

import { PAD, clip, type PaintContext } from "./context";

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
    ctx.fillStyle = loading ? theme.loading : h.hovered ? theme.headerHovered.color : theme.header.color;
    ctx.fillText(measure.fit(text, theme.headerFont, h.rect.width - 2 * PAD), h.rect.x + PAD, top / 2);
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
