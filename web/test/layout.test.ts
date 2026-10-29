// Saving and restoring a layout through the real wasm Grid and feed: a
// layout read from one grid and restored onto a fresh one over the same
// data reads back the same; restoring names the source lacks falls back
// as decided; and a restore before data loads shows the view pending while
// pins, sizes and bands apply at once.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { AXIS_CODE, EDGE_CODE } from "../src/codes";
import { readFileSync } from "node:fs";
import { migrateLayout, readLayout, restoreLayout, LAYOUT_VERSION, type Layout, type LayoutOrder, type LayoutV1 } from "../src/layout";
import { rows as DEMO_ROWS } from "../demo/sources/records-data";
import { fromRows } from "../src/rows";
import type { AppliedView, Comparison, Condition, Scalar, TableSink, TableSource, ViewSpec } from "../src/source";
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
function restore<R, C>(h: Harness<R, C, unknown>, layout: Layout<R, C> | LayoutV1<R, C>): boolean {
  const done = restoreLayout(layout, h);
  h.drain();
  h.settled();
  return done;
}

const read = <R, C>(h: Harness<R, C, unknown>) => readLayout(h.snapshot, h);

const row = gs.integers({ minValue: 0, maxValue: RECORDS.length + 2 });
const field = gs.sampledFrom(FIELDS);
const edge = gs.sampledFrom(["leading", "trailing"] as const);

/**
 * Puts `h` into some state by the controls a user has: pins, sizes, band thicknesses and a View Spec.
 * Sizes and thicknesses are any number: the grid ignores or caps what it does not take.
 */
function scramble(tc: hegel.TestCase, h: Harness<number, Field, unknown>): void {
  for (const [id, e] of tc.draw(gs.arrays(gs.tuples(field, edge), { maxSize: 3 }))) {
    h.grid.pin(AXIS_CODE.cols, EDGE_CODE[e], h.feed.colKey(id));
  }
  for (const [id, e] of tc.draw(gs.arrays(gs.tuples(row, edge), { maxSize: 3 }))) {
    h.grid.pin(AXIS_CODE.rows, EDGE_CODE[e], h.feed.rowKey(id));
  }
  for (const [id, size] of tc.draw(gs.arrays(gs.tuples(field, gs.floats()), { maxSize: 3 }))) {
    h.grid.set_size(AXIS_CODE.cols, h.feed.colKey(id), size);
  }
  for (const [id, size] of tc.draw(gs.arrays(gs.tuples(row, gs.floats()), { maxSize: 2 }))) {
    h.grid.set_size(AXIS_CODE.rows, h.feed.rowKey(id), size);
  }
  if (tc.draw(gs.booleans())) h.grid.set_header(AXIS_CODE.rows, tc.draw(gs.floats()));
  if (tc.draw(gs.booleans())) h.grid.set_header(AXIS_CODE.cols, tc.draw(gs.floats()));
  const sorted = tc.draw(gs.arrays(gs.tuples(gs.sampledFrom(["a", "b", "c"] as Field[]), gs.sampledFrom(["ascending", "descending"] as const)), { maxSize: 2 }));
  const view: ViewSpec<number, Field> = {
    rows: {
      sort: [...new Map(sorted).entries()].map(([id, direction]) => ({ id, direction })),
      filter: {
        quick: tc.draw(gs.booleans()) ? { text: tc.draw(gs.sampledFrom(["x", "1", ""])), caseSensitive: false, regex: false, searches: "shown" } : null,
        conditions: [
          ...(tc.draw(gs.booleans()) ? [{ id: "b" as Field, kind: { type: "OneOf" as const, values: ["x", null] }, not: false }] : []),
          ...tc.draw(gs.arrays(compared, { maxSize: 2 })),
        ],
      },
      hidden: tc.draw(gs.arrays(row, { maxSize: 3 })),
    },
    cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: tc.draw(gs.arrays(field, { maxSize: 2 })) },
  };
  h.grid.request_view(h.feed.viewKeys(view as AppliedView<number, Field>));
  h.drain();
  h.settled();
}

/** A comparison on a number or a date, between included, maybe with `not`. */
const compared = gs.composite((tc): Condition<Field> => {
  const scalar = (): Scalar =>
    tc.draw(gs.booleans())
      ? { type: "number", value: tc.draw(gs.floats({ allowNan: false, allowInfinity: false })) }
      : { type: "date", value: tc.draw(gs.integers({ minValue: -8.64e15, maxValue: 8.64e15 })) };
  const op = tc.draw(gs.sampledFrom(["equal", "notEqual", "less", "atMost", "greater", "atLeast", "between"] as const));
  const comparison: Comparison = op === "between" ? { op, low: scalar(), high: scalar() } : { op, value: scalar() };
  return { id: tc.draw(gs.sampledFrom(["a", "b", "c"] as Field[])), kind: { type: "Compare", comparison }, not: tc.draw(gs.booleans()) };
});

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
          // An order is read only while every item is drawn, which the scroll
          // and the sizes decide; the order's own tests are below.
          const { order: _, ...rest } = read(b);
          const { order: __, ...want } = saved;
          expect(rest).toEqual(want);
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
        version: 2,
        view: {
          rows: { sort: [{ id: "a", direction: "descending" }], filter: { quick: null, conditions: [] }, hidden: [] },
          cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: ["c"] },
          grouping: null,
        },
        // Sorted, the rows have no order of their own; hidden, "c" is in no order.
        order: { rows: [], cols: ["a", "b"] },
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
    version: LAYOUT_VERSION,
    view: {
      rows: { sort: [{ id: GONE, direction: "ascending" }, { id: "b", direction: "descending" }], filter: { quick: null, conditions: [{ id: GONE, kind: { type: "Text", query: { text: "q", caseSensitive: false, regex: false } }, not: false }] }, hidden: [2, 99] },
      cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: [GONE] },
    },
    pins: { rows: { leading: [], trailing: [] }, cols: { leading: [GONE, "b"], trailing: [] } },
    sizes: { rows: [[99, 40]], cols: [[GONE, 90]] },
    bands: { rows: 30, cols: 50 },
  };

  test("succeeds: a missing pin sleeps, unknown sort terms and hidden IDs are pruned, a condition on a missing column is not applied, sizes wait", () => {
    const h = harness();
    try {
      h.settled();
      expect(restore(h, missing)).toBe(true);
      const s = h.snapshot;
      // The pin on the missing column waits, dormant; "b" is drawn pinned.
      expect(s.cols.pins.leading.map((k) => h.feed.colId(k))).toEqual(["gone", "b"]);
      expect(s.cols.pinned).toBe(1);
      // The source answers with the sort terms and hidden IDs it has.
      expect(s.appliedView.rows.sort.map((c) => h.feed.colId(c.key))).toEqual(["b"]);
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
      expect(restore(h, { ...missing, version: 3 } as unknown as typeof missing)).toBe(false);
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
      version: LAYOUT_VERSION,
      view: {
        rows: { sort: [{ id: "c1", direction: "ascending" }], filter: { quick: null, conditions: [] }, hidden: ["r4"] },
        cols: { sort: [], filter: { quick: null, conditions: [] }, hidden: [] },
        grouping: null,
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
    expect(read(h)).toEqual({ ...layout, order: { rows: [], cols: ["c0", "c1", "c2"] } });
  } finally {
    h.destroy();
  }
});

test("a missing item's pin or size keeps its ID through any number of Resets, until unpinned or reset", () => {
  let sinks!: TableSink<string, string>;
  const ids = (n: number, p: string) => Array.from({ length: n }, (_, i) => `${p}${i}`);
  const source: TableSource<string, string, string> = {
    rows: { count: () => 10, window: (s, e) => ids(10, "r").slice(s, e) },
    cols: { count: () => 3, window: (s, e) => ids(3, "c").slice(s, e) },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
    attach: (s) => ((sinks = s), () => {}),
  };
  const h = new Harness(source);
  const resets = () => {
    for (let i = 0; i < 3; i++) {
      sinks.rows.notify({ type: "Reset" });
      sinks.cols.notify({ type: "Reset" });
      h.drain();
      h.settled();
    }
  };
  try {
    h.settled();
    const none = { quick: null, conditions: [] };
    const layout: Layout<string, string> = {
      version: LAYOUT_VERSION,
      view: { rows: { sort: [], filter: none, hidden: [] }, cols: { sort: [], filter: none, hidden: [] }, grouping: null },
      pins: { rows: { leading: [], trailing: ["gone row"] }, cols: { leading: ["gone", "c1"], trailing: [] } },
      // Sized only, not pinned.
      sizes: { rows: [["sized row", 40]], cols: [["sized", 90]] },
      bands: { rows: 24, cols: 40 },
    };
    restore(h, layout);
    resets();
    expect(read(h)).toEqual({ ...layout, order: { rows: ids(10, "r"), cols: ids(3, "c") } });
    // Unpinned, and its size reset, a missing column's ID goes like any other.
    const [gone, sized] = [h.feed.colKey("gone"), h.feed.colKey("sized")];
    h.grid.unpin(AXIS_CODE.cols, gone);
    h.grid.clear_size(AXIS_CODE.cols, sized);
    h.drain();
    resets();
    expect([h.feed.colId(gone), h.feed.colId(sized)]).toEqual([undefined, undefined]);
    expect(read(h).pins.cols.leading).toEqual(["c1"]);
    // The rows' pin and size were left: their IDs stay.
    expect(read(h).pins.rows.trailing).toEqual(["gone row"]);
    expect(read(h).sizes.rows).toEqual([["sized row", 40]]);
    // A pin set just before the Resets, with no Effects drained between, holds its ID too.
    h.grid.pin(AXIS_CODE.cols, EDGE_CODE.trailing, h.feed.colKey("late"));
    for (let i = 0; i < 3; i++) sinks.cols.notify({ type: "Reset" });
    h.drain();
    h.settled();
    expect(read(h).pins.cols.trailing).toEqual(["late"]);
  } finally {
    h.destroy();
  }
});

test("a layout leaves out the widths the widget fitted or filled, and restoring keeps the other grid's own", () => {
  const ids = (n: number, p: string) => Array.from({ length: n }, (_, i) => `${p}${i}`);
  const source = (): TableSource<string, string, string> => ({
    rows: { count: () => 5, window: (s, e) => ids(5, "r").slice(s, e) },
    cols: { count: () => 4, window: (s, e) => ids(4, "c").slice(s, e) },
    cells: (rs, cs) => rs.map(() => cs.map((c) => "x".repeat(Number(c.slice(1)) + 3))),
  });
  // c0 fits, c1 fills, c2 starts at a fixed 77, c3 at the default.
  const choices = { cols: (id: string) => (["fit", "fill", 77, undefined] as const)[Number(id.slice(1))] };
  const a = new Harness(source(), 1000, 300, choices);
  const b = new Harness(source(), 1000, 300, choices);
  try {
    a.settled();
    b.settled();
    const width = (h: Harness<string, string, string>, id: string) => h.grid.size_override(AXIS_CODE.cols, h.feed.colKey(id));
    const fitted = [width(a, "c0"), width(a, "c1")];
    expect(fitted.every((w) => !Number.isNaN(w))).toBe(true);
    expect(read(a).sizes.cols).toEqual([["c2", 77]]);
    // A width the user sets is theirs, fitted column or not.
    a.grid.set_size(AXIS_CODE.cols, a.feed.colKey("c0"), 123);
    a.grid.set_size(AXIS_CODE.cols, a.feed.colKey("c3"), 60);
    const saved = read(a);
    expect(saved.sizes.cols).toEqual([["c0", 123], ["c2", 77], ["c3", 60]]);
    // b keeps the width it filled; the saved ones replace the rest.
    restore(b, saved);
    expect([width(b, "c0"), width(b, "c1"), width(b, "c2"), width(b, "c3")]).toEqual([123, fitted[1], 77, 60]);
    expect(read(b).sizes.cols).toEqual(saved.sizes.cols);
  } finally {
    a.destroy();
    b.destroy();
  }
});

describe("version 2: the order, every part optional, and version 1 migrated", () => {
  /** A layout saved before the order, as version 1 wrote it: every part set. */
  const V1 = {
    version: 1,
    view: {
      rows: { filter: { quick: null, conditions: [] }, sort: [{ id: "a", direction: "descending" }], hidden: [2] },
      cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
      grouping: null,
    },
    pins: { rows: { leading: [], trailing: [] }, cols: { leading: ["c"], trailing: [] } },
    sizes: { rows: [], cols: [["b", 120]] },
    bands: { rows: 30, cols: 44 },
  } as const satisfies LayoutV1<number, Field>;

  test("a version 1 layout restores after migration, as a version 2 one with no order", () => {
    const h = harness();
    try {
      h.settled();
      const saved = JSON.parse(JSON.stringify(V1)) as LayoutV1<number, Field>;
      expect(restore(h, saved)).toBe(true);
      const { version: _, ...parts } = V1;
      // Sorted, the rows have no order; the columns keep theirs.
      expect(read(h)).toEqual({ ...parts, version: 2, order: { rows: [], cols: ["a", "b", "c"] } });
      expect(migrateLayout(saved)).toEqual({ ...V1, version: 2 });
    } finally {
      h.destroy();
    }
  });

  test("a part left out leaves the grid's own", () => {
    const h = harness();
    try {
      h.settled();
      h.grid.pin(AXIS_CODE.cols, EDGE_CODE.leading, h.feed.colKey("b"));
      h.grid.set_size(AXIS_CODE.cols, h.feed.colKey("a"), 150);
      h.drain();
      h.settled();
      const before = read(h);
      expect(restore(h, { version: LAYOUT_VERSION, bands: { rows: 30, cols: 50 } })).toBe(true);
      expect(read(h)).toEqual({ ...before, bands: { rows: 30, cols: 50 } });
    } finally {
      h.destroy();
    }
  });

  const DEMO_COLUMNS = [{ key: "id" }, { key: "name" }, { key: "weight" }, { key: "crew" }, { key: "port" }] as const;
  type DemoField = (typeof DEMO_COLUMNS)[number]["key"];

  test("an order moves the demo table's listed columns to the front, in that order, the rest after", () => {
    const h = new Harness(fromRows(DEMO_ROWS, DEMO_COLUMNS), 900, 300);
    try {
      h.settled();
      const shown = () => h.snapshot.cols.strip.map((item) => h.feed.colId(item.key!));
      expect(shown()).toEqual(["id", "name", "weight", "crew", "port"]);
      const unplaced: LayoutOrder<number, DemoField>[] = [];
      const order = { rows: [], cols: ["port", "weight"] as DemoField[] };
      expect(restoreLayout({ version: LAYOUT_VERSION, order }, h, { unplaced: (o) => unplaced.push(o) })).toBe(true);
      h.drain();
      h.settled();
      expect(shown()).toEqual(["port", "weight", "id", "name", "crew"]);
      expect(unplaced).toEqual([]);
      // Read back: the columns' order; not every row is drawn, so the rows have none.
      expect(read(h).order).toEqual({ rows: [], cols: ["port", "weight", "id", "name", "crew"] });
      // In place already, it asks for no move.
      expect(restoreLayout({ version: LAYOUT_VERSION, order }, h)).toBe(true);
      expect(h.grid.poll_effects()).toEqual([]);
    } finally {
      h.destroy();
    }
  });

  test("a move the source refuses, or an order on a sorted axis, is reported as not placed", () => {
    const ids = (n: number, p: string) => Array.from({ length: n }, (_, i) => `${p}${i}`);
    const source: TableSource<string, string, string> = {
      rows: { count: () => 4, window: (s, e) => ids(4, "r").slice(s, e) },
      cols: { count: () => 3, window: (s, e) => ids(3, "c").slice(s, e), move: () => ({ refused: "Not these" }) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
      view: (spec) => spec,
    };
    const h = new Harness(source);
    try {
      h.settled();
      const unplaced: LayoutOrder<string, string>[] = [];
      const layout: Layout<string, string> = {
        version: LAYOUT_VERSION,
        view: {
          rows: { filter: { quick: null, conditions: [] }, sort: [{ id: "c1", direction: "ascending" }], hidden: [] },
          cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
        },
        order: { rows: ["r3"], cols: ["c2", "c1"] },
      };
      restoreLayout(layout, h, { unplaced: (o) => unplaced.push(o) });
      // The sorted rows' order is not placed at once.
      expect(unplaced).toEqual([{ rows: ["r3"], cols: [] }]);
      h.drain();
      h.settled();
      expect(unplaced).toEqual([
        { rows: ["r3"], cols: [] },
        { rows: [], cols: ["c2", "c1"] },
      ]);
      expect(h.snapshot.cols.strip.map((item) => h.feed.colId(item.key!))).toEqual(["c0", "c1", "c2"]);
      expect(h.snapshot.moveRefusal).toEqual({ axis: "cols", reason: "Not these" });
    } finally {
      h.destroy();
    }
  });

  test("the fixture the Rust crate reads parses here and writes back byte for byte", () => {
    const text = readFileSync(new URL("../../crates/binnacle-layout/tests/fixtures/layout.json", import.meta.url), "utf8").trimEnd();
    const layout = JSON.parse(text) as Layout<string, string>;
    expect(layout.version).toBe(LAYOUT_VERSION);
    expect(JSON.stringify(layout)).toBe(text);
    // And it restores onto a grid.
    const columns = ["vessel", "port", "weight", "due", "region"];
    const h = new Harness<string, string, string>({
      rows: { count: () => 3, window: (s, e) => ["row 1", "row 7", "row 12"].slice(s, e), hide: true },
      cols: { count: () => columns.length, window: (s, e) => columns.slice(s, e) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}:${c}`)),
      view: (spec) => spec,
    });
    try {
      h.settled();
      expect(restore(h, layout)).toBe(true);
      expect(read(h).sizes.cols).toEqual(layout.sizes!.cols);
    } finally {
      h.destroy();
    }
  });
});
