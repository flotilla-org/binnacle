// The sparkline renderer: a series as a line across the cell, ending in a
// dot at the latest value.

import { PAD } from "../canvas/context";
import type { CellRenderer } from "../renderer";
import { ink, spoken } from "./marks";

export interface SparklineOptions<V> {
  /** The series a value holds; the value itself by default. */
  series?: (value: V) => readonly number[];
  /** Scales every cell to `[min, max]` so rows compare; each series' own range by default. */
  range?: readonly [number, number];
  /** The line's colour; the Theme's focus colour by default. */
  color?: string;
}

/** Vertical room above and below the line, CSS px. */
const INSET = 5;
/** Horizontal room a value asks for when a column fits, CSS px. */
const STEP = 4;

export function sparklineRenderer<V = readonly number[]>(options: SparklineOptions<V> = {}): CellRenderer<V> {
  const series = options.series ?? ((v: V) => (Array.isArray(v) ? (v as readonly number[]) : []));
  const bounds = (values: readonly number[]): [number, number] => {
    if (options.range) return [options.range[0], options.range[1]];
    const finite = values.filter(Number.isFinite);
    return [Math.min(...finite), Math.max(...finite)];
  };
  return {
    draw(cell, value) {
      const values = series(value);
      if (values.length === 0) return;
      const [min, max] = bounds(values);
      const { ctx } = cell;
      const left = PAD;
      const across = Math.max(0, cell.width - 2 * PAD);
      const top = INSET;
      const down = Math.max(0, cell.height - 2 * INSET);
      const x = (i: number) => left + (values.length === 1 ? across : (i / (values.length - 1)) * across);
      // A flat series runs through the middle.
      const y = (v: number) => top + (max > min ? (1 - (Math.min(max, Math.max(min, v)) - min) / (max - min)) * down : down / 2);
      ctx.strokeStyle = ctx.fillStyle = ink(cell, options.color);
      ctx.lineWidth = 1.5;
      ctx.lineJoin = "round";
      ctx.beginPath();
      let drawing = false;
      values.forEach((v, i) => {
        if (!Number.isFinite(v)) {
          drawing = false;
          return;
        }
        if (drawing) ctx.lineTo(x(i), y(v));
        else ctx.moveTo(x(i), y(v));
        drawing = true;
      });
      ctx.stroke();
      const last = values.at(-1)!;
      if (Number.isFinite(last)) {
        ctx.beginPath();
        ctx.arc(x(values.length - 1), y(last), 2.5, 0, 2 * Math.PI);
        ctx.fill();
      }
    },
    preferredWidth: (value) => series(value).length * STEP + 2 * PAD,
    text(value) {
      const finite = series(value).filter(Number.isFinite);
      if (finite.length === 0) return "no values";
      const last = finite.at(-1)!;
      if (finite.length === 1) return spoken(last);
      const trend = last > finite[0]! ? "rising" : last < finite[0]! ? "falling" : "level";
      const low = spoken(Math.min(...finite));
      const high = spoken(Math.max(...finite));
      return `${spoken(last)}, ${trend}, ${low} to ${high} over ${finite.length} values`;
    },
  };
}
