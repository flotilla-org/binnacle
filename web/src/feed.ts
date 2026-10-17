// Plumbing between a Table Source and the Grid: counts, Window answers and
// the visible block of cell values. This slice handles synchronous answers
// with position identity only.

import { AXES, AXIS_CODE } from "./codes";
import type { Content } from "./content";
import type { Snapshot, StripItem } from "./snapshot";
import type { AxisSource, MaybePromise, TableSource } from "./source";
import type { WebGrid } from "./wasm";

export interface Feed {
  counts(): { rows: number; cols: number };
  /** Answers the Windows `begin_frame` asked for (`[rowStart, rowEnd, colStart, colEnd]`, `NaN` for none). */
  fill(grid: WebGrid, requests: Float64Array): void;
  /** The values and labels for the Snapshot's visible items. */
  content(s: Snapshot): Content;
  /** Aborts outstanding requests. */
  destroy(): void;
}

function now<T>(answer: MaybePromise<T>, what: string): T {
  if (answer instanceof Promise) throw new Error(`binnacle: ${what} answered with a Promise, which is not supported yet`);
  return answer;
}

function checkIdentity(axis: AxisSource<unknown, unknown>, name: string) {
  if (axis.identity !== "position") {
    throw new Error(`binnacle: the ${name} axis needs identity: "position" until Key Interners land`);
  }
}

export function createFeed<V>(source: TableSource<number, number, V>): Feed {
  checkIdentity(source.rows, "rows");
  checkIdentity(source.cols, "cols");
  const abort = new AbortController();
  const axes = { rows: source.rows, cols: source.cols };
  // The value block for the visible Keys it was asked for.
  let blockRows: (bigint | null)[] = [];
  let blockCols: (bigint | null)[] = [];
  let block: (V | undefined)[][] = [];

  const sameKeys = (a: (bigint | null)[], items: StripItem[]) =>
    a.length === items.length && items.every((item, i) => item.key === a[i]);

  const refresh = (s: Snapshot) => {
    const rows = s.rows.strip;
    const cols = s.cols.strip;
    if (sameKeys(blockRows, rows) && sameKeys(blockCols, cols)) return;
    blockRows = rows.map((i) => i.key);
    blockCols = cols.map((i) => i.key);
    const ids = (items: StripItem[]) => items.flatMap((i) => (i.key === null ? [] : [Number(i.key)]));
    const rowIds = ids(rows);
    const colIds = ids(cols);
    const answer = rowIds.length && colIds.length ? now(source.cells(rowIds, colIds, abort.signal), "cells") : [];
    // Spread the answer back over the Strips, leaving unfilled items empty.
    let r = 0;
    block = rows.map((row) => {
      if (row.key === null) return [];
      const values = answer[r++] ?? [];
      let c = 0;
      return cols.map((col) => (col.key === null ? undefined : values[c++]));
    });
  };

  const content: Content = {
    value: (row, col) => block[row]?.[col],
    label: (axis, item) => {
      const id = item.key === null ? item.position : Number(item.key);
      const label = axes[axis].label?.(id, item.position);
      return label ?? (axis === "rows" ? String(item.position + 1) : String(id));
    },
  };

  return {
    counts: () => ({
      rows: now(source.rows.count(), "rows.count"),
      cols: now(source.cols.count(), "cols.count"),
    }),
    fill(grid, requests) {
      for (const axis of AXES) {
        const i = AXIS_CODE[axis];
        const start = requests[i * 2]!;
        const end = requests[i * 2 + 1]!;
        if (Number.isNaN(start)) continue;
        const ids = now(axes[axis].window(start, end, abort.signal), `${axis}.window`);
        grid.set_window(i, start, BigUint64Array.from(ids, (id) => BigInt(id)));
      }
    },
    content(s) {
      refresh(s);
      return content;
    },
    destroy: () => abort.abort(),
  };
}
