// The body: row backgrounds (zebra) and each cell's value as text, or the
// loading marker while its Key or value has not arrived.

import { PAD, clip, type PaintContext } from "./context";

export function paintBody(p: PaintContext): void {
  const { ctx, s, theme, measure, left, top, right } = p;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.fillStyle = theme.body.background;
  ctx.fillRect(left, top, p.width - left, p.height - top);
  ctx.font = theme.font;
  const cols = s.cols.strip;
  s.rows.strip.forEach((row, i) => {
    if (theme.zebra && row.position % 2 === 1) {
      ctx.fillStyle = theme.zebra;
      ctx.fillRect(left, row.offset, right - left, row.size);
    }
    const y = row.offset + row.size / 2;
    cols.forEach((col, j) => {
      const value = row.key === null || col.key === null ? undefined : p.content.value(i, j);
      const loading = value === undefined;
      const text = loading ? "…" : value === null ? "" : String(value);
      ctx.fillStyle = loading ? theme.loading : theme.body.color;
      ctx.fillText(measure.fit(text, theme.font, col.size - 2 * PAD), col.offset + PAD, y);
    });
  });
  ctx.restore();
}
