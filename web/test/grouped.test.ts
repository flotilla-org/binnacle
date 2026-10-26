// groupedSource over an aggregate backend that answers when it likes: out of
// order, in parts, jointly, with more than asked, unasked, late and after a
// newer View Spec. Whatever the timing, the rows it shows match fromRows
// grouping the same records at once.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, expect, test } from "vitest";
import { light } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import {
  groupedSource,
  type AggregateAnswer,
  type AggregateBatch,
  type AggregateQuery,
  type Figure,
  type GroupedSource,
  type GroupPath,
  type GroupRow,
} from "../src/grouped";
import { fromRows } from "../src/rows";
import { rowsBy, type ChangeNotice, type Condition, type Grouping, type ListedValue, type Located, type OutlinedWindow, type Placement, type ViewSpec } from "../src/source";
import { fakeDocument, type DrawCall } from "./canvas";
import { BAND, HEADER, Harness } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

interface Rec {
  s: string | null;
  t: string | null;
  n: number;
}
type Field = keyof Rec;

const COLUMNS = [{ key: "s" }, { key: "t" }, { key: "n" }] as const;

const record = gs.record({
  s: gs.sampledFrom(["a", "b", "c", null]),
  t: gs.sampledFrom(["x", "y", null]),
  n: gs.integers({ minValue: 0, maxValue: 5 }),
});
const records = gs.arrays(record, { maxSize: 30 });

const signal = new AbortController().signal;

/** Cache sizes: none, a little, and the default (`NaN`). */
const caches = gs.oneOf(gs.integers({ minValue: 0, maxValue: 200 }), gs.just(Number.NaN));

/** Groups' values in order: ascending, the blank last. */
const ordered = (values: ListedValue[]) =>
  [...new Set(values)].sort((a, b) => (a === b ? 0 : a === null ? 1 : b === null ? -1 : a < b ? -1 : 1));

/**
 * The naive backend: every answer worked out from the records at once,
 * whole and exactly as asked, keeping the rows the filter's `OneOf`
 * conditions keep.
 */
function compute(rows: Rec[], batch: AggregateBatch<Field>, q: AggregateQuery<Field>, start = q.start, end = q.end): AggregateAnswer<Field> {
  const { levels, values } = batch.scope;
  const hidden = new Set(batch.scope.hidden);
  const kept = (id: number) =>
    batch.scope.filter.conditions.every((c) => c.kind.type !== "OneOf" || c.kind.values.includes(rows[id]![c.id as Field] as ListedValue) !== c.not);
  const ids = rows.flatMap((_, id) => (hidden.has(id) || !kept(id) ? [] : [id]));
  const under = (path: GroupPath<Field>) => ids.filter((id) => path.every((p) => rows[id]![p.field as Field] === p.value));
  const figures = (leaves: number[]): Figure[] =>
    values.map((v) => (v.aggregate === "count" ? leaves.length : leaves.length ? leaves.reduce((sum, id) => sum + rows[id]!.n, 0) : null));
  const groups = (path: GroupPath<Field>): GroupRow[] => {
    const field = levels[path.length]!;
    return ordered(under(path).map((id) => rows[id]![field] as ListedValue)).map((value) => {
      const inner = [...path, { field, value }];
      const leaves = under(inner);
      const children =
        path.length + 1 < levels.length ? ordered(leaves.map((id) => rows[id]![levels[path.length + 1]!] as ListedValue)).length : leaves.length;
      return { value, children, figures: figures(leaves) };
    });
  };
  const answer = { stamp: batch.stamp, id: q.id, path: q.path, start };
  if (q.want === "total") return { ...answer, body: { type: "total", figures: figures(under(q.path)) } };
  if (q.want === "groups") {
    const all = groups(q.path);
    return { ...answer, body: { type: "groups", total: all.length, groups: all.slice(start, end) } };
  }
  if (q.want === "leaves") return { ...answer, body: { type: "leaves", ids: under(q.path).slice(start, end) } };
  const id = q.want.locate;
  if (!ids.includes(id)) return { ...answer, path: [], start: 0, body: { type: "located", id, found: false } };
  const path = levels.map((field) => ({ field, value: rows[id]![field] as ListedValue }));
  return { ...answer, path, start: under(path).indexOf(id), body: { type: "located", id, found: true } };
}

/** How many items a query's range can reach: the groups or Leaves under its path. */
function extent(rows: Rec[], batch: AggregateBatch<Field>, q: AggregateQuery<Field>): number {
  const whole = compute(rows, batch, { ...q, start: 0, end: Number.MAX_SAFE_INTEGER });
  return whole.body.type === "groups" ? whole.body.total : whole.body.type === "leaves" ? whole.body.ids.length : 0;
}

/**
 * A backend whose answers wait in a pool the test delivers from in an order
 * it draws, each query answered whole, in parts, with more than asked, or
 * jointly with a sibling, and now and then with data nobody asked for.
 */
function hostile(tc: hegel.TestCase, rows: Rec[]) {
  const pool: { stamp: number; send(): void }[] = [];
  const batches: number[] = [];
  const backend = {
    answer(batch: AggregateBatch<Field>, reply: (a: AggregateAnswer<Field>) => void) {
      batches.push(batch.stamp);
      const later = (a: AggregateAnswer<Field>) => pool.push({ stamp: batch.stamp, send: () => reply(a) });
      const joined = new Set<number>();
      for (const q of batch.queries) {
        if (joined.has(q.id)) continue;
        const range = typeof q.want === "string" && q.want !== "total";
        // One answer for this query and the next of its kind under the same
        // path, naming only this one: the other is done once covered.
        const twin = batch.queries.find((o) => o.id !== q.id && !joined.has(o.id) && o.want === q.want && sameSteps(o.path, q.path));
        if (range && twin && tc.draw(gs.booleans())) {
          joined.add(twin.id);
          later(compute(rows, batch, q, Math.min(q.start, twin.start), Math.max(q.end, twin.end)));
          continue;
        }
        const shape = range ? tc.draw(gs.sampledFrom(["whole", "parts", "more"] as const)) : "whole";
        if (shape === "more") later(compute(rows, batch, q, q.start, extent(rows, batch, q)));
        else if (shape === "parts" && q.end - q.start > 1) {
          const cut = tc.draw(gs.integers({ minValue: q.start + 1, maxValue: q.end - 1 }));
          later({ ...compute(rows, batch, q, q.start, cut), id: null });
          later(compute(rows, batch, q, cut, q.end));
        } else later(compute(rows, batch, q));
      }
      if (tc.draw(gs.integers({ minValue: 0, maxValue: 4 })) === 0) {
        const top = { id: -1, path: [], want: "groups" as const, start: 0, end: Number.MAX_SAFE_INTEGER };
        later({ ...compute(rows, batch, top), id: null });
      }
    },
  };
  /** Delivers pooled answers in a drawn order until `p` settles. */
  const settle = async <T>(p: T | Promise<T>): Promise<T> => {
    let done = false;
    let value!: T;
    void Promise.resolve(p).then((v) => ((done = true), (value = v)));
    for (let idle = 0, sent = 0; ; ) {
      for (let i = 0; i < 8; i++) await Promise.resolve();
      if (done) return value;
      if (pool.length === 0) {
        if (++idle > 50) throw new Error("stuck: nothing to deliver and nothing settled");
        continue;
      }
      idle = 0;
      // Answers that never let it settle: the layer keeps asking for what it was given.
      if (++sent > 500) throw new Error("no progress: 500 answers and nothing settled");
      const i = tc.draw(gs.integers({ minValue: 0, maxValue: pool.length - 1 }));
      pool.splice(i, 1)[0]!.send();
    }
  };
  return { backend, settle, batches, pool };
}

const sameSteps = (a: GroupPath<Field>, b: GroupPath<Field>) =>
  a.length === b.length && a.every((s, i) => s.field === b[i]!.field && s.value === b[i]!.value);

const placements = gs.sampledFrom<Placement>(["before", "after", "off"]);

/** A grouping by one or two of the text columns, with drawn Subtotals, Grand Total and depth open, counting and summing `n`. */
const groupings = gs.composite((tc) => {
  const levels: Field[] = tc.draw(gs.sampledFrom<Field[]>([["s"], ["t"], ["s", "t"], ["t", "s"]]));
  const base = rowsBy<Field>(levels[0]!);
  const grouping: Grouping<Field, number, Field> = {
    ...base,
    rowLevels: levels.map((field) => ({ field, subtotal: tc.draw(placements) })),
    values: [
      { field: "n", aggregate: "sum" },
      { field: null, aggregate: "count" },
    ],
    rowAxis: { ...base.rowAxis, grandTotal: tc.draw(placements), expand: { depth: tc.draw(gs.sampledFrom<0 | 1 | "all">([0, 1, "all"])), except: [] } },
  };
  return grouping;
});

const viewOf = (grouping: Grouping<Field, number, Field>, hidden: number[], conditions: Condition<Field>[] = []): ViewSpec<number, Field> => ({
  rows: { filter: { quick: null, conditions }, sort: [], hidden },
  cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
  grouping,
});

type Shown = number | { kind: string; value: ListedValue; figures: Figure[] };

/** What fromRows shows grouping `rows` by `view` at once. */
function expected(rows: Rec[], view: ViewSpec<number, Field>): Shown[] {
  const source = fromRows(rows, COLUMNS);
  try {
    source.view!(view, signal);
    const n = source.rows.count() as number;
    const window = source.rows.window(0, n, signal) as OutlinedWindow<number, Field>;
    return window.ids.map((id) => {
      const g = source.group(id);
      return g ? { kind: g.kind, value: g.kind === "grandTotal" ? null : g.value, figures: g.aggregates } : id;
    });
  } finally {
    source.destroy();
  }
}

/** What `source` shows from `start`, its Summary Items by their figures. */
async function shown(source: GroupedSource<Field, unknown>, settle: <T>(p: T | Promise<T>) => Promise<T>): Promise<Shown[]> {
  const n = source.rows.count() as number;
  // As the Grid does, ask again while the answer stops short.
  let got: OutlinedWindow<number, Field> = { ids: [], outlines: [] };
  while (got.ids.length < n) got = (await settle(source.rows.window(0, n, signal))) as OutlinedWindow<number, Field>;
  // A Summary Item's figures may land after it does: its cells wait for them.
  await settle(source.cells(got.ids, ["n"], signal));
  return got.ids.map((id) => {
    if (id >= 0) return id;
    const s = source.summary(id)!;
    return { kind: s.kind, value: s.path.at(-1)?.value ?? null, figures: s.figures };
  });
}

test("rows grouped over a backend answering in any order and shape match grouping them at once", () =>
  hegel.testAsync(async (tc) => {
    const rows = tc.draw(records);
    const { backend, settle } = hostile(tc, rows);
    const source = groupedSource({ base: fromRows(rows, COLUMNS), backend, cache: tc.draw(caches) });
    try {
      const hidden = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: Math.max(0, rows.length - 1) }), { maxSize: 3 }));
      const view = viewOf(tc.draw(groupings), hidden);
      const applied = await settle(source.view!(view, signal));
      expect(applied.grouping?.rowLevels).toEqual(view.grouping!.rowLevels);
      const got = await shown(source, settle);
      const want = expected(rows, view);
      if (JSON.stringify(got) !== JSON.stringify(want)) console.log("GOT", JSON.stringify(got), "WANT", JSON.stringify(want));
      expect(got).toEqual(want);
    } finally {
      source.destroy();
    }
  }));

test("answers to a View Spec a newer one superseded are dropped, however late they land", () =>
  hegel.testAsync(async (tc) => {
    const rows = tc.draw(records);
    const { backend, settle, pool } = hostile(tc, rows);
    const source = groupedSource({ base: fromRows(rows, COLUMNS), backend, cache: tc.draw(caches) });
    try {
      const first = viewOf(tc.draw(groupings), []);
      const old = new AbortController();
      const asked = source.view!(first, old.signal);
      // Some of the first spec's answers land before the newer one is asked.
      for (let i = tc.draw(gs.integers({ minValue: 0, maxValue: 3 })); i > 0 && pool.length > 0; i--) pool.shift()!.send();
      old.abort();
      void Promise.resolve(asked).catch(() => {});
      const view = viewOf(tc.draw(groupings), []);
      await settle(source.view!(view, signal));
      const got = await shown(source, settle);
      const want = expected(rows, view);
      if (JSON.stringify(got) !== JSON.stringify(want)) console.log("GOT", JSON.stringify(got), "WANT", JSON.stringify(want));
      expect(got).toEqual(want);
    } finally {
      source.destroy();
    }
  }));

test("a Leaf is located where grouping at once places it, and a hidden one is gone", () =>
  hegel.testAsync(async (tc) => {
    const rows = tc.draw(records);
    const { backend, settle } = hostile(tc, rows);
    const source = groupedSource({ base: fromRows(rows, COLUMNS), backend, cache: tc.draw(caches) });
    const model = fromRows(rows, COLUMNS);
    try {
      const hidden = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: Math.max(0, rows.length - 1) }), { maxSize: 3 }));
      const view = viewOf(tc.draw(groupings), hidden);
      model.view!(view, signal);
      await settle(source.view!(view, signal));
      const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length }));
      const want = model.rows.locate!(id, signal) as Located;
      expect(await settle(source.rows.locate!(id, signal))).toEqual(want.type === "Found" ? want : { type: "Gone", suggest: null });
    } finally {
      source.destroy();
      model.destroy();
    }
  }));

test("an edit refigures every figure where it is, pending until the backend answers", () =>
  hegel.testAsync(async (tc) => {
    const rows = tc.draw(records.filter((r) => r.length > 0));
    const { backend, settle, pool } = hostile(tc, rows);
    const base = fromRows(rows, COLUMNS);
    const source = groupedSource({ base, backend, cache: tc.draw(caches) });
    const told: ChangeNotice[] = [];
    const quiet = { notify: () => {}, supportChanged: () => {}, movableChanged: () => {} };
    source.attach!({
      rows: { ...quiet, notify: (n) => told.push(n) },
      cols: quiet,
      viewChanged: () => {},
      findCounted: () => {},
    });
    const items = async () => {
      const n = source.rows.count() as number;
      let got: OutlinedWindow<number, Field> = { ids: [], outlines: [] };
      while (got.ids.length < n) got = (await settle(source.rows.window(0, n, signal))) as OutlinedWindow<number, Field>;
      return got.ids;
    };
    try {
      await settle(source.view!(viewOf(tc.draw(groupings), []), signal));
      const before = await items();
      await settle(source.cells(before, ["n"], signal));
      const paths = new Map(before.filter((id) => id < 0).map((id) => [id, source.summary(id)!.path]));
      for (let edits = tc.draw(gs.integers({ minValue: 1, maxValue: 3 })); edits > 0; edits--) {
        const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
        const field = tc.draw(gs.sampledFrom<Field>(["s", "t", "n"]));
        rows[id] = { ...rows[id]!, [field]: tc.draw(record)[field] };
        told.length = 0;
        base.edited(id);
        expect(told.some((n) => n.type === "ContentChanged"), "the rows are drawn anew").toBe(true);
        for (const s of paths.keys()) expect(source.summary(s)?.figures ?? [], "figures pending").toEqual([]);
        // Some answers land before the next edit.
        for (let i = tc.draw(gs.integers({ minValue: 0, maxValue: 3 })); i > 0 && pool.length > 0; i--) pool.shift()!.send();
      }
      told.length = 0;
      await settle(source.cells(before, ["n"], signal));
      expect(await items(), "nothing moves").toEqual(before);
      const figures = (path: GroupPath<Field>): Figure[] => {
        const leaves = rows.filter((r) => path.every((p) => r[p.field as Field] === p.value));
        return [leaves.length ? leaves.reduce((sum, r) => sum + r.n, 0) : null, leaves.length];
      };
      for (const [id, path] of paths) expect(source.summary(id)?.figures, JSON.stringify(path)).toEqual(figures(path));
    } finally {
      source.destroy();
    }
  }));

/** A filter keeping the rows whose `s` or `t` is one of a few values, or none of them. */
const filters = gs.composite((tc): Condition<Field>[] => {
  if (tc.draw(gs.integers({ minValue: 0, maxValue: 3 })) === 0) return [];
  const id = tc.draw(gs.sampledFrom<Field>(["s", "t"]));
  const values = tc.draw(gs.arrays(gs.sampledFrom<ListedValue>(["a", "b", "c", "x", "y", null]), { minSize: 1, maxSize: 3 }));
  return [{ id, kind: { type: "OneOf", values }, not: tc.draw(gs.booleans()) }];
});

test("an edit that stops a Leaf matching the filter, or fitting its group, leaves it unmatched, as fromRows does", () =>
  hegel.testAsync(async (tc) => {
    const rows = tc.draw(records.filter((r) => r.length > 0));
    const { backend, settle, pool } = hostile(tc, rows);
    const base = fromRows(rows, COLUMNS);
    const source = groupedSource({ base, backend, cache: tc.draw(caches) });
    const model = fromRows(rows, COLUMNS);
    try {
      const view = viewOf(tc.draw(groupings), [], tc.draw(filters));
      model.view!(view, signal);
      await settle(source.view!(view, signal));
      const n = source.rows.count() as number;
      let got: OutlinedWindow<number, Field> = { ids: [], outlines: [] };
      while (got.ids.length < n) got = (await settle(source.rows.window(0, n, signal))) as OutlinedWindow<number, Field>;
      for (let edits = tc.draw(gs.integers({ minValue: 1, maxValue: 3 })); edits > 0; edits--) {
        const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
        const field = tc.draw(gs.sampledFrom<Field>(["s", "t", "n"]));
        rows[id] = { ...rows[id]!, [field]: tc.draw(record)[field] };
        base.edited(id);
        model.edited(id);
        for (let i = tc.draw(gs.integers({ minValue: 0, maxValue: 3 })); i > 0 && pool.length > 0; i--) pool.shift()!.send();
      }
      const flags = await settle(source.rows.unmatched!(got.ids, signal));
      const leaves = got.ids.filter((id) => id >= 0);
      const want = model.rows.unmatched!(leaves, signal) as boolean[];
      const byLeaf = new Map(leaves.map((id, i) => [id, want[i]]));
      expect(flags).toEqual(got.ids.map((id) => (id < 0 ? false : byLeaf.get(id))));
    } finally {
      source.destroy();
      model.destroy();
    }
  }));

test("rows a grouped axis has not had answered draw pending in the band, then their labels", async () => {
  const rows: Rec[] = Array.from({ length: 30 }, (_, i) => ({ s: "abc"[i % 3]!, t: null, n: i }));
  // The groups come at once; the Leaves wait for the test.
  const leaves: (() => void)[] = [];
  const source = groupedSource({
    base: fromRows(rows, COLUMNS),
    backend: {
      answer(batch, reply) {
        for (const q of batch.queries) {
          const send = () => reply(compute(rows, batch, q));
          if (q.want === "leaves") leaves.push(send);
          else send();
        }
      },
    },
  });
  const h = new Harness(source);
  const settle = async () => {
    for (let i = 0; i < 10; i++) {
      await new Promise((r) => setTimeout(r, 0));
      h.frame();
      h.drain();
    }
  };
  const band = () => {
    const s = h.frame();
    const painter = createCanvasPainter(light);
    painter.resize(h.width, h.height, 1);
    calls.length = 0;
    painter.paint(s, h.feed.content(s));
    return calls
      .filter((c) => c.name === "fillText" && (c.args[1] as number) < BAND && (c.args[2] as number) > HEADER)
      .map((c) => ({ text: c.args[0], fill: c.fill }));
  };
  try {
    h.frame();
    h.grid.group_by(0, h.feed.colKey("s"));
    h.drain();
    await settle();
    const pending = band();
    expect(h.snapshot.rows.headers.length).toBeGreaterThan(0);
    expect(pending).toEqual(h.snapshot.rows.headers.map(() => ({ text: "…", fill: light.loading })));
    // A Window answers what the layer holds, here the first group alone, and comes again for the rest.
    expect((await source.rows.window(0, 3, signal)) as OutlinedWindow<number, Field>).toMatchObject({ ids: [-2] });
    leaves.splice(0).forEach((send) => send());
    await settle();
    const drawn = band();
    expect(drawn.map((t) => t.text)).not.toContain("…");
    // A group's disclosure mark and its label, cut to the band's width.
    expect(drawn.map((t) => t.text)).toContain("▾");
    expect(drawn.map((t) => t.text)).toContain("b…");
  } finally {
    h.destroy();
    source.destroy();
  }
});
