// Unmatched rows in the feed: the flags a Table Source gives for the items
// shown, asked again after each Change Notice (on either axis) and applied
// View Spec, through late answers in any order; and fromRows end to end,
// where the user's edit leaves a row or column in view, flagged, until a
// re-apply.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows } from "../src/rows";
import type { TableSource } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Late {
  resolve(): void;
  reject(): void;
}

/** Rows `r0..` by one column, whose unmatched flags the test sets and answers late. */
function lateFlags(n: number) {
  const ids = Array.from({ length: n }, (_, i) => `r${i}`);
  const unmatched = new Set<string>();
  const pending: Late[] = [];
  const sinks: { notify(n: { type: "ContentChanged"; start: number; end: number }): void }[] = [];
  const source: TableSource<string, string, string> = {
    rows: {
      count: () => ids.length,
      window: (s, e) => ids.slice(s, e),
      unmatched: (asked) => {
        let done: { ok(v: boolean[]): void; fail(e: unknown): void } | undefined;
        pending.push({
          resolve: () => done?.ok(asked.map((id) => unmatched.has(id))),
          reject: () => done?.fail(new Error("unmatched failed")),
        });
        const thenable = { then: (ok: (v: boolean[]) => void, fail: (e: unknown) => void) => void (done = { ok, fail }) };
        return thenable as unknown as Promise<boolean[]>;
      },
    },
    cols: { count: () => 1, window: (s, e) => ["c"].slice(s, e) },
    cells: (rs) => rs.map((r) => [r]),
    attach(sink) {
      sinks.push(sink.rows);
      return () => {};
    },
  };
  /** Flips row `i`'s flag and tells the widget its content changed, as an edit would. */
  const flip = (i: number) => {
    const id = ids[i]!;
    if (!unmatched.delete(id)) unmatched.add(id);
    for (const s of sinks) s.notify({ type: "ContentChanged", start: i, end: i + 1 });
  };
  return { source, pending, unmatched, flip };
}

function flags(h: Harness<unknown, unknown, unknown>): boolean[] {
  return h.snapshot.rows.strip.map((_, i) => h.content?.unmatched("rows", i) ?? false);
}

describe("unmatched rows in the feed", () => {
  test("with late answers in any order and failures, the flags shown settle to the source's once every answer lands", () =>
    hegel.test((tc) => {
      const n = tc.draw(gs.integers({ minValue: 1, maxValue: 30 }));
      const { source, pending, unmatched, flip } = lateFlags(n);
      const h = new Harness(source);
      try {
        h.frame();
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 20 }));
        for (let i = 0; i < steps; i++) {
          const op = tc.draw(gs.sampledFrom(["flip", "resolve", "reject", "scroll", "frame"]));
          if (op === "flip") flip(tc.draw(gs.integers({ minValue: 0, maxValue: n - 1 })));
          else if ((op === "resolve" || op === "reject") && pending.length) {
            const late = pending.splice(tc.draw(gs.integers({ minValue: 0, maxValue: pending.length - 1 })), 1)[0]!;
            if (op === "resolve") late.resolve();
            else late.reject();
          } else if (op === "scroll") h.grid.scroll_by(0, tc.draw(gs.integers({ minValue: -10, maxValue: 10 })));
          h.drain();
          h.frame();
        }
        for (let round = 0; round < 20 && pending.length; round++) {
          for (const late of pending.splice(0)) late.resolve();
          h.drain();
          h.frame();
        }
        expect(pending).toEqual([]);
        const shown = h.snapshot.rows.strip.map((r) => h.feed.rowId(r.key!));
        expect(flags(h)).toEqual(shown.map((id) => unmatched.has(id as string)));
      } finally {
        h.destroy();
      }
    }));

  test("a failed answer is reported and leaves the rows unflagged", () => {
    const { source, pending, flip } = lateFlags(3);
    const h = new Harness(source);
    try {
      h.frame();
      flip(1);
      h.frame();
      for (const late of pending.splice(0)) late.reject();
      h.frame();
      expect(flags(h)).toEqual([false, false, false]);
      expect(h.errors.map((e) => e.request)).toContainEqual({ type: "unmatched", axis: "rows" });
    } finally {
      h.destroy();
    }
  });

  test("in fromRows, an edited row the filter drops stays flagged until the View Spec is applied again", () => {
    const rows = [
      { name: "Bergen", port: "Oslo" },
      { name: "Cádiz", port: "Bergen" },
      { name: "Hull", port: "Hull" },
    ];
    const source = fromRows(rows, [{ key: "name" }, { key: "port" }]);
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_quick_filter(0, { text: "bergen", caseSensitive: false, regex: false, searches: null });
      h.drain();
      h.frame();
      expect(h.values()).toEqual([
        ["Bergen", "Oslo"],
        ["Cádiz", "Bergen"],
      ]);
      expect(flags(h)).toEqual([false, false]);
      rows[1]!.port = "Leith";
      source.edited(1);
      h.frame();
      expect(h.values()).toEqual([
        ["Bergen", "Oslo"],
        ["Cádiz", "Leith"],
      ]);
      expect(flags(h)).toEqual([false, true]);
      h.grid.reapply_view();
      h.drain();
      h.frame();
      expect(h.values()).toEqual([["Bergen", "Oslo"]]);
      expect(flags(h)).toEqual([false]);
    } finally {
      h.destroy();
      source.destroy();
    }
  });

  test("in fromRows, a column an edited row stops matching stays flagged until the View Spec is applied again", () => {
    const rows = [
      { name: "Bergen", port: "Oslo" },
      { name: "Cádiz", port: "Bergen" },
    ];
    const source = fromRows(rows, [{ key: "name" }, { key: "port" }]);
    const h = new Harness(source);
    const columns = () => h.snapshot.cols.strip.map((_, j) => h.content?.label("cols", h.snapshot.cols.strip[j]!) ?? "");
    const colFlags = () => h.snapshot.cols.strip.map((_, j) => h.content?.unmatched("cols", j) ?? false);
    try {
      h.frame();
      // The columns with "bergen" in the second row.
      h.grid.set_quick_filter(1, { text: "bergen", caseSensitive: false, regex: false, searches: [h.feed.rowKey(1)] });
      h.drain();
      h.frame();
      expect(columns()).toEqual(["port"]);
      expect(colFlags()).toEqual([false]);
      rows[1]!.port = "Leith";
      source.edited(1);
      h.frame();
      expect(columns()).toEqual(["port"]);
      expect(colFlags()).toEqual([true]);
      h.grid.reapply_view();
      h.drain();
      h.frame();
      expect(columns()).toEqual([]);
    } finally {
      h.destroy();
      source.destroy();
    }
  });

  test("a row a change brings back into view shows its new value, never one cached from before it was hidden", () => {
    const rows = [
      { name: "Bergen", port: "Oslo" },
      { name: "Hull", port: "Hull" },
    ];
    const source = fromRows(rows, [{ key: "name" }, { key: "port" }]);
    const h = new Harness(source);
    try {
      h.frame();
      expect(h.values()).toEqual([
        ["Bergen", "Oslo"],
        ["Hull", "Hull"],
      ]);
      h.grid.set_quick_filter(0, { text: "bergen", caseSensitive: false, regex: false, searches: null });
      h.drain();
      h.frame();
      expect(h.values()).toEqual([["Bergen", "Oslo"]]);
      // Hidden, the row changes in the data and so comes back.
      rows[1]!.port = "Bergen";
      source.changed(1);
      h.frame();
      h.drain();
      h.frame();
      expect(h.values()).toEqual([
        ["Bergen", "Oslo"],
        ["Hull", "Bergen"],
      ]);
    } finally {
      h.destroy();
      source.destroy();
    }
  });
});
