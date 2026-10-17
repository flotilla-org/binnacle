// Answers to the Grid's auto-size Effects, measured from the visible cells
// through their Cell Renderers: the Grid never measures, the widget does.

import { PAD } from "./canvas/context";
import type { Content } from "./content";
import type { Effect } from "./effects";
import type { Measure } from "./measure";
import { cellMeasure, preferredWidth, type RendererChoice } from "./renderer";
import type { Snapshot } from "./snapshot";
import type { Theme } from "./theme";
import type { WebGrid } from "./wasm";
import { AXIS_CODE } from "./codes";

export interface SizingParts {
  grid: WebGrid;
  measure: Measure;
  theme: Theme;
  /** The column header band's thickness the widget was created with. */
  headerHeight: number;
  renderer: RendererChoice<unknown, unknown, unknown>;
}

/** Answers `AutoSizeRequested` and `BandAutoSizeRequested`; returns whether `effect` was one. */
export function answerAutoSize(effect: Effect, s: Snapshot | null, content: Content | null, parts: SizingParts): boolean {
  const { grid, measure, theme } = parts;
  switch (effect.type) {
    case "AutoSizeRequested": {
      // Rows hold one line of text: fitting one is going back to the default height.
      if (effect.axis === "rows") {
        grid.clear_size(AXIS_CODE.rows, effect.key);
        return true;
      }
      if (!s || !content) return true;
      const j = s.cols.strip.findIndex((i) => i.key === effect.key);
      if (j < 0) return true;
      const col = s.cols.strip[j]!;
      const cells = cellMeasure(measure, theme);
      const colId = content.id("cols", col);
      let width = measure.width(content.label("cols", col), theme.headerFont) + 2 * PAD;
      s.rows.strip.forEach((row, i) => {
        const value = row.key === null ? undefined : content.value(i, j);
        if (value === undefined) return;
        const renderer = parts.renderer(content.id("rows", row), colId);
        width = Math.max(width, preferredWidth(renderer, value, cells));
      });
      grid.set_size(AXIS_CODE.cols, effect.key, Math.ceil(width));
      return true;
    }
    case "BandAutoSizeRequested": {
      if (effect.axis === "rows") {
        grid.set_header(AXIS_CODE.rows, parts.headerHeight);
        return true;
      }
      if (!s || !content) return true;
      let width = 0;
      for (const row of s.rows.strip) width = Math.max(width, measure.width(content.label("rows", row), theme.font));
      grid.set_header(AXIS_CODE.cols, Math.ceil(width + 2 * PAD));
      return true;
    }
    default:
      return false;
  }
}
