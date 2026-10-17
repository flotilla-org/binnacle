// The text renderer: a value as one line, aligned, numbers through an
// optional `Intl.NumberFormat`.

import type { Align, CellRenderer } from "../renderer";

export interface TextOptions<V> {
  /** Where the text sits; `"start"` by default. */
  align?: Align;
  /** Formats numbers and bigints, or every value when a function. */
  format?: Intl.NumberFormat | ((value: V) => string);
}

export function textRenderer<V = unknown>(options: TextOptions<V> = {}): CellRenderer<V> {
  const { align = "start", format } = options;
  const show = (value: V): string => {
    if (typeof format === "function") return format(value);
    if (value == null) return "";
    if (format && (typeof value === "number" || typeof value === "bigint")) return format.format(value);
    return String(value);
  };
  return {
    draw: (cell, value) => cell.text(show(value), align),
    preferredWidth: (value, measure) => measure.text(show(value)),
    text: show,
  };
}
