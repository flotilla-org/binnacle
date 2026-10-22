// The Table Source as the widget drives it: the user's own IDs in and out,
// the value cache asking only for what it lacks, Change Notices from the
// sink, and the answers to sort, Locate and move.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import type { AxisName } from "../src/codes";
import type { AxisSink, ChangeNotice, Distinct, Located, SortSpec, TableSource } from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

type Sinks = { rows: AxisSink<string>; cols: AxisSink<string> };

/** A keyed table whose cells read `<row>/<col>@<version>`, recording every cell it is asked for. */
type Source = TableSource<string, string, string>;

class Recorder {
  readonly asked: string[] = [];
  readonly versions = new Map<string, number>();
  sinks!: Sinks;
  readonly source: TableSource<string, string, string>;

  constructor(
    rows: number,
    cols: number,
    { view, support, distinct }: Pick<Source, "view"> & Pick<Source["rows"], "support" | "distinct"> = {},
    across: Pick<Source["cols"], "support" | "distinct"> = {},
  ) {
    const ids = (prefix: string, n: number) => Array.from({ length: n }, (_, i) => `${prefix}${i}`);
    const rowIds = ids("r", rows);
    const colIds = ids("c", cols);
    this.source = {
      ...(view && { view }),
      rows: {
        count: () => rowIds.length,
        window: (s, e) => rowIds.slice(s, e),
        ...(support && { support }),
        ...(distinct && { distinct }),
      },
      cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e), ...across },
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

  test("a Reset forgets every value; a Reordered or Refiltered keeps them", () => {
    const rec = new Recorder(10, 3);
    const h = new Harness(rec.source);
    try {
      h.frame();
      rec.take();
      rec.sinks.rows.notify({ type: "Reordered" });
      h.frame();
      expect(rec.take()).toEqual([]);
      rec.sinks.rows.notify({ type: "Refiltered" });
      h.frame();
      h.drain();
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
      view: (spec) => {
        asked.push(spec.rows.sort);
        // Applies only the first column asked for.
        return { ...spec, rows: { sort: spec.rows.sort.slice(0, 1) } };
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

  test("a source may answer with a columns sort of its own, shown on the row headers by the row's Key", () => {
    const rec = new Recorder(10, 3, {
      view: (spec) => ({ ...spec, cols: { sort: [{ id: "r1", direction: "descending" }] } }),
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      h.click(header(0), HEADER / 2);
      h.frame();
      const r1 = h.snapshot.rows.strip[1]!.key!;
      expect(h.snapshot.appliedView.cols.sort).toEqual([{ key: r1, direction: "descending" }]);
      const applied = h.snapshot.rows.headers.map((x) => x.sort.applied);
      expect(applied.slice(0, 3)).toEqual([null, { direction: "descending", index: 0 }, null]);
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
      expect(h.snapshot.requestedView).toBeNull();
      expect(h.snapshot.cols.headers.every((x) => x.sort.applied === null && !x.sort.pending)).toBe(true);
    } finally {
      h.destroy();
    }
  });

  test("support keeps a column's header from requesting a sort", () => {
    const asked: SortSpec<string>[] = [];
    const rec = new Recorder(10, 3, {
      view: (spec) => (asked.push(spec.rows.sort), spec),
      support: (col) => ({ sort: col !== "c1" }),
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
      expect(h.snapshot.moveRefusal).toEqual({ axis: "cols", reason: "These can't be moved" });
      allow = true;
      drag();
      expect(h.snapshot.moveRefusal).toBeNull();
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

describe("moves", () => {
  /** Keyed rows `r0..` over one column; `move` as given, recording each ask. */
  function rowsSource(move?: (ids: string[], to: number) => ReturnType<NonNullable<Source["rows"]["move"]>>) {
    let rows = ["r0", "r1", "r2", "r3"];
    const asked: [string[], number][] = [];
    let sinks: Sinks | undefined;
    let movable = true;
    const source: Source = {
      rows: {
        count: () => rows.length,
        window: (s, e) => rows.slice(s, e),
        ...(move && {
          move: (ids: string[], to: number) => {
            asked.push([ids, to]);
            return move(ids, to);
          },
          movable: () => movable,
        }),
      },
      cols: { count: () => 1, window: (s, e) => ["c"].slice(s, e) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
      attach: (s) => {
        sinks = s;
        return () => {};
      },
    };
    const place = (ids: string[], to: number) => {
      const before = rows.slice(0, to).filter((r) => !ids.includes(r));
      rows = [...before, ...ids, ...rows.filter((r) => !ids.includes(r) && !before.includes(r))];
    };
    return {
      source,
      asked,
      place,
      order: () => rows,
      withdraw: (now: boolean) => {
        movable = now;
        sinks!.rows.movableChanged();
      },
    };
  }

  /** Drags row `from`'s header to below row `to`, Effects answered. */
  function dragRow(h: Harness<string, string, string>, from: number, to: number) {
    const y = (r: number) => HEADER + r * ROW_HEIGHT + ROW_HEIGHT / 2;
    h.grid.pointer(0, BAND / 2, y(from), 0, 0, 1);
    h.grid.pointer(1, BAND / 2, y(to) + ROW_HEIGHT / 4, 0, 0, 1);
    h.grid.pointer(2, BAND / 2, y(to) + ROW_HEIGHT / 4, 0, 0, 1);
    h.drain();
    h.frame();
  }

  const shown = (h: Harness<string, string, string>) => h.snapshot.rows.strip.map((r) => h.feed.rowId(r.key!));

  test("an axis without move offers no move, and dragging its headers asks nothing", () => {
    const t = rowsSource();
    const h = new Harness(t.source);
    try {
      h.frame();
      expect(h.snapshot.rows.movable).toBe(false);
      dragRow(h, 0, 2);
      expect(h.snapshot.pendingMove).toBeNull();
      expect(shown(h)).toEqual(["r0", "r1", "r2", "r3"]);
    } finally {
      h.destroy();
    }
  });

  test("a row moves by its header; while the answer is due it shows pending and no other move starts", async () => {
    let answer: (moved: boolean) => void = () => {};
    const t = rowsSource(
      (ids, to) =>
        new Promise<boolean>((resolve) => {
          answer = (moved) => {
            if (moved) t.place(ids, to);
            resolve(moved);
          };
        }),
    );
    const h = new Harness(t.source);
    try {
      h.frame();
      expect(h.snapshot.rows.movable).toBe(true);
      dragRow(h, 0, 2);
      expect(t.asked).toEqual([[["r0"], 3]]);
      const key = h.feed.rowKey("r0");
      expect(h.snapshot.pendingMove).toEqual({ axis: "rows", keys: [key] });
      expect(h.snapshot.rows.headers.find((p) => p.key === key)?.movePending).toBe(true);
      // Nothing moves early, and a second drag starts no move.
      expect(shown(h)).toEqual(["r0", "r1", "r2", "r3"]);
      dragRow(h, 3, 0);
      expect(t.asked).toHaveLength(1);
      answer(true);
      await Promise.resolve();
      await Promise.resolve();
      h.frame();
      expect(h.snapshot.pendingMove).toBeNull();
      expect(shown(h)).toEqual(["r1", "r2", "r0", "r3"]);
    } finally {
      h.destroy();
    }
  });

  test("a refusal shows its reason until the next press, and a failure refuses with its message", async () => {
    let next: () => MaybeMoved = () => ({ refused: "Not while sorted" });
    type MaybeMoved = boolean | { refused: string } | Promise<boolean>;
    const t = rowsSource(() => next());
    const h = new Harness(t.source);
    try {
      h.frame();
      dragRow(h, 0, 2);
      expect(h.snapshot.moveRefusal).toEqual({ axis: "rows", reason: "Not while sorted" });
      expect(h.snapshot.pendingMove).toBeNull();
      h.click(BAND + 10, HEADER + 5);
      h.frame();
      expect(h.snapshot.moveRefusal).toBeNull();
      next = () => Promise.reject(new Error("the source went away"));
      dragRow(h, 0, 2);
      await new Promise((resolve) => setTimeout(resolve));
      h.frame();
      expect(h.snapshot.moveRefusal).toEqual({ axis: "rows", reason: "the source went away" });
      expect(h.errors.map((e) => e.request)).toEqual([{ type: "move", axis: "rows" }]);
    } finally {
      h.destroy();
    }
  });

  test("withdrawing movable through the sink stops the move being dragged, and offering it again lets one start", () => {
    const t = rowsSource((ids, to) => (t.place(ids, to), true));
    const h = new Harness(t.source);
    try {
      h.frame();
      const y = (r: number) => HEADER + r * ROW_HEIGHT + ROW_HEIGHT / 2;
      h.grid.pointer(0, BAND / 2, y(0), 0, 0, 1);
      h.grid.pointer(1, BAND / 2, y(2), 0, 0, 1);
      h.frame();
      expect(h.snapshot.mode.type).toBe("MovingHeaders");
      t.withdraw(false);
      h.frame();
      expect(h.snapshot.mode.type).toBe("Idle");
      expect(h.snapshot.rows.movable).toBe(false);
      h.grid.pointer(2, BAND / 2, y(2), 0, 0, 1);
      h.drain();
      expect(t.asked).toEqual([]);
      t.withdraw(true);
      dragRow(h, 0, 2);
      expect(shown(h)).toEqual(["r1", "r2", "r0", "r3"]);
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
  kind: "count" | "window" | "cells" | "view" | "locate";
  /** What was asked: the Window's range, the cells' row IDs, the Sort Spec, the located ID. */
  what: unknown;
  signal: AbortSignal;
  answer(): void;
  fail(): void;
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
      new Promise<T>((resolve, reject) =>
        this.held.push({
          kind,
          what,
          signal,
          answer: () => resolve(value()),
          fail: () => reject(new Error(`${kind} failed`)),
        }),
      );
    this.source = {
      rows: {
        count: () => this.order.length,
        window: (s, e, signal) => hold("window", [s, e], signal, () => this.order.slice(s, e)),
        locate: (id, signal) =>
          hold("locate", id, signal, (): Located => ({ type: "Found", position: this.order.indexOf(id) })),
      },
      cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
      view: (spec, signal) =>
        hold("view", spec, signal, () => {
          const by = spec.rows.sort[0];
          this.order = [...this.order].sort((a, b) => Number(a.slice(1)) - Number(b.slice(1)));
          if (by?.direction === "descending") this.order.reverse();
          this.sinks.rows.notify({ type: "Reordered" });
          return spec;
        }),
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
    for (const h of this.take(kind)) h.answer();
    await new Promise((r) => setTimeout(r, 0));
  }

  /** Rejects every request of `kind` in flight and lets the failures land. */
  async fail(kind: Held["kind"]): Promise<void> {
    for (const h of this.take(kind)) h.fail();
    await new Promise((r) => setTimeout(r, 0));
  }

  private take(kind: Held["kind"]): Held[] {
    const due = this.open(kind);
    this.held.splice(0, this.held.length, ...this.held.filter((h) => !due.includes(h)));
    return due;
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
      const [ascending, descending] = slow.held.filter((x) => x.kind === "view");
      expect(ascending!.signal.aborted).toBe(true);
      expect(descending!.signal.aborted).toBe(false);
      h.frame();
      expect(h.snapshot.cols.headers[0]!.sort.pending).toBe(true);
      await slow.answer("view");
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
    expect(inFlight.map((x) => x.kind).sort()).toEqual(["cells", "view"]);
    h.destroy();
    expect(inFlight.every((x) => x.signal.aborted)).toBe(true);
  });
});

describe("failed requests", () => {
  test("a rejected Window shows its cells failed and is reported; it is asked again only after a Change Notice, whose answer clears it", async () => {
    const slow = new Slow(3, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.fail("window");
      h.frame();
      expect(h.failed()).toEqual([
        [true, true],
        [true, true],
        [true, true],
      ]);
      expect(h.feed.loading(h.snapshot)).toBe(false);
      expect(h.errors).toEqual([{ message: "window failed", request: { type: "window", axis: "rows", start: 0, end: 3 } }]);
      h.frame();
      expect(slow.open("window")).toEqual([]);
      slow.sinks.rows.notify({ type: "Reset" });
      h.frame();
      expect(slow.open("window")).toHaveLength(1);
      await slow.answer("window");
      h.frame();
      expect(h.failed().flat().some(Boolean)).toBe(false);
      await slow.answer("cells");
      h.frame();
      expect(h.values()[2]).toEqual(["r2/c0", "r2/c1"]);
    } finally {
      h.destroy();
    }
  });

  test("rejected cells show failed until a ContentChanged over them asks again and the answer lands", async () => {
    const slow = new Slow(3, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      await slow.fail("cells");
      h.frame();
      expect(h.failed().flat().every(Boolean)).toBe(true);
      expect(h.values().flat().every((v) => v === undefined)).toBe(true);
      expect(h.errors).toEqual([
        { message: "cells failed", request: { type: "cells", rowIds: ["r0", "r1", "r2"], colIds: ["c0", "c1"] } },
      ]);
      expect(slow.open("cells")).toEqual([]);
      slow.sinks.rows.notify({ type: "ContentChanged", start: 1, end: 2 });
      h.frame();
      await slow.answer("window");
      h.frame();
      expect(slow.open("cells").map((c) => c.what)).toEqual([["r1"]]);
      await slow.answer("cells");
      h.frame();
      expect(h.failed()).toEqual([
        [true, true],
        [false, false],
        [true, true],
      ]);
      expect(h.values()[1]).toEqual(["r1/c0", "r1/c1"]);
    } finally {
      h.destroy();
    }
  });

  test("a source that throws fails its request as a rejection would", async () => {
    const source: TableSource<string, string, string> = {
      rows: { count: () => 2, window: (s, e) => ["x", "y"].slice(s, e) },
      cols: { count: () => 1, window: () => ["k"] },
      cells: () => {
        throw new Error("no cells");
      },
    };
    const h = new Harness(source);
    try {
      h.frame();
      await new Promise((r) => setTimeout(r, 0));
      h.frame();
      expect(h.failed()).toEqual([[true], [true]]);
      expect(h.errors.map((e) => e.message)).toEqual(["no cells"]);
    } finally {
      h.destroy();
    }
  });

  test("a rejected sort is reported, and the headers keep the spec in force", async () => {
    const slow = new Slow(3, 2);
    const h = new Harness(slow.source);
    try {
      h.frame();
      await slow.answer("window");
      h.frame();
      h.click(BAND + COL_WIDTH / 2, HEADER / 2);
      await slow.fail("view");
      h.frame();
      expect(h.snapshot.cols.headers.every((c) => c.sort.applied === null && !c.sort.pending)).toBe(true);
      expect(h.errors).toEqual([{ message: "view failed", request: { type: "view" } }]);
    } finally {
      h.destroy();
    }
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

describe("the quick filter", () => {
  type Spec = Parameters<NonNullable<Source["view"]>>[0];
  /** A Recorder whose rows take a quick filter, answering each View Spec with `answer`. */
  const filtering = (answer: (spec: Spec) => ReturnType<NonNullable<Source["view"]>>, regex = false) => {
    const rec = new Recorder(10, 3, { view: answer });
    const source: Source = { ...rec.source, rows: { ...rec.source.rows, quickFilter: { regex } } };
    return { rec, h: new Harness(source) };
  };
  const query = (h: Harness<string, string, string>, text: string, searches: string[] | null) => ({
    text,
    caseSensitive: true,
    regex: false,
    searches: searches && searches.map((id) => h.feed.colKey(id)),
  });

  test("a quick filter reaches the source by column IDs, and its answer, totals and support reach the Snapshot", () => {
    const asked: Spec[] = [];
    const { h } = filtering((spec) => (asked.push(spec), { ...spec, totals: { rows: 99 } }), true);
    try {
      h.frame();
      expect(h.snapshot.rows.quickFilter).toEqual({ regex: true });
      expect(h.snapshot.cols.quickFilter).toBeNull();
      const named = query(h, "Bér", ["c2", "c0"]);
      h.grid.set_quick_filter(0, named);
      h.drain();
      h.frame();
      expect(asked.map((s) => s.rows.filter.quick)).toEqual([
        { text: "Bér", caseSensitive: true, regex: false, searches: ["c2", "c0"] },
      ]);
      expect(h.snapshot.appliedView.rows.filter.quick).toEqual(named);
      expect([h.snapshot.rows.total, h.snapshot.cols.total]).toEqual([99, null]);
      h.grid.set_quick_filter(0, query(h, "x", null));
      h.drain();
      expect(asked.at(-1)?.rows.filter.quick?.searches).toBe("shown");
    } finally {
      h.destroy();
    }
  });

  test("an answer that leaves out the filter applies none", () => {
    const { h } = filtering((spec) => ({ rows: { sort: spec.rows.sort }, cols: { sort: [] } }));
    try {
      h.frame();
      h.grid.set_quick_filter(0, query(h, "a", null));
      h.drain();
      h.frame();
      expect(h.snapshot.appliedView.rows.filter.quick).toBeNull();
      expect(h.snapshot.rows.total).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("a source without view takes no quick filter, whatever its rows say", () => {
    const rec = new Recorder(10, 3);
    const h = new Harness({ ...rec.source, rows: { ...rec.source.rows, quickFilter: { regex: true } } });
    try {
      expect(h.frame().rows.quickFilter).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("a rejected quick filter keeps the filter and totals in force and stays asked for, not applied; one naming an unknown column is not asked", async () => {
    let calls = 0;
    const { h } = filtering((spec) => (++calls > 1 ? Promise.reject(new Error("no")) : { ...spec, totals: { rows: 40 } }));
    try {
      h.frame();
      const first = query(h, "a", null);
      h.grid.set_quick_filter(0, first);
      h.drain();
      h.grid.set_quick_filter(0, query(h, "b", null));
      h.drain();
      await new Promise((resolve) => setTimeout(resolve, 0));
      h.frame();
      expect(h.snapshot.appliedView.rows.filter.quick).toEqual(first);
      expect(h.snapshot.rows.total).toBe(40);
      expect(h.snapshot.requestedView?.rows.filter.quick).toEqual(query(h, "b", null));
      expect(h.snapshot.viewPending).toBe(false);
      h.grid.set_quick_filter(0, { ...first, searches: [12345n] });
      h.drain();
      h.frame();
      expect(calls).toBe(2);
      expect(h.snapshot.appliedView.rows.filter.quick).toEqual(first);
      expect(h.snapshot.rows.total).toBe(40);
    } finally {
      h.destroy();
    }
  });
});

describe("column conditions", () => {
  type Spec = Parameters<NonNullable<Source["view"]>>[0];
  const textOn = (h: Harness<string, string, string>, col: string, text: string, not = false) => {
    const key = h.feed.colKey(col);
    const query = { text, caseSensitive: false, regex: false, searches: [key] };
    return { key, kind: { type: "Text" as const, query }, not };
  };

  test("a condition reaches the source by its column's ID, and the source's answer reaches the headers and the list, one it left out not applied", () => {
    const asked: Spec[] = [];
    // Applies only the conditions on c1.
    const rec = new Recorder(10, 3, {
      view: (spec) => {
        asked.push(spec);
        const conditions = spec.rows.filter.conditions.filter((c) => c.id === "c1");
        return { ...spec, rows: { ...spec.rows, filter: { ...spec.rows.filter, conditions } } };
      },
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      h.grid.add_condition(0, textOn(h, "c1", "Bér", true));
      h.grid.add_condition(0, textOn(h, "c2", "x"));
      h.drain();
      h.frame();
      expect(asked.at(-1)?.rows.filter.conditions).toEqual([
        { id: "c1", kind: { type: "Text", query: { text: "Bér", caseSensitive: false, regex: false } }, not: true },
        { id: "c2", kind: { type: "Text", query: { text: "x", caseSensitive: false, regex: false } }, not: false },
      ]);
      expect(h.snapshot.appliedView.rows.filter.conditions).toEqual([textOn(h, "c1", "Bér", true)]);
      expect(h.snapshot.cols.headers.map((x) => x.filter)).toEqual([
        { filtered: false, pending: false, notApplied: false },
        { filtered: true, pending: false, notApplied: false },
        { filtered: false, pending: false, notApplied: true },
      ]);
      expect(h.snapshot.rows.conditions).toEqual([
        { condition: textOn(h, "c1", "Bér", true), state: "applied" },
        { condition: textOn(h, "c2", "x"), state: "notApplied" },
      ]);
    } finally {
      h.destroy();
    }
  });

  test("support declares contains on every column by default, a source narrows it or adds comparisons, and without view there is none", () => {
    const view = (spec: Spec) => spec;
    const support = (col: string) =>
      col === "c0" ? { text: false as const } : col === "c1" ? { text: { regex: true } } : { compare: "dates" as const };
    const cases: [Recorder, unknown[]][] = [
      [
        new Recorder(4, 3, { view }),
        [
          { sort: true, text: { regex: false }, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
          { sort: true, text: { regex: false }, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
          { sort: true, text: { regex: false }, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
        ],
      ],
      [
        new Recorder(4, 3, { view, support }),
        [
          { sort: true, text: null, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
          { sort: true, text: { regex: true }, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
          { sort: true, text: { regex: false }, oneOf: false, compare: "dates", group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
        ],
      ],
      [new Recorder(4, 3, { support }), Array(3).fill({ sort: false, text: null, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } })],
    ];
    for (const [rec, want] of cases) {
      const h = new Harness(rec.source);
      try {
        h.frame();
        expect(h.frame().cols.headers.map((x) => x.support)).toEqual(want);
      } finally {
        h.destroy();
      }
    }
  });

  test("a condition naming a column the feed has no ID for is not asked, the spec in force stays and it shows not applied", () => {
    let calls = 0;
    const rec = new Recorder(4, 3, { view: (spec) => (calls++, spec) });
    const h = new Harness(rec.source);
    try {
      h.frame();
      const query = { text: "a", caseSensitive: false, regex: false, searches: [99999n] };
      h.grid.add_condition(0, { key: 99999n, kind: { type: "Text", query }, not: false });
      h.drain();
      h.frame();
      expect(calls).toBe(0);
      expect(h.snapshot.appliedView.rows.filter.conditions).toEqual([]);
      expect(h.snapshot.viewPending).toBe(false);
      expect(h.snapshot.rows.conditions.map((c) => c.state)).toEqual(["notApplied"]);
    } finally {
      h.destroy();
    }
  });
});

describe("distinct values", () => {
  type Spec = Parameters<NonNullable<Source["view"]>>[0];
  type Asked = Parameters<NonNullable<Source["rows"]["distinct"]>>;
  const answer: Distinct = { values: [{ text: "a", count: 2 }], blanks: true, partial: false, narrowed: true };

  test("OneOf is offered only where the source lists distinct values, and a column may opt out", () => {
    const view = (spec: Spec) => spec;
    const distinct = () => answer;
    const support = (col: string) => (col === "c0" ? { oneOf: false } : {});
    const cases: [Recorder, boolean[]][] = [
      [new Recorder(4, 3, { view, distinct }), [true, true, true]],
      [new Recorder(4, 3, { view, distinct, support }), [false, true, true]],
      [new Recorder(4, 3, { view }), [false, false, false]],
      [new Recorder(4, 3, { distinct }), [false, false, false]],
    ];
    for (const [rec, want] of cases) {
      const h = new Harness(rec.source);
      try {
        h.frame();
        expect(h.frame().cols.headers.map((x) => x.support.oneOf)).toEqual(want);
        expect(h.feed.distinct("c0", "", 10, () => {}) !== null).toBe(want[0]);
      } finally {
        h.destroy();
      }
    }
  });

  test("a checklist asks by the column's ID, narrowed by the applied filter less the column's own conditions", () => {
    const asked: Asked[] = [];
    const rec = new Recorder(10, 3, {
      view: (spec) => spec,
      distinct: (...args) => (asked.push(args), answer),
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      const quick = { text: "q", caseSensitive: true, regex: false, searches: null };
      h.grid.set_quick_filter(0, quick);
      const [c0, c1] = [h.feed.colKey("c0"), h.feed.colKey("c1")];
      h.grid.add_condition(0, { key: c0, kind: { type: "OneOf", values: ["Bér", null, ""] }, not: true });
      h.grid.add_condition(0, { key: c1, kind: { type: "OneOf", values: ["x"] }, not: false });
      h.drain();
      h.frame();
      const got: Distinct[] = [];
      h.feed.distinct("c1", "be", 5, (d) => got.push(d));
      expect(got).toEqual([answer]);
      expect(asked.map(([id, query]) => [id, query])).toEqual([
        [
          "c1",
          {
            search: "be",
            limit: 5,
            narrow: {
              quick: { text: "q", caseSensitive: true, regex: false, searches: "shown" },
              conditions: [{ id: "c0", kind: { type: "OneOf", values: ["Bér", null, ""] }, not: true }],
            },
          },
        ],
      ]);
    } finally {
      h.destroy();
    }
  });

  test("a OneOf reaches the source by its column's ID, blanks apart from empty text, and comes back in the Snapshot", () => {
    const asked: Spec[] = [];
    const rec = new Recorder(10, 3, { view: (spec) => (asked.push(spec), spec), distinct: () => answer });
    const h = new Harness(rec.source);
    try {
      h.frame();
      const key = h.feed.colKey("c2");
      const listed = { key, kind: { type: "OneOf" as const, values: [null, "", "Ø🚢"] }, not: false };
      h.grid.add_condition(0, listed);
      h.drain();
      h.frame();
      expect(asked.at(-1)?.rows.filter.conditions).toEqual([
        { id: "c2", kind: { type: "OneOf", values: [null, "", "Ø🚢"] }, not: false },
      ]);
      expect(h.snapshot.rows.conditions).toEqual([{ condition: listed, state: "applied" }]);
      expect(h.snapshot.cols.headers[2]?.filter).toEqual({ filtered: true, pending: false, notApplied: false });
    } finally {
      h.destroy();
    }
  });

  test("an aborted question's answer never lands, and a failed one is reported", async () => {
    const signals: AbortSignal[] = [];
    let fail = false;
    const rec = new Recorder(4, 3, {
      view: (spec) => spec,
      distinct: (_id, query, signal) => {
        signals.push(signal);
        return fail ? Promise.reject(new Error("no")) : Promise.resolve({ ...answer, values: [{ text: query.search }] });
      },
    });
    const h = new Harness(rec.source);
    try {
      h.frame();
      const got: Distinct[] = [];
      const first = h.feed.distinct("c0", "a", 10, (d) => got.push(d))!;
      first.abort();
      h.feed.distinct("c0", "ab", 10, (d) => got.push(d));
      fail = true;
      h.feed.distinct("c0", "x", 10, (d) => got.push(d));
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(signals.map((s) => s.aborted)).toEqual([true, false, false]);
      expect(got.map((d) => d.values[0]?.text)).toEqual(["ab"]);
      expect(h.errors).toEqual([{ message: "no", request: { type: "distinct", axis: "rows" } }]);
    } finally {
      h.destroy();
    }
  });

  test("the columns take no conditions by a row unless the source gives cols.support; given, the defaults fill in as for the rows", () => {
    const view = (spec: Spec) => spec;
    const distinct = () => answer;
    const none = { sort: false, text: null, oneOf: false, compare: null, group: false, native: false, pivot: false, subtotals: { before: false, after: false } };
    const cases: [Recorder, unknown][] = [
      [new Recorder(3, 2, { view, distinct }, { distinct }), none],
      [
        new Recorder(3, 2, { view }, { support: () => ({ sort: false, compare: "numbers" as const }), distinct }),
        { sort: false, text: { regex: false }, oneOf: true, compare: "numbers", group: false, native: false, pivot: false, subtotals: { before: false, after: false } },
      ],
      [new Recorder(3, 2, {}, { support: () => ({ text: { regex: true } }), distinct }), none],
    ];
    for (const [rec, want] of cases) {
      const h = new Harness(rec.source);
      try {
        h.frame();
        expect(h.frame().rows.headers.map((x) => x.support)).toEqual(Array(3).fill(want));
        expect(h.feed.rowSupport("r1")).toEqual({ ...(want as object), aggregates: [] });
        expect(h.feed.distinct("r1", "", 10, () => {}, "cols") !== null).toBe((want as { oneOf: boolean }).oneOf);
      } finally {
        h.destroy();
      }
    }
  });

  test("a row's checklist asks the columns by the row's ID, narrowed by their applied filter less the row's own conditions", () => {
    const asked: Parameters<NonNullable<Source["cols"]["distinct"]>>[] = [];
    const rec = new Recorder(4, 3, { view: (spec) => spec }, { support: () => ({}), distinct: (...args) => (asked.push(args), answer) });
    const h = new Harness(rec.source);
    try {
      h.frame();
      const [r0, r1] = [h.feed.rowKey("r0"), h.feed.rowKey("r1")];
      h.grid.set_quick_filter(1, { text: "q", caseSensitive: false, regex: false, searches: [r0] });
      h.grid.add_condition(1, { key: r0, kind: { type: "OneOf", values: ["a"] }, not: false });
      h.grid.add_condition(1, { key: r1, kind: { type: "OneOf", values: ["b"] }, not: true });
      h.drain();
      h.frame();
      expect(h.snapshot.rows.headers.map((x) => x.filter.filtered)).toEqual([true, true, false, false]);
      const got: Distinct[] = [];
      h.feed.distinct("r1", "x", 7, (d) => got.push(d), "cols");
      expect(got).toEqual([answer]);
      expect(asked.map(([id, query]) => [id, query])).toEqual([
        [
          "r1",
          {
            search: "x",
            limit: 7,
            narrow: {
              quick: { text: "q", caseSensitive: false, regex: false, searches: ["r0"] },
              conditions: [{ id: "r0", kind: { type: "OneOf", values: ["a"] }, not: false }],
            },
          },
        ],
      ]);
    } finally {
      h.destroy();
    }
  });
});
