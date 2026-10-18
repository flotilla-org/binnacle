// Find in fromRows: marks, steps and the count, checked against a naive
// model that lists every cell in the view's order (by rows or by columns)
// and matches each value's text one by one, under random sorts, filters and
// column moves.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows, type RowsSource } from "../src/rows";
import type { FindMatch, FindQuery, FindStep, SortDirection, TableSource, TextQuery } from "../src/source";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface Rec {
  n: number | null;
  s: string | null;
  b: number | null;
}

type Col = keyof Rec;
const IDS: Col[] = ["n", "s", "b"];
const COLUMNS = IDS.map((key) => ({ key }));
const signal = new AbortController().signal;

const letters = ["a", "A", "é", "É", "1", "2", " "];
/** Mostly the letters, so queries often match and case matters, and any character besides. */
const chars = gs.oneOf(gs.sampledFrom(letters), gs.characters());

const records = gs.arrays(
  gs.record({
    n: gs.oneOf(gs.just(null), gs.integers({ minValue: -12, maxValue: 25 })),
    s: gs.oneOf(gs.just(null), gs.arrays(chars, { maxSize: 4 }).map((cs) => cs.join(""))),
    b: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 2 })),
  }),
  { maxSize: 12 },
);

/** Plain text or a pattern (sometimes invalid), searching every column or some. */
const queries = gs.composite((tc): TextQuery<Col> => {
  const regex = tc.draw(gs.booleans());
  const pieces = regex ? gs.sampledFrom(["a", "É", "1", ".", "^", "$", "|", "(", "[a2]"]) : chars;
  const text = tc.draw(gs.arrays(pieces, { minSize: 1, maxSize: 3 })).join("");
  const searches = tc.draw(gs.booleans()) ? "shown" : tc.draw(gs.arrays(gs.sampledFrom(IDS), { maxSize: 3 }));
  return { text, caseSensitive: tc.draw(gs.booleans()), regex, searches };
});

const finds = gs.composite(
  (tc): FindQuery<Col> => ({ query: tc.draw(queries), by: tc.draw(gs.sampledFrom<"rows" | "cols">(["rows", "cols"])) }),
);

/** A source under a random view: maybe sorted, maybe quick filtered, maybe filtered by the host, columns maybe moved. */
const viewed = gs.composite((tc): { rows: Rec[]; source: RowsSource<Rec> } => {
  const rows = tc.draw(records);
  const source = fromRows(rows, COLUMNS);
  const sort = tc.draw(
    gs.arrays(gs.tuples(gs.sampledFrom(IDS), gs.sampledFrom<SortDirection>(["ascending", "descending"])), { maxSize: 2 }),
  );
  const quick = tc.draw(gs.oneOf(gs.just(null), queries));
  source.view!(
    {
      rows: { filter: { quick, conditions: [] }, sort: sort.map(([id, direction]) => ({ id, direction })) },
      cols: { filter: { quick: null, conditions: [] }, sort: [] },
    },
    signal,
  );
  if (tc.draw(gs.booleans())) source.filter((r) => r.b !== 1);
  for (const [ids, to] of tc.draw(gs.arrays(gs.tuples(gs.arrays(gs.sampledFrom(IDS), { minSize: 1, maxSize: 2 }), gs.integers({ minValue: 0, maxValue: 3 })), { maxSize: 2 }))) {
    source.cols.move!([...new Set(ids)], to);
  }
  return { rows, source };
});

function matches(query: TextQuery<Col>, text: string): boolean {
  if (!query.regex) {
    return query.caseSensitive ? text.includes(query.text) : text.toLowerCase().includes(query.text.toLowerCase());
  }
  try {
    return new RegExp(query.text, query.caseSensitive ? "u" : "iu").test(text);
  } catch {
    return false;
  }
}

function shown(source: RowsSource<Rec>): { rowIds: number[]; colIds: Col[] } {
  return {
    rowIds: source.rows.window(0, source.rows.count() as number, signal) as number[],
    colIds: source.cols.window(0, 3, signal) as Col[],
  };
}

/** Every match in the Find Query's order. */
function model(rows: Rec[], source: RowsSource<Rec>, find: FindQuery<Col>): FindMatch<number, Col>[] {
  const { rowIds, colIds } = shown(source);
  const searched = (c: Col) => find.query.searches === "shown" || find.query.searches.includes(c);
  const cells =
    find.by === "rows"
      ? rowIds.flatMap((row) => colIds.map((col) => ({ row, col })))
      : colIds.flatMap((col) => rowIds.map((row) => ({ row, col })));
  return cells.filter(({ row, col }) => {
    const v = rows[row]![col];
    return searched(col) && v !== null && matches(find.query, String(v));
  });
}

/** The model's step: the first match after `from` in the order (or the last before it), wrapping. */
function modelStep(
  source: RowsSource<Rec>,
  find: FindQuery<Col>,
  all: FindMatch<number, Col>[],
  from: FindMatch<number, Col> | null,
  direction: "next" | "previous",
): FindStep<number, Col> | null {
  const { rowIds, colIds } = shown(source);
  const place = (m: FindMatch<number, Col>): [number, number] | null => {
    const r = rowIds.indexOf(m.row);
    const c = colIds.indexOf(m.col);
    if (r < 0 || c < 0) return null;
    return find.by === "rows" ? [r, c] : [c, r];
  };
  const before = (a: [number, number], b: [number, number]) => a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]);
  const f = from && place(from);
  if (all.length === 0) return null;
  const order = all.map((m, i) => ({ m, i, p: place(m)! }));
  const found =
    direction === "next"
      ? order.find(({ p }) => !f || before(f, p))
      : [...order].reverse().find(({ p }) => !f || before(p, f));
  const { m, i } = found ?? (direction === "next" ? order[0]! : order.at(-1)!);
  return { ...m, index: i, wrapped: !found && f !== null };
}

const froms = (rows: number) =>
  gs.oneOf(
    gs.just(null),
    gs.record({
      row: gs.integers({ minValue: 0, maxValue: rows + 1 }),
      // A column the table never had steps as one the view leaves out.
      col: gs.sampledFrom<Col>([...IDS, "gone" as Col]),
    }),
  );

describe("find in fromRows", () => {
  test("under any view, the count and the marks agree with the model", () =>
    hegel.test((tc) => {
      const { rows, source } = tc.draw(viewed);
      try {
        const find = tc.draw(finds);
        const all = model(rows, source, find);
        expect(source.find.count!(find, signal)).toEqual({ total: all.length });
        const { rowIds, colIds } = shown(source);
        const marks = source.find.marks(rowIds, colIds, find, signal) as boolean[][];
        const marked = new Set(all.map((m) => `${m.row}:${m.col}`));
        expect(marks).toEqual(rowIds.map((r) => colIds.map((c) => marked.has(`${r}:${c}`))));
      } finally {
        source.destroy();
      }
    }));

  test("under any view, a step from any cell, either way, agrees with the model", () =>
    hegel.test((tc) => {
      const { rows, source } = tc.draw(viewed);
      try {
        const find = tc.draw(finds);
        const all = model(rows, source, find);
        const from = tc.draw(froms(rows.length));
        const direction = tc.draw(gs.sampledFrom<"next" | "previous">(["next", "previous"]));
        expect(source.find.next(from, direction, find, signal)).toEqual(modelStep(source, find, all, from, direction));
      } finally {
        source.destroy();
      }
    }));

  test("stepping next visits every match once, in order, then wraps; previous undoes next", () =>
    hegel.test((tc) => {
      const { rows, source } = tc.draw(viewed);
      try {
        const find = tc.draw(finds);
        const { total } = source.find.count!(find, signal) as { total: number };
        let at: FindMatch<number, Col> | null = null;
        const visited: FindMatch<number, Col>[] = [];
        for (let i = 0; i < total; i++) {
          const step = source.find.next(at, "next", find, signal) as FindStep<number, Col>;
          expect(step.wrapped).toBe(false);
          expect(step.index).toBe(i);
          at = { row: step.row, col: step.col };
          visited.push(at);
        }
        expect(visited).toEqual(model(rows, source, find));
        const last = source.find.next(at, "next", find, signal) as FindStep<number, Col> | null;
        expect(last && { row: last.row, col: last.col, wrapped: last.wrapped }).toEqual(
          visited[0] ? { ...visited[0], wrapped: at !== null } : null,
        );
        for (let i = 1; i < visited.length; i++) {
          const back = source.find.next(visited[i]!, "previous", find, signal) as FindStep<number, Col>;
          expect({ row: back.row, col: back.col }).toEqual(visited[i - 1]);
        }
      } finally {
        source.destroy();
      }
    }));

  test("a changed row is matched again", () => {
    const rows: Rec[] = [
      { n: 1, s: "Bergen", b: null },
      { n: 2, s: "Cádiz", b: 1 },
    ];
    const source = fromRows(rows, COLUMNS);
    const find: FindQuery<Col> = { query: { text: "bergen", caseSensitive: false, regex: false, searches: "shown" }, by: "rows" };
    expect(source.find.count!(find, signal)).toEqual({ total: 1 });
    rows[1]!.s = "Bergen too";
    source.changed(1);
    expect(source.find.count!(find, signal)).toEqual({ total: 2 });
    expect(source.find.regex).toBe(true);
    source.destroy();
  });
});

/** An answer the test settles by hand, at once: a thenable, so the feed treats it as late. */
interface Late {
  kind: "marks" | "next" | "count";
  resolve(): void;
  reject(): void;
}

/**
 * A keyed table of `rows` by 4 columns whose cells hold `texts`, finding
 * through late answers the test settles in any order. `query.text` is
 * matched as a plain, case-sensitive substring.
 */
function lateFinder(texts: string[][]) {
  const rowIds = texts.map((_, i) => `r${i}`);
  const colIds = ["c0", "c1", "c2", "c3"];
  const pending: Late[] = [];
  const hit = (row: string, col: string, q: FindQuery<string>) =>
    texts[Number(row.slice(1))]![Number(col.slice(1))]!.includes(q.query.text);
  const cells = (q: FindQuery<string>) =>
    q.by === "rows"
      ? rowIds.flatMap((row) => colIds.map((col) => ({ row, col })))
      : colIds.flatMap((col) => rowIds.map((row) => ({ row, col })));
  const late = <T,>(kind: Late["kind"], answer: () => T): Promise<T> => {
    let done: { ok(v: T): void; fail(e: unknown): void } | undefined;
    pending.push({ kind, resolve: () => done?.ok(answer()), reject: () => done?.fail(new Error(`${kind} failed`)) });
    const thenable = { then: (ok: (v: T) => void, fail: (e: unknown) => void) => void (done = { ok, fail }) };
    return thenable as unknown as Promise<T>;
  };
  const source: TableSource<string, string, string> = {
    rows: {
      count: () => rowIds.length,
      window: (s, e) => rowIds.slice(s, e),
      locate: (id) => ({ type: "Found", position: rowIds.indexOf(id) }),
    },
    cols: { count: () => colIds.length, window: (s, e) => colIds.slice(s, e) },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => texts[Number(r.slice(1))]![Number(c.slice(1))]!)),
    find: {
      marks: (rs, cs, q) => late("marks", () => rs.map((r) => cs.map((c) => hit(r, c, q)))),
      next: (from, direction, q) =>
        late("next", () => {
          const all = cells(q);
          const at = from ? all.findIndex((m) => m.row === from.row && m.col === from.col) : -1;
          const n = all.length;
          for (let k = 1; k <= n; k++) {
            const i = from && at >= 0 ? (at + (direction === "next" ? k : -k) + n) % n : direction === "next" ? k - 1 : n - k;
            if (hit(all[i]!.row, all[i]!.col, q)) return { ...all[i]! };
          }
          return null;
        }),
      count: (q) => late("count", () => ({ total: cells(q).filter((m) => hit(m.row, m.col, q)).length })),
    },
  };
  return { source, pending, hit, total: (q: FindQuery<string>) => cells(q).filter((m) => hit(m.row, m.col, q)).length };
}

const findText = (text: string, by: "rows" | "cols" = "rows"): FindQuery<string> => ({
  query: { text, caseSensitive: true, regex: false, searches: "shown" },
  by,
});

describe("find in the feed", () => {
  test("with late answers in any order, failures and new queries, the marks shown are always the query's, every step is answered, and all settles to the model", () =>
    hegel.test((tc) => {
      const texts = tc.draw(
        gs.arrays(gs.arrays(gs.sampledFrom(["", "a", "b", "ab", "ba", "c"]), { minSize: 4, maxSize: 4 }), {
          minSize: 1,
          maxSize: 40,
        }),
      );
      const { source, pending, hit, total } = lateFinder(texts);
      const h = new Harness(source);
      let query: FindQuery<string> | null = null;
      /** Every visible cell marked matches the query in force. */
      const marksAreTheQuerys = () => {
        const s = h.snapshot;
        s.rows.strip.forEach((r, i) =>
          s.cols.strip.forEach((c, j) => {
            if (!h.content?.marked(i, j)) return;
            expect(query).not.toBeNull();
            expect(hit(h.feed.rowId(r.key!)!, h.feed.colId(c.key!)!, query!)).toBe(true);
          }),
        );
      };
      try {
        h.frame();
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 25 }));
        for (let i = 0; i < steps; i++) {
          const op = tc.draw(gs.sampledFrom(["find", "next", "previous", "resolve", "reject", "scroll", "frame"]));
          if (op === "find") {
            const text = tc.draw(gs.sampledFrom(["", "a", "b", "ab", "x"]));
            query = text === "" ? null : findText(text, tc.draw(gs.sampledFrom<"rows" | "cols">(["rows", "cols"])));
            h.grid.set_find(query && h.feed.findKeys(query));
          } else if (op === "next") h.grid.find_next();
          else if (op === "previous") h.grid.find_previous();
          else if ((op === "resolve" || op === "reject") && pending.length) {
            const late = pending.splice(tc.draw(gs.integers({ minValue: 0, maxValue: pending.length - 1 })), 1)[0]!;
            if (op === "resolve") late.resolve();
            else late.reject();
          } else if (op === "scroll") h.grid.scroll_by(0, tc.draw(gs.integers({ minValue: -20, maxValue: 20 })));
          h.drain();
          h.frame();
          marksAreTheQuerys();
        }
        const settle = () => {
          for (let round = 0; round < 50 && pending.length; round++) {
            for (const late of pending.splice(0)) late.resolve();
            h.drain();
            h.frame();
          }
          expect(pending).toEqual([]);
        };
        settle();
        const s = h.snapshot;
        s.rows.strip.forEach((r, i) =>
          s.cols.strip.forEach((c, j) => {
            const wanted = query !== null && hit(h.feed.rowId(r.key!)!, h.feed.colId(c.key!)!, query);
            expect(h.content?.marked(i, j) ?? false).toBe(wanted);
          }),
        );
        if (!query) return;
        // A count that failed stays unknown: nothing asks again until the query or the view changes.
        const count = h.feed.findState().count;
        if (count) expect(count).toEqual({ total: total(query) });
        // Not stuck waiting for an answer: one more step lands on a match if there is one.
        h.grid.find_next();
        h.drain();
        settle();
        const found = h.snapshot.findMatch;
        if (total(query) === 0) expect(found).toBeNull();
        else expect(found && hit(h.feed.rowId(found.row)!, h.feed.colId(found.col)!, query)).toBe(true);
      } finally {
        h.destroy();
      }
    }));

  test("a failed or superseded step is answered with no match, and the failure reported", () => {
    const { source, pending } = lateFinder([
      ["a", "", "", ""],
      ["", "a", "", ""],
    ]);
    const h = new Harness(source);
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("a")));
      h.grid.find_next();
      h.drain();
      const next = () => pending.splice(pending.findIndex((p) => p.kind === "next"), 1)[0]!;
      next().reject();
      h.drain();
      h.frame();
      expect(h.snapshot.findMatch).toBeNull();
      expect(h.errors.map((e) => e.request)).toEqual([{ type: "find", request: "next" }]);
      // Not stuck: the next step is asked for and lands.
      h.grid.find_next();
      h.drain();
      next().resolve();
      h.drain();
      h.frame();
      expect(h.snapshot.findMatch).toEqual({ row: h.feed.rowKey("r0"), col: h.feed.colKey("c0") });
      // A step asked for under "a", answered after the query changed, is ignored.
      h.grid.find_next();
      h.drain();
      h.grid.set_find(h.feed.findKeys(findText("b")));
      h.drain();
      next().resolve();
      h.drain();
      h.frame();
      expect(h.snapshot.findMatch).toBeNull();
      expect(h.feed.findState().step).toBeNull();
    } finally {
      h.destroy();
    }
  });

  test("a new query clears the old marks at once, before its own arrive; closing find asks nothing", () => {
    const { source, pending } = lateFinder([
      ["a", "b", "", ""],
      ["b", "a", "", ""],
    ]);
    const h = new Harness(source);
    const marked = () => h.snapshot.rows.strip.map((_, i) => h.snapshot.cols.strip.map((_, j) => h.content!.marked(i, j)));
    try {
      h.frame();
      h.grid.set_find(h.feed.findKeys(findText("a")));
      h.drain();
      h.frame();
      for (const p of pending.splice(0)) p.resolve();
      h.frame();
      expect(marked()).toEqual([
        [true, false, false, false],
        [false, true, false, false],
      ]);
      expect(h.feed.findState().count).toEqual({ total: 2 });
      h.grid.set_find(h.feed.findKeys(findText("b")));
      h.drain();
      h.frame();
      expect(marked().flat().some(Boolean)).toBe(false);
      expect(h.feed.findState().count).toBeNull();
      h.grid.set_find(null);
      h.drain();
      h.frame();
      for (const p of pending.splice(0)) p.resolve();
      h.frame();
      expect(pending).toEqual([]);
      expect(marked().flat().some(Boolean)).toBe(false);
    } finally {
      h.destroy();
    }
  });
});
