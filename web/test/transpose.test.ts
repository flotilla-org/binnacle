// transpose: a source on its side, checked against the same source the right
// way up. Each answer about the turned table must be the answer about the
// source with rows and columns swapped.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows } from "../src/rows";
import type { AppliedView, ChangeNotice, FindQuery, SortDirection, TableSink, TextQuery, ViewSpec } from "../src/source";
import { transpose } from "../src/transpose";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  n: number | null;
  s: string | null;
}

type Field = keyof Rec;

const COLUMNS = [{ key: "n" }, { key: "s" }] as const;
const FIELDS: Field[] = ["n", "s"];

const records = gs.arrays(
  gs.record({
    n: gs.oneOf(gs.just(null), gs.integers({ minValue: -3, maxValue: 13 })),
    s: gs.oneOf(gs.just(null), gs.sampledFrom(["a", "ab", "B", "1", ""])),
  }),
  { maxSize: 12 },
);

const texts = gs.sampledFrom(["1", "a", "b", "-", "3"]);

const signal = () => new AbortController().signal;

const NONE = { filter: { quick: null, conditions: [] }, sort: [] };

function shown(text: string): TextQuery<Field> {
  return { text, caseSensitive: false, regex: false, searches: "shown" };
}

/** Sinks that keep each axis's Change Notices. */
function sinks<RowId, ColId>(notices: { rows: ChangeNotice[]; cols: ChangeNotice[] }): TableSink<RowId, ColId> {
  const ignore = () => {};
  return {
    rows: { notify: (n) => void notices.rows.push(n), supportChanged: ignore },
    cols: { notify: (n) => void notices.cols.push(n), supportChanged: ignore },
    viewChanged: ignore,
    findCounted: ignore,
  };
}

describe("transpose", () => {
  test("the turned table is the source on its side: counts, windows, cells, views and Change Notices", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const upright = fromRows(rows, COLUMNS);
      const inner = fromRows(rows, COLUMNS);
      const turned = transpose(inner);
      const told = { upright: { rows: [], cols: [] }, turned: { rows: [], cols: [] } };
      upright.attach!(sinks(told.upright));
      turned.attach!(sinks(told.turned));
      try {
        const sort = tc
          .draw(gs.arrays(gs.tuples(gs.sampledFrom(FIELDS), gs.sampledFrom<SortDirection>(["ascending", "descending"])), { maxSize: 2 }))
          .map(([id, direction]) => ({ id, direction }));
        const quick = tc.draw(gs.optional(texts));
        // The shown fields, or some named ones: either way, items of the other axis, which turning leaves where they are.
        const searches: "shown" | Field[] = tc.draw(gs.booleans()) ? "shown" : tc.draw(gs.arrays(gs.sampledFrom(FIELDS), { maxSize: 2 }));
        const filter = { quick: quick === null ? null : { ...shown(quick), regex: tc.draw(gs.booleans()), searches }, conditions: [] };
        const records = { filter, sort };
        const asked: ViewSpec<number, Field> = { rows: records, cols: NONE };
        const turnedAsk: ViewSpec<Field, number> = { rows: NONE, cols: records };
        const applied = upright.view!(asked, signal()) as AppliedView<number, Field>;
        expect(turned.view!(turnedAsk, signal())).toEqual({
          rows: applied.cols,
          cols: applied.rows,
          totals: { rows: applied.totals!.cols, cols: applied.totals!.rows },
        });
        expect(told.turned).toEqual({ rows: told.upright.cols, cols: told.upright.rows });
        const n = upright.rows.count() as number;
        expect(turned.cols.count()).toBe(n);
        expect(turned.rows.count()).toBe(FIELDS.length);
        const ids = upright.rows.window(0, n, signal()) as number[];
        expect(turned.cols.window(0, n, signal())).toEqual(ids);
        expect(turned.rows.window(0, 2, signal())).toEqual(FIELDS);
        const cells = upright.cells(ids, FIELDS, signal()) as unknown[][];
        expect(turned.cells(FIELDS, ids, signal())).toEqual(FIELDS.map((_, f) => ids.map((_, r) => cells[r]![f])));
      } finally {
        upright.destroy();
        inner.destroy();
      }
    }));

  test("find steps, marks and counts as on the source with rows and columns swapped", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const upright = fromRows(rows, COLUMNS);
      const turned = transpose(fromRows(rows, COLUMNS));
      try {
        const text = tc.draw(texts);
        const by = tc.draw(gs.sampledFrom<"rows" | "cols">(["rows", "cols"]));
        const find: FindQuery<Field> = { query: shown(text), by };
        const turnedFind: FindQuery<number> = { query: shown(text) as TextQuery<never>, by: by === "rows" ? "cols" : "rows" };
        const ids = rows.map((_, i) => i);
        expect(turned.find!.count!(turnedFind, signal())).toEqual(upright.find.count!(find, signal()));
        const marks = upright.find.marks(ids, FIELDS, find, signal()) as boolean[][];
        expect(turned.find!.marks(FIELDS, ids, turnedFind, signal())).toEqual(FIELDS.map((_, f) => ids.map((_, r) => marks[r]![f])));
        const direction = tc.draw(gs.sampledFrom<"next" | "previous">(["next", "previous"]));
        let from = null as { row: number; col: Field } | null;
        for (let i = tc.draw(gs.integers({ minValue: 1, maxValue: 5 })); i > 0; i--) {
          const step = upright.find.next(from, direction, find, signal()) as { row: number; col: Field } | null;
          const turnedStep = turned.find!.next(from && { row: from.col, col: from.row }, direction, turnedFind, signal());
          expect(turnedStep).toEqual(step && { ...step, row: step.col, col: step.row });
          from = step && { row: step.row, col: step.col };
        }
      } finally {
        upright.destroy();
      }
    }));

  test("a find naming the columns it searches finds nothing, rather than searching the wrong items", () => {
    const turned = transpose(fromRows([{ n: 13, s: "a" }], COLUMNS));
    const named: FindQuery<number> = { query: { text: "1", caseSensitive: false, regex: false, searches: [0] }, by: "rows" };
    expect(turned.find!.next(null, "next", named, signal())).toBeNull();
    expect(turned.find!.marks(FIELDS, [0], named, signal())).toEqual([[false], [false]]);
    expect(turned.find!.count!(named, signal())).toEqual({ total: 0 });
    const all: FindQuery<number> = { query: { text: "1", caseSensitive: false, regex: false, searches: "shown" }, by: "rows" };
    expect(turned.find!.next(null, "next", all, signal())).toMatchObject({ row: "n", col: 0 });
  });
});
