// A slow Table Source as the widget drives it: answers that land late, are
// superseded, fail or leave part of the request out, news the source pushes
// through its sinks, and the typing pause in front of the controls.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import { createPause } from "../src/pause";
import type { AppliedView, Support, FindCount, FindQuery, TableSink, TableSource, ViewSpec } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

type Source = TableSource<string, string, string>;

/** A request the test answers when it likes. */
interface Late<Asked, T> {
  asked: Asked;
  signal: AbortSignal;
  resolve(answer: T): void;
  reject(error: Error): void;
}

/** A thenable the test settles by hand, so answers land synchronously. */
function late<Asked, T>(into: Late<Asked, T>[], asked: Asked, signal: AbortSignal): Promise<T> {
  let done: { ok(v: T): void; fail(e: unknown): void } | undefined;
  into.push({ asked, signal, resolve: (v) => done?.ok(v), reject: (e) => done?.fail(e) });
  return {
    then: (ok: (v: T) => void, fail: (e: unknown) => void) => void (done = { ok, fail }),
  } as unknown as Promise<T>;
}

/** Ten rows, three columns, every view and find request answered by hand. */
function slowSource() {
  const rowIds = Array.from({ length: 10 }, (_, i) => `r${i}`);
  const colIds = ["c0", "c1", "c2"];
  const views: Late<ViewSpec<string, string>, AppliedView<string, string>>[] = [];
  const nexts: Late<FindQuery<string>, { row: string; col: string } | null>[] = [];
  const counts: Late<FindQuery<string>, FindCount>[] = [];
  const marks: Late<{ rows: string[]; cols: string[] }, boolean[][]>[] = [];
  const support = new Map<string, Support>();
  let sink!: TableSink<string, string>;
  const source: Source = {
    rows: {
      count: () => rowIds.length,
      window: (s, e) => rowIds.slice(s, e),
      locate: (id) => ({ type: "Found", position: rowIds.indexOf(id) }),
      support: (id) => support.get(id) ?? {},
      quickFilter: { regex: true },
    },
    cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}/${c}`)),
    view: (spec, signal) => late(views, spec, signal),
    find: {
      regex: true,
      marks: (rs, cs, _q, signal) => late(marks, { rows: rs, cols: cs }, signal),
      next: (_from, _direction, q, signal) => late(nexts, q, signal),
      count: (q, signal) => late(counts, q, signal),
    },
    attach: (s) => {
      sink = s;
      return () => {};
    },
  };
  return { source, views, nexts, counts, marks, support, sink: () => sink };
}

const quick = (text: string) => ({ text, caseSensitive: false, regex: true, searches: null });
const findText = (text: string): FindQuery<string> => ({
  query: { text, caseSensitive: false, regex: false, searches: "shown" },
  by: "rows",
});
const none = { filter: { quick: null, conditions: [] }, sort: [] };
/** What the source applies: the rows' part of `spec`, with `quick` in place of its quick filter. */
const applying = (spec: ViewSpec<string, string>, q = spec.rows.filter.quick): AppliedView<string, string> => ({
  rows: { ...spec.rows, filter: { ...spec.rows.filter, quick: q } },
  cols: none,
  totals: { rows: 10, cols: 3 },
});

describe("a slow view", () => {
  test("the rows stay while a filter is pending; a superseded request is aborted and its late answer paints nothing", () => {
    const { source, views } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      const before = h.values();
      h.grid.set_quick_filter(0, quick("a"));
      h.drain();
      h.frame();
      expect(h.snapshot.viewPending).toBe(true);
      expect(h.values()).toEqual(before);
      h.grid.set_quick_filter(0, quick("b"));
      h.drain();
      expect(views.map((v) => v.signal.aborted)).toEqual([true, false]);
      views[0]!.resolve(applying(views[0]!.asked));
      h.drain();
      h.frame();
      expect(h.snapshot.viewPending).toBe(true);
      expect(h.snapshot.appliedView.rows.filter.quick).toBeNull();
      views[1]!.resolve(applying(views[1]!.asked));
      h.drain();
      h.frame();
      expect(h.snapshot.viewPending).toBe(false);
      expect(h.snapshot.appliedView.rows.filter.quick?.text).toBe("b");
      expect(h.snapshot.requestedView).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("an answer leaving the quick filter out keeps it wanted, not applied, with the source's message, until an answer leaves nothing out", () => {
    const { source, views } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_quick_filter(0, quick("("));
      h.drain();
      views[0]!.resolve({ ...applying(views[0]!.asked, null), error: "Unterminated group" });
      h.drain();
      h.frame();
      expect(h.snapshot.viewPending).toBe(false);
      expect(h.snapshot.requestedView?.rows.filter.quick?.text).toBe("(");
      expect(h.feed.viewError()).toBe("Unterminated group");
      expect(h.errors).toEqual([{ message: "Unterminated group", request: { type: "view" } }]);
      h.grid.set_quick_filter(0, quick("a"));
      h.drain();
      views[1]!.resolve(applying(views[1]!.asked));
      h.drain();
      h.frame();
      expect(h.snapshot.requestedView).toBeNull();
      expect(h.feed.viewError()).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("a failed request keeps the spec in force, the text asked for not applied, and the failure's message", () => {
    const { source, views } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_quick_filter(0, quick("a"));
      h.drain();
      views[0]!.reject(new Error("source is down"));
      h.drain();
      h.frame();
      expect(h.snapshot.appliedView.rows.filter.quick).toBeNull();
      expect(h.snapshot.requestedView?.rows.filter.quick?.text).toBe("a");
      expect(h.feed.viewError()).toBe("source is down");
    } finally {
      h.destroy();
    }
  });

  test("news the source pushes: a condition it drops shows not applied, new totals land, and answers due stay due", () => {
    const { source, views, sink } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      const key = h.feed.colKey("c1");
      const condition = (text: string) => ({
        key,
        kind: { type: "Text" as const, query: { text, caseSensitive: false, regex: false, searches: [key] } },
        not: false,
      });
      h.grid.add_condition(0, condition("x"));
      h.drain();
      views[0]!.resolve(applying(views[0]!.asked));
      h.drain();
      h.frame();
      expect(h.snapshot.rows.conditions.map((c) => c.state)).toEqual(["applied"]);
      h.grid.add_condition(0, condition("y"));
      h.drain();
      sink().viewChanged({ rows: none, cols: none, totals: { rows: 7 } });
      h.frame();
      expect(h.snapshot.viewPending).toBe(true);
      expect(h.snapshot.rows.total).toBe(7);
      views[1]!.resolve({ ...applying(views[1]!.asked), rows: none });
      h.drain();
      h.frame();
      expect(h.snapshot.viewPending).toBe(false);
      expect(h.snapshot.rows.conditions.map((c) => c.state)).toEqual(["notApplied", "notApplied"]);
      expect(h.snapshot.cols.headers[1]?.filter).toEqual({ filtered: false, pending: false, notApplied: true });
    } finally {
      h.destroy();
    }
  });

  test("support changed: the widget asks the source again, for the columns named or every one in view", () => {
    const { source, support, sink } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      const texts = () => h.frame().cols.headers.map((x) => x.support.text);
      expect(texts()).toEqual([{ regex: false }, { regex: false }, { regex: false }]);
      support.set("c1", { text: { regex: true } });
      support.set("c2", { text: false });
      sink().rows.supportChanged(["c1"]);
      expect(texts()).toEqual([{ regex: false }, { regex: true }, { regex: false }]);
      sink().rows.supportChanged();
      expect(texts()).toEqual([{ regex: false }, { regex: true }, null]);
    } finally {
      h.destroy();
    }
  });
});

describe("a slow find", () => {
  const settle = (h: Harness<string, string, string>) => {
    h.drain();
    h.frame();
    h.drain();
  };

  test("presses before the query's first step answer step once; after it, one more press waits for the step in flight", () => {
    const { source, nexts } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("r")));
      for (let i = 0; i < 3; i++) h.grid.find_next();
      settle(h);
      expect(nexts.length).toBe(1);
      nexts[0]!.resolve({ row: "r0", col: "c0" });
      settle(h);
      for (let i = 0; i < 3; i++) h.grid.find_next();
      settle(h);
      expect(nexts.length).toBe(2);
      nexts[1]!.resolve({ row: "r0", col: "c1" });
      settle(h);
      expect(nexts.length).toBe(3);
      nexts[2]!.resolve({ row: "r0", col: "c2" });
      settle(h);
      expect(nexts.length).toBe(3);
      expect(h.snapshot.findMatch).toEqual({ row: h.feed.rowKey("r0"), col: h.feed.colKey("c2") });
    } finally {
      h.destroy();
    }
  });

  test("however many times the rows change before a frame, the count is asked once and the marks again", () => {
    const { source, counts, marks, sink } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("r")));
      settle(h);
      expect(counts.length).toBe(1);
      counts[0]!.resolve({ total: 30 });
      for (const m of marks.splice(0)) m.resolve(m.asked.rows.map(() => m.asked.cols.map(() => false)));
      settle(h);
      expect(h.feed.findState().count).toEqual({ total: 30 });
      for (let i = 0; i < 3; i++) sink().rows.notify({ type: "Refiltered" });
      settle(h);
      expect(counts.length).toBe(2);
      expect(marks.length).toBeGreaterThan(0);
      expect(h.feed.findState().counting).toBe(true);
    } finally {
      h.destroy();
    }
  });

  test("running counts pushed for the query in force land, partial until the last; those for an old query are dropped", () => {
    const { source, counts, sink } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("r")));
      settle(h);
      const asked = counts[0]!.asked;
      counts[0]!.resolve({ total: 3, partial: true });
      settle(h);
      expect(h.feed.findState().count).toEqual({ total: 3, partial: true });
      sink().findCounted(asked, { total: 12, partial: true });
      expect(h.feed.findState().count).toEqual({ total: 12, partial: true });
      sink().findCounted(asked, { total: 20 });
      expect(h.feed.findState().count).toEqual({ total: 20 });
      h.grid.set_find(h.feed.findKeys(findText("c")));
      settle(h);
      sink().findCounted(asked, { total: 99 });
      expect(h.feed.findState().count).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("a failed count reads as an error on its query, which a new query clears", () => {
    const { source, counts } = slowSource();
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("(")));
      settle(h);
      counts[0]!.reject(new Error("Unterminated group"));
      settle(h);
      expect(h.feed.findState()).toMatchObject({ error: "Unterminated group", counting: false, count: null });
      expect(h.errors.map((e) => e.request)).toEqual([{ type: "find", request: "count" }]);
      h.grid.set_find(h.feed.findKeys(findText("r")));
      settle(h);
      expect(h.feed.findState()).toMatchObject({ error: null, counting: true });
    } finally {
      h.destroy();
    }
  });
});

describe("the typing pause", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  test("typed text goes once typing stops for the pause; sending at once drops the wait", () => {
    vi.useFakeTimers();
    let sent = 0;
    const pause = createPause(
      () => 150,
      () => sent++,
    );
    pause.typed();
    vi.advanceTimersByTime(100);
    pause.typed();
    vi.advanceTimersByTime(149);
    expect([sent, pause.waiting]).toEqual([0, true]);
    vi.advanceTimersByTime(1);
    expect([sent, pause.waiting]).toEqual([1, false]);
    pause.typed();
    pause.now();
    vi.advanceTimersByTime(1000);
    expect(sent).toBe(2);
  });

  test("whatever the keys and the waits, text goes exactly when typing has stopped for the pause or something sends at once", () =>
    hegel.test((tc) => {
      vi.useFakeTimers();
      try {
        const delay = tc.draw(gs.integers({ minValue: 0, maxValue: 300 }));
        let sent = 0;
        const pause = createPause(
          () => delay,
          () => sent++,
        );
        // The model: when what was typed is due, if anything waits.
        let [clock, due, expected] = [0, null as number | null, 0];
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 20 }));
        for (let i = 0; i < steps; i++) {
          const action = tc.draw(gs.sampledFrom(["typed", "now", "cancel", "wait"] as const));
          if (action === "typed") {
            pause.typed();
            due = clock + delay;
          } else if (action === "now") {
            pause.now();
            expected++;
            due = null;
          } else if (action === "cancel") {
            pause.cancel();
            due = null;
          } else {
            const ms = tc.draw(gs.integers({ minValue: 0, maxValue: 400 }));
            vi.advanceTimersByTime(ms);
            clock += ms;
            if (due !== null && due <= clock) {
              expected++;
              due = null;
            }
          }
          expect(sent).toBe(expected);
          expect(pause.waiting).toBe(due !== null);
        }
      } finally {
        vi.useRealTimers();
      }
    }));
});
