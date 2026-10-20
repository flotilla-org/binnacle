// The Border Style's lines: handles in the header bands' Gaps, column and row
// lines down the body, band rules and closing lines, as in the ratatui
// adapter. Each line is centred in its Gap and snapped to device pixels; the
// part inside a header band is drawn as a handle, the rest as border. The
// pinned edge is drawn over them, whatever the Border Style.

import { drawsBandRules, lineWidth, type BorderStyle } from "../border";
import type { AxisName } from "../codes";
import { itemSpan, stripOf, type AxisSnapshot, type StripItem } from "../snapshot";
import type { PaintContext } from "./context";

/** A line centred across `[start, start + size)` and running over `[from, to)`, CSS px. */
interface Rule {
  vertical: boolean;
  start: number;
  size: number;
  from: number;
  to: number;
}

const end = (item: StripItem) => item.offset + item.size;

/** The lines `style` draws for the frame in `p`. */
function rules(p: PaintContext, style: BorderStyle): Rule[] {
  const { s, left, top } = p;
  const width = lineWidth(style.weight);
  const columns = style.extent === "columns" || style.extent === "grid";
  const grid = style.extent === "grid";
  const isLast = (axis: AxisName, item: StripItem) => item.position + 1 >= s[axis].count;
  // Where the body's lines stop: the last item's end, or past the Gap after
  // it when more items follow out of view.
  const reach = (axis: AxisName, band: number) => {
    const last = s[axis].strip.at(-1);
    if (!last) return band;
    return Math.max(band, end(last) + (isLast(axis, last) ? 0 : s[axis].gap));
  };
  // Where the item at `index` of the axis's drawn items shows: a scrolling
  // item slides under the pinned ones.
  const shows = (axis: AxisName, index: number) => itemSpan(s[axis], index);
  // The end of the axis's last item, when it is shown: a closing line goes past it.
  const closing = (axis: AxisName) => {
    const last = s[axis].strip.at(-1);
    const span = shows(axis, s[axis].strip.length - 1);
    return last && isLast(axis, last) && end(last) >= span.start ? end(last) : null;
  };
  const closeX = columns ? closing("cols") : null;
  const closeY = grid ? closing("rows") : null;
  const right = Math.max(reach("cols", left), closeX === null ? 0 : closeX + width);
  const bottom = Math.max(reach("rows", top), closeY === null ? 0 : closeY + width);

  const out: Rule[] = [];
  const line = (vertical: boolean, start: number, size: number, to: number) =>
    out.push({ vertical, start, size, from: 0, to });
  if (style.extent !== "none") {
    // A Gap's line runs through the body for `columns` (or `grid` for rows);
    // past the axis's last item there is no Gap, only the handle that resizes it.
    const gaps = (axis: AxisName, body: boolean, across: number, bandEnd: number) => {
      const gap = s[axis].gap;
      if (gap <= 0) return;
      for (const [i, item] of s[axis].strip.entries()) {
        const span = shows(axis, i);
        if (end(item) < span.start || end(item) > span.end) continue;
        const vertical = axis === "cols";
        if (isLast(axis, item)) line(vertical, end(item), width, bandEnd);
        else line(vertical, end(item), gap, body ? across : bandEnd);
      }
    };
    gaps("cols", columns, bottom, top);
    gaps("rows", grid, right, left);
  }
  if (closeX !== null) line(true, closeX, width, bottom);
  if (closeY !== null) line(false, closeY, width, right);
  if (drawsBandRules(style)) {
    // A rule reaches the handle after the last item, where there is one.
    const handle = (axis: AxisName) => (s[axis].gap > 0 ? closing(axis) : null);
    if (left >= 2 * width) {
      const y = handle("rows");
      line(true, left - width, width, y === null ? bottom : Math.max(bottom, y + width));
    }
    if (top >= 2 * width) {
      const x = handle("cols");
      line(false, top - width, width, x === null ? right : Math.max(right, x + width));
    }
  }
  return out;
}

/** A device-pixel span of thickness `thickness` CSS px centred across a rule. */
function across(rule: Rule, thickness: number, dpr: number): [number, number] {
  const w = Math.max(1, Math.round(thickness * dpr));
  const start = Math.round((rule.start + rule.size / 2) * dpr - w / 2);
  return [start, w];
}

/**
 * Fills a rule's span in device pixels, in the handle colour inside the
 * header band it crosses and the border colour past it.
 */
function fill(ctx: CanvasRenderingContext2D, p: PaintContext, rule: Rule, thickness: number, dpr: number): void {
  const [start, w] = across(rule, thickness, dpr);
  const from = Math.round(rule.from * dpr);
  const to = Math.round(rule.to * dpr);
  const band = Math.round((rule.vertical ? p.top : p.left) * dpr);
  const rect = (a: number, b: number) => {
    if (b <= a) return;
    if (rule.vertical) ctx.fillRect(start, a, w, b - a);
    else ctx.fillRect(a, start, b - a, w);
  };
  ctx.fillStyle = p.theme.handle;
  rect(from, Math.min(to, band));
  ctx.fillStyle = p.theme.border;
  rect(Math.max(from, band), to);
}

export interface BorderLayer {
  /** An offscreen canvas for double lines, created on first use. */
  canvas: HTMLCanvasElement | null;
}

export function paintBorders(p: PaintContext, style: BorderStyle, layer: BorderLayer): void {
  const dpr = 1 / p.pixel;
  const lines = rules(p, style);
  if (lines.length === 0) return;
  const { ctx } = p;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const double = style.weight === "double";
  if (!double) {
    for (const rule of lines) fill(ctx, p, rule, lineWidth(style.weight), dpr);
    ctx.restore();
    return;
  }
  // Double lines: each line's full width, then its middle cut out, on a
  // layer of their own so the cuts open the crossings over any background.
  // A Gap under 3 px has room for one light line only.
  const canvas = (layer.canvas ??= document.createElement("canvas"));
  if (canvas.width !== ctx.canvas.width || canvas.height !== ctx.canvas.height) {
    canvas.width = ctx.canvas.width;
    canvas.height = ctx.canvas.height;
  }
  const lc = canvas.getContext("2d")!;
  lc.clearRect(0, 0, canvas.width, canvas.height);
  const fits = (rule: Rule) => rule.size >= 3;
  for (const rule of lines) fill(lc, p, rule, fits(rule) ? 3 : 1, dpr);
  lc.globalCompositeOperation = "destination-out";
  const hair = Math.max(1, Math.round(dpr));
  for (const rule of lines.filter(fits)) {
    const [start, w] = across(rule, 1, dpr);
    // A line that stops in view closes its end; one at the canvas edge runs on.
    const from = Math.round(rule.from * dpr) + (rule.from > 0 ? hair : 0);
    const limit = Math.round((rule.vertical ? p.height : p.width) * dpr);
    const to = Math.round(rule.to * dpr) - (Math.round(rule.to * dpr) < limit ? hair : 0);
    if (to <= from) continue;
    if (rule.vertical) lc.fillRect(start, from, w, to - from);
    else lc.fillRect(from, start, to - from, w);
  }
  lc.globalCompositeOperation = "source-over";
  ctx.drawImage(canvas, 0, 0);
  ctx.restore();
}

/**
 * The pinned edges of each axis: a line in the Gap after the leading pinned
 * Strip and in the Gap before the trailing one, across the whole grid, and a
 * shadow falling on the scrolling side once the body has scrolled under it
 * (under the trailing Strip, until the body has scrolled to its end).
 */
export function paintPinnedEdges(p: PaintContext): void {
  const { ctx, s, theme } = p;
  const SHADOW = 8;
  for (const axis of ["cols", "rows"] as const) {
    const a = s[axis];
    const reach = axis === "cols" ? p.bottom : p.right;
    const edge = (at: number, shadow: number | null, toward: 1 | -1) => {
      if (shadow !== null) {
        const far = shadow + toward * SHADOW;
        const fade =
          axis === "cols" ? ctx.createLinearGradient(shadow, 0, far, 0) : ctx.createLinearGradient(0, shadow, 0, far);
        fade.addColorStop(0, theme.pinnedShadow);
        fade.addColorStop(1, "transparent");
        ctx.fillStyle = fade;
        const from = Math.min(shadow, far);
        if (axis === "cols") ctx.fillRect(from, 0, SHADOW, reach);
        else ctx.fillRect(0, from, reach, SHADOW);
      }
      ctx.fillStyle = theme.pinnedEdge;
      const line = Math.round((at - 0.5) / p.pixel) * p.pixel;
      if (axis === "cols") ctx.fillRect(line, 0, 1, reach);
      else ctx.fillRect(0, line, reach, 1);
    };
    // At the inside end of each pinned Strip's region, where it clips.
    if (a.pinned > 0) {
      const scrolled = a.anchor.position > 0 || a.anchor.offset > 0;
      edge(a.regions.leading.end + a.gap / 2, scrolled ? a.scrolling.start : null, 1);
    }
    if (a.trailing > 0) edge(a.regions.trailing.start - a.gap / 2, scrolledToEnd(a) ? null : a.scrolling.end, -1);
  }
}

/** Whether the scrolling Strip shows the axis's last unpinned item whole. */
function scrolledToEnd(a: AxisSnapshot): boolean {
  const pins = new Set(a.strip.filter((_, i) => stripOf(a, i) !== "scrolling").map((i) => i.position));
  let last = a.count - 1;
  while (last >= 0 && pins.has(last)) last--;
  const item = a.strip.find((i, index) => stripOf(a, index) === "scrolling" && i.position === last);
  return last < 0 || (item !== undefined && end(item) <= a.scrolling.end);
}
