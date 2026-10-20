// What every painting stage gets: the 2D context (in CSS px), the frame, and
// the areas it divides into.

import type { Content } from "../content";
import type { Flashes } from "../flash";
import type { Measure } from "../measure";
import type { RendererChoice } from "../renderer";
import type { AxisName } from "../codes";
import { itemSpan, type Snapshot, type Span } from "../snapshot";
import type { Theme } from "../theme";

export interface PaintContext {
  ctx: CanvasRenderingContext2D;
  s: Snapshot;
  content: Content;
  theme: Theme;
  measure: Measure;
  /** The viewport in CSS px. */
  width: number;
  height: number;
  /** Where the body starts: past the row-number band and under the column header band. */
  left: number;
  top: number;
  /** Where the visible items end, short of the viewport edge on a small table. */
  right: number;
  bottom: number;
  /** One device pixel in CSS px, for snapping hairlines. */
  pixel: number;
  /** The widget has DOM focus, so the Grid's focus is drawn. */
  focused: boolean;
  /** Chooses each cell's Cell Renderer. */
  renderer: RendererChoice<unknown, unknown, unknown>;
  flashes: Flashes;
  /** The frame's time, `performance.now()` ms. */
  now: number;
  /** Whether pending states animate; `false` under `prefers-reduced-motion`. */
  motion: boolean;
  /** Whether a column (and a row) menu is mounted, so filterable headers in that band draw its button. */
  filterButtons: { cols: boolean; rows: boolean };
}

/** Horizontal text padding inside a cell or header, CSS px. */
export const PAD = 6;

export function clip(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number): void {
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();
}

/**
 * Where a selection rect from `start` to `end` along `axis` may draw: one
 * over pinned items (the Grid clips those to their region, so it ends
 * before the scrolling Strip starts or starts after it ends) in that pinned
 * Strip's region, one over scrolling items within the scrolling Strip, so it
 * slides under the pinned Strips.
 */
export function drawsIn(s: Snapshot, axis: AxisName, start: number, end: number): Span {
  const { scrolling, regions } = s[axis];
  if (end <= scrolling.start) return regions.leading;
  return start >= scrolling.end ? regions.trailing : scrolling;
}

/** Clips to where a selection rect may draw (see {@link drawsIn}). */
export function clipBody(p: PaintContext, x: number, y: number, width: number, height: number): void {
  clipSpans(p, drawsIn(p.s, "cols", x, x + width), drawsIn(p.s, "rows", y, y + height));
}

/** Clips to where the cell of the `row`th row and `col`th column drawn items may draw. */
export function clipCell(p: PaintContext, row: number, col: number): void {
  clipSpans(p, itemSpan(p.s.cols, col), itemSpan(p.s.rows, row));
}

function clipSpans(p: PaintContext, cols: Span, rows: Span): void {
  clip(p.ctx, cols.start, rows.start, cols.end - cols.start, rows.end - rows.start);
}
