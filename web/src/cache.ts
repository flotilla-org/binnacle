// The value cache: cell values by Key pair, so scrolling asks the Table
// Source only for cells it lacks. A cell asked for and not yet answered holds
// `PENDING`, so it is not asked for again meanwhile.

export const PENDING: unique symbol = Symbol("pending");

export interface ValueCache<V> {
  /** The value, `PENDING` while asked for, or `undefined` if never asked. */
  get(row: bigint, col: bigint): V | typeof PENDING | undefined;
  set(row: bigint, col: bigint, value: V | typeof PENDING): void;
  /** Removes the pair if it still holds `PENDING` (its answer was dropped). */
  unpend(row: bigint, col: bigint): void;
  /** Keeps only the items on `axis` that `keep` accepts. */
  retain(axis: "rows" | "cols", keep: (key: bigint) => boolean): void;
  clear(): void;
  /** Drops rows and columns outside the visible ones once the cache holds well over a screenful. */
  prune(rows: Set<bigint>, cols: Set<bigint>): void;
}

/** How many times the visible rows (or columns) the cache holds before pruning. */
const SLACK = 4;
const FLOOR = 256;

export function createValueCache<V>(): ValueCache<V> {
  const byRow = new Map<bigint, Map<bigint, V | typeof PENDING>>();
  let cols = 0;
  const retain: ValueCache<V>["retain"] = (axis, keep) => {
    if (axis === "rows") {
      for (const row of byRow.keys()) if (!keep(row)) byRow.delete(row);
      return;
    }
    cols = 0;
    for (const values of byRow.values()) {
      for (const col of values.keys()) if (!keep(col)) values.delete(col);
      cols = Math.max(cols, values.size);
    }
  };
  return {
    get: (row, col) => byRow.get(row)?.get(col),
    set(row, col, value) {
      let values = byRow.get(row);
      if (!values) byRow.set(row, (values = new Map()));
      values.set(col, value);
      cols = Math.max(cols, values.size);
    },
    unpend(row, col) {
      const values = byRow.get(row);
      if (values?.get(col) === PENDING) values.delete(col);
    },
    retain,
    clear() {
      byRow.clear();
      cols = 0;
    },
    prune(rows, visibleCols) {
      if (byRow.size > Math.max(FLOOR, SLACK * rows.size)) retain("rows", (k) => rows.has(k));
      if (cols > Math.max(FLOOR, SLACK * visibleCols.size)) retain("cols", (k) => visibleCols.has(k));
    },
  };
}
