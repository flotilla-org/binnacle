// A Described Table on the web: the wasm source through `fromDescribed`,
// fed the recorded and made fixtures as a host feeds a watch's bytes.

import { readFileSync } from "node:fs";
import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { afterEach, beforeAll, describe, expect, test } from "vitest";
import {
  columnFormatter,
  DIM,
  fixedZone,
  fromDescribed,
  WARNING,
  type AppliedView,
  type ChangeNotice,
  type DescribedTable,
  type TableSink,
  type TypedValue,
  type ViewSpec,
} from "../src";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

const NDJSON = "application/x-ndjson";
const CBOR_SEQ = "application/cbor-seq";
const signal = new AbortController().signal;

const bytes = (path: string) => new Uint8Array(readFileSync(new URL(path, import.meta.url)));
const fixture = (name: string) => bytes(`../../crates/binnacle-described/tests/fixtures/${name}`);
const recorded = (name: string) => bytes(`../../crates/binnacle-described/tests/data/${name}`);
const CREWS = fixture("crews.ndjson");
const CREWS_LINES = new TextDecoder().decode(CREWS).trim().split("\n");
const line = (message: unknown) => new TextEncoder().encode(`${JSON.stringify(message)}\n`);

const made: DescribedTable[] = [];
afterEach(() => {
  for (const d of made.splice(0)) d.destroy();
});

/** A described table, fed `body` of `contentType` in one piece if given. */
function described(
  table = "crews",
  body?: Uint8Array,
  contentType = NDJSON,
  scope: { namespace: string; wheres?: string[] } = { namespace: "flotilla" },
): DescribedTable {
  const d = fromDescribed(table, scope, { zone: fixedZone(0) });
  made.push(d);
  if (body) {
    d.pollRequest();
    d.open(contentType);
    d.feed(body);
  }
  return d;
}

/** Every row ID, in the order shown. */
const rowIds = (d: DescribedTable) => d.source.rows.window(0, d.source.rows.count() as number, signal) as bigint[];
const colIds = (d: DescribedTable) => d.source.cols.window(0, d.source.cols.count() as number, signal) as string[];
/** Every cell, by the columns' names in schema order. */
const cells = (d: DescribedTable) =>
  d.source.cells(
    rowIds(d),
    d.columns().map((c) => c.name),
    signal,
  ) as TypedValue[][];
const names = (d: DescribedTable) => rowIds(d).map((id) => d.rowKey(id)?.[0]);
const ns = (iso: string, extra = 0n) => BigInt(Date.parse(iso)) * 1_000_000n + extra;

const plain: ViewSpec<bigint, string> = {
  rows: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
  cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
};

describe("requests", () => {
  test("the watch carries the scope, once, and again after the stream ends", () => {
    const d = described("crews", undefined, NDJSON, { namespace: "flotilla", wheres: [".status.phase == Active"] });
    expect(d.pollRequest()).toEqual({
      table: "crews",
      path: "/tables/crews/rows",
      query: "watch&limit=10000&namespace=flotilla&where=.status.phase%20%3D%3D%20Active",
      target: "/tables/crews/rows?watch&limit=10000&namespace=flotilla&where=.status.phase%20%3D%3D%20Active",
      limit: 10000,
    });
    expect(d.pollRequest()).toBeUndefined();
    d.ended();
    expect(d.pollRequest()?.target).toBe("/tables/crews/rows?watch&limit=10000&namespace=flotilla&where=.status.phase%20%3D%3D%20Active");
    expect(d.pollRequest()).toBeUndefined();
  });

  test("an empty namespace throws, and so does a body that is not a rows stream", () => {
    expect(() => fromDescribed("crews", { namespace: "" })).toThrow(/namespace/);
    expect(() => described().open("text/html")).toThrow(/text\/html/);
    expect(() => described().feed(CREWS)).toThrow(/open a body first/);
  });
});

describe("decoding", () => {
  test("the crews rows read as Typed Values, each of its field's type or invalid", () => {
    const d = described("crews", CREWS);
    expect(d.status()).toMatchObject({ described: true, synced: true, partial: false, rows: 6, mismatches: 3 });
    expect(d.columns().map((c) => [c.name, c.mismatches])).toEqual([
      ["name", 0],
      ["phase", 0],
      ["started", 1],
      ["cpus", 1],
      ["load", 1],
      ["note", 0],
      ["host", 0],
      ["spec", 0],
    ]);
    const spec = (s: unknown) => ({ nested: s });
    expect(cells(d)).toEqual([
      ["alpha", "Running", { instant: 1793268000000000002n }, 4n, 0.5, "", "feta", spec({ image: "rust" })],
      ["bravo", null, null, 2n, 1.25, null, "feta", spec({})],
      ["charlie", "Stalled", { invalid: "soon", expected: "datetime" }, { invalid: "many", expected: "int" }, null, "check disk", "brie", spec({})],
      ["delta", "Running", { instant: ns("2026-10-28T09:00:00Z") }, null, 3.5, "", "brie", spec({})],
      ["echo", "Done", { instant: ns("2026-10-29T10:00:00Z", 1n) }, 18446744073709551615n, 2, "late", null, spec({})],
      ["foxtrot", "Running", { instant: ns("2026-10-27T00:00:00Z") }, 8n, { invalid: "high", expected: "number" }, null, "feta", spec({ image: "zig" })],
    ]);
    expect(d.status()).toMatchObject({ resyncs: 0, reconnects: 0, decodeErrors: 0 });
  });

  test("the hosts rows read the same from CBOR as from JSON", () => {
    const json = described("hosts", recorded("rows-hosts.ndjson"));
    const cbor = described("hosts", recorded("rows-hosts.cborseq"), CBOR_SEQ);
    expect(json.status().rows).toBeGreaterThan(0);
    expect(cbor.columns()).toEqual(json.columns());
    expect(cells(cbor)).toEqual(cells(json));
  });

  test("a sink or listener that throws leaves the others told, then the error comes", () => {
    const d = described();
    const heard: string[] = [];
    const ignore = () => {};
    const sink = (name: string, fails: boolean): TableSink<bigint, string> => ({
      rows: {
        notify: () => {
          heard.push(name);
          if (fails) throw new Error(name);
        },
        supportChanged: ignore,
        movableChanged: ignore,
      },
      cols: { notify: ignore, supportChanged: ignore, movableChanged: ignore },
      viewChanged: ignore,
      findCounted: ignore,
    });
    d.source.attach?.(sink("first", true));
    d.source.attach?.(sink("second", false));
    d.onChange(() => heard.push("listener"));
    d.pollRequest();
    d.open(NDJSON);
    expect(() => d.feed(CREWS)).toThrow("first");
    expect(heard).toEqual(["first", "second", "listener"]);
    expect(d.status().rows).toBe(6);
  });

  test("a line that does not decode is skipped and counted", () => {
    const d = described("crews", CREWS);
    d.feed(new TextEncoder().encode("{not json\n"));
    expect(d.status()).toMatchObject({ rows: 6, decodeErrors: 1 });
  });

  test("the crews bytes split anywhere read as fed whole", () =>
    hegel.test((tc) => {
      const whole = described("crews", CREWS);
      const cuts = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: CREWS.length }), { maxSize: 12 }));
      const pieces = described();
      pieces.pollRequest();
      pieces.open(NDJSON);
      let from = 0;
      for (const at of [...cuts].sort((a, b) => a - b)) {
        pieces.feed(CREWS.subarray(from, at));
        from = at;
      }
      pieces.feed(CREWS.subarray(from));
      expect(pieces.status()).toEqual(whole.status());
      expect(pieces.columns()).toEqual(whole.columns());
      expect(cells(pieces)).toEqual(cells(whole));
    }));
});

describe("columns", () => {
  test("each column says what its field describes", () => {
    const d = described("crews", CREWS);
    const cols = d.columns();
    const col = (name: string) => cols.find((c) => c.name === name)!;
    expect(cols.map((c) => c.name)).toEqual(["name", "phase", "started", "cpus", "load", "note", "host", "spec"]);
    expect(colIds(d)).toEqual(cols.map((c) => c.name));
    expect(col("name")).toMatchObject({ header: "Name", title: "Name", type: "string", valueType: "text", nullable: false, roles: ["key"] });
    expect(col("note")).toMatchObject({ header: "note", title: null });
    expect(col("started")).toMatchObject({ role: "time", valueType: "datetime" });
    expect(col("cpus")).toMatchObject({ role: "none", valueType: "int", nullable: true });
    expect(col("load")).toMatchObject({ unit: "%", valueType: "number" });
    expect(col("host").reference).toEqual({ table: "hosts", fields: ["metadata.name"] });
    expect(col("spec")).toMatchObject({ nested: true, type: "object", valueType: "nested" });
    expect(typeof col("spec").key).toBe("bigint");
    expect(d.property("host", "x-flotilla")).toEqual({ link: "hosts" });
    expect(d.property("name", "nosuch")).toBeUndefined();
    expect(d.property("nosuch", "title")).toBeUndefined();
    expect(d.source.cols.label?.("note", 0)).toBe("note");
    expect(d.source.cols.label?.("cpus", 0)).toBe("CPUs");
  });

  test("the default layout hides the Nested columns and pins the key and state ones", () => {
    const d = described("crews", CREWS);
    expect(d.defaultLayout()).toMatchObject({
      view: { cols: { hidden: ["spec"] } },
      pins: { cols: { leading: ["name", "phase"] } },
    });
  });

  test("each row's key is its primary-key tuple", () => {
    const d = described("crews", CREWS);
    expect(names(d)).toEqual(["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]);
    expect(d.rowKey(12345n)).toBeUndefined();
  });
});

describe("through the widget", () => {
  /** A widget over a fresh crews table, with a sink recording the notices. */
  function widget() {
    const d = described();
    const h = new Harness(d.source, 2000, 300);
    const told: [string, ChangeNotice][] = [];
    const views: AppliedView<bigint, string>[] = [];
    const ignore = () => {};
    const sink: TableSink<bigint, string> = {
      rows: { notify: (n) => void told.push(["rows", n]), supportChanged: ignore, movableChanged: ignore },
      cols: { notify: (n) => void told.push(["cols", n]), supportChanged: ignore, movableChanged: ignore },
      viewChanged: (view) => void views.push(view),
      findCounted: ignore,
    };
    d.source.attach?.(sink);
    d.pollRequest();
    d.open(NDJSON);
    d.feed(CREWS);
    h.frame();
    return { d, h, told, views };
  }

  test("the crews rows come in, and an upsert adds a row", () => {
    const { d, h, told, views } = widget();
    expect(h.frame().rows.count).toBe(6);
    d.source.view!({ ...plain, cols: { ...plain.cols, hidden: ["spec"] } }, signal);
    told.length = 0;
    d.feed(line({ type: "upsert", key: ["golf"], row: { name: "golf", phase: "Running", cpus: 1, spec: {} } }));
    expect(h.frame().rows.count).toBe(7);
    expect(told).toContainEqual(["rows", { type: "Inserted", at: 6, n: 1 }]);
    // The View Spec in force counts the new row in its totals.
    expect(views.at(-1)).toMatchObject({ cols: { hidden: ["spec"] }, totals: { rows: 7 } });
    h.destroy();
  });

  test("an added field comes last among the columns, and the sort holds", () => {
    const { d, h, views } = widget();
    d.source.view!({ ...plain, rows: { ...plain.rows, sort: [{ id: "cpus", direction: "ascending" }] } }, signal);
    const table = JSON.parse(CREWS_LINES[0]!);
    table.schema.fields.push({ name: "zone", type: "string", title: "Zone" });
    d.feed(line(table));
    h.frame();
    expect(colIds(d).at(-1)).toBe("zone");
    expect(h.frame().cols.count).toBe(9);
    expect(views.at(-1)?.rows.sort).toEqual([{ id: "cpus", direction: "ascending" }]);
    expect(names(d)).toEqual(["delta", "charlie", "bravo", "alpha", "foxtrot", "echo"]);
    h.destroy();
  });

  test("a resync that sends the same rows keeps every row ID", () => {
    const { d, h } = widget();
    const before = rowIds(d);
    d.feed(line({ type: "resync" }));
    expect(d.status()).toMatchObject({ synced: false, resyncs: 1 });
    d.feed(CREWS);
    h.frame();
    expect(rowIds(d)).toEqual(before);
    expect(d.status()).toMatchObject({ synced: true, rows: 6 });
    h.destroy();
  });

  test("the stream's end resyncs the rows the next one sends", () => {
    const { d, h } = widget();
    const before = rowIds(d);
    d.ended();
    expect(d.status()).toMatchObject({ synced: false, reconnects: 1 });
    expect(d.pollRequest()).toBeDefined();
    d.open(NDJSON);
    d.feed(CREWS);
    h.frame();
    expect(rowIds(d)).toEqual(before);
    h.destroy();
  });
});

describe("views", () => {
  test("sorting by cpus puts the null first, then the invalid value, then the numbers", () => {
    const d = described("crews", CREWS);
    const applied = d.source.view!(
      { ...plain, rows: { ...plain.rows, sort: [{ id: "cpus", direction: "ascending" }] } },
      signal,
    ) as Awaited<ReturnType<NonNullable<typeof d.source.view>>>;
    expect(names(d)).toEqual(["delta", "charlie", "bravo", "alpha", "foxtrot", "echo"]);
    expect(applied.rows.sort).toEqual([{ id: "cpus", direction: "ascending" }]);
  });

  test("a quick filter matches Stable Text in the zone given", () => {
    const d = described("crews", CREWS);
    const quick = { text: "2026-10-29", caseSensitive: false, regex: false, searches: "shown" as const };
    const applied = d.source.view!({ ...plain, rows: { ...plain.rows, filter: { quick, conditions: [] } } }, signal) as {
      rows: { filter?: { quick?: unknown } };
    };
    expect(names(d)).toEqual(["alpha", "echo"]);
    expect(applied.rows.filter?.quick).toEqual(quick);
  });

  test("the applied spec names columns by field name", () => {
    const d = described("crews", CREWS);
    const applied = d.source.view!(
      {
        rows: {
          filter: { quick: null, conditions: [{ id: "phase", kind: { type: "OneOf", values: ["Running"] }, not: false }] },
          sort: [{ id: "load", direction: "descending" }],
          hidden: [],
        },
        cols: { ...plain.cols, hidden: ["spec"] },
      },
      signal,
    ) as { rows: { sort: unknown; filter?: { conditions?: unknown } }; cols: { hidden?: unknown } };
    expect(applied.rows.sort).toEqual([{ id: "load", direction: "descending" }]);
    expect(applied.rows.filter?.conditions).toEqual([{ id: "phase", kind: { type: "OneOf", values: ["Running"] }, not: false }]);
    expect(applied.cols.hidden).toEqual(["spec"]);
    expect(names(d)).toEqual(["delta", "alpha", "foxtrot"]);
    expect(colIds(d)).not.toContain("spec");
  });

  test("a column's checklist lists its values and its blanks", () => {
    const d = described("crews", CREWS);
    const narrow = { quick: null, conditions: [] };
    const answer = d.source.rows.distinct!("phase", { search: "", limit: 10, narrow }, signal);
    expect(answer).toMatchObject({ values: [{ text: "Done" }, { text: "Running" }, { text: "Stalled" }], blanks: 1 });
  });
});

describe("find", () => {
  const query = (text: string) => ({ query: { text, caseSensitive: false, regex: false, searches: "shown" as const }, by: "rows" as const });

  test("marks the cells that match and counts them, Nested ones included", () => {
    const d = described("crews", CREWS);
    const find = d.source.find!;
    const cols = d.columns().map((c) => c.name);
    const marks = find.marks(rowIds(d), cols, query("check"), signal) as boolean[][];
    const marked = marks.flatMap((row, r) => row.flatMap((m, c) => (m ? [[names(d)[r], cols[c]]] : [])));
    expect(marked).toEqual([["charlie", "note"]]);
    expect(find.count!(query("rust"), signal)).toMatchObject({ total: 1 });
    expect(find.next(null, "next", query("rust"), signal)).toMatchObject({ col: "spec", wrapped: false, index: 0 });
  });
});

describe("style", () => {
  test("an invalid value draws as a warning and a null dim", () => {
    const d = described("crews", CREWS);
    const [rows, cols] = [cells(d), d.columns()];
    const at = (row: number, name: string) => {
      const c = cols.findIndex((col) => col.name === name);
      const { unit, role } = cols[c]!;
      return columnFormatter({ ...(unit === null ? {} : { unit }), role }).format(rows[row]![c], { now: 0, zone: fixedZone(0) });
    };
    expect(at(2, "cpus").style & WARNING).toBe(WARNING);
    expect(at(1, "phase").style & DIM).toBe(DIM);
  });
});
