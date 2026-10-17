// Border Styles: which lines the painter draws in the Gaps, in what weight,
// and whether each Header Band ends in a rule. The extents and rules follow
// the ratatui Border Styles; the widget lays out Gaps that fit the weight.

export type BorderExtent =
  /** Gaps stay blank. */
  | "none"
  /** Handles in the header bands' Gaps only. */
  | "headers"
  /** Handles, and column lines down the body and after the last column. */
  | "columns"
  /** Column lines, and rules between rows and under the last row. */
  | "grid";

/** Light is 1 CSS px, heavy 2, double two hairlines in a Gap of 3. */
export type BorderWeight = "light" | "heavy" | "double";

export interface BorderStyle {
  extent: BorderExtent;
  weight: BorderWeight;
  /**
   * For `headers`: a rule at the end of each Header Band, under the column
   * headers and beside the row numbers. `columns` and `grid` always draw
   * the rules; `none` never does.
   */
  bandRules?: boolean;
}

/** Handles in the header bands only, in light lines: the default. */
export const headersBorder: BorderStyle = { extent: "headers", weight: "light" };

/** Every Border Style a host cycles, in the ratatui demo's order. */
export const borderStyles: readonly BorderStyle[] = [
  headersBorder,
  { extent: "headers", weight: "light", bandRules: true },
  { extent: "columns", weight: "light" },
  { extent: "columns", weight: "heavy" },
  { extent: "columns", weight: "double" },
  { extent: "grid", weight: "light" },
  { extent: "grid", weight: "heavy" },
  { extent: "grid", weight: "double" },
  { extent: "none", weight: "light" },
];

/** Names the style as `<extent> <weight>`, then ` + band rules` for `headers` with them on. */
export function borderName(style: BorderStyle): string {
  const rules = style.extent === "headers" && style.bandRules ? " + band rules" : "";
  return `${style.extent} ${style.weight}${rules}`;
}

export function drawsBandRules(style: BorderStyle): boolean {
  if (style.extent === "none") return false;
  return style.extent !== "headers" || style.bandRules === true;
}

/** How thick a weight's lines are, CSS px. */
export function lineWidth(weight: BorderWeight): number {
  return weight === "light" ? 1 : weight === "heavy" ? 2 : 3;
}

/** The Gaps a style is drawn in: columns always, rows only for `grid`. */
export function borderGaps(style: BorderStyle): { rows: number; cols: number } {
  const width = lineWidth(style.weight);
  return { rows: style.extent === "grid" ? width : 0, cols: width };
}
