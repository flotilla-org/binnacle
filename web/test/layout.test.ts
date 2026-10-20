// Saving and restoring a layout through the real wasm Grid and feed: a
// layout read from one grid and restored onto a fresh one over the same
// data reads back the same; restoring names the source lacks falls back
// as decided; and a restore before data loads shows the view pending while
// pins, sizes and bands apply at once.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { AXIS_CODE, EDGE_CODE } from "../src/codes";
import { readLayout, restoreLayout, LAYOUT_VERSION, type Layout } from "../src/layout";
import { fromRows } from "../src/rows";
import type { AppliedView, TableSink, TableSource, ViewSpec } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  a: number;
  b: string;
  c: number | null;
}

const COLUMNS = [{ key: "a" }, { key: "b" }, { key: "c" }] as const;
type Field = (typeof COLUMNS)[number]["key"];
/** The columns, and one the data lacks. */
const FIELDS = ["a", "b", "c", "z"] as Field[];

const RECORDS: Rec[] = Array.from({ length: 12 }, (_, i) => ({ a: (i * 7) % 5, b: ["x", "y", ""][i % 3]!, c: i % 4 ? i : null }));

const harness = (rows: Rec[] = RECORDS) => new Harness(fromRows(rows, COLUMNS));

/** Restores `layout` as the widget does, answering what it asks, and settles a frame. */
function restore<R, C>(h: Harness<R, C, unknown>, layout: Layout<R, C>): boolean {
  const done = restoreLayout(layout, h);
  h.drain();
  h.settled();
  return done;
}

const read = <R, C>(h: Harness<R, C, unknown>) => readLayout(h.snapshot, h);

const row = gs.integers({ minValue: 0, maxValue: RECORDS.length + 2 });
const field = gs.sampledFrom(FIELDS);
const edge = gs.sampledFrom(["leading", "trailing"] as const);

/** Puts `h` into some state by the controls a user has: pins, sizes, band thicknesses and a View Spec. */
function scramble(tc: hegel.TestCase, h: Harness<number, Field, unknown>): void {
  for (const [id, e] of tc.draw(gs.arrays(gs.tuples(field, edge), { maxSize: 3 }))) {
    h.grid.pin(AXIS_CODE.cols, EDGE_CODE[e], h.feed.colKey(id));
  }
  for (const [id, e] of tc.draw(gs.arrays(gs.tuples(row, edge), { maxSize: 3 }))) {
    h.grid.pin(AXIS_CODE.rows, EDGE_CODE[e], h.feed.rowKey(id));
  }
  for (const [id, size] of tc.draw(gs.arrays(gs.tuples(field, gs.integers({ minValue: 30, maxValue: 200 })), { maxSize: 3 }))) {
    h.grid.set_size(AXIS_CODE.cols, h.feed.colKey(id), size);
  }
  for (const [id, size] of tc.draw(gs.arrays(gs.tuples(row, gs.integers({ minValue: 20, maxValue: 60 })), { maxSize: 2 }))) {
    h.grid.set_size(AXIS_CODE.rows, h.feed.rowKey(id), size);
  }
  if (tc.draw(gs.booleans())) h.grid.set_header(AXIS_CODE.rows, tc.draw(gs.integers({ minValue: 0, maxValue: 60 })));
  if (tc.draw(gs.booleans())) h.grid.set_header(AXIS_CODE.cols, tc.draw(gs.integers({ minValue: 0, maxValue: 90 })));
  const sorted = tc.draw(gs.arrays(gs.tuples(gs.sampledFrom(["a", "b", "c"] as Field[]), gs.sampledFrom(["ascending", "descending"] as const)), { maxSize: 2 }));
  const view: ViewSpec<number, Field> = {
    rows: {
      sort: [...new Map(sorted).entries()].map(([id, direction]) => ({ id, direction })),
      filter: {
        quick: tc.draw(gs.booleans()) ? { text: tc.draw(gs.sampledFrom(["x", "1", ""])), caseSensitive: false, regex: false, searches: "shown" } : null,
        conditions: tc.draw(gs.booleans()) ? [{ id: "b", kind: { type: "OneOf", values: ["x", null] }, not: false }] : [],
      },
      hidden: tc.draw(gs.arrays(row, { maxSize: 3 })),
    },
    cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: tc.draw(gs.arrays(field, { maxSize: 2 })) },
  };
  h.grid.request_view(h.feed.viewKeys(view as AppliedView<number, Field>));
  h.drain();
  h.settled();
}

describe("a layout restored onto another grid over the same data reads back the same", () => {
  test("property: View Spec, pins, sizes and band thickness, whatever the other grid held", () =>
    hegel.test(
      (tc) => {
        const a = harness();
        const b = harness();
        try {
          a.settled();
          b.settled();
          scramble(tc, a);
          scramble(tc, b);
          const saved = read(a);
          expect(saved.version).toBe(LAYOUT_VERSION);
          // Plain: it survives JSON.
          const copy = JSON.parse(JSON.stringify(saved)) as Layout<number, Field>;
          expect(restore(b, copy)).toBe(true);
          expect(read(b)).toEqual(saved);
        } finally {
          a.destroy();
          b.destroy();
        }
      },
      { testCases: 150 },
    ));

  test("the layout carries what was set, by ID", () => {
    const h = harness();
    try {
      h.settled();
      h.grid.pin(AXIS_CODE.cols, EDGE_CODE.trailing, h.feed.colKey("c"));
      h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey("b"));
      h.grid.pin(AXIS_CODE.rows, EDGE_CODE.leading, h.feed.rowKey(3));
      h.grid.set_size(AXIS_CODE.cols, h.feed.colKey("a"), 150);
      h.grid.set_header(AXIS_CODE.cols, 70);
      h.grid.request_view(h.feed.viewKeys({ rows: { sort: [{ id: "a", direction: "descending" }] }, cols: { sort: [], hidden: ["c"] } }));
      h.drain();
      h.settled();
      const layout = read(h);
      expect(layout).toEqual({
        version: 1,
        view: {
          rows: { sort: [{ id: "a", direction: "descending" }], filter: { quick: null, conditions: [] }, hidden: [] },
          cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: ["c"] },
        },
        pins: { rows: { leading: [3], trailing: [] }, cols: { leading: ["b"], trailing: ["c"] } },
        sizes: { rows: [], cols: [["a", 150]] },
        bands: { rows: 24, cols: 70 },
      });
    } finally {
      h.destroy();
    }
  });
});

describe("restoring a layout that names what the source lacks", () => {
  /** A column the data lacks. */
  const GONE = "gone" as Field;
  const missing: Layout<number, Field> = {
    version: 1,
    view: {
      rows: { sort: [], filter: { quick: null, conditions: [{ id: GONE, kind: { type: "Text", query: { text: "q", caseSensitive: false, regex: false } }, not: false }] }, hidden: [2, 99] },
      cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: [GONE] },
    },
    pins: { rows: { leading: [], trailing: [] }, cols: { leading: [GONE, "b"], trailing: [] } },
    sizes: { rows: [[99, 40]], cols: [[GONE, 90]] },
    bands: { rows: 30, cols: 50 },
  };

  test("succeeds: a missing pin sleeps, unknown hidden IDs are pruned, a condition on a missing column is not applied, sizes wait", () => {
    const h = harness();
    try {
      h.settled();
      expect(restore(h, missing)).toBe(true);
      const s = h.snapshot;
      // The pin on the missing column waits, dormant; "b" is drawn pinned.
      expect(s.cols.pins.leading.map((k) => h.feed.colId(k))).toEqual(["gone", "b"]);
      expect(s.cols.pinned).toBe(1);
      // The source answers with the hidden IDs it has.
      expect(s.appliedView.rows.hidden.map((k) => h.feed.rowId(k))).toEqual([2]);
      expect(s.appliedView.cols.hidden).toEqual([]);
      // The condition on the missing column stays asked for, not applied.
      expect(s.rows.conditions.map((c) => c.state)).toEqual(["notApplied"]);
      // Sizes wait by ID; the bands apply.
      expect(h.grid.size_override(AXIS_CODE.cols, h.feed.colKey(GONE))).toBe(90);
      expect(h.grid.size_override(AXIS_CODE.rows, h.feed.rowKey(99))).toBe(40);
      expect([s.rows.header, s.cols.header]).toEqual([30, 50]);
    } finally {
      h.destroy();
    }
  });

  test("a waiting size applies when its item appears", () => {
    const source = fromRows(RECORDS, COLUMNS);
    const h = new Harness(source);
    try {
      source.filter((r) => r !== RECORDS[11]);
      h.drain();
      h.settled();
      expect(h.snapshot.rows.count).toBe(11);
      restore(h, { ...read(h), sizes: { rows: [[11, 44]], cols: [] } });
      source.filter(null);
      h.drain();
      h.settled();
      const item = h.snapshot.rows.strip.find((i) => i.key === h.feed.rowKey(11));
      expect(item?.size).toBe(44);
    } finally {
      h.destroy();
    }
  });

  test("a layout of another version restores nothing", () => {
    const h = harness();
    try {
      h.settled();
      const before = read(h);
      expect(restore(h, { ...missing, version: 2 } as unknown as typeof missing)).toBe(false);
      expect(read(h)).toEqual(before);
    } finally {
      h.destroy();
    }
  });
});

test("restored before data loads: the View Spec shows pending, pins, sizes and bands at once, then it applies", () => {
  let answer: ((v: AppliedView<string, string>) => void) | null = null;
  let loaded = false;
  let sink: TableSink<string, string> | null = null;
  const ids = (n: number, p: string) => Array.from({ length: n }, (_, i) => `${p}${i}`);
  const source: TableSource<string, string, string> = {
    rows: { count: () => (loaded ? 10 : 0), window: (s, e) => ids(10, "r").slice(s, e), hide: true },
    cols: { count: () => (loaded ? 3 : 0), window: (s, e) => ids(3, "c").slice(s, e) },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
    attach: (s) => {
      sink = s;
      return () => {};
    },
    view: (spec) =>
      ({
        then: (ok: (v: AppliedView<string, string>) => void) => void (answer = () => ok(spec)),
      }) as unknown as Promise<AppliedView<string, string>>,
  };
  const h = new Harness(source);
  try {
    h.settled();
    const layout: Layout<string, string> = {
      version: 1,
      view: {
        rows: { sort: [{ id: "c1", direction: "ascending" }], filter: { quick: null, conditions: [] }, hidden: ["r4"] },
        cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: [] },
      },
      pins: { rows: { leading: [], trailing: [] }, cols: { leading: ["c2"], trailing: [] } },
      sizes: { rows: [], cols: [["c2", 77]] },
      bands: { rows: 28, cols: 44 },
    };
    restore(h, layout);
    let s = h.snapshot;
    expect(s.viewPending).toBe(true);
    // Read while pending, the layout carries the View Spec asked for.
    expect(read(h).view).toEqual(layout.view);
    expect(s.appliedView.rows.sort).toEqual([]);
    expect(s.cols.pins.leading.map((k) => h.feed.colId(k))).toEqual(["c2"]);
    expect(h.grid.size_override(AXIS_CODE.cols, h.feed.colKey("c2"))).toBe(77);
    expect([s.rows.header, s.cols.header]).toEqual([28, 44]);
    loaded = true;
    sink!.rows.notify({ type: "Reset" });
    sink!.cols.notify({ type: "Reset" });
    h.settled();
    answer!(layout.view as AppliedView<string, string>);
    h.drain();
    s = h.settled();
    expect(s.viewPending).toBe(false);
    expect(read(h)).toEqual(layout);
  } finally {
    h.destroy();
  }
});
