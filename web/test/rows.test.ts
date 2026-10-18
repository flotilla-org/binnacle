// fromRows: the in-memory source over binnacle-memory in wasm, checked
// against a sort written here from the spec (stable, multi-column, nulls last
// both ways, NaN after every number, strings by code point).

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows } from "../src/rows";
import type { ChangeNotice, SortDirection, SortSpec, ViewSpec } from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  n: number | null;
  s: string | null;
  b: number | null;
}

const COLUMNS = [{ key: "n" }, { key: "s" }, { key: "b" }] as const;

const fields = {
  n: gs.oneOf(gs.just(null), gs.just(Number.NaN), gs.integers({ minValue: -5, maxValue: 5 }), gs.floats({ allowNan: false })),
  s: gs.oneOf(gs.just(null), gs.text({ maxSize: 3 }), gs.sampledFrom(["a", "B", "é", "😀", "ﬀ", ""])),
  b: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 2 })),
};

const records = gs.arrays(gs.record(fields), { maxSize: 40 });

const specs = gs.arrays(
  gs.tuples(gs.sampledFrom<"n" | "s" | "b">(["n", "s", "b"]), gs.sampledFrom<SortDirection>(["ascending", "descending"])),
  { maxSize: 3 },
);

function codePoints(a: string, b: string): number {
  const x = Array.from(a, (c) => c.codePointAt(0)!);
  const y = Array.from(b, (c) => c.codePointAt(0)!);
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i]! - y[i]!;
  return x.length - y.length;
}

/** -1, 0 or 1 with nulls (and then NaN, for numbers) last whichever the direction. */
function compare(a: number | string | null, b: number | string | null, direction: SortDirection): number {
  const rank = (v: number | string | null) => (v === null ? 2 : typeof v === "number" && Number.isNaN(v) ? 1 : 0);
  if (rank(a) !== rank(b)) return rank(a) - rank(b);
  if (rank(a) !== 0) return 0;
  const c = typeof a === "string" ? codePoints(a, b as string) : (a as number) - (b as number);
  const sign = Math.sign(c) || (Object.is(a, -0) && Object.is(b, 0) ? -1 : Object.is(a, 0) && Object.is(b, -0) ? 1 : 0);
  return direction === "ascending" ? sign : -sign;
}

function reference(rows: Rec[], spec: SortSpec<keyof Rec>, keep: (r: Rec) => boolean): number[] {
  return rows
    .map((_, i) => i)
    .sort((i, j) => {
      for (const { id, direction } of spec) {
        const c = compare(rows[i]![id], rows[j]![id], direction);
        if (c !== 0) return c;
      }
      return i - j;
    })
    .filter((i) => keep(rows[i]!));
}

/** A View Spec sorting the rows by `sort`. */
function rowsSorted<ColId = keyof Rec>(sort: SortSpec<ColId>): ViewSpec<number, ColId> {
  return { rows: { sort }, cols: { sort: [] } };
}

function order(source: ReturnType<typeof fromRows<Rec>>): number[] {
  return source.rows.window(0, source.rows.count() as number, new AbortController().signal) as number[];
}

describe("fromRows", () => {
  test("sorting agrees with a reference sort, filtered or not", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      try {
        const spec: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const filtered = tc.draw(gs.booleans());
        const keep = (r: Rec) => !filtered || r.b !== 1;
        if (filtered && tc.draw(gs.booleans())) source.filter((r) => keep(r));
        expect(source.view!(rowsSorted(spec), new AbortController().signal)).toEqual(rowsSorted(spec));
        if (filtered) source.filter((r) => keep(r));
        expect(order(source)).toEqual(reference(rows, spec, keep));
      } finally {
        source.destroy();
      }
    }));

  test("after rows change in place, a sort in force re-sorts by their new values, and each change is told", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
        tc.assume(rows.length > 0);
      const source = fromRows(rows, COLUMNS);
      const notices: ChangeNotice[] = [];
      source.attach!({ rows: { notify: (n) => notices.push(n) }, cols: { notify: () => {} } });
      try {
        const spec: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const filtered = tc.draw(gs.booleans());
        const keep = (r: Rec) => !filtered || r.b !== 1;
        if (filtered) source.filter((r) => keep(r));
        // The filter is not applied again on a change: the rows shown stay shown.
        const shown = new Set(rows.filter(keep));
        source.view!(rowsSorted(spec), new AbortController().signal);
        const changes = tc.draw(gs.integers({ minValue: 1, maxValue: 4 }));
        for (let i = 0; i < changes; i++) {
          const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
          const before = order(source).indexOf(id);
          Object.assign(rows[id]!, tc.draw(gs.record(fields)));
          notices.length = 0;
          source.changed(id);
          expect(order(source)).toEqual(reference(rows, spec, (r) => shown.has(r)));
          const told: ChangeNotice[] = before < 0 ? [] : [{ type: "ContentChanged", start: before, end: before + 1 }];
          if (spec.length) told.push({ type: "Reordered" });
          expect(notices).toEqual(told);
        }
      } finally {
        source.destroy();
      }
    }));

  test("a column of numbers and strings sorts as strings; booleans and dates sort as numbers", () => {
    type Mixed = { m: number | string; f: boolean; d: Date };
    const mixed: Mixed[] = [
      { m: 10, f: true, d: new Date(3) },
      { m: "9", f: false, d: new Date(1) },
      { m: 2, f: true, d: new Date(2) },
    ];
    const source = fromRows(mixed, [{ key: "m" }, { key: "f" }, { key: "d" }]);
    const by = (id: keyof Mixed) => {
      source.view!(rowsSorted([{ id, direction: "ascending" }]), new AbortController().signal);
      return source.rows.window(0, 3, new AbortController().signal);
    };
    expect(by("m")).toEqual([0, 2, 1]);
    expect(by("f")).toEqual([1, 0, 2]);
    expect(by("d")).toEqual([1, 2, 0]);
    source.destroy();
  });

  test("a View Spec that also sorts the columns is answered with the rows' sort alone", () => {
    const source = fromRows<Rec>([{ n: 2, s: null, b: 1 }, { n: 1, s: null, b: 0 }], COLUMNS);
    const byN: SortSpec<keyof Rec> = [{ id: "n", direction: "ascending" }];
    const asked = { rows: { sort: byN }, cols: { sort: [{ id: 1, direction: "descending" as const }] } };
    expect(source.view!(asked, new AbortController().signal)).toEqual(rowsSorted(byN));
    expect(order(source)).toEqual([1, 0]);
    source.destroy();
  });

  test("cells read the rows' own properties by row and column ID", () => {
    const source = fromRows<Rec>([{ n: 1, s: "x", b: null }, { n: 2, s: "y", b: 0 }], COLUMNS);
    expect(source.cells([1, 0], ["s", "b"], new AbortController().signal)).toEqual([
      ["y", 0],
      ["x", null],
    ]);
    expect(source.cols.window(0, 3, new AbortController().signal)).toEqual(["n", "s", "b"]);
    source.destroy();
  });

  test("sorting and filtering tell the attached sinks", () => {
    const source = fromRows<Rec>([{ n: 2, s: null, b: 1 }, { n: 1, s: null, b: 0 }], COLUMNS);
    const notices: ChangeNotice[] = [];
    const detach = source.attach!({ rows: { notify: (n) => notices.push(n) }, cols: { notify: () => {} } });
    source.view!(rowsSorted([{ id: "n", direction: "ascending" }]), new AbortController().signal);
    source.filter((r) => r.b === 0);
    source.filter(null);
    detach();
    source.filter((r) => r.b === 0);
    expect(notices.map((n) => n.type)).toEqual(["Reordered", "Refiltered", "Refiltered"]);
    source.destroy();
  });

  test("columns marked unsortable say so, and labels default to the property name", () => {
    const source = fromRows<Rec>([], [{ key: "n", label: "Number" }, { key: "s", sortable: false }]);
    expect(source.rows.support?.("n")).toEqual({ sort: true });
    expect(source.rows.support?.("s")).toEqual({ sort: false });
    expect(source.cols.label?.("n", 0)).toBe("Number");
    expect(source.cols.label?.("s", 1)).toBe("s");
    source.destroy();
  });

  test("in the Grid, a header click sorts, focus follows its row, and a filter that drops it moves focus", () => {
    const rows: Rec[] = [
      { n: 3, s: "c", b: 0 },
      { n: 1, s: "a", b: 1 },
      { n: 2, s: "b", b: 0 },
      { n: null, s: "d", b: 0 },
    ];
    const source = fromRows(rows, COLUMNS);
    const h = new Harness(source);
    try {
      h.frame();
      // Focus the row at Position 0 (n = 3), then sort by n.
      h.click(BAND + 10, HEADER + ROW_HEIGHT / 2);
      h.frame();
      h.click(BAND + COL_WIDTH / 2, HEADER / 2);
      h.frame();
      h.drain();
      h.frame();
      expect(h.values().map((r) => r[0])).toEqual([1, 2, 3, null]);
      expect(h.snapshot.rows.focus?.position).toBe(2);
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe(0);
      // Descending keeps nulls last.
      h.click(BAND + COL_WIDTH / 2, HEADER / 2);
      h.frame();
      h.drain();
      h.frame();
      expect(h.values().map((r) => r[0])).toEqual([3, 2, 1, null]);
      expect(h.snapshot.rows.focus?.position).toBe(0);
      source.filter((r) => r.n !== 3);
      h.frame();
      h.drain();
      h.frame();
      expect(h.snapshot.rows.count).toBe(3);
      expect(h.values().map((r) => r[0])).toEqual([2, 1, null]);
      const moved = h.feed.rowId(h.snapshot.rows.focus!.key!);
      // Clearing the filter keeps focus on the row it moved to.
      source.filter(null);
      h.frame();
      h.drain();
      h.frame();
      expect(h.snapshot.rows.count).toBe(4);
      expect(h.values().map((r) => r[0])).toEqual([3, 2, 1, null]);
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe(moved);
      expect(h.snapshot.rows.focus?.position).toBe(1);
    } finally {
      h.destroy();
      source.destroy();
    }
  });
});
