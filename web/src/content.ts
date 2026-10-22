// What a painter reads besides the Snapshot: cell values and header labels
// for the visible items, by their place in the Snapshot's Strips.

import type { AxisName } from "./codes";
import type { HeaderSpan, StripItem } from "./snapshot";

export interface Content {
  /** The value where the `row`th row item and `col`th column item cross; `undefined` while loading. */
  value(row: number, col: number): unknown;
  /** Whether the Table Source failed the request for that cell's value, or for its row's or column's Key. */
  failed(row: number, col: number): boolean;
  /** The header text for a visible item on `axis`. */
  label(axis: AxisName, item: StripItem): string;
  /** The label of a group drawn in header form, at the top of its span. */
  spanLabel(axis: AxisName, span: HeaderSpan): string;
  /** The Table Source's own ID for a visible item; `undefined` while its Key has not arrived. */
  id(axis: AxisName, item: StripItem): unknown;
  /** Whether the cell matches the Find Query in force, as the Table Source last said. */
  marked(row: number, col: number): boolean;
  /** Whether the visible item at index `i` of `axis`'s Strip is unmatched (kept after an edit though the filter drops it), as the Table Source last said. */
  unmatched(axis: AxisName, i: number): boolean;
  /** The name of the one value a grouped table shows, the corner's caption; `null` unless there is exactly one. */
  caption(): string | null;
  /** The name of a level of `axis` in the corner: its field's header (a column's for a row level), or the values pseudo-level's for `null`. */
  levelName(axis: AxisName, field: bigint | null): string;
}
