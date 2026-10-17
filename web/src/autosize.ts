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
      const width = fitWidth(s, content, effect.key, parts, false);
      if (width !== null) grid.set_size(AXIS_CODE.cols, effect.key, Math.ceil(width));
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

/**
 * The width that shows column `key`'s header and the values of the rows in
 * the Strip whole; `null` if the column is not in the Strip, or, when
 * `complete`, while any of those values is still to come.
 */
export function fitWidth(s: Snapshot, content: Content, key: bigint, parts: SizingParts, complete: boolean): number | null {
  const { measure, theme } = parts;
  const j = s.cols.strip.findIndex((i) => i.key === key);
  if (j < 0) return null;
  const col = s.cols.strip[j]!;
  const cells = cellMeasure(measure, theme);
  const colId = content.id("cols", col);
  let width = measure.width(content.label("cols", col), theme.headerFont) + 2 * PAD;
  for (const [i, row] of s.rows.strip.entries()) {
    const value = row.key === null ? undefined : content.value(i, j);
    if (value === undefined) {
      if (complete) return null;
      continue;
    }
    const renderer = parts.renderer(content.id("rows", row), colId);
    width = Math.max(width, preferredWidth(renderer, value, cells));
  }
  return width;
}
