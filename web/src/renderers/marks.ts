// What the chart renderers share: the colour their marks take, and numbers
// written out for their accessible text.

import type { CellCanvas } from "../renderer";

/**
 * `color`, or the Theme's focus colour. While the cell is lit, a mark the
 * same colour as the flash background (mono's black) takes the flash's text
 * colour so it stays visible.
 */
export function ink(cell: CellCanvas, color: string | undefined): string {
  const ink = color ?? cell.theme.focus.color;
  const { flash } = cell.theme;
  return cell.status.flashing >= 0.5 && ink === flash.background ? flash.color : ink;
}

const number = new Intl.NumberFormat("en", { maximumFractionDigits: 2 });

export const spoken = (v: number): string => number.format(v);
