// The canvas painter: one canvas with a backing store at CSS size times the
// device pixel ratio, repainted from scratch each frame by the stages below.

import type { Content } from "../content";
import { createMeasure, type Measure } from "../measure";
import type { Painter } from "../painter";
import type { Snapshot } from "../snapshot";
import type { Theme } from "../theme";
import { paintColumnHeaders, paintCorner, paintRowNumbers } from "./bands";
import { paintBody } from "./body";
import type { PaintContext } from "./context";
import { paintGaps } from "./gaps";

export interface CanvasPainter extends Painter {
  readonly element: HTMLCanvasElement;
  readonly measure: Measure;
  theme: Theme;
}

export function createCanvasPainter(theme: Theme): CanvasPainter {
  const canvas = document.createElement("canvas");
  canvas.style.display = "block";
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) throw new Error("binnacle: no 2D canvas context");
  const measure = createMeasure(ctx);
  let width = 0;
  let height = 0;
  let dpr = 1;

  const painter: CanvasPainter = {
    element: canvas,
    measure,
    theme,
    resize(w, h, ratio) {
      width = w;
      height = h;
      dpr = ratio;
      canvas.width = Math.max(1, Math.round(w * ratio));
      canvas.height = Math.max(1, Math.round(h * ratio));
      canvas.style.width = `${w}px`;
      canvas.style.height = `${h}px`;
    },
    paint(s: Snapshot, content: Content) {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      // Set once per paint: a baseline set inside save/restore reverts.
      ctx.textBaseline = "middle";
      ctx.textAlign = "left";
      const lastRow = s.rows.strip.at(-1);
      const lastCol = s.cols.strip.at(-1);
      const p: PaintContext = {
        ctx,
        s,
        content,
        theme: painter.theme,
        measure,
        width,
        height,
        left: s.cols.header,
        top: s.rows.header,
        right: Math.min(width, lastCol ? lastCol.offset + lastCol.size : s.cols.header),
        bottom: Math.min(height, lastRow ? lastRow.offset + lastRow.size : s.rows.header),
        pixel: 1 / dpr,
      };
      paintBody(p);
      paintColumnHeaders(p);
      paintRowNumbers(p);
      paintCorner(p);
      paintGaps(p);
      return false;
    },
  };
  return painter;
}
