// What every painting stage gets: the 2D context (in CSS px), the frame, and
// the areas it divides into.

import type { Content } from "../content";
import type { Flashes } from "../flash";
import type { Measure } from "../measure";
import type { RendererChoice } from "../renderer";
import type { AxisName } from "../codes";
import type { Snapshot } from "../snapshot";
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
 * Where a body item ending at `end` along `axis` is clipped: a pinned item
 * (one that ends before the scrolling Strip starts) at the Header Band, a
 * scrolling item where the scrolling Strip starts, so it slides under the
 * pinned Strip.
 */
export function drawsFrom(s: Snapshot, axis: AxisName, end: number): number {
  const scrolling = s[axis].scrolling.start;
  return end <= scrolling ? s[axis].header : scrolling;
}

/** Clips to where a body rect may draw: past both Header Bands, and past each pinned Strip it scrolls under. */
export function clipBody(p: PaintContext, x: number, y: number, width: number, height: number): void {
  const x0 = drawsFrom(p.s, "cols", x + width);
  const y0 = drawsFrom(p.s, "rows", y + height);
  clip(p.ctx, x0, y0, p.width - x0, p.height - y0);
}
