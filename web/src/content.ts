// What a painter reads besides the Snapshot: cell values and header labels
// for the visible items, by their place in the Snapshot's Strips.

import type { AxisName } from "./codes";
import type { StripItem } from "./snapshot";

export interface Content {
  /** The value where the `row`th row item and `col`th column item cross; `undefined` while loading. */
  value(row: number, col: number): unknown;
  /** Whether the Table Source failed the request for that cell's value, or for its row's or column's Key. */
  failed(row: number, col: number): boolean;
  /** The header text for a visible item on `axis`. */
  label(axis: AxisName, item: StripItem): string;
  /** The Table Source's own ID for a visible item; `undefined` while its Key has not arrived. */
  id(axis: AxisName, item: StripItem): unknown;
}
