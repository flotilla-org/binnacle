// Find in fromRows: marks, steps and the count, checked against a naive
// model that lists every cell in the view's order (by rows or by columns)
// and matches each value's text one by one, under random sorts, filters and
// column moves.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { fromRows, type RowsSource } from "../src/rows";
import type { FindMatch, FindQuery, FindStep, SortDirection, TextQuery } from "../src/source";
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

const records = gs.arrays(
  gs.record({
    n: gs.oneOf(gs.just(null), gs.integers({ minValue: -12, maxValue: 25 })),
    s: gs.oneOf(gs.just(null), gs.arrays(gs.sampledFrom(letters), { maxSize: 4 }).map((cs) => cs.join(""))),
    b: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 2 })),
  }),
  { maxSize: 12 },
);

/** Plain text or a pattern (sometimes invalid), searching every column or some. */
const queries = gs.composite((tc): TextQuery<Col> => {
  const regex = tc.draw(gs.booleans());
  const pieces = regex ? ["a", "É", "1", ".", "^", "$", "|", "(", "[a2]"] : letters;
  const text = tc.draw(gs.arrays(gs.sampledFrom(pieces), { minSize: 1, maxSize: 3 })).join("");
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
    gs.record({ row: gs.integers({ minValue: 0, maxValue: rows + 1 }), col: gs.sampledFrom(IDS) }),
  );

describe("find in fromRows", () => {
  test("under any view, the count, the marks and a step from any cell agree with the model", () =>
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
