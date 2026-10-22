// fromRows's comparison conditions, checked against a model written here
// from the spec: numbers compare on columns of numbers, dates (by their
// milliseconds) on columns of dates, both ends of a between included, and a
// null, a NaN or a value of the other kind never compares, so `not` keeps
// it. A column compares the kind its values share; a comparison on a
// column of another kind, or of no values, is left out of the answer.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows } from "../src/rows";
import type { AppliedView, CompareOp, Comparison, Condition, Scalar, TableSink, ViewSpec } from "../src/source";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  n: number | null;
  d: Date | null;
  s: string | null;
}

const COLUMNS = [{ key: "n" }, { key: "d" }, { key: "s" }] as const;
const DAY = 86_400_000;

const sinks: TableSink<number, keyof Rec> = {
  rows: { notify: () => {}, supportChanged: () => {}, movableChanged: () => {} },
  cols: { notify: () => {}, supportChanged: () => {}, movableChanged: () => {} },
  viewChanged: () => {},
  findCounted: () => {},
};

/** Mostly small numbers, so comparisons often meet equal values; NaN, both zeros and the infinities among them. */
const number = gs.oneOf(
  gs.integers({ minValue: -2, maxValue: 2 }).map((n) => n / 2),
  gs.sampledFrom([Number.NaN, -0, 0, Infinity, -Infinity]),
  gs.floats({ allowNan: false }),
);

/** Mostly midnights a few days apart, sometimes a time of day. */
const millis = gs.oneOf(
  gs.integers({ minValue: -2, maxValue: 2 }).map((d) => d * DAY),
  gs.integers({ minValue: -3 * DAY, maxValue: 3 * DAY }),
);

const records = gs.arrays(
  gs.record({
    n: gs.oneOf(gs.just(null), number),
    d: gs.oneOf(gs.just(null), millis.map((ms) => new Date(ms))),
    s: gs.oneOf(gs.just(null), gs.sampledFrom(["1", "a", ""])),
  }),
  { maxSize: 12 },
);

const scalar = gs.oneOf(
  number.map((value): Scalar => ({ type: "number", value })),
  millis.map((value): Scalar => ({ type: "date", value })),
);

const OPS: CompareOp[] = ["equal", "notEqual", "less", "atMost", "greater", "atLeast"];

const comparison = gs.composite((tc): Comparison => {
  if (tc.draw(gs.integers({ minValue: 0, maxValue: 6 })) === 6) {
    // Mostly both ends of one kind, sometimes not.
    const low = tc.draw(scalar);
    const same = low.type === "number" ? number : millis;
    const high = tc.draw(gs.booleans()) ? { type: low.type, value: tc.draw(same) } : tc.draw(scalar);
    return { op: "between", low, high };
  }
  return { op: tc.draw(gs.sampledFrom(OPS)), value: tc.draw(scalar) };
});

const conditions = gs.arrays(
  gs.composite(
    (tc): Condition<keyof Rec> => ({
      id: tc.draw(gs.sampledFrom<keyof Rec>(["n", "d", "s"])),
      kind: { type: "Compare", comparison: tc.draw(comparison) },
      not: tc.draw(gs.booleans()),
    }),
  ),
  { maxSize: 3 },
);

/** `v` against `s` as -1, 0 or 1, or `undefined` if they don't compare. */
function order(v: unknown, s: Scalar): number | undefined {
  if (s.type === "number" && typeof v === "number") {
    if (Number.isNaN(v) || Number.isNaN(s.value)) return undefined;
    return v < s.value ? -1 : v > s.value ? 1 : 0;
  }
  if (s.type === "date" && v instanceof Date) return v.getTime() < s.value ? -1 : v.getTime() > s.value ? 1 : 0;
  return undefined;
}

function passes(c: Comparison, v: unknown): boolean {
  const is = (s: Scalar, wanted: number[]) => {
    const o = order(v, s);
    return o !== undefined && wanted.includes(o);
  };
  switch (c.op) {
    case "equal":
      return is(c.value, [0]);
    case "notEqual":
      return is(c.value, [-1, 1]);
    case "less":
      return is(c.value, [-1]);
    case "atMost":
      return is(c.value, [-1, 0]);
    case "greater":
      return is(c.value, [1]);
    case "atLeast":
      return is(c.value, [0, 1]);
    case "between":
      return is(c.low, [0, 1]) && is(c.high, [-1, 0]);
  }
}

/** The kind both ends of `c` share, if they do. */
function kindOf(c: Comparison): Scalar["type"] | undefined {
  if (c.op !== "between") return c.value.type;
  return c.low.type === c.high.type ? c.low.type : undefined;
}

/** The kind a column of `rows` compares: column n numbers and column d dates, if they hold any; column s none. */
function kindIn(rows: Rec[], id: keyof Rec): Scalar["type"] | undefined {
  if (id === "s" || rows.every((r) => r[id] === null)) return undefined;
  return id === "n" ? "number" : "date";
}

/** Whether fromRows applies `condition` over `rows`: a comparison of the kind its column holds. */
function applies(rows: Rec[], condition: Condition<keyof Rec>): boolean {
  const kind = condition.kind.type === "Compare" ? kindOf(condition.kind.comparison) : undefined;
  return kind !== undefined && kindIn(rows, condition.id) === kind;
}

function filtering(conditions: Condition<keyof Rec>[]): ViewSpec<number, keyof Rec> {
  return {
    rows: { filter: { quick: null, conditions }, sort: [], hidden: [] },
    cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
  };
}

function shown(source: ReturnType<typeof fromRows<Rec>>): number[] {
  return source.rows.window(0, source.rows.count() as number, new AbortController().signal) as number[];
}

describe("fromRows's comparisons", () => {
  test("whatever comparisons arrive, the rows shown are those whose values pass every applied one, and the rest are left out of the answer", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const asked = tc.draw(conditions);
      const source = fromRows(rows, COLUMNS);
      try {
        source.attach!(sinks);
        const answer = source.view!(filtering(asked), new AbortController().signal) as AppliedView<number, keyof Rec>;
        const applied = asked.filter((c) => applies(rows, c));
        expect(answer.rows.filter?.conditions).toEqual(applied);
        const model = rows
          .map((_, i) => i)
          .filter((i) =>
            applied.every((c) => passes((c.kind as { comparison: Comparison }).comparison, rows[i]![c.id]) !== c.not),
          );
        expect(shown(source)).toEqual(model);
      } finally {
        source.destroy();
      }
    }, { testCases: 1000 }));

  test("a column of numbers compares numbers, a column of dates dates, and any other none; the columns compare a row's values when every column holds one kind", () => {
    const mixed = fromRows([{ n: 1, d: new Date(0), s: "a" }], COLUMNS);
    const numbers = fromRows([{ a: 1, b: null, c: 2n }], [{ key: "a" }, { key: "b" }, { key: "c" }]);
    const dates = fromRows([{ a: new Date(0), b: null }], [{ key: "a" }, { key: "b", filterable: false }]);
    try {
      expect(COLUMNS.map((c) => mixed.rows.support?.(c.key).compare)).toEqual(["numbers", "dates", null]);
      expect(mixed.cols.support?.(0).compare).toBeNull();
      expect(numbers.rows.support?.("b").compare).toBeNull();
      expect(numbers.cols.support?.(0).compare).toBe("numbers");
      expect(dates.rows.support?.("a").compare).toBe("dates");
      expect(dates.rows.support?.("b").compare).toBeNull();
      expect(dates.cols.support?.(0).compare).toBe("dates");
    } finally {
      for (const s of [mixed, numbers, dates]) s.destroy();
    }
  });

  test("the columns shown are those whose value in each condition's row passes it", () => {
    const rows = [
      { a: 5, b: 50, c: null },
      { a: Number.NaN, b: -1, c: 3 },
    ];
    const source = fromRows(rows, [{ key: "a" }, { key: "b" }, { key: "c" }]);
    try {
      const at = (op: CompareOp, value: number, row: number, not = false): Condition<number> => ({
        id: row,
        kind: { type: "Compare", comparison: { op, value: { type: "number", value } } },
        not,
      });
      const view = (...conditions: Condition<number>[]) => {
        source.view!(
          {
            rows: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
            cols: { filter: { quick: null, conditions }, sort: [], hidden: [] },
          },
          new AbortController().signal,
        );
        return source.cols.window(0, source.cols.count() as number, new AbortController().signal);
      };
      expect(view(at("atLeast", 5, 0))).toEqual(["a", "b"]);
      expect(view(at("less", 10, 1))).toEqual(["b", "c"]);
      expect(view(at("less", 10, 1, true))).toEqual(["a"]);
      expect(view(at("greater", 0, 0), at("notEqual", 3, 1))).toEqual(["b"]);
    } finally {
      source.destroy();
    }
  });
});
