// Gridlines: the Gaps between items filled with the border colour, and the
// edges of the header bands. Border Styles replace this.

import type { PaintContext } from "./context";

export function paintGaps(p: PaintContext): void {
  const { ctx, s, theme, left, top, right, bottom } = p;
  ctx.fillStyle = theme.border;
  const rowGap = s.rows.gap;
  const colGap = s.cols.gap;
  const rows = s.rows.strip;
  const cols = s.cols.strip;
  if (rowGap > 0) {
    for (const row of rows.slice(0, -1)) ctx.fillRect(left, row.offset + row.size, right - left, rowGap);
  }
  if (colGap > 0) {
    for (const col of cols.slice(0, -1)) ctx.fillRect(col.offset + col.size, 0, colGap, bottom);
  }
  // The band edges, a device pixel inside each band.
  if (top > 0) ctx.fillRect(0, top - p.pixel, right, p.pixel);
  if (left > 0) ctx.fillRect(left - p.pixel, 0, p.pixel, bottom);
}
