// The value renderer: a Typed Value through the shared formatter, its style
// hints in the Theme's colours, an invalid value marked in its corner.

import { columnFormatter, DIM, WARNING, type ColumnFormat, type FormatClock, type Formatted, type TypedValue } from "../format";
import type { CellRenderer } from "../renderer";

export interface ValueOptions<V> extends ColumnFormat {
  /** The cell's Typed Value; the value itself by default. */
  value?: (value: V) => TypedValue;
  /** The clock and zone at a frame's time (`performance.now()` ms): the browser's by default. */
  clock?: (frameNow: number) => FormatClock;
}

/** The side of the warning mark's triangle, CSS px. */
const MARK = 6;
/** How often a cell showing a relative time redraws, ms. */
const TICK = 1000;

const browserClock = (frameNow: number): FormatClock => ({ now: performance.timeOrigin + frameNow });

const isDateTime = (v: TypedValue): boolean => v instanceof Date || (typeof v === "object" && v !== null && "instant" in v);

/**
 * Typed Values through the shared formatter. A null draws in the Theme's
 * `dim` colour, an invalid value in its `warning` colour with a mark in the
 * cell's corner. A relative time redraws each second. Its text for search,
 * copying and assistive technology is the formatter's stable text, with
 * dates absolute.
 */
export function valueRenderer<V = TypedValue>(options: ValueOptions<V> = {}): CellRenderer<V> {
  const { value: typed = (v: V) => v as TypedValue, clock = browserClock } = options;
  const formatter = columnFormatter(options);
  const at = (value: V, frameNow: number): Formatted => formatter.format(typed(value), clock(frameNow));
  return {
    draw(cell, value) {
      const f = at(value, cell.now);
      const { theme } = cell;
      const color = f.style & WARNING ? theme.warning : f.style & DIM ? theme.dim : undefined;
      cell.text(f.text, f.align, color);
      if (f.style & WARNING) {
        const { ctx } = cell;
        ctx.fillStyle = theme.warning;
        ctx.beginPath();
        ctx.moveTo(0, 0);
        ctx.lineTo(MARK, 0);
        ctx.lineTo(0, MARK);
        ctx.closePath();
        ctx.fill();
      }
      if (formatter.relative && isDateTime(typed(value))) cell.requestFrame(TICK);
    },
    preferredWidth: (value, measure) => measure.text(at(value, performance.now()).text),
    text: (value) => formatter.stable(typed(value), clock(performance.now())).text,
  };
}
