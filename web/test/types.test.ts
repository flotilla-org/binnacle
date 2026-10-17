// What the type checker accepts and rejects in a Table Source. The checks are
// `pnpm typecheck`'s; the runtime test only keeps vitest from calling the
// file empty.

import { expect, test } from "vitest";
import type { TableSource } from "../src/source";

type Cell = [row: string, sheet: number];

const tupleRows = {
  count: () => 1,
  window: (): Cell[] => [["a", 1]],
};

export const withIdKey: TableSource<Cell, string> = {
  rows: { ...tupleRows, idKey: ([row, sheet]) => `${sheet}/${row}` },
  cols: { count: () => 1, window: () => ["x"] },
  cells: () => [[1]],
};

export const withoutIdKey: TableSource<Cell, string> = {
  // @ts-expect-error: a tuple ID needs idKey
  rows: tupleRows,
  cols: { count: () => 1, window: () => ["x"] },
  cells: () => [[1]],
};

export const objectColumns: TableSource<string, { name: string }> = {
  rows: { count: () => 1, window: () => ["a"] },
  // @ts-expect-error: an object ID needs idKey
  cols: { count: () => 1, window: () => [{ name: "x" }] },
  cells: () => [[1]],
};

export const primitives: TableSource<string, bigint> = {
  rows: { count: () => 1, window: () => ["a"] },
  cols: { count: () => 1, window: () => [1n] },
  cells: () => [[1]],
};

test("the Table Source type checks compile", () => {
  expect(withIdKey.rows.idKey?.(["r", 2])).toBe("2/r");
});
