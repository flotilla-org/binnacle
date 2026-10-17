// Initial sizes per item: fixed sizes land with the item's Key, fits once the
// visible values are in, fills share what the axis has left, and a Size
// Override already there (a user's resize) always wins.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import type { InitialSize } from "../src/sizes";
import type { TableSource } from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

/** Columns `c0..`, rows `r0..`; a cell's value is as many `x` as its column's `widths` entry. */
function table(rows: number, widths: number[]): TableSource<string, string, string> {
  const rowIds = Array.from({ length: rows }, (_, i) => `r${i}`);
  const colIds = widths.map((_, j) => `c${j}`);
  return {
    rows: { count: () => rowIds.length, window: (s, e) => rowIds.slice(s, e), sort: (spec) => spec },
    cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
    cells: (rs, cs) => rs.map(() => cs.map((c) => "x".repeat(widths[Number(c.slice(1))]!))),
  };
}

/** The size each column has in the Snapshot, by ID. */
function colSizes(h: Harness<string, string, string>): Map<string, number> {
  return new Map(h.snapshot.cols.strip.map((i) => [h.feed.colId(i.key!)!, i.size]));
}

const fixed = gs.oneOf(gs.just<InitialSize>(undefined), gs.integers({ minValue: 10, maxValue: 200 }));

describe("initial sizes", () => {
  test("fixed widths show from the first frame; the rest keep the default", () =>
    hegel.test((tc) => {
      const choices = tc.draw(gs.arrays(fixed, { minSize: 1, maxSize: 6 }));
      const h = new Harness(table(5, choices.map(() => 1)), 2000, 300, {
        cols: (id) => choices[Number(id.slice(1))],
      });
      try {
        h.frame();
        const sizes = colSizes(h);
        choices.forEach((c, j) => expect(sizes.get(`c${j}`)).toBe(typeof c === "number" ? c : COL_WIDTH));
      } finally {
        h.destroy();
      }
    }));

  test("a fit column takes its widest visible value, once the values are in", () =>
    hegel.test((tc) => {
      const widths = tc.draw(gs.arrays(gs.integers({ minValue: 1, maxValue: 20 }), { minSize: 1, maxSize: 5 }));
      const fit = tc.draw(gs.integers({ minValue: 0, maxValue: widths.length - 1 }));
      const h = new Harness(table(5, widths), 2000, 300, { cols: (id) => (id === `c${fit}` ? "fit" : undefined) });
      try {
        h.settled();
        const sizes = colSizes(h);
        widths.forEach((w, j) => expect(sizes.get(`c${j}`)).toBe(j === fit ? 10 * w : COL_WIDTH));
      } finally {
        h.destroy();
      }
    }));

  test("a fit waits for late values, and a resize before they land wins", async () => {
    const late = latecomer(table(5, [12, 3]));
    const h = new Harness(late.source, 2000, 300, { cols: () => "fit" });
    try {
      h.settled();
      expect([...colSizes(h).values()]).toEqual([COL_WIDTH, COL_WIDTH]);
      h.grid.set_size(1, h.feed.colKey("c1"), 55);
      await late.release();
      h.settled();
      expect(Object.fromEntries(colSizes(h))).toEqual({ c0: 120, c1: 55 });
    } finally {
      h.destroy();
    }
  });
});

describe("fill", () => {
  test("fill columns share the width the others leave, never shrinking below the default", () =>
    hegel.test((tc) => {
      const choices = tc.draw(
        gs.arrays(gs.oneOf(fixed, gs.just<InitialSize>("fill"), gs.just<InitialSize>("fit")), { minSize: 1, maxSize: 6 }),
      );
      const width = tc.draw(gs.integers({ minValue: 100, maxValue: 1500 }));
      const h = new Harness(table(5, choices.map(() => 4)), width, 300, { cols: (id) => choices[Number(id.slice(1))] });
      try {
        h.settled();
        const sizes = colSizes(h);
        const fills = choices.flatMap((c, j) => (c === "fill" ? [`c${j}`] : []));
        let others = 0;
        choices.forEach((c, j) => {
          if (c !== "fill") others += typeof c === "number" ? c : c === "fit" ? 40 : COL_WIDTH;
          if (c !== "fill" && sizes.has(`c${j}`)) expect(sizes.get(`c${j}`)).toBe(typeof c === "number" ? c : c === "fit" ? 40 : COL_WIDTH);
        });
        const share = Math.floor((width - BAND - others) / fills.length);
        for (const id of fills) if (sizes.has(id)) expect(sizes.get(id)).toBe(Math.max(COL_WIDTH, share));
      } finally {
        h.destroy();
      }
    }));
});

describe("a user's resize", () => {
  test("before a fill waiting on a fit wins over the fill", async () => {
    const late = latecomer(table(5, [12, 3, 3]));
    const h = new Harness(late.source, 1000, 300, { cols: (id) => (id === "c0" ? "fit" : "fill") });
    try {
      h.settled();
      h.grid.set_size(1, h.feed.colKey("c1"), 55);
      await late.release();
      h.settled();
      expect(Object.fromEntries(colSizes(h))).toEqual({ c0: 120, c1: 55, c2: 1000 - BAND - 120 - 55 });
    } finally {
      h.destroy();
    }
  });


  test("outlasts the initial size through sorts and scrolling away and back", () =>
    hegel.test((tc) => {
      const n = tc.draw(gs.integers({ minValue: 2, maxValue: 30 }));
      const resized = tc.draw(gs.integers({ minValue: 0, maxValue: n - 1 }));
      const size = tc.draw(gs.integers({ minValue: 10, maxValue: 300 }));
      const h = new Harness(table(5, Array(n).fill(1)), 400, 300, { cols: () => 150 });
      try {
        h.settled();
        const key = h.feed.colKey(`c${resized}`);
        h.grid.set_size(1, key, size);
        for (const step of tc.draw(gs.arrays(gs.sampledFrom(["sort", "end", "start"] as const), { maxSize: 5 }))) {
          if (step === "sort") h.grid.notify(0, 0, NaN, NaN);
          else h.grid.scroll_to(1, step === "end" ? 1 : 0);
          h.settled();
        }
        expect(h.grid.size_override(1, key)).toBe(size);
      } finally {
        h.destroy();
      }
    }));
});

describe("rows", () => {
  test("fixed heights show from the first frame, and a row fitted back to the default stays there", () => {
    const h = new Harness(table(200, [1]), 400, 300, { rows: (id) => (Number(id.slice(1)) % 2 ? 40 : "fit") });
    try {
      h.settled();
      const heights = h.snapshot.rows.strip.slice(0, 4).map((i) => i.size);
      expect(heights).toEqual([ROW_HEIGHT, 40, ROW_HEIGHT, 40]);
      // As the widget answers a double-click on a row's edge.
      h.grid.clear_size(0, h.feed.rowKey("r1"));
      h.grid.scroll_to(0, 1);
      h.settled();
      h.grid.scroll_to(0, 0);
      h.settled();
      expect(h.snapshot.rows.strip[1]!.size).toBe(ROW_HEIGHT);
    } finally {
      h.destroy();
    }
  });

  test("a fill row takes the height the others leave", () => {
    const h = new Harness(table(3, [1]), 400, 300, { rows: (id) => (id === "r1" ? "fill" : undefined) });
    try {
      h.settled();
      expect(h.snapshot.rows.strip.map((i) => i.size)).toEqual([ROW_HEIGHT, 300 - HEADER - 2 * ROW_HEIGHT, ROW_HEIGHT]);
    } finally {
      h.destroy();
    }
  });
});

/** `source` with its cells held back until `release`. */
function latecomer<V>(source: TableSource<string, string, V>) {
  const held: (() => void)[] = [];
  return {
    source: {
      ...source,
      cells: (rs: string[], cs: string[], signal: AbortSignal) =>
        new Promise<V[][]>((resolve) => held.push(() => resolve(source.cells(rs, cs, signal) as V[][]))),
    },
    async release() {
      for (const go of held.splice(0)) go();
      await new Promise((r) => setTimeout(r, 0));
    },
  };
}
