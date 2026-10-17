// The huge source: 10^15 rows by 12 columns, position identity on both axes,
// values made up from the Position, like the ratatui demo's `--huge`.

import type { TableSource } from "../../src";
import type { DemoSource } from "./index";

const ROWS = 1e15;
const COLS = 12;

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}

const source: TableSource<number, number, string> = {
  rows: { identity: "position", count: () => ROWS, window: range },
  cols: { identity: "position", count: () => COLS, window: range, label: (id) => `col ${id}` },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => `r${r}c${c}`)),
};

export const huge: DemoSource = {
  name: "huge",
  description: "10^15 rows, position identity",
  options: { source, columnWidth: 168 },
};
