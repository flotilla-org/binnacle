// The Table Source as the widget drives it: the user's own IDs in and out,
// the value cache asking only for what it lacks, Change Notices from the
// sink, and the answers to sort, Locate and move.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import type { AxisName } from "../src/codes";
import type { AxisSink, ChangeNotice, Located, SortSpec, TableSource } from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

type Sinks = { rows: AxisSink<string>; cols: AxisSink<string> };

/** A keyed table whose cells read `<row>/<col>@<version>`, recording every cell it is asked for. */
class Recorder {
  readonly asked: string[] = [];
  readonly versions = new Map<string, number>();
  sinks!: Sinks;
  readonly source: TableSource<string, string, string>;

  constructor(
    rows: number,
    cols: number,
    extra: Partial<TableSource<string, string, string>["rows"]> = {},
  ) {
    const ids = (prefix: string, n: number) => Array.from({ length: n }, (_, i) => `${prefix}${i}`);
    const rowIds = ids("r", rows);
    const colIds = ids("c", cols);
    this.source = {
      rows: { count: () => rowIds.length, window: (s, e) => rowIds.slice(s, e), ...extra },
      cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
      cells: (rs, cs) =>
        rs.map((r) =>
          cs.map((c) => {
            this.asked.push(`${r}/${c}`);
            return `${r}/${c}@${this.versions.get(r) ?? 0}`;
          }),
        ),
      attach: (sinks) => {
        this.sinks = sinks;
        return () => {};
      },
    };
  }

  take(): string[] {
    return this.asked.splice(0).sort();
  }
}

/** Every visible cell as `<row>/<col>`, for the items `pick` accepts. */
function visible(h: Harness<string, string, string>, pick: (axis: AxisName, position: number) => boolean = () => true) {
  const s = h.snapshot;
  const out: string[] = [];
  for (const r of s.rows.strip) {
    for (const c of s.cols.strip) {
      if (pick("rows", r.position) && pick("cols", c.position)) {
        out.push(`${h.feed.rowId(r.key!)}/${h.feed.colId(c.key!)}`);
      }
    }
  }
  return out.sort();
}

const sizes = {
  rows: gs.integers({ minValue: 1, maxValue: 60 }),
  cols: gs.integers({ minValue: 1, maxValue: 12 }),
  width: gs.integers({ minValue: 100, maxValue: 900 }),
  height: gs.integers({ minValue: 60, maxValue: 700 }),
};

function drawHarness(tc: hegel.TestCase) {
  const rec = new Recorder(tc.draw(sizes.rows), tc.draw(sizes.cols));
  const h = new Harness(rec.source, tc.draw(sizes.width), tc.draw(sizes.height));
  return { rec, h };
}

describe("the value cache", () => {
  test("a frame asks once for each visible cell, and an unchanged frame asks for nothing", () =>
    hegel.test((tc) => {
      const { rec, h } = drawHarness(tc);
      try {
        h.frame();
        expect(rec.take()).toEqual(visible(h));
        h.grid.scroll(0, 0);
        h.frame();
        expect(rec.take()).toEqual([]);
        expect(h.values().flat().every((v) => typeof v === "string")).toBe(true);
      } finally {
        h.destroy();
      }
    }));

  test("scrolling asks only for cells never seen before", () =>
    hegel.test((tc) => {
      const { rec, h } = drawHarness(tc);
      try {
        h.frame();
        rec.take();
        const seen = new Set(visible(h));
        for (let i = tc.draw(gs.integers({ minValue: 1, maxValue: 4 })); i > 0; i--) {
          const dx = tc.draw(gs.integers({ minValue: -200, maxValue: 200 }));
          const dy = tc.draw(gs.integers({ minValue: -200, maxValue: 200 }));
          h.grid.scroll(dx, dy);
          h.frame();
          // Asked once each, and exactly the visible cells not seen on an earlier frame.
          expect(rec.take()).toEqual(visible(h).filter((c) => !seen.has(c)));
          for (const c of visible(h)) seen.add(c);
          expect(h.values().flat().every((v) => typeof v === "string")).toBe(true);
        }
      } finally {
        h.destroy();
      }
    }));

  test("ContentChanged asks again for exactly the visible cells it covers", () =>
    hegel.test((tc) => {
      const { rec, h } = drawHarness(tc);
      try {
        h.frame();
        rec.take();
        const axis = tc.draw(gs.sampledFrom<AxisName>(["rows", "cols"]));
        const count = h.snapshot[axis].count;
        const start = tc.draw(gs.integers({ minValue: 0, maxValue: count - 1 }));
        const end = tc.draw(gs.integers({ minValue: start + 1, maxValue: count }));
        tc.note(`${axis} ${start}..${end}`);
        for (const r of h.snapshot.rows.strip) {
          if (axis === "rows" && r.position >= start && r.position < end) {
            const id = h.feed.rowId(r.key!)!;
            rec.versions.set(id, (rec.versions.get(id) ?? 0) + 1);
          }
        }
        rec.sinks[axis].notify({ type: "ContentChanged", start, end });
        h.frame();
        const inRange = (a: AxisName, p: number) => a !== axis || (p >= start && p < end);
        expect(rec.take()).toEqual(visible(h, inRange));
        // Every visible cell shows its current version.
        const s = h.snapshot;
        s.rows.strip.forEach((r, i) =>
          s.cols.strip.forEach((c, j) => {
            const row = h.feed.rowId(r.key!)!;
            expect(h.content?.value(i, j)).toBe(`${row}/${h.feed.colId(c.key!)}@${rec.versions.get(row) ?? 0}`);
          }),
        );
      } finally {
        h.destroy();
      }
    }));

  test("a Reset forgets every value; a Reordered keeps them", () => {
    const rec = new Recorder(10, 3);
    const h = new Harness(rec.source);
    try {
      h.frame();
      rec.take();
      rec.sinks.rows.notify({ type: "Reordered" });
      h.frame();
      expect(rec.take()).toEqual([]);
      rec.sinks.rows.notify({ type: "Reset" });
      h.frame();
      expect(rec.take()).toEqual(visible(h));
    } finally {
      h.destroy();
    }
  });
});

test("with position identity, a reorder forgets values, since the Keys are Positions", () => {
  const asked: number[] = [];
  let sinks!: { rows: AxisSink<number>; cols: AxisSink<number> };
  const range = (s: number, e: number) => Array.from({ length: e - s }, (_, i) => s + i);
  const h = new Harness<number, number, string>({
    rows: { identity: "position", count: () => 5, window: range },
    cols: { identity: "position", count: () => 1, window: range },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => (asked.push(r), `${r}${c}`))),
    attach: (s) => ((sinks = s), () => {}),
  });
  try {
    h.frame();
    asked.splice(0);
    sinks.rows.notify({ type: "Reordered" });
    h.frame();
    expect(asked).toEqual([0, 1, 2, 3, 4]);
  } finally {
    h.destroy();
  }
});

describe("answers to the Grid's Effects", () => {
  const header = (col: number) => BAND + col * COL_WIDTH + COL_WIDTH / 2;
  const SHIFT = 1;

  test("a header click asks the source to sort by the column's own ID, and shows the spec it applies", () => {
    const asked: SortSpec<string>[] = [];
    const rec = new Recorder(10, 3, {
      sort: (spec) => {
        asked.push(spec);
        // Applies only the first column asked for.
        return spec.slice(0, 1);
      },
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      h.click(header(1), HEADER / 2);
      h.click(header(2), HEADER / 2, SHIFT);
      h.frame();
      expect(asked).toEqual([[{ id: "c1", direction: "ascending" }], [
        { id: "c1", direction: "ascending" },
        { id: "c2", direction: "ascending" },
      ]]);
      const sorts = h.snapshot.cols.headers.map((x) => x.sort);
      expect(sorts.map((x) => x.applied)).toEqual([null, { direction: "ascending", index: 0 }, null]);
      expect(sorts.every((x) => !x.pending)).toBe(true);
    } finally {
      h.destroy();
    }
  });

  test("a source that cannot sort makes no header sortable", () => {
    const rec = new Recorder(10, 3);
    const h = new Harness(rec.source);
    try {
      h.frame();
      h.click(header(1), HEADER / 2);
      h.frame();
      expect(h.snapshot.requestedSort).toBeNull();
      expect(h.snapshot.cols.headers.every((x) => x.sort.applied === null && !x.sort.pending)).toBe(true);
    } finally {
      h.destroy();
    }
  });

  test("sortable keeps a column's header from requesting a sort", () => {
    const asked: SortSpec<string>[] = [];
    const rec = new Recorder(10, 3, {
      sort: (spec) => (asked.push(spec), spec),
      sortable: (col) => col !== "c1",
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      h.click(header(1), HEADER / 2);
      h.click(header(2), HEADER / 2);
      expect(asked).toEqual([[{ id: "c2", direction: "ascending" }]]);
    } finally {
      h.destroy();
    }
  });

  test("focus follows its row through a reorder, located by the row's own ID", () => {
    let order = ["r0", "r1", "r2", "r3", "r4", "r5"];
    const located: string[] = [];
    let sinks!: Sinks;
    const source: TableSource<string, string, string> = {
      rows: {
        count: () => order.length,
        window: (s, e) => order.slice(s, e),
        locate: (id): Located => {
          located.push(id);
          const at = order.indexOf(id);
          return at < 0 ? { type: "Gone", suggest: null } : { type: "Found", position: at };
        },
      },
      cols: { count: () => 2, window: (s, e) => ["a", "b"].slice(s, e) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
      attach: (s) => ((sinks = s), () => {}),
    };
    const h = new Harness(source);
    try {
      h.frame();
      // Click the cell at row Position 1.
      h.click(BAND + 10, HEADER + ROW_HEIGHT + ROW_HEIGHT / 2);
      h.frame();
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe("r1");
      order = [...order].reverse();
      sinks.rows.notify({ type: "Reordered" });
      h.frame();
      h.drain();
      h.frame();
      expect(located).toEqual(["r1"]);
      expect(h.snapshot.rows.focus).toMatchObject({ position: 4 });
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe("r1");
    } finally {
      h.destroy();
    }
  });

  test("a column move hands the source the moved IDs, and a refusal leaves the order", () => {
    let cols = ["a", "b", "c"];
    const moves: [string[], number][] = [];
    let allow = false;
    const source: TableSource<number, string, string> = {
      rows: { identity: "position", count: () => 3, window: (s, e) => [0, 1, 2].slice(s, e) },
      cols: {
        count: () => cols.length,
        window: (s, e) => cols.slice(s, e),
        move: (ids, to) => {
          moves.push([ids, to]);
          if (allow) cols = [...ids, ...cols.filter((c) => !ids.includes(c))];
          return allow;
        },
      },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
    };
    const h = new Harness(source);
    const drag = () => {
      h.grid.pointer(0, BAND + 2 * COL_WIDTH + 40, HEADER / 2, 0, 0, 1);
      h.grid.pointer(1, BAND + 20, HEADER / 2, 0, 0, 1);
      h.grid.pointer(2, BAND + 5, HEADER / 2, 0, 0, 1);
      h.drain();
      h.frame();
    };
    const order = () => h.snapshot.cols.strip.map((c) => h.feed.colId(c.key!));
    try {
      h.frame();
      drag();
      expect(order()).toEqual(["a", "b", "c"]);
      allow = true;
      drag();
      expect(moves).toEqual([
        [["c"], 0],
        [["c"], 0],
      ]);
      expect(order()).toEqual(["c", "a", "b"]);
      // The values follow their columns.
      expect(h.values()[0]).toEqual(["0c", "0a", "0b"]);
    } finally {
      h.destroy();
    }
  });
});

describe("late answers", () => {
  test("a Promise answer lands on a later frame, and one made stale by a Change Notice is dropped", async () => {
    const pending: { resolve: () => void; label: string }[] = [];
    let version = 0;
    let sinks!: Sinks;
    const later = <T,>(label: string, value: () => T) =>
      new Promise<T>((resolve) => pending.push({ label, resolve: () => resolve(value()) }));
    const source: TableSource<string, string, string> = {
      rows: { count: () => 2, window: (s, e) => ["x", "y"].slice(s, e) },
      cols: { count: () => 1, window: () => ["k"] },
      cells: (rs, cs) => {
        const v = version;
        return later("cells", () => rs.map((r) => cs.map((c) => `${r}${c}${v}`)));
      },
      attach: (s) => ((sinks = s), () => {}),
    };
    const h = new Harness(source);
    try {
      h.frame();
      expect(h.values()).toEqual([[undefined], [undefined]]);
      // A change before the first answer lands makes it stale.
      version = 1;
      sinks.rows.notify({ type: "ContentChanged" satisfies ChangeNotice["type"], start: 0, end: 2 });
      // The frame asks again before the stale answer lands.
      h.frame();
      expect(pending).toHaveLength(2);
      pending.shift()!.resolve();
      await Promise.resolve();
      h.frame();
      expect(h.values()).toEqual([[undefined], [undefined]]);
      for (const p of pending.splice(0)) p.resolve();
      await Promise.resolve();
      h.frame();
      expect(h.values()).toEqual([["xk1"], ["yk1"]]);
    } finally {
      h.destroy();
    }
  });
});

/** A request a slow source holds until the test answers it. */
interface Held {
  kind: "count" | "window" | "cells" | "sort" | "locate";
  /** What was asked: the Window's range, the cells' row IDs, the Sort Spec, the located ID. */
  what: unknown;
  signal: AbortSignal;
  answer(): void;
}

/**
 * A keyed table whose Windows, cells, sorts and Locates all answer with
 * Promises the test settles. Rows are `r<i>` in `order`; cells read `<row>/<col>`.
 */
class Slow {
  readonly held: Held[] = [];
  order: string[];
  sinks!: Sinks;
  readonly source: TableSource<string, string, string>;

  constructor(rows: number, cols: number) {
    this.order = Array.from({ length: rows }, (_, i) => `r${i}`);
    const colIds = Array.from({ length: cols }, (_, i) => `c${i}`);
    const hold = <T,>(kind: Held["kind"], what: unknown, signal: AbortSignal, value: () => T) =>
      new Promise<T>((resolve) => this.held.push({ kind, what, signal, answer: () => resolve(value()) }));
    this.source = {
      rows: {
        count: () => this.order.length,
        window: (s, e, signal) => hold("window", [s, e], signal, () => this.order.slice(s, e)),
        sort: (spec, signal) =>
          hold("sort", spec, signal, () => {
            const by = spec[0];
            this.order = [...this.order].sort((a, b) => Number(a.slice(1)) - Number(b.slice(1)));
            if (by?.direction === "descending") this.order.reverse();
            this.sinks.rows.notify({ type: "Reordered" });
            return spec;
          }),
        locate: (id, signal) =>
          hold("locate", id, signal, (): Located => ({ type: "Found", position: this.order.indexOf(id) })),
      },
      cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
      cells: (rs, cs, signal) => hold("cells", rs, signal, () => rs.map((r) => cs.map((c) => `${r}/${c}`))),
      attach: (sinks) => ((this.sinks = sinks), () => {}),
    };
  }

  /** The requests of `kind` still in flight, oldest first. */
  open(kind: Held["kind"]): Held[] {
    return this.held.filter((h) => h.kind === kind && !h.signal.aborted);
  }

  /** Answers every request of `kind` in flight and lets the answers land. */
  async answer(kind: Held["kind"]): Promise<void> {
    const due = this.open(kind);
    this.held.splice(0, this.held.length, ...this.held.filter((h) => !due.includes(h)));
    for (const h of due) h.answer();
    await new Promise((r) => setTimeout(r, 0));
  }
}

describe("slow sources", () => {
  const header = (col: number) => BAND + col * COL_WIDTH + COL_WIDTH / 2;

  test("a Change Notice aborts the cells in flight, and asks again with a fresh signal", async () => {
    const slow = new Slow(5, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      const [first] = slow.open("cells");
      expect(first).toBeDefined();
      slow.sinks.rows.notify({ type: "ContentChanged", start: 0, end: 5 });
      expect(first!.signal.aborted).toBe(true);
      // ContentChanged asks for the rows' Window again too.
      h.frame();
      await slow.answer("window");
      h.frame();
      const again = slow.open("cells");
      expect(again.map((c) => c.what)).toEqual([["r0", "r1", "r2", "r3", "r4"]]);
      // The source ignores the abort and answers anyway: nothing lands from it.
      first!.answer();
      await new Promise((r) => setTimeout(r, 0));
      h.frame();
      expect(h.values().flat().every((v) => v === undefined)).toBe(true);
      await slow.answer("cells");
      h.frame();
      expect(h.values()[0]).toEqual(["r0/c0", "r0/c1"]);
    } finally {
      h.destroy();
    }
  });

  test("cells scrolled out of view before they land are no longer wanted", async () => {
    const slow = new Slow(200, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      const [top] = slow.open("cells");
      h.grid.scroll(0, 150 * ROW_HEIGHT);
      h.frame();
      await slow.answer("window");
      h.frame();
      expect(top!.signal.aborted).toBe(true);
      // Back at the top, the cells are asked for again.
      h.grid.scroll(0, -150 * ROW_HEIGHT);
      h.frame();
      await slow.answer("window");
      h.frame();
      expect(slow.open("cells").some((c) => (c.what as string[]).includes("r0"))).toBe(true);
    } finally {
      h.destroy();
    }
  });

  test("a Window superseded by another before it lands is aborted", async () => {
    const slow = new Slow(500, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      const [first] = slow.open("window");
      h.grid.scroll(0, 300 * ROW_HEIGHT);
      h.frame();
      expect(first!.signal.aborted).toBe(true);
      expect(slow.open("window")).toHaveLength(1);
    } finally {
      h.destroy();
    }
  });

  test("a re-sort while the first sort is in flight aborts it, and the headers end on the latest", async () => {
    const slow = new Slow(5, 3);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      h.click(header(0), HEADER / 2);
      h.frame();
      expect(h.snapshot.cols.headers[0]!.sort.pending).toBe(true);
      // Clicking again asks for descending; the ascending request is no longer wanted.
      h.click(header(0), HEADER / 2);
      const [ascending, descending] = slow.held.filter((x) => x.kind === "sort");
      expect(ascending!.signal.aborted).toBe(true);
      expect(descending!.signal.aborted).toBe(false);
      h.frame();
      expect(h.snapshot.cols.headers[0]!.sort.pending).toBe(true);
      await slow.answer("sort");
      h.drain();
      h.frame();
      const sort = h.snapshot.cols.headers[0]!.sort;
      expect(sort).toEqual({ applied: { direction: "descending", index: 0 }, pending: false });
    } finally {
      h.destroy();
    }
  });

  test("a Locate superseded by a newer one for the same row is aborted, and focus lands where the newer says", async () => {
    const slow = new Slow(6, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      h.click(BAND + 10, HEADER + ROW_HEIGHT + ROW_HEIGHT / 2);
      h.frame();
      expect(h.snapshot.rows.focus).toMatchObject({ position: 1 });
      slow.sinks.rows.notify({ type: "Reordered" });
      h.frame();
      expect(h.snapshot.rows.locating).toBe(true);
      const [first] = slow.open("locate");
      slow.order.reverse();
      slow.sinks.rows.notify({ type: "Reordered" });
      h.frame();
      expect(first!.signal.aborted).toBe(true);
      expect(h.snapshot.rows.locating).toBe(true);
      await slow.answer("locate");
      await slow.answer("window");
      h.frame();
      expect(h.snapshot.rows.locating).toBe(false);
      expect(h.snapshot.rows.focus).toMatchObject({ position: 4 });
    } finally {
      h.destroy();
    }
  });

  test("destroying the widget aborts every request in flight", async () => {
    const slow = new Slow(5, 2);
    const h = new Harness(slow.source);
    h.frame();
    await slow.answer("window");
    h.frame();
    h.click(header(0), HEADER / 2);
    const inFlight = slow.held.filter((x) => !x.signal.aborted);
    expect(inFlight.map((x) => x.kind).sort()).toEqual(["cells", "sort"]);
    h.destroy();
    expect(inFlight.every((x) => x.signal.aborted)).toBe(true);
  });
});

describe("edited values", () => {
  test("a committed value shows until the source's fresh answer for the cell lands", async () => {
    const pending: (() => void)[] = [];
    const data: Record<string, string> = { x: "old x", y: "old y" };
    let late = false;
    let sinks!: Sinks;
    const source: TableSource<string, string, string> = {
      rows: { count: () => 2, window: (s, e) => ["x", "y"].slice(s, e) },
      cols: { count: () => 1, window: () => ["k"] },
      cells: (rs) => {
        const answer = () => rs.map((r) => [data[r]!]);
        return late ? new Promise((resolve) => pending.push(() => resolve(answer()))) : answer();
      },
      attach: (s) => ((sinks = s), () => {}),
    };
    const h = new Harness(source);
    try {
      h.frame();
      const [x] = h.snapshot.rows.strip;
      const [k] = h.snapshot.cols.strip;
      h.feed.edited(x!.key!, k!.key!, "typed");
      h.frame();
      expect(h.values()).toEqual([["typed"], ["old y"]]);
      expect(h.feed.value(x!.key!, k!.key!)).toBe("typed");
      // The app saves slowly: while the fresh answer is on its way the edit still shows.
      late = true;
      data.x = "saved";
      sinks.rows.notify({ type: "ContentChanged", start: 0, end: 1 });
      h.frame();
      expect(h.values()).toEqual([["typed"], ["old y"]]);
      for (const p of pending.splice(0)) p();
      await Promise.resolve();
      h.frame();
      expect(h.values()).toEqual([["saved"], ["old y"]]);
    } finally {
      h.destroy();
    }
  });
});
