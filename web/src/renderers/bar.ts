// The bar renderer: a fraction as a bar along a track across the cell.

import { PAD } from "../canvas/context";
import type { CellRenderer } from "../renderer";
import { ink } from "./marks";

export interface BarOptions<V> {
  /** The fraction a value shows, clamped into 0 to 1 (NaN shows none); the value itself by default. */
  fraction?: (value: V) => number;
  /** The width a column fits to, CSS px; 100 by default. */
  width?: number;
  /** The bar's colour; the Theme's focus colour by default. The track is the Theme's border colour. */
  color?: string;
}

/** The bar's thickness, CSS px, at most. */
const THICKNESS = 8;

export function barRenderer<V = number>(options: BarOptions<V> = {}): CellRenderer<V> {
  const fraction = (value: V): number => {
    const f = options.fraction ? options.fraction(value) : Number(value);
    return Number.isNaN(f) ? NaN : Math.min(1, Math.max(0, f));
  };
  return {
    draw(cell, value) {
      const { ctx } = cell;
      const across = Math.max(0, cell.width - 2 * PAD);
      const thick = Math.min(THICKNESS, Math.max(2, cell.height - 8));
      const y = (cell.height - thick) / 2;
      // The track fades out as a flash lights the cell, so the bar stays the mark.
      ctx.globalAlpha = 1 - 0.7 * cell.status.flashing;
      ctx.fillStyle = cell.theme.border;
      ctx.beginPath();
      ctx.roundRect(PAD, y, across, thick, thick / 2);
      ctx.fill();
      ctx.globalAlpha = 1;
      const f = fraction(value);
      if (!(f > 0)) return;
      ctx.fillStyle = ink(cell, options.color);
      ctx.beginPath();
      ctx.roundRect(PAD, y, Math.max(thick, f * across), thick, thick / 2);
      ctx.fill();
    },
    preferredWidth: () => options.width ?? 100,
    text(value) {
      const f = fraction(value);
      return Number.isNaN(f) ? "no value" : `${Math.round(f * 100)}%`;
    },
  };
}
