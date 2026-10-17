// What every painting stage gets: the 2D context (in CSS px), the frame, and
// the areas it divides into.

import type { Content } from "../content";
import type { Measure } from "../measure";
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
}

/** Horizontal text padding inside a cell or header, CSS px. */
export const PAD = 6;

export function clip(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number): void {
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();
}
