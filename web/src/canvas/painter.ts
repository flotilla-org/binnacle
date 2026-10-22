// The canvas painter: one canvas with a backing store at CSS size times the
// device pixel ratio, repainted from scratch each frame by the stages below.

import { headersBorder, type BorderStyle } from "../border";
import type { Content } from "../content";
import { createFlashes, type Flashes } from "../flash";
import { createMeasure, type Measure } from "../measure";
import type { Painter } from "../painter";
import { plainText, type RendererChoice } from "../renderer";
import type { Snapshot } from "../snapshot";
import type { Theme } from "../theme";
import { paintColumnHeaders, paintRowNumbers } from "./bands";
import { paintCorner } from "./corner";
import { paintBody, paintEmpty } from "./body";
import { paintBorders, paintPinnedEdges, type BorderLayer } from "./borders";
import type { PaintContext } from "./context";
import { paintDropCaption, paintPinZones } from "./drop";
import { createPendingMarks, paintPulse, paintRefusal, shimmering } from "./pending";
import { paintFocus, paintGuides, paintHeaderStates, paintSelection } from "./selection";
import { paintSpans } from "./spans";

export interface CanvasPainter extends Painter {
  readonly element: HTMLCanvasElement;
  readonly measure: Measure;
  theme: Theme;
  /** Whether the widget has DOM focus: the Grid's focus is drawn only then. */
  focused: boolean;
  /** Whether pending states animate (and so ask for frames); the widget clears it under `prefers-reduced-motion`. */
  motion: boolean;
  border: BorderStyle;
  /** Chooses each cell's Cell Renderer, by the Table Source's IDs. */
  renderer: RendererChoice<unknown, unknown, unknown>;
  /** The value-change flashes the host marked. */
  readonly flashes: Flashes;
  /** Centred in the body when an axis has no items. */
  empty: string;
  /** Whether a column (and a row) menu is mounted, so filterable headers in that band draw its button. */
  filterButtons: { cols: boolean; rows: boolean };
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
  const layer: BorderLayer = { canvas: null };
  /** The last drop line drawn, where a refused move's reason shows. */
  let drop: Snapshot["dropIndicator"] = null;
  const marks = createPendingMarks();

  const painter: CanvasPainter = {
    element: canvas,
    measure,
    theme,
    focused: false,
    motion: true,
    border: headersBorder,
    renderer: () => plainText,
    flashes: createFlashes(),
    empty: "No rows",
    filterButtons: { cols: false, rows: false },
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
      const now = performance.now();
      marks.seen(s, now);
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
        focused: painter.focused,
        renderer: painter.renderer,
        flashes: painter.flashes,
        now,
        motion: painter.motion,
        filterButtons: { ...painter.filterButtons },
        markPending: (key) => marks.pending(key, now),
      };
      const animating = paintBody(p);
      const pulsing = paintPulse(p);
      if (s.rows.count === 0 || s.cols.count === 0) paintEmpty(p, painter.empty);
      paintSelection(p);
      paintColumnHeaders(p);
      paintRowNumbers(p);
      paintCorner(p);
      paintHeaderStates(p);
      paintBorders(p, painter.border, layer);
      paintSpans(p);
      paintPinnedEdges(p);
      paintPinZones(p);
      paintFocus(p);
      paintGuides(p);
      paintDropCaption(p);
      drop = s.dropIndicator ?? drop;
      paintRefusal(p, drop);
      const pending = painter.motion && (pulsing || shimmering(s));
      // A toggled mark needs frames until it draws pending, then to pulse.
      return animating || painter.flashes.sweep(p.now) || pending || marks.waiting;
    },
  };
  return painter;
}
