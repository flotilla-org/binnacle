// Hiding through the feed: the real wasm Grid over fromRows, hiding and
// showing rows and columns by ID (some the data lacks), checked against a
// model of the hidden lists. The hidden items leave each axis, the answer
// prunes IDs the data lacks, and a column's size stays while it is hidden.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows } from "../src/rows";
import type { TableSource } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  a: number;
  b: string;
  c: number | null;
  d: string;
}

const COLUMNS = [{ key: "a" }, { key: "b" }, { key: "c" }, { key: "d" }] as const;
type Field = (typeof COLUMNS)[number]["key"];
/** The columns, and one the data lacks. */
const FIELDS = ["a", "b", "c", "d", "z"] as Field[];

const records = gs.arrays(
  gs.record({
    a: gs.integers({ minValue: 0, maxValue: 9 }),
    b: gs.sampledFrom(["x", "y", ""]),
    c: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 3 })),
    d: gs.sampledFrom(["p", "q"]),
  }),
  { minSize: 1, maxSize: 10 },
);

type Step =
  | { type: "hide" | "show"; axis: "rows"; ids: number[] }
  | { type: "hide" | "show"; axis: "cols"; ids: Field[] }
  | { type: "size"; id: Field; size: number };

const ROWS = 0;
const COLS = 1;

interface Settled {
  h: Harness<number, Field, unknown>;
  rows: Rec[];
  hidden: { rows: number[]; cols: Field[] };
  sizes: Map<Field, number>;
}

/** Runs random hides, shows and column sizes through the feed, calling `check` once each has settled. */
const drive = (check: (s: Settled) => void) =>
  hegel.test(
    (tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      const h = new Harness(source);
      try {
        h.settled();
        expect([h.snapshot.rows.hideSupport, h.snapshot.cols.hideSupport]).toEqual([true, true]);
        const step = gs.oneOf(
          gs.tuples(gs.sampledFrom<"hide" | "show">(["hide", "hide", "show"]), gs.arrays(gs.integers({ minValue: 0, maxValue: rows.length + 1 }), { minSize: 1, maxSize: 3 })).map(
            ([type, ids]): Step => ({ type, axis: "rows", ids }),
          ),
          gs.tuples(gs.sampledFrom<"hide" | "show">(["hide", "hide", "show"]), gs.arrays(gs.sampledFrom(FIELDS), { minSize: 1, maxSize: 2 })).map(
            ([type, ids]): Step => ({ type, axis: "cols", ids }),
          ),
          gs.tuples(gs.sampledFrom(FIELDS.slice(0, 4)), gs.integers({ minValue: 30, maxValue: 120 })).map(
            ([id, size]): Step => ({ type: "size", id, size }),
          ),
        );
        // The model: each axis's hidden IDs as the source keeps them, and the sizes set.
        const hidden: { rows: number[]; cols: Field[] } = { rows: [], cols: [] };
        const sizes = new Map<Field, number>();
        const has = { rows: (id: number) => id < rows.length, cols: (id: Field) => (FIELDS.slice(0, 4) as string[]).includes(id) };
        for (const s of tc.draw(gs.arrays(step, { maxSize: 8 }))) {
          if (s.type === "size") {
            h.grid.set_size(COLS, h.feed.colKey(s.id), s.size);
            sizes.set(s.id, s.size);
          } else if (s.axis === "rows") {
            const keys = BigUint64Array.from(s.ids, (id) => h.feed.rowKey(id));
            if (s.type === "hide") h.grid.hide(ROWS, keys);
            else h.grid.show(ROWS, keys);
            hidden.rows = s.type === "hide" ? [...new Set([...hidden.rows, ...s.ids])] : hidden.rows.filter((i) => !s.ids.includes(i));
            hidden.rows = hidden.rows.filter(has.rows);
          } else {
            const keys = BigUint64Array.from(s.ids, (id) => h.feed.colKey(id));
            if (s.type === "hide") h.grid.hide(COLS, keys);
            else h.grid.show(COLS, keys);
            hidden.cols = s.type === "hide" ? [...new Set([...hidden.cols, ...s.ids])] : hidden.cols.filter((i) => !s.ids.includes(i));
            hidden.cols = hidden.cols.filter(has.cols);
          }
          h.drain();
          h.settled();
          check({ h, rows, hidden, sizes });
        }
      } finally {
        h.destroy();
        source.destroy();
      }
    },
    { testCases: 150 },
  );

describe("hiding through the feed", () => {
  test("hidden items leave each axis and the answer prunes IDs the data lacks", () =>
    drive(({ h, rows, hidden }) => {
      const snap = h.snapshot;
      expect(snap.requestedView).toBeNull();
      const applied = snap.appliedView;
      expect(applied.rows.hidden.map((k) => h.feed.rowId(k))).toEqual(hidden.rows);
      expect(applied.cols.hidden.map((k) => h.feed.colId(k))).toEqual(hidden.cols);
      const shownRows = rows.map((_, i) => i).filter((i) => !hidden.rows.includes(i));
      const shownCols = COLUMNS.map((c) => c.key).filter((c) => !hidden.cols.includes(c));
      expect([snap.rows.count, snap.cols.count]).toEqual([shownRows.length, shownCols.length]);
      expect(snap.rows.strip.map((i) => h.feed.rowId(i.key!))).toEqual(shownRows.slice(0, snap.rows.strip.length));
      expect(snap.cols.strip.map((i) => h.feed.colId(i.key!))).toEqual(shownCols);
    }));

  test("a column's size stays while it is hidden", () =>
    drive(({ h, sizes }) => {
      for (const [id, size] of sizes) expect(h.grid.size_override(COLS, h.feed.colKey(id))).toBe(size);
    }));

  test("an axis whose source does not hide takes no hide, and a source without view hides nothing", () => {
    const rows = [{ a: 1, b: "x", c: null, d: "p" }];
    const inner = fromRows<Rec>(rows, COLUMNS);
    const { hide: _, ...cols } = inner.cols;
    const source: TableSource<number, Field, unknown> = { ...inner, cols };
    const h = new Harness(source);
    try {
      const snap = h.settled();
      expect([snap.rows.hideSupport, snap.cols.hideSupport]).toEqual([true, false]);
      h.grid.hide(COLS, BigUint64Array.of(h.feed.colKey("a")));
      expect((h.grid.poll_effects() as { type: string }[]).filter((e) => e.type === "ViewRequested")).toEqual([]);
    } finally {
      h.destroy();
      inner.destroy();
    }
    const { view: __, ...viewless } = fromRows<Rec>(rows, COLUMNS);
    const plain = new Harness(viewless as TableSource<number, Field, unknown>);
    try {
      const snap = plain.settled();
      expect([snap.rows.hideSupport, snap.cols.hideSupport]).toEqual([false, false]);
    } finally {
      plain.destroy();
    }
  });
});

describe("the hide cap", () => {
  /** A declared cap: none, a small one the few items can pass, or one past 1,000. */
  const cap = (most: number) =>
    gs.oneOf(gs.just(undefined), gs.integers({ minValue: 0, maxValue: most }), gs.integers({ minValue: 998, maxValue: 2 ** 40 }));
  const declare = (rows: number | undefined, cols: number | undefined) => ({
    hideCap: { ...(rows === undefined ? {} : { rows }), ...(cols === undefined ? {} : { cols }) },
  });

  test("a source's cap reaches each axis, 1,000 at most", () =>
    hegel.test(
      (tc) => {
        const rows = tc.draw(records);
        const declared = { rows: tc.draw(cap(12)), cols: tc.draw(cap(5)) };
        const source = fromRows(rows, COLUMNS, declare(declared.rows, declared.cols));
        const h = new Harness(source);
        try {
          const snap = h.settled();
          expect([snap.rows.hideCap, snap.cols.hideCap]).toEqual([
            Math.min(declared.rows ?? 1000, 1000),
            Math.min(declared.cols ?? 1000, 1000),
          ]);
        } finally {
          h.destroy();
          source.destroy();
        }
      },
      { testCases: 100 },
    ));

  test("a hide is refused, hiding nothing, exactly when the items hidden would pass the cap", () =>
    hegel.test(
      (tc) => {
        const rows = tc.draw(records);
        const caps = { rows: tc.draw(gs.integers({ minValue: 0, maxValue: rows.length + 1 })), cols: tc.draw(gs.integers({ minValue: 0, maxValue: 4 })) };
        const source = fromRows(rows, COLUMNS, declare(caps.rows, caps.cols));
        const h = new Harness(source);
        try {
          h.settled();
          const ids = { rows: rows.map((_, i) => i), cols: COLUMNS.map((c) => c.key) as unknown[] };
          const hidden = { rows: new Set<unknown>(), cols: new Set<unknown>() };
          const step = gs.tuples(
            gs.sampledFrom<"rows" | "cols">(["rows", "cols"]),
            gs.sampledFrom<"hide" | "show">(["hide", "hide", "show"]),
            gs.arrays(gs.integers({ minValue: 0, maxValue: 9 }), { minSize: 1, maxSize: 4 }),
          );
          for (const [axis, type, at] of tc.draw(gs.arrays(step, { maxSize: 8 }))) {
            const picked = at.map((i) => ids[axis][i % ids[axis].length]!);
            const keys = BigUint64Array.from(picked, (id) => (axis === "rows" ? h.feed.rowKey(id as number) : h.feed.colKey(id as Field)));
            const code = axis === "rows" ? ROWS : COLS;
            if (type === "show") {
              h.grid.show(code, keys);
              for (const id of picked) hidden[axis].delete(id);
            } else {
              const after = new Set([...hidden[axis], ...picked]);
              const over = after.size > caps[axis];
              expect(h.grid.hide(code, keys)).toBe(!over);
              if (!over) hidden[axis] = after;
            }
            h.drain();
            const snap = h.settled();
            expect(new Set(snap.appliedView.rows.hidden.map((k) => h.feed.rowId(k)))).toEqual(hidden.rows);
            expect(new Set(snap.appliedView.cols.hidden.map((k) => h.feed.colId(k)))).toEqual(hidden.cols);
          }
        } finally {
          h.destroy();
          source.destroy();
        }
      },
      { testCases: 150 },
    ));
});
