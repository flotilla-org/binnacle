// Selection events: what the widget hands `onSelectionChange` after each
// frame, by the Table Source's own IDs, and only when it changed.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, expect, test } from "vitest";
import { createSelectionEvents, type SelectionChange } from "../src/selection";
import type { AxisSink, Located, TableSource } from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

const SHIFT = 1;
const CTRL = 2;

/** A keyed table of `rows` rows `r0..` and the columns `a`, `b`, `c`, whose row order the test can reverse. */
function table(rows: number) {
  let order = Array.from({ length: rows }, (_, i) => `r${i}`);
  let sink!: AxisSink<string>;
  const source: TableSource<string, string, string> = {
    rows: {
      count: () => order.length,
      window: (s, e) => order.slice(s, e),
      locate: (id): Located => ({ type: "Found", position: order.indexOf(id) }),
    },
    cols: { count: () => 3, window: (s, e) => ["a", "b", "c"].slice(s, e) },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
    attach: (sinks) => ((sink = sinks.rows), () => {}),
  };
  return {
    source,
    reverse() {
      order = [...order].reverse();
      sink.notify({ type: "Reordered" });
    },
  };
}

/** A harness whose every frame goes through the selection events, recording what they emit. */
function watched(source: TableSource<string, string, string>) {
  const h = new Harness(source);
  const changes: SelectionChange<string, string>[] = [];
  const observe = createSelectionEvents(h.feed, (change) => changes.push(change));
  const frame = () => observe(h.frame());
  return { h, changes, frame };
}

const cell = (row: number, col: number) => [BAND + col * COL_WIDTH + 10, HEADER + row * ROW_HEIGHT + ROW_HEIGHT / 2] as const;

test("fires once per change with the focused cell's IDs, and not on frames that change nothing", () => {
  const { h, changes, frame } = watched(table(10).source);
  try {
    frame();
    frame();
    expect(changes).toEqual([]);
    h.click(...cell(2, 1));
    frame();
    frame();
    frame();
    expect(changes).toEqual([
      {
        focus: { rowId: "r2", colId: "b" },
        selection: null,
        picked: [],
      },
    ]);
    h.click(...cell(2, 1));
    frame();
    expect(changes).toHaveLength(1);
  } finally {
    h.destroy();
  }
});

test("a range in view reports every ID in order; picked rows report theirs", () => {
  const { h, changes, frame } = watched(table(10).source);
  try {
    frame();
    h.click(...cell(1, 0));
    h.click(...cell(3, 2), SHIFT);
    frame();
    expect(changes.at(-1)!.selection).toEqual({
      rows: { count: 3, first: "r1", last: "r3", ids: ["r1", "r2", "r3"] },
      cols: { count: 3, first: "a", last: "c", ids: ["a", "b", "c"] },
    });
    h.click(...cell(5, 0), CTRL);
    h.click(...cell(4, 0), CTRL);
    frame();
    expect(new Set(changes.at(-1)!.picked)).toEqual(new Set(["r5", "r4"]));
    expect(changes.at(-1)!.focus).toEqual({ rowId: "r4", colId: "a" });
    // Unpicking the focused row changes only the Picks, and still fires.
    const sent = changes.length;
    h.click(...cell(4, 0), CTRL);
    frame();
    expect(changes).toHaveLength(sent + 1);
    expect(changes.at(-1)!.picked).toEqual(["r5"]);
  } finally {
    h.destroy();
  }
});

test("a range larger than the rows the grid holds reports its count and the endpoints it knows", () => {
  const { h, changes, frame } = watched(table(10_000).source);
  try {
    frame();
    h.click(...cell(0, 0));
    h.grid.select_all();
    h.drain();
    frame();
    expect(changes.at(-1)!.selection).toEqual({
      rows: { count: 10_000, first: "r0", last: null, ids: null },
      cols: { count: 3, first: "a", last: "c", ids: ["a", "b", "c"] },
    });
  } finally {
    h.destroy();
  }
});

test("the focused row's IDs survive a reorder, so a reorder alone does not fire", () => {
  const t = table(6);
  const { h, changes, frame } = watched(t.source);
  try {
    frame();
    h.click(...cell(1, 1));
    frame();
    expect(changes).toHaveLength(1);
    t.reverse();
    frame();
    h.drain();
    frame();
    frame();
    expect(h.snapshot.rows.focus).toMatchObject({ position: 4 });
    expect(changes).toHaveLength(1);
    expect(changes[0]!.focus).toEqual({ rowId: "r1", colId: "b" });
  } finally {
    h.destroy();
  }
});

/** Up to eight clicks on visible cells, each plain, Shift or Ctrl, with a frame after each. */
function clicks(tc: hegel.TestCase) {
  const w = watched(table(tc.draw(gs.integers({ minValue: 1, maxValue: 40 }))).source);
  w.frame();
  // Only rows on screen can be clicked: 300 px tall less the header shows 14.
  const onScreen = Math.min(w.h.snapshot.rows.count, 14);
  for (let i = tc.draw(gs.integers({ minValue: 1, maxValue: 8 })); i > 0; i--) {
    const row = tc.draw(gs.integers({ minValue: 0, maxValue: onScreen - 1 }));
    const col = tc.draw(gs.integers({ minValue: 0, maxValue: 2 }));
    w.h.click(...cell(row, col), tc.draw(gs.sampledFrom([0, SHIFT, CTRL])));
    w.frame();
  }
  return w;
}

test("over any clicks, no change repeats the one before, and quiet frames send nothing", () =>
  hegel.test((tc) => {
    const { h, changes, frame } = clicks(tc);
    try {
      const sent = changes.length;
      frame();
      frame();
      expect(changes).toHaveLength(sent);
      for (let i = 1; i < changes.length; i++) expect(changes[i]).not.toEqual(changes[i - 1]);
    } finally {
      h.destroy();
    }
  }));

test("over any clicks, the last change describes the grid's focus, range and Picks", () =>
  hegel.test((tc) => {
    const { h, changes } = clicks(tc);
    try {
      const s = h.snapshot;
      const last = changes.at(-1) ?? { focus: null, selection: null, picked: [] };
      const focus = s.rows.focus && s.cols.focus ? { rowId: h.feed.rowId(s.rows.focus.key!), colId: h.feed.colId(s.cols.focus.key!) } : null;
      expect(last.focus).toEqual(focus);
      expect(new Set(last.picked)).toEqual(new Set(s.picked.map((k) => h.feed.rowId(k))));
      const range = s.selectionRange;
      if (range === null) {
        expect(last.selection).toBeNull();
      } else {
        // Every row clicked is on screen, so the range's IDs are all known.
        const rows = Array.from({ length: range.rows.end - range.rows.start }, (_, i) => `r${range.rows.start + i}`);
        const cols = ["a", "b", "c"].slice(range.cols.start, range.cols.end);
        expect(last.selection).toEqual({
          rows: { count: rows.length, first: rows[0], last: rows.at(-1), ids: rows },
          cols: { count: cols.length, first: cols[0], last: cols.at(-1), ids: cols },
        });
      }
    } finally {
      h.destroy();
    }
  }));
