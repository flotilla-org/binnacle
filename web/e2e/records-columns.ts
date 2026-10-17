// Where the records demo's columns sit, from the widths it starts them at.

import { WIDTHS } from "../demo/sources/records-data";

/** Inside the first column whatever the row-number band's width (40 to 60 px). */
export const FIRST = 62;

const GAP = 1;
const lefts: number[] = [0];
for (const w of Object.values(WIDTHS)) if (typeof w === "number") lefts.push(lefts.at(-1)! + w + GAP);

/** An x inside column `col` (a Position, in the demo's starting order), from the layer's left edge. */
export const column = (col: number) => FIRST + lefts[col]!;
