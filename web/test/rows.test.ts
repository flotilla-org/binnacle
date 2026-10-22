// fromRows: the in-memory source over binnacle-memory in wasm, checked
// against a sort written here from the spec (stable, multi-column, nulls last
// both ways, NaN after every number, strings by code point), and a quick
// filter and column conditions matched here cell by cell, and distinct
// values counted here.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { textRenderer } from "../src/renderers/text";
import { fromRows } from "../src/rows";
import { rowsBy } from "../src/source";
import type {
  AppliedView,
  AxisMethods,
  ChangeNotice,
  Condition,
  Distinct,
  FilterSpec,
  OutlinedWindow,
  SortDirection,
  SortSpec,
  TableSink,
  TextMatch,
  TextQuery,
  ViewSpec,
} from "../src/source";
import { BAND, COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

/** Sinks that keep the rows' Change Notices in `notices`, the columns' in `cols`, and drop the rest. */
function sinksTo<RowId, ColId>(notices: ChangeNotice[], cols: ChangeNotice[] = []): TableSink<RowId, ColId> {
  const ignore = () => {};
  return {
    rows: { notify: (n) => void notices.push(n), supportChanged: ignore, movableChanged: ignore },
    cols: { notify: (n) => void cols.push(n), supportChanged: ignore, movableChanged: ignore },
    viewChanged: ignore,
    findCounted: ignore,
  };
}

beforeAll(loadForTests);

interface Rec {
  n: number | null;
  s: string | null;
  b: number | null;
}

const COLUMNS = [{ key: "n" }, { key: "s" }, { key: "b" }] as const;

const fields = {
  n: gs.oneOf(gs.just(null), gs.just(Number.NaN), gs.integers({ minValue: -5, maxValue: 5 }), gs.floats({ allowNan: false })),
  s: gs.oneOf(gs.just(null), gs.text({ maxSize: 3 }), gs.sampledFrom(["a", "B", "é", "😀", "ﬀ", ""])),
  b: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 2 })),
};

const records = gs.arrays(gs.record(fields), { maxSize: 40 });

const specs = gs.arrays(
  gs.tuples(gs.sampledFrom<"n" | "s" | "b">(["n", "s", "b"]), gs.sampledFrom<SortDirection>(["ascending", "descending"])),
  { maxSize: 3 },
);

function codePoints(a: string, b: string): number {
  const x = Array.from(a, (c) => c.codePointAt(0)!);
  const y = Array.from(b, (c) => c.codePointAt(0)!);
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i]! - y[i]!;
  return x.length - y.length;
}

/** -1, 0 or 1 with nulls (and then NaN, for numbers) last whichever the direction. */
function compare(a: number | string | null, b: number | string | null, direction: SortDirection): number {
  const rank = (v: number | string | null) => (v === null ? 2 : typeof v === "number" && Number.isNaN(v) ? 1 : 0);
  if (rank(a) !== rank(b)) return rank(a) - rank(b);
  if (rank(a) !== 0) return 0;
  const c = typeof a === "string" ? codePoints(a, b as string) : (a as number) - (b as number);
  const sign = Math.sign(c) || (Object.is(a, -0) && Object.is(b, 0) ? -1 : Object.is(a, 0) && Object.is(b, -0) ? 1 : 0);
  return direction === "ascending" ? sign : -sign;
}

/**
 * The columns in the order `spec` sorts them by rows' values: each row
 * compares its values as numbers if every one that is not null is a number,
 * else all as strings (`String` of each).
 */
function columnReference(rows: Rec[], spec: SortSpec<number>): (keyof Rec)[] {
  const ids = COLUMNS.map((c) => c.key);
  const keyed = spec.map(({ id, direction }) => {
    const values = ids.map((c) => rows[id]![c]);
    const numbers = values.every((v) => v === null || typeof v === "number");
    return { direction, values: numbers ? values : values.map((v) => (v === null ? null : String(v))) };
  });
  return ids
    .map((_, i) => i)
    .sort((i, j) => {
      for (const { direction, values } of keyed) {
        const c = compare(values[i]!, values[j]!, direction);
        if (c !== 0) return c;
      }
      return i - j;
    })
    .map((i) => ids[i]!);
}

function reference(rows: Rec[], spec: SortSpec<keyof Rec>, keep: (r: Rec) => boolean): number[] {
  return rows
    .map((_, i) => i)
    .sort((i, j) => {
      for (const { id, direction } of spec) {
        const c = compare(rows[i]![id], rows[j]![id], direction);
        if (c !== 0) return c;
      }
      return i - j;
    })
    .filter((i) => keep(rows[i]!));
}

/** A View Spec sorting the rows by `sort` and filtering them by `quick` and `conditions`. */
function rowsSorted<ColId = keyof Rec>(
  sort: SortSpec<ColId>,
  quick: TextQuery<ColId> | null = null,
  conditions: Condition<ColId>[] = [],
): ViewSpec<number, ColId> {
  return {
    rows: { filter: { quick, conditions }, sort, hidden: [] },
    cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
  };
}

/** What fromRows answers for `rowsSorted(sort, quick, conditions)` over `n` rows of three columns. */
function answered<ColId = keyof Rec>(
  n: number,
  sort: SortSpec<ColId>,
  quick: TextQuery<ColId> | null = null,
  conditions: Condition<ColId>[] = [],
): AppliedView<number, ColId> {
  return { ...rowsSorted(sort, quick, conditions), totals: { rows: n, cols: 3 }, grouping: null };
}

/** Whether `text` matches `query` as the spec reads it; `undefined` for a pattern RegExp rejects. */
function matches(query: TextMatch, text: string): boolean | undefined {
  if (!query.regex) {
    return query.caseSensitive ? text.includes(query.text) : text.toLowerCase().includes(query.text.toLowerCase());
  }
  try {
    return new RegExp(query.text, query.caseSensitive ? "u" : "iu").test(text);
  } catch {
    return undefined;
  }
}

/** Whether the quick filter keeps `row`: some searched column's value, as text, matches. */
function quickKeeps(query: TextQuery<keyof Rec> | null, row: Rec): boolean {
  if (!query) return true;
  const searched = query.searches === "shown" ? (["n", "s", "b"] as const) : query.searches;
  return searched.some((id) => row[id] !== null && matches(query, String(row[id])) === true);
}

/**
 * Whether `condition` keeps `row`: its column's value, as text, matches (a
 * null never does) or is listed (a null as `null`), or with `not` the
 * opposite.
 */
function satisfies(condition: Condition<keyof Rec>, row: Rec): boolean {
  return holds(condition, row[condition.id]);
}

/** Whether `condition` keeps an item whose value in the item it names is `value`. */
function holds(condition: Condition<unknown>, value: Rec[keyof Rec]): boolean {
  const kind = condition.kind;
  if (kind.type === "Compare") throw new Error("only Text and OneOf conditions are drawn");
  const matched =
    kind.type === "OneOf"
      ? kind.values.includes(value === null ? null : String(value))
      : value !== null && matches(kind.query, String(value)) === true;
  return matched !== condition.not;
}

/** Whether fromRows can apply `condition`: a OneOf, or Text with a pattern RegExp takes. */
function applicable(condition: Condition<unknown>): boolean {
  const kind = condition.kind;
  return kind.type === "OneOf" || (kind.type === "Text" && matches(kind.query, "") !== undefined);
}

const letters = ["a", "A", "é", "É", "ﬀ", "i", "İ", "1", "-", " "];
/** Mostly the letters, so queries often match; sometimes any text. */
const pieceOf = (pieces: string[]) => gs.oneOf(gs.sampledFrom(pieces), gs.sampledFrom(pieces), gs.text({ maxSize: 2 }));

/** Plain text from a few letters in both cases, or a pattern of regex pieces (sometimes invalid). */
const queries = gs.composite((tc): TextQuery<keyof Rec> => {
  const regex = tc.draw(gs.booleans());
  const pieces = regex ? [...letters, ".", "^", "$", "|", "*", "(", ")", "[aé]", "\\d"] : letters;
  const text = tc.draw(gs.arrays(pieceOf(pieces), { minSize: 1, maxSize: 3 })).join("");
  const searches = tc.draw(gs.booleans())
    ? "shown"
    : tc.draw(gs.arrays(gs.sampledFrom<keyof Rec>(["n", "s", "b"]), { maxSize: 3 }));
  return { text, caseSensitive: tc.draw(gs.booleans()), regex, searches };
});

/** A record whose strings come from the query letters, so queries often match. */
const text = gs.record({
  n: gs.oneOf(gs.just(null), gs.just(Number.NaN), gs.integers({ minValue: -12, maxValue: 12 }), gs.floats()),
  s: gs.oneOf(gs.just(null), gs.arrays(pieceOf(letters), { maxSize: 4 }).map((cs) => cs.join(""))),
  b: gs.oneOf(gs.just(null), gs.integers({ minValue: 0, maxValue: 2 })),
});

const texts = gs.arrays(text, { maxSize: 20 });

/** A value a OneOf lists: often a null, empty text, a small number or a letter, so it often matches; sometimes any text. */
const listedOf = gs.oneOf(
  gs.just(null),
  gs.sampledFrom(["", "0", "1", "-1", "2", "NaN"]),
  gs.integers({ minValue: -12, maxValue: 12 }).map(String),
  pieceOf(letters),
);

/** A Text or OneOf condition on any column, maybe with `not`. */
const conditionOf = gs.composite((tc): Condition<keyof Rec> => {
  const id = tc.draw(gs.sampledFrom<keyof Rec>(["n", "s", "b"]));
  const not = tc.draw(gs.booleans());
  if (tc.draw(gs.booleans())) return { id, kind: { type: "OneOf", values: tc.draw(gs.arrays(listedOf, { maxSize: 4 })) }, not };
  const { text, caseSensitive, regex } = tc.draw(queries);
  return { id, kind: { type: "Text", query: { text, caseSensitive, regex } }, not };
});

const conditionsOf = gs.arrays(conditionOf, { maxSize: 3 });

const COLUMN_IDS = ["n", "s", "b"] as const;

/** The columns the columns' quick filter and conditions keep, in their order: as the rows' are kept, read along a row. */
function columnsKept(rows: Rec[], filter: FilterSpec<number>): (keyof Rec)[] {
  const { quick, conditions } = filter;
  const searched = quick?.searches === "shown" ? rows.map((_, i) => i) : (quick?.searches ?? []);
  return COLUMN_IDS.filter(
    (c) =>
      (!quick || searched.some((r) => rows[r]![c] !== null && matches(quick, String(rows[r]![c])) === true)) &&
      conditions.every((k) => holds(k, rows[k.id]![c])),
  );
}

/** A quick filter on the columns, searching every row or some of the `n`. */
const colsQueryOf = (n: number) =>
  gs.composite((tc): TextQuery<number> => {
    const { searches: _, ...query } = tc.draw(queries);
    const searches = tc.draw(gs.booleans())
      ? "shown"
      : tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: n - 1 }), { maxSize: 3 }));
    return { ...query, searches };
  });

/** Conditions on the columns, each read in one of the `n` rows or, sometimes, a row that is not there. */
const colsConditionsOf = (n: number) =>
  gs.arrays(
    gs.composite((tc): Condition<number> => ({ ...tc.draw(conditionOf), id: tc.draw(gs.integers({ minValue: 0, maxValue: n })) })),
    { maxSize: 3 },
  );

/** Whether fromRows can apply a condition on the columns of `n` rows: as `applicable`, read in a row that is there. */
const applicableIn = (n: number) => (condition: Condition<number>) => condition.id < n && applicable(condition);

/** Whether some pattern in `spec` is one RegExp rejects. */
function rejects(spec: FilterSpec<unknown>): boolean {
  const patterns = [spec.quick, ...spec.conditions.map((c) => (c.kind.type === "Text" ? c.kind.query : null))];
  return patterns.some((q) => q !== null && matches(q, "") === undefined);
}

/** Often a few rows, so that edits often change what the columns' filter reads. */
const fewTexts = gs.oneOf(gs.arrays(text, { minSize: 1, maxSize: 4 }), gs.arrays(text, { minSize: 1, maxSize: 20 }));

/** A View Spec filtering the rows by `rows` and the columns by `cols`. */
function filtered(rows: FilterSpec<keyof Rec>, cols: FilterSpec<number>): ViewSpec<number, keyof Rec> {
  return { rows: { filter: rows, sort: [], hidden: [] }, cols: { filter: cols, sort: [], hidden: [] } };
}

const NONE = { quick: null, conditions: [] };

/**
 * `shown` after moving `ids` to before Position `to`: before the first item
 * at or after `to` that is not moving, else at the end, in the order given.
 */
function placed<Id>(shown: Id[], ids: Id[], to: number): Id[] {
  const moving = [...new Set(ids)];
  const before = shown.slice(to).find((id) => !moving.includes(id));
  const rest = shown.filter((id) => !moving.includes(id));
  const at = before === undefined ? rest.length : rest.indexOf(before);
  return [...rest.slice(0, at), ...moving, ...rest.slice(at)];
}

function order(source: ReturnType<typeof fromRows<Rec>>): number[] {
  return source.rows.window(0, source.rows.count() as number, new AbortController().signal) as number[];
}

describe("fromRows", () => {
  test("sorting agrees with a reference sort, filtered or not", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      try {
        const spec: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const filtered = tc.draw(gs.booleans());
        const keep = (r: Rec) => !filtered || r.b !== 1;
        if (filtered && tc.draw(gs.booleans())) source.filter((r) => keep(r));
        expect(source.view!(rowsSorted(spec), new AbortController().signal)).toEqual(answered(rows.length, spec));
        if (filtered) source.filter((r) => keep(r));
        expect(order(source)).toEqual(reference(rows, spec, keep));
      } finally {
        source.destroy();
      }
    }));

  test("a View Spec tells an axis it leaves in the same order no Reordered, and one it changes something", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      const told = { rows: [] as ChangeNotice[], cols: [] as ChangeNotice[] };
      source.attach!(sinksTo(told.rows, told.cols));
      const shownCols = () => source.cols.window(0, source.cols.count() as number, new AbortController().signal);
      try {
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 5 }));
        for (let i = 0; i < steps; i++) {
          const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
          const hidden = tc.draw(gs.arrays(gs.sampledFrom<keyof Rec>(["n", "s", "b"]), { maxSize: 2 }));
          const byRow0 = rows.length > 0 && tc.draw(gs.booleans());
          const spec = rowsSorted(sort);
          spec.cols = {
            filter: { quick: null, conditions: [] },
            sort: byRow0 ? [{ id: 0, direction: "descending" }] : [],
            hidden,
          };
          const before = { rows: order(source), cols: shownCols() };
          told.rows.length = 0;
          told.cols.length = 0;
          source.view!(spec, new AbortController().signal);
          const after = { rows: order(source), cols: shownCols() };
          for (const axis of ["rows", "cols"] as const) {
            if (JSON.stringify(after[axis]) === JSON.stringify(before[axis])) {
              expect(told[axis], axis).not.toContainEqual({ type: "Reordered" });
            } else {
              expect(told[axis].length, axis).toBeGreaterThan(0);
            }
          }
        }
      } finally {
        source.destroy();
      }
    }));

  test("a move on either axis places the items in the order shown; on a sorted axis it drops the sort, or refuses with `refuse`", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const refuse = tc.draw(gs.booleans());
      const source = fromRows(rows, COLUMNS, { sortedMoves: refuse ? "refuse" : "bake" });
      const views: AppliedView<number, keyof Rec>[] = [];
      const ignore = () => {};
      const axisSink = { notify: ignore, supportChanged: ignore, movableChanged: ignore };
      source.attach!({ rows: axisSink, cols: axisSink, viewChanged: (v) => views.push(v), findCounted: ignore });
      try {
        const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        if (tc.draw(gs.booleans())) source.filter((r) => r.b !== 1);
        const byRows: SortSpec<number> =
          rows.length > 0 && tc.draw(gs.booleans())
            ? [{ id: tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 })), direction: "descending" }]
            : [];
        const spec = { ...rowsSorted(sort), cols: { filter: { quick: null, conditions: [] }, sort: byRows, hidden: [] } };
        source.view!(spec, new AbortController().signal);
        const axis = tc.draw(gs.sampledFrom<"rows" | "cols">(["rows", "cols"]));
        const sorted = (axis === "rows" ? sort : byRows).length > 0;
        const check = <Id,>(m: AxisMethods<Id, unknown>) => {
          const all = () => m.window(0, m.count() as number, new AbortController().signal) as Id[];
          const shown = all();
          tc.assume(shown.length > 0);
          const picks = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: shown.length - 1 }), { minSize: 1, maxSize: 3 }));
          const ids = picks.map((i) => shown[i]!);
          const to = tc.draw(gs.integers({ minValue: 0, maxValue: shown.length }));
          expect(m.movable!()).toBe(!(refuse && sorted));
          const answer = m.move!(ids, to);
          if (refuse && sorted) {
            expect(answer).toEqual({ refused: "Can't move while sorted: clear the sort first" });
            expect(all()).toEqual(shown);
            expect(views).toEqual([]);
            return;
          }
          expect(answer).toBe(true);
          expect(all()).toEqual(placed(shown, ids, to));
          expect(m.movable!()).toBe(true);
          const dropped = { ...spec, [axis]: { ...spec[axis], sort: [] } };
          expect(views).toEqual(sorted ? [{ ...dropped, totals: { rows: rows.length, cols: 3 }, grouping: null }] : []);
        };
        if (axis === "rows") check(source.rows);
        else check(source.cols as AxisMethods<keyof Rec, unknown>);
      } finally {
        source.destroy();
      }
    }));

  test("after rows change in place, a sort in force re-sorts; a change from the data is filtered at once, and the user's edit leaves its row unmatched", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      tc.assume(rows.length > 0);
      const source = fromRows(rows, COLUMNS);
      const notices: ChangeNotice[] = [];
      source.attach!(sinksTo(notices));
      try {
        const spec: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const filtered = tc.draw(gs.booleans());
        const keep = (r: Rec) => !filtered || r.b !== 1;
        if (filtered) source.filter((r) => keep(r));
        source.view!(rowsSorted(spec), new AbortController().signal);
        const unmatched = new Set<number>();
        const changes = tc.draw(gs.integers({ minValue: 1, maxValue: 6 }));
        for (let i = 0; i < changes; i++) {
          const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
          const edit = tc.draw(gs.booleans());
          const before = order(source).indexOf(id);
          Object.assign(rows[id]!, tc.draw(gs.record(fields)));
          notices.length = 0;
          if (edit) source.edited(id);
          else source.changed(id);
          if (edit && before >= 0 && !keep(rows[id]!)) unmatched.add(id);
          else unmatched.delete(id);
          const shown = order(source);
          expect(shown).toEqual(reference(rows, spec, (r) => keep(r) || unmatched.has(rows.indexOf(r))));
          expect(source.rows.unmatched!(shown, new AbortController().signal)).toEqual(shown.map((r) => unmatched.has(r)));
          const moved = (before >= 0) !== shown.includes(id);
          const told: ChangeNotice[] = before < 0 ? [] : [{ type: "ContentChanged", start: before, end: before + 1 }];
          if (moved) told.push({ type: "Refiltered" });
          else if (spec.length) told.push({ type: "Reordered" });
          expect(notices).toEqual(told);
        }
        // Any View Spec, even the one in force, filters the unmatched rows;
        // with none, the rows stay in order and are told nothing.
        notices.length = 0;
        source.view!(rowsSorted(spec), new AbortController().signal);
        expect(order(source)).toEqual(reference(rows, spec, keep));
        expect(notices).toEqual(unmatched.size ? [{ type: "Refiltered" }] : []);
      } finally {
        source.destroy();
      }
    }));

  test("whatever View Specs and host filters arrive, the rows shown are the model's; an invalid pattern leaves the quick filter in force, drops a condition and says why", () =>
    hegel.test((tc) => {
      const rows = tc.draw(texts);
      const source = fromRows(rows, COLUMNS);
      const notices: ChangeNotice[] = [];
      source.attach!(sinksTo(notices));
      let [sort, quick, min]: [SortSpec<keyof Rec>, TextQuery<keyof Rec> | null, number | null] = [[], null, null];
      let applied: Condition<keyof Rec>[] = [];
      try {
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 6 }));
        for (let i = 0; i < steps; i++) {
          notices.length = 0;
          if (tc.draw(gs.integers({ minValue: 0, maxValue: 3 })) > 0) {
            const asked = tc.draw(gs.oneOf(gs.just(null), queries));
            const conditions = tc.draw(gs.oneOf(gs.just(applied), conditionsOf));
            sort = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
            const before = order(source);
            const answer = source.view!(rowsSorted(sort, asked, conditions), new AbortController().signal);
            const changed = JSON.stringify(asked) !== JSON.stringify(quick);
            const valid = asked === null || matches(asked, "") !== undefined;
            if (valid) quick = asked;
            // A condition whose pattern RegExp rejects is left out.
            const kept = conditions.filter(applicable);
            const refiltered = (changed && valid) || JSON.stringify(kept) !== JSON.stringify(applied);
            applied = kept;
            const { error, ...rest } = answer as AppliedView<number, keyof Rec>;
            expect(rest).toEqual(answered(rows.length, sort, quick, applied));
            // What RegExp said of the first pattern it rejected.
            expect(error !== undefined).toBe(!valid || kept.length < conditions.length);
            if (error !== undefined) expect(error).toMatch(/^(?!Invalid regular expression)\S/);
            const moved = JSON.stringify(order(source)) !== JSON.stringify(before);
            expect(notices).toEqual(refiltered ? [{ type: "Refiltered" }] : moved ? [{ type: "Reordered" }] : []);
          } else {
            const m = tc.draw(gs.oneOf(gs.just(null), gs.integers({ minValue: -12, maxValue: 12 })));
            min = m;
            source.filter(m === null ? null : (r) => r.n !== null && r.n >= m);
            expect(notices).toEqual([{ type: "Refiltered" }]);
          }
          const host = (r: Rec) => min === null || (r.n !== null && r.n >= min);
          const kept = (r: Rec) => host(r) && quickKeeps(quick, r) && applied.every((c) => satisfies(c, r));
          expect(order(source)).toEqual(reference(rows, sort, kept));
        }
      } finally {
        source.destroy();
      }
    }));

  test("the quick filter matches a cell's text as its renderer gives it, and a missing value never", () => {
    const rows: Rec[] = [
      { n: 1234.5, s: "Bérénice", b: null },
      { n: 7, s: null, b: 1 },
      { n: null, s: "null", b: null },
    ];
    const grouped = textRenderer<number | null>({ format: new Intl.NumberFormat("en-US") });
    const source = fromRows(rows, [{ key: "n", renderer: grouped }, { key: "s" }, { key: "b" }]);
    const shown = (text: string, regex = false) => {
      const quick: TextQuery<keyof Rec> = { text, caseSensitive: false, regex, searches: "shown" };
      source.view!(rowsSorted([], quick), new AbortController().signal);
      return order(source);
    };
    expect(shown("1,234")).toEqual([0]);
    expect(shown("1234")).toEqual([]);
    expect(shown("BÉR")).toEqual([0]);
    expect(shown("null")).toEqual([2]);
    expect(shown("^$", true)).toEqual([]);
    expect(source.rows.quickFilter).toEqual({ regex: true });
    source.destroy();
  });

  test("a condition and its not split the rows between them", () =>
    hegel.test((tc) => {
      const rows = tc.draw(texts);
      const condition = tc.draw(conditionOf);
      tc.assume(applicable(condition));
      const shown = (not: boolean) => {
        const source = fromRows(rows, COLUMNS);
        source.view!(rowsSorted([], null, [{ ...condition, not }]), new AbortController().signal);
        const shown = order(source);
        source.destroy();
        return shown;
      };
      const both = [...shown(false), ...shown(true)].sort((a, b) => a - b);
      expect(both).toEqual(rows.map((_, i) => i));
    }));

  test("a column that takes no conditions says so, and a condition on it is left out", () => {
    const rows: Rec[] = [
      { n: 1, s: "Bergen", b: null },
      { n: 2, s: null, b: 1 },
      { n: 3, s: "Cádiz", b: 0 },
    ];
    const source = fromRows(rows, [{ key: "n" }, { key: "s" }, { key: "b", filterable: false }]);
    expect(source.rows.support?.("b")).toEqual({ sort: true, text: false, oneOf: false, compare: null, group: true });
    const text = (text: string): TextMatch => ({ text, caseSensitive: false, regex: false });
    const notBergen: Condition<keyof Rec> = { id: "s", kind: { type: "Text", query: text("bergen") }, not: true };
    const onB: Condition<keyof Rec> = { id: "b", kind: { type: "Text", query: text("1") }, not: false };
    const answer = source.view!(rowsSorted([], null, [notBergen, onB]), new AbortController().signal);
    expect(answer).toEqual(answered(3, [], null, [notBergen]));
    expect(order(source)).toEqual([1, 2]);
    source.destroy();
  });

  test("a column of numbers and strings sorts as strings; booleans and dates sort as numbers", () => {
    type Mixed = { m: number | string; f: boolean; d: Date };
    const mixed: Mixed[] = [
      { m: 10, f: true, d: new Date(3) },
      { m: "9", f: false, d: new Date(1) },
      { m: 2, f: true, d: new Date(2) },
    ];
    const source = fromRows(mixed, [{ key: "m" }, { key: "f" }, { key: "d" }]);
    const by = (id: keyof Mixed) => {
      source.view!(rowsSorted([{ id, direction: "ascending" }]), new AbortController().signal);
      return source.rows.window(0, 3, new AbortController().signal);
    };
    expect(by("m")).toEqual([0, 2, 1]);
    expect(by("f")).toEqual([1, 0, 2]);
    expect(by("d")).toEqual([1, 2, 0]);
    source.destroy();
  });

  test("sorting the columns by rows agrees with a reference sort, whatever the rows' sort", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      try {
        const byRows = rows.length
          ? tc.draw(
              gs.arrays(
                gs.tuples(
                  gs.integers({ minValue: 0, maxValue: rows.length - 1 }),
                  gs.sampledFrom<SortDirection>(["ascending", "descending"]),
                ),
                { maxSize: 3 },
              ),
            )
          : [];
        const spec: SortSpec<number> = byRows.map(([id, direction]) => ({ id, direction }));
        const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const asked = { ...rowsSorted(sort), cols: { filter: { quick: null, conditions: [] }, sort: spec, hidden: [] } };
        expect(source.view!(asked, new AbortController().signal)).toEqual({
          ...asked,
          totals: { rows: rows.length, cols: 3 },
          grouping: null,
        });
        expect(source.cols.window(0, 3, new AbortController().signal)).toEqual(columnReference(rows, spec));
        expect(order(source)).toEqual(reference(rows, sort, () => true));
      } finally {
        source.destroy();
      }
    }));

  test("a sort term on a row or column that is not there is left out", () => {
    const source = fromRows<Rec>([{ n: 2, s: null, b: 1 }], COLUMNS);
    const none = { quick: null, conditions: [] };
    const asked = {
      rows: { filter: none, sort: [{ id: "gone" as keyof Rec, direction: "ascending" as const }, { id: "n" as const, direction: "descending" as const }], hidden: [] },
      cols: { filter: none, sort: [{ id: 1, direction: "descending" as const }, { id: 0, direction: "ascending" as const }], hidden: [] },
    };
    expect(source.view!(asked, new AbortController().signal)).toEqual({
      rows: { ...asked.rows, sort: [{ id: "n", direction: "descending" }] },
      cols: { ...asked.cols, sort: [{ id: 0, direction: "ascending" }] },
      totals: { rows: 1, cols: 3 },
      grouping: null,
    });
    // Row 0 holds 2, a null and 1: ascending, nulls last.
    expect(source.cols.window(0, 3, new AbortController().signal)).toEqual(["b", "n", "s"]);
    source.destroy();
  });


  test("a change to the row the columns sort by sorts them again and tells the columns", () => {
    const rows: Rec[] = [{ n: 1, s: null, b: 2 }];
    const source = fromRows(rows, [{ key: "n" }, { key: "b" }]);
    const notices: ChangeNotice[] = [];
    const ignore = () => {};
    source.attach!({
      rows: { notify: ignore, supportChanged: ignore, movableChanged: ignore },
      cols: { notify: (n) => void notices.push(n), supportChanged: ignore, movableChanged: ignore },
      viewChanged: ignore,
      findCounted: ignore,
    });
    const byRow0 = { ...rowsSorted<keyof Rec>([]), cols: { filter: { quick: null, conditions: [] }, sort: [{ id: 0, direction: "ascending" as const }], hidden: [] } };
    // Already in that order: the columns are told nothing.
    source.view!(byRow0, new AbortController().signal);
    expect(source.cols.window(0, 2, new AbortController().signal)).toEqual(["n", "b"]);
    rows[0]!.n = 5;
    source.changed(0);
    expect(source.cols.window(0, 2, new AbortController().signal)).toEqual(["b", "n"]);
    expect(notices.map((n) => n.type)).toEqual(["Reordered"]);
    // Clearing the sort puts the columns back in their own order.
    source.view!(rowsSorted([]), new AbortController().signal);
    expect(source.cols.window(0, 2, new AbortController().signal)).toEqual(["n", "b"]);
    expect(notices.length).toBe(2);
    source.destroy();
  });

  test("cells read the rows' own properties by row and column ID", () => {
    const source = fromRows<Rec>([{ n: 1, s: "x", b: null }, { n: 2, s: "y", b: 0 }], COLUMNS);
    expect(source.cells([1, 0], ["s", "b"], new AbortController().signal)).toEqual([
      ["y", 0],
      ["x", null],
    ]);
    expect(source.cols.window(0, 3, new AbortController().signal)).toEqual(["n", "s", "b"]);
    source.destroy();
  });

  test("sorting and filtering tell the attached sinks", () => {
    const source = fromRows<Rec>([{ n: 2, s: null, b: 1 }, { n: 1, s: null, b: 0 }], COLUMNS);
    const notices: ChangeNotice[] = [];
    const detach = source.attach!(sinksTo(notices));
    source.view!(rowsSorted([{ id: "n", direction: "ascending" }]), new AbortController().signal);
    source.filter((r) => r.b === 0);
    source.filter(null);
    detach();
    source.filter((r) => r.b === 0);
    expect(notices.map((n) => n.type)).toEqual(["Reordered", "Refiltered", "Refiltered"]);
    source.destroy();
  });

  test("columns marked unsortable say so, and labels default to the property name", () => {
    const source = fromRows<Rec>([], [{ key: "n", label: "Number" }, { key: "s", sortable: false }]);
    expect(source.rows.support?.("n")).toMatchObject({ sort: true, text: { regex: true }, oneOf: true, group: true });
    expect(source.rows.support?.("s")).toMatchObject({ sort: false, text: { regex: true }, oneOf: true, group: false });
    expect(source.cols.label?.("n", 0)).toBe("Number");
    expect(source.cols.label?.("s", 1)).toBe("s");
    source.destroy();
  });

  test("in the Grid, a row-header click set to sort sorts the columns by that row and marks its header", () => {
    const rows: Rec[] = [
      { n: 3, s: null, b: 1 },
      { n: 1, s: null, b: 2 },
    ];
    const source = fromRows(rows, COLUMNS);
    const h = new Harness(source);
    try {
      h.grid.set_header_click(0, 0);
      h.frame();
      // Row 1 holds 1, null, 2.
      h.click(BAND / 2, HEADER + ROW_HEIGHT * 1.5);
      h.frame();
      h.drain();
      h.frame();
      expect(h.snapshot.cols.strip.map((i) => h.feed.colId(i.key!))).toEqual(["n", "b", "s"]);
      expect(h.snapshot.rows.headers.map((r) => r.sort)).toEqual([
        { applied: null, pending: false },
        { applied: { direction: "ascending", index: 0 }, pending: false },
      ]);
      expect(h.values()).toEqual([
        [3, 1, null],
        [1, 2, null],
      ]);
    } finally {
      h.destroy();
      source.destroy();
    }
  });

  test("in the Grid, a header click sorts, focus follows its row, and a filter that drops it moves focus", () => {
    const rows: Rec[] = [
      { n: 3, s: "c", b: 0 },
      { n: 1, s: "a", b: 1 },
      { n: 2, s: "b", b: 0 },
      { n: null, s: "d", b: 0 },
    ];
    const source = fromRows(rows, COLUMNS);
    const h = new Harness(source);
    try {
      h.frame();
      // Focus the row at Position 0 (n = 3), then sort by n.
      h.click(BAND + 10, HEADER + ROW_HEIGHT / 2);
      h.frame();
      h.click(BAND + COL_WIDTH / 2, HEADER / 2);
      h.frame();
      h.drain();
      h.frame();
      expect(h.values().map((r) => r[0])).toEqual([1, 2, 3, null]);
      expect(h.snapshot.rows.focus?.position).toBe(2);
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe(0);
      // Descending keeps nulls last.
      h.click(BAND + COL_WIDTH / 2, HEADER / 2);
      h.frame();
      h.drain();
      h.frame();
      expect(h.values().map((r) => r[0])).toEqual([3, 2, 1, null]);
      expect(h.snapshot.rows.focus?.position).toBe(0);
      source.filter((r) => r.n !== 3);
      h.frame();
      h.drain();
      h.frame();
      expect(h.snapshot.rows.count).toBe(3);
      expect(h.values().map((r) => r[0])).toEqual([2, 1, null]);
      const moved = h.feed.rowId(h.snapshot.rows.focus!.key!);
      // Clearing the filter keeps focus on the row it moved to.
      source.filter(null);
      h.frame();
      h.drain();
      h.frame();
      expect(h.snapshot.rows.count).toBe(4);
      expect(h.values().map((r) => r[0])).toEqual([3, 2, 1, null]);
      expect(h.feed.rowId(h.snapshot.rows.focus!.key!)).toBe(moved);
      expect(h.snapshot.rows.focus?.position).toBe(1);
    } finally {
      h.destroy();
      source.destroy();
    }
  });

  test("distinct values are the model's: counted, searched, narrowed, in the column's order, capped, with blanks apart from empty text", () =>
    hegel.test((tc) => {
      const rows = tc.draw(texts);
      const source = fromRows(rows, COLUMNS);
      try {
        const min = tc.draw(gs.oneOf(gs.just(null), gs.integers({ minValue: -12, maxValue: 12 })));
        if (min !== null) source.filter((r) => r.n !== null && r.n >= min);
        const quick = tc.draw(gs.oneOf(gs.just(null), queries));
        const narrow: FilterSpec<keyof Rec> = {
          quick: quick && matches(quick, "") !== undefined ? quick : null,
          conditions: tc.draw(conditionsOf).filter(applicable),
        };
        const id = tc.draw(gs.sampledFrom<keyof Rec>(["n", "s", "b"]));
        const search = tc.draw(gs.oneOf(gs.just(""), pieceOf(letters)));
        const limit = tc.draw(gs.integers({ minValue: 0, maxValue: 8 }));
        const answer = source.rows.distinct!(id, { search, limit, narrow }, new AbortController().signal);
        // The model: the rows kept, their texts in the column's ascending sort order (ties by text).
        const kept = rows.filter(
          (r) =>
            (min === null || (r.n !== null && r.n >= min)) &&
            quickKeeps(narrow.quick, r) &&
            narrow.conditions.every((c) => satisfies(c, r)),
        );
        const blanks = kept.filter((r) => r[id] === null).length;
        const found = kept
          .filter((r) => r[id] !== null && (search === "" || String(r[id]).toLowerCase().includes(search.toLowerCase())))
          .sort((a, b) => compare(a[id], b[id], "ascending") || codePoints(String(a[id]), String(b[id])));
        const values: { text: string; count: number }[] = [];
        for (const r of found) {
          const text = String(r[id]);
          const v = values.find((v) => v.text === text);
          if (v) v.count++;
          else values.push({ text, count: 1 });
        }
        const expected: Distinct = {
          values: values.slice(0, limit),
          ...(blanks > 0 && search === "" ? { blanks } : {}),
          partial: values.length > limit,
          narrowed: true,
        };
        expect(answer).toEqual(expected);
      } finally {
        source.destroy();
      }
    }, { testCases: 500 }));

  test("distinct values are a cell's text as its renderer gives it, numbers in numeric order", () => {
    const rows: Rec[] = [
      { n: 1234.5, s: "b", b: null },
      { n: 7, s: null, b: 1 },
      { n: 7, s: "", b: 1 },
      { n: null, s: "a", b: null },
    ];
    const grouped = textRenderer<number | null>({ format: new Intl.NumberFormat("en-US") });
    const source = fromRows(rows, [{ key: "n", renderer: grouped }, { key: "s" }, { key: "b" }]);
    const ask = (id: keyof Rec, search = "", limit = 10) =>
      source.rows.distinct!(id, { search, limit, narrow: { quick: null, conditions: [] } }, new AbortController().signal);
    expect(ask("n")).toEqual({
      values: [
        { text: "7", count: 2 },
        { text: "1,234.5", count: 1 },
      ],
      blanks: 1,
      partial: false,
      narrowed: true,
    });
    expect(ask("s")).toEqual({
      values: [
        { text: "", count: 1 },
        { text: "a", count: 1 },
        { text: "b", count: 1 },
      ],
      blanks: 1,
      partial: false,
      narrowed: true,
    });
    expect(ask("n", "1,2", 1)).toEqual({ values: [{ text: "1,234.5", count: 1 }], partial: false, narrowed: true });
    expect(ask("s", "", 2)).toMatchObject({ partial: true });
    source.destroy();
  });
});

describe("fromRows's columns", () => {
  const shownColumns = (source: ReturnType<typeof fromRows<Rec>>) =>
    source.cols.window(0, source.cols.count() as number, new AbortController().signal) as (keyof Rec)[];

  test("whatever View Specs arrive, each axis shows the model's items apart from the other's filter, and only a change to the columns' filter tells them", () =>
    hegel.test((tc) => {
      const rows = tc.draw(fewTexts);
      const source = fromRows(rows, COLUMNS);
      const [notices, colsNotices]: [ChangeNotice[], ChangeNotice[]] = [[], []];
      source.attach!(sinksTo(notices, colsNotices));
      let applied: { rows: FilterSpec<keyof Rec>; cols: FilterSpec<number> } = { rows: NONE, cols: NONE };
      try {
        expect(source.cols.quickFilter).toEqual({ regex: true });
        expect(source.cols.support!(0)).toMatchObject({ sort: true, text: { regex: true }, oneOf: true });
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 5 }));
        for (let i = 0; i < steps; i++) {
          colsNotices.length = 0;
          const keepOr = <T,>(old: T, fresh: gs.Generator<T>) => tc.draw(gs.oneOf(gs.just(old), fresh));
          const asked = {
            rows: {
              quick: keepOr(applied.rows.quick, gs.oneOf(gs.just(null), queries)),
              conditions: keepOr(applied.rows.conditions, conditionsOf),
            },
            cols: {
              quick: keepOr(applied.cols.quick, gs.oneOf(gs.just(null), colsQueryOf(rows.length))),
              conditions: keepOr(applied.cols.conditions, colsConditionsOf(rows.length)),
            },
          };
          const answer = source.view!(filtered(asked.rows, asked.cols), new AbortController().signal) as AppliedView<
            number,
            keyof Rec
          >;
          const take = <C,>(old: FilterSpec<C>, spec: FilterSpec<C>, takes: (c: Condition<C>) => boolean): FilterSpec<C> => ({
            quick: spec.quick === null || matches(spec.quick, "") !== undefined ? spec.quick : old.quick,
            conditions: spec.conditions.filter(takes),
          });
          const next = {
            rows: take(applied.rows, asked.rows, applicable),
            cols: take(applied.cols, asked.cols, applicableIn(rows.length)),
          };
          const changed = JSON.stringify(next.cols) !== JSON.stringify(applied.cols);
          applied = next;
          const { error, ...rest } = answer;
          expect(rest).toEqual({ ...filtered(applied.rows, applied.cols), totals: { rows: rows.length, cols: 3 }, grouping: null });
          expect(error !== undefined).toBe(rejects(asked.rows) || rejects(asked.cols));
          expect(colsNotices).toEqual(changed ? [{ type: "Refiltered" }] : []);
          expect(shownColumns(source)).toEqual(columnsKept(rows, applied.cols));
          const keeps = (r: Rec) => quickKeeps(applied.rows.quick, r) && applied.rows.conditions.every((c) => satisfies(c, r));
          expect(order(source)).toEqual(reference(rows, [], keeps));
        }
      } finally {
        source.destroy();
      }
    }));

  test("after rows change in place, a change from the data moves the columns whose match changed at once, and the user's edit leaves a column that stops matching unmatched until the next View Spec", () =>
    hegel.test((tc) => {
      const rows = tc.draw(fewTexts);
      const source = fromRows(rows, COLUMNS);
      const colsNotices: ChangeNotice[] = [];
      source.attach!(sinksTo([], colsNotices));
      try {
        const cols: FilterSpec<number> = {
          quick: tc.draw(gs.oneOf(gs.just(null), colsQueryOf(rows.length).filter((q) => matches(q, "") !== undefined))),
          conditions: tc.draw(colsConditionsOf(rows.length)).filter(applicableIn(rows.length)),
        };
        source.view!(filtered(NONE, cols), new AbortController().signal);
        // Whether the filter keeps each column, as last told; which are shown, and which of those unmatched.
        const filtering = cols.quick !== null || cols.conditions.length > 0;
        const kept = () => COLUMN_IDS.map((c) => columnsKept(rows, cols).includes(c));
        let flags = kept();
        let shown = flags.slice();
        const unmatched = new Set<number>();
        const changes = tc.draw(gs.integers({ minValue: 1, maxValue: 5 }));
        for (let i = 0; i < changes; i++) {
          const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
          const edit = tc.draw(gs.booleans());
          Object.assign(rows[id]!, tc.draw(text));
          colsNotices.length = 0;
          if (edit) source.edited(id);
          else source.changed(id);
          const before = shown;
          const now = kept();
          if (filtering) {
            shown = now.map((k, c) => {
              if (edit) {
                if (before[c] && !k) unmatched.add(c);
                else unmatched.delete(c);
              } else if (k !== flags[c]) unmatched.delete(c);
              else return before[c]!;
              return k || unmatched.has(c);
            });
            flags = now;
          }
          expect(shownColumns(source)).toEqual(COLUMN_IDS.filter((_, c) => shown[c]));
          const ids = shownColumns(source);
          expect(source.cols.unmatched!(ids, new AbortController().signal)).toEqual(ids.map((c) => unmatched.has(COLUMN_IDS.indexOf(c))));
          const moved = shown.some((s, c) => s !== before[c]);
          expect(colsNotices).toEqual(moved ? [{ type: "Refiltered" }] : []);
        }
        // Any View Spec, even the one in force, filters the unmatched columns.
        colsNotices.length = 0;
        source.view!(filtered(NONE, cols), new AbortController().signal);
        expect(shownColumns(source)).toEqual(columnsKept(rows, cols));
        expect(colsNotices).toEqual(unmatched.size ? [{ type: "Refiltered" }] : []);
      } finally {
        source.destroy();
      }
    }, { testCases: 500 }));

  test("a row's distinct values are the model's: its texts across the columns the narrowing keeps, counted, searched, in text order, capped, with blanks apart", () =>
    hegel.test((tc) => {
      const rows = tc.draw(texts);
      tc.assume(rows.length > 0);
      const source = fromRows(rows, COLUMNS);
      try {
        const narrow: FilterSpec<number> = {
          quick: tc.draw(gs.oneOf(gs.just(null), colsQueryOf(rows.length).filter((q) => matches(q, "") !== undefined))),
          conditions: tc.draw(colsConditionsOf(rows.length)).filter(applicableIn(rows.length)),
        };
        const id = tc.draw(gs.integers({ minValue: 0, maxValue: rows.length - 1 }));
        const search = tc.draw(gs.oneOf(gs.just(""), pieceOf(letters)));
        const limit = tc.draw(gs.integers({ minValue: 0, maxValue: 4 }));
        const answer = source.cols.distinct!(id, { search, limit, narrow }, new AbortController().signal);
        const cells = columnsKept(rows, narrow).map((c) => rows[id]![c]);
        const blanks = cells.filter((v) => v === null).length;
        const texts = cells
          .flatMap((v) => (v === null ? [] : [String(v)]))
          .filter((t) => t.toLowerCase().includes(search.toLowerCase()))
          .sort(codePoints);
        const values: { text: string; count: number }[] = [];
        for (const t of texts) {
          const v = values.find((v) => v.text === t);
          if (v) v.count++;
          else values.push({ text: t, count: 1 });
        }
        expect(answer).toEqual({
          values: values.slice(0, limit),
          ...(blanks > 0 && search === "" ? { blanks } : {}),
          partial: values.length > limit,
          narrowed: true,
        });
      } finally {
        source.destroy();
      }
    }, { testCases: 300 }));
});

describe("fromRows's hiding", () => {
  const valid = <C,>(q: TextQuery<C>) => matches(q, "") !== undefined;

  /** Whether the rows' quick filter keeps `row`, searching the columns not hidden when it searches the shown ones. */
  const rowKept = (query: TextQuery<keyof Rec> | null, row: Rec, hiddenCols: (keyof Rec)[]) => {
    if (!query) return true;
    const searched = query.searches === "shown" ? COLUMN_IDS.filter((c) => !hiddenCols.includes(c)) : query.searches;
    return searched.some((id) => row[id] !== null && matches(query, String(row[id])) === true);
  };

  /** The columns the columns' quick filter keeps, searching the rows not hidden when it searches the shown ones. */
  const columnsKeptOf = (rows: Rec[], query: TextQuery<number> | null, hiddenRows: number[]) => {
    if (!query) return [...COLUMN_IDS];
    const searched = query.searches === "shown" ? rows.map((_, i) => i).filter((i) => !hiddenRows.includes(i)) : query.searches;
    return COLUMN_IDS.filter((c) => searched.some((r) => rows[r]![c] !== null && matches(query, String(rows[r]![c])) === true));
  };

  const firstOf = <T,>(ids: T[], has: (id: T) => boolean) => [...new Set(ids)].filter(has);

  test("hiding the only item of one axis a quick filter on the other matched drops what it kept", () => {
    const rows: Rec[] = [
      { n: 1, s: "a", b: null },
      { n: 2, s: "b", b: null },
    ];
    const source = fromRows(rows, COLUMNS);
    const [notices, colsNotices]: [ChangeNotice[], ChangeNotice[]] = [[], []];
    source.attach!(sinksTo(notices, colsNotices));
    const signal = new AbortController().signal;
    const shown = (text: string) => ({ text, caseSensitive: false, regex: false, searches: "shown" as const });
    const spec = (hiddenRows: number[], hiddenCols: (keyof Rec)[]): ViewSpec<number, keyof Rec> => ({
      rows: { filter: { quick: shown("2"), conditions: [] }, sort: [], hidden: hiddenRows },
      cols: { filter: { quick: shown("a"), conditions: [] }, sort: [], hidden: hiddenCols },
    });
    try {
      source.view!(spec([], []), signal);
      expect([order(source), source.cols.window(0, 3, signal)]).toEqual([[1], ["s"]]);
      // Each hide changes only its own axis's hidden items.
      notices.length = colsNotices.length = 0;
      source.view!(spec([], ["n"]), signal);
      expect([order(source), source.cols.window(0, 3, signal)]).toEqual([[], ["s"]]);
      expect(notices).toContainEqual({ type: "Refiltered" });
      source.view!(spec([], []), signal);
      notices.length = colsNotices.length = 0;
      source.view!(spec([0], []), signal);
      expect([order(source), source.cols.window(0, 3, signal)]).toEqual([[1], []]);
      expect(colsNotices).toEqual([{ type: "Refiltered" }]);
    } finally {
      source.destroy();
    }
  });

  test("whatever View Specs arrive, hidden items leave each axis whatever the filter keeps, a quick filter searching the shown items skips them, and the answer prunes IDs the data lacks", () =>
    hegel.test((tc) => {
      const rows = tc.draw(fewTexts);
      const source = fromRows(rows, COLUMNS);
      const [notices, colsNotices]: [ChangeNotice[], ChangeNotice[]] = [[], []];
      source.attach!(sinksTo(notices, colsNotices));
      try {
        expect([source.rows.hide, source.cols.hide]).toEqual([true, true]);
        const signal = new AbortController().signal;
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 5 }));
        let last: { rows: number[]; cols: (keyof Rec)[] } = { rows: [], cols: [] };
        let was: { quick: TextQuery<keyof Rec> | null; colsQuick: TextQuery<number> | null } = { quick: null, colsQuick: null };
        for (let i = 0; i < steps; i++) {
          const before = { rows: order(source), cols: source.cols.window(0, 3, signal) as (keyof Rec)[] };
          notices.length = colsNotices.length = 0;
          const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
          // Often the quick filters in force, so that only the hidden items change.
          const keepOr = <T,>(old: T, fresh: gs.Generator<T>) => tc.draw(gs.oneOf(gs.just(old), fresh));
          const quick = keepOr(was.quick, gs.oneOf(gs.just(null), queries.filter(valid)));
          const colsQuick = keepOr(was.colsQuick, gs.oneOf(gs.just(null), colsQueryOf(rows.length).filter(valid)));
          was = { quick, colsQuick };
          // Some IDs not there, some repeated.
          const hiddenRows = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: rows.length + 1 }), { maxSize: 4 }));
          const hiddenCols = tc.draw(gs.arrays(gs.sampledFrom<keyof Rec>([...COLUMN_IDS, "x" as keyof Rec]), { maxSize: 3 }));
          const asked: ViewSpec<number, keyof Rec> = {
            rows: { filter: { quick, conditions: [] }, sort, hidden: hiddenRows },
            cols: { filter: { quick: colsQuick, conditions: [] }, sort: [], hidden: hiddenCols },
          };
          const answer = source.view!(asked, signal) as AppliedView<number, keyof Rec>;
          const hidden = {
            rows: firstOf(hiddenRows, (r) => r < rows.length),
            cols: firstOf(hiddenCols, (c) => (COLUMN_IDS as readonly string[]).includes(c)),
          };
          expect([answer.rows.hidden, answer.cols.hidden]).toEqual([hidden.rows, hidden.cols]);
          last = hidden;
          const shown = {
            rows: reference(rows, sort, (r) => rowKept(quick, r, hidden.cols)).filter((r) => !hidden.rows.includes(r)),
            cols: columnsKeptOf(rows, colsQuick, hidden.rows).filter((c) => !hidden.cols.includes(c)),
          };
          expect(order(source)).toEqual(shown.rows);
          expect(source.cols.window(0, 3, signal)).toEqual(shown.cols);
          const sameSet = <T,>(a: T[], b: T[]) => a.length === b.length && a.every((x) => b.includes(x));
          if (!sameSet(before.rows, shown.rows)) expect(notices).toContainEqual({ type: "Refiltered" });
          if (!sameSet(before.cols, shown.cols)) expect(colsNotices).toContainEqual({ type: "Refiltered" });
          // Distinct values leave the hidden items out.
          if (rows.length > 0) {
            const NONE_NARROW = { quick: null, conditions: [] };
            const byColumn = source.rows.distinct!("s", { search: "", limit: 100, narrow: NONE_NARROW }, signal) as Distinct;
            const counted = rows.filter((_, r) => !hidden.rows.includes(r)).filter((r) => r.s !== null).length;
            expect(byColumn.values.reduce((n, v) => n + v.count!, 0)).toBe(counted);
            const byRow = source.cols.distinct!(0, { search: "", limit: 100, narrow: NONE_NARROW }, signal) as Distinct;
            const across = COLUMN_IDS.filter((c) => !hidden.cols.includes(c) && rows[0]![c] !== null).length;
            expect(byRow.values.reduce((n, v) => n + v.count!, 0)).toBe(across);
          }
        }
        // Clearing the filters leaves the hidden items hidden.
        const cleared = filtered(NONE, NONE);
        cleared.rows.hidden = last.rows;
        cleared.cols.hidden = last.cols;
        source.view!(cleared, signal);
        expect(order(source)).toEqual(rows.map((_, r) => r).filter((r) => !last.rows.includes(r)));
        expect(source.cols.window(0, 3, signal)).toEqual(COLUMN_IDS.filter((c) => !last.cols.includes(c)));
      } finally {
        source.destroy();
      }
    }, { testCases: 300 }));
});

describe("fromRows's grouping", () => {
  const signal = new AbortController().signal;

  /** A View Spec grouping the rows by `field`, sorted by `sort`, with `hidden` rows hidden. */
  const groupedBy = (field: keyof Rec, sort: SortSpec<keyof Rec> = [], hidden: number[] = []): ViewSpec<number, keyof Rec> => ({
    ...rowsSorted(sort),
    rows: { ...rowsSorted(sort).rows, hidden },
    grouping: rowsBy(field),
  });

  /**
   * What the model shows: the rows kept, sorted by `field` (the sort's
   * direction on it, else ascending) then by the sort, grouped by `field`'s
   * text; each group as its text and count, then its rows.
   */
  const model = (rows: Rec[], field: keyof Rec, sort: SortSpec<keyof Rec>, keep: (r: Rec, i: number) => boolean) => {
    const direction = sort.find((c) => c.id === field)?.direction ?? "ascending";
    const kept = reference(rows, [{ id: field, direction }, ...sort], () => true).filter((i) => keep(rows[i]!, i));
    const out: ({ group: string | null; count: number } | number)[] = [];
    let i = 0;
    while (i < kept.length) {
      const text = (r: number) => (rows[r]![field] === null ? null : String(rows[r]![field]));
      const value = text(kept[i]!);
      const run = kept.slice(i).filter((_, k, all) => all.slice(0, k + 1).every((r) => text(r) === value));
      out.push({ group: value, count: run.length }, ...run);
      i += run.length;
    }
    return out;
  };

  /** What the source shows: each Group Item as its group's text and count (checked against its label and outline), each Leaf by its row ID. */
  const shown = (source: ReturnType<typeof fromRows<Rec>>, field: keyof Rec) => {
    const n = source.rows.count() as number;
    const window = source.rows.window(0, n, signal) as OutlinedWindow<number, keyof Rec>;
    let parent: number | undefined;
    return window.ids.map((id, i) => {
      const outline = window.outlines[i]!;
      if (outline.kind === "leaf") {
        expect(id).toBeGreaterThanOrEqual(0);
        expect(outline).toEqual({ kind: "leaf", depth: 1, open: "cantOpen", ancestors: [parent], group: null });
        return id;
      }
      parent = id;
      expect(id).toBeLessThan(0);
      const group = source.group(id)!;
      expect(outline).toEqual({ kind: "group", depth: 0, open: "open", ancestors: [], group: { field, value: group.value } });
      expect(group.field).toBe(field);
      expect(source.rows.groupLabel!(id, outline)).toBe(`${group.value ?? "(blank)"} (${group.count})`);
      expect(source.rows.locate!(id, signal)).toEqual({ type: "Found", position: i });
      return { group: group.value, count: group.count };
    });
  };

  test("whatever the sort, hidden rows and host filter, View Spec after View Spec, the grouped rows are the model's and the level column leaves the columns", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      const notices: ChangeNotice[] = [];
      source.attach!(sinksTo(notices));
      try {
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 3 }));
        for (let step = 0; step < steps; step++) {
          notices.length = 0;
          const field = tc.draw(gs.sampledFrom<keyof Rec>(["n", "s", "b"]));
          const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
          const hidden = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: Math.max(0, rows.length - 1) }), { maxSize: 4 }));
          const filtered = tc.draw(gs.booleans());
          source.filter(filtered ? (r) => r.b !== 1 : null);
          const answer = source.view!(groupedBy(field, sort, hidden), signal) as AppliedView<number, keyof Rec>;
          expect(answer.grouping).toEqual(rowsBy(field));
          expect(answer.rows.sort).toEqual(sort);
          expect(answer.cols.hidden).toEqual([]);
          expect(notices).toContainEqual({ type: "Refiltered" });
          const keep = (r: Rec, i: number) => !hidden.includes(i) && (!filtered || r.b !== 1);
          expect(shown(source, field)).toEqual(model(rows, field, sort, keep));
          for (const [p, id] of (source.rows.window(0, source.rows.count() as number, signal) as OutlinedWindow<number, keyof Rec>).ids.entries()) {
            if (id >= 0) expect(source.rows.locate!(id, signal)).toEqual({ type: "Found", position: p });
          }
          expect(source.cols.window(0, 3, signal)).toEqual(COLUMN_IDS.filter((c) => c !== field));
          // The host's filter regroups.
          notices.length = 0;
          source.filter((r) => r.n !== null);
          expect(notices).toEqual([{ type: "Refiltered" }]);
          expect(shown(source, field)).toEqual(model(rows, field, sort, (r, i) => !hidden.includes(i) && r.n !== null));
        }
      } finally {
        source.destroy();
      }
    }));

  type Placement = "before" | "after" | "off";
  type Shown = { kind: string; value: string | null; sum: number | null; count: number } | number;

  /**
   * The model of two levels with Subtotals and the Grand Total placed as
   * asked, summing `b` and counting: the rows kept, sorted by the levels
   * then the sort; groups in that order unless a sort column before a
   * level's own orders them, which only `b` does (by the group's sum).
   */
  const nestedModel = (
    rows: Rec[],
    levels: [keyof Rec, Placement][],
    total: Placement,
    sort: SortSpec<keyof Rec>,
    keep: (r: Rec, i: number) => boolean,
  ): Shown[] => {
    const by = levels.map(([id]) => ({ id, direction: sort.find((c) => c.id === id)?.direction ?? ("ascending" as const) }));
    const kept = reference(rows, [...by, ...sort], () => true).filter((i) => keep(rows[i]!, i));
    const text = (r: number, id: keyof Rec) => (rows[r]![id] === null ? null : String(rows[r]![id]));
    const figures = (members: number[]) => {
      const bs = members.map((r) => rows[r]!.b).filter((b): b is number => b !== null);
      return { sum: bs.length ? bs.reduce((a, b) => a + b, 0) : null, count: members.length };
    };
    const walk = (members: number[], depth: number): Shown[] => {
      const level = levels[depth];
      if (!level) return members;
      const [field, subtotal] = level;
      const groups: [string | null, number[]][] = [];
      for (const r of members) {
        const found = groups.find(([v]) => v === text(r, field));
        if (found) found[1].push(r);
        else groups.push([text(r, field), [r]]);
      }
      const own = sort.findIndex((c) => c.id === field);
      const sumBy = (own < 0 ? sort : sort.slice(0, own)).find((c) => c.id === "b");
      if (sumBy && field !== "b") {
        groups.sort(([, a], [, b]) => {
          const [x, y] = [figures(a).sum, figures(b).sum];
          if (x === y) return 0;
          if (x === null) return 1;
          if (y === null) return -1;
          return sumBy.direction === "ascending" ? x - y : y - x;
        });
      }
      return groups.flatMap(([value, group]) => {
        const sub = { kind: "subtotal", value, ...figures(group) };
        return [
          { kind: "group", value, ...figures(group) },
          ...(subtotal === "before" ? [sub] : []),
          ...walk(group, depth + 1),
          ...(subtotal === "after" ? [sub] : []),
        ];
      });
    };
    const grand = { kind: "grandTotal", value: null, ...figures(kept) };
    return [...(total === "before" ? [grand] : []), ...walk(kept, 0), ...(total === "after" ? [grand] : [])];
  };

  test("two levels with Subtotals, a Grand Total, a sum and a count match the model, ordered by the one sort rule", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      try {
        const placement = gs.sampledFrom<Placement>(["before", "after", "off"]);
        const outer = tc.draw(gs.sampledFrom<keyof Rec>(["n", "s", "b"]));
        const inner = tc.draw(gs.sampledFrom<keyof Rec>((["n", "s", "b"] as const).filter((f) => f !== outer)));
        const levels: [keyof Rec, Placement][] = [
          [outer, tc.draw(placement)],
          [inner, tc.draw(placement)],
        ];
        const total = tc.draw(placement);
        const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        const hidden = tc.draw(gs.arrays(gs.integers({ minValue: 0, maxValue: Math.max(0, rows.length - 1) }), { maxSize: 4 }));
        const grouping = {
          ...rowsBy<keyof Rec>(outer),
          rowLevels: levels.map(([field, subtotal]) => ({ field, subtotal })),
          values: [
            { field: "b" as const, aggregate: "sum" },
            { field: null, aggregate: "count" },
          ],
          rowAxis: { ...rowsBy(outer).rowAxis, grandTotal: total },
        };
        const answer = source.view!({ ...groupedBy(outer, sort, hidden), grouping }, signal) as AppliedView<number, keyof Rec>;
        expect(answer.grouping).toEqual(grouping);
        const n = source.rows.count() as number;
        const window = source.rows.window(0, n, signal) as OutlinedWindow<number, keyof Rec>;
        const shown = window.ids.map((id, i): Shown => {
          if (id >= 0) return id;
          const group = source.group(id)!;
          const outline = window.outlines[i]!;
          expect(group.kind).toBe(outline.kind);
          const [sum, count] = group.aggregates;
          expect(source.cells([id], ["b", "n"], signal)).toEqual([[sum, null]]);
          const label = { group: group.value ?? "(blank)", subtotal: `${group.value ?? "(blank)"} total`, grandTotal: "Grand total" };
          expect(source.rows.groupLabel!(id, outline)).toBe(`${label[group.kind]} (${count})`);
          if (group.kind === "grandTotal") expect(id).toBe(source.rows.grandTotal);
          return { kind: group.kind, value: group.value, sum: sum ?? null, count: count! };
        });
        expect(shown).toEqual(nestedModel(rows, levels, total, sort, (_, i) => !hidden.includes(i)));
      } finally {
        source.destroy();
      }
    }));

  test("a Grand Total pinned out of view has its figures and label before any Window lists it", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      try {
        const grouping = {
          ...rowsBy<keyof Rec>("s"),
          values: [
            { field: "b" as const, aggregate: "sum" },
            { field: null, aggregate: "count" },
          ],
          rowAxis: { ...rowsBy("s").rowAxis, grandTotal: "after" as const },
        };
        source.view!({ ...groupedBy("s", [], []), grouping }, signal);
        const id = source.rows.grandTotal!;
        const bs = rows.map((r) => r.b).filter((b): b is number => b !== null);
        const sum = bs.length ? bs.reduce((a, b) => a + b, 0) : null;
        expect(source.cells([id], ["b"], signal)).toEqual([[sum]]);
        const outline = { kind: "grandTotal" as const, depth: 0, open: "cantOpen" as const, ancestors: [], group: null };
        expect(source.rows.groupLabel!(id, outline)).toBe(`Grand total (${rows.length})`);
      } finally {
        source.destroy();
      }
    }));

  test("ungrouping brings back the plain rows, in the sort in force, and the level column", () =>
    hegel.test((tc) => {
      const rows = tc.draw(records);
      const source = fromRows(rows, COLUMNS);
      const notices: ChangeNotice[] = [];
      source.attach!(sinksTo(notices));
      try {
        const field = tc.draw(gs.sampledFrom<keyof Rec>(["n", "s", "b"]));
        const sort: SortSpec<keyof Rec> = tc.draw(specs).map(([id, direction]) => ({ id, direction }));
        source.view!(groupedBy(field, sort), signal);
        notices.length = 0;
        const answer = source.view!(rowsSorted(sort), signal) as AppliedView<number, keyof Rec>;
        expect(answer.grouping).toBeNull();
        expect(notices).toContainEqual({ type: "Refiltered" });
        expect(order(source)).toEqual(reference(rows, sort, () => true));
        expect(source.cols.window(0, 3, signal)).toEqual([...COLUMN_IDS]);
      } finally {
        source.destroy();
      }
    }));

  test("the rows group by sortable columns only; a level on any other column is left out of the answer", () => {
    const source = fromRows([{ n: 1, s: "a", b: null }], [{ key: "n" }, { key: "s", sortable: false }, { key: "b" }]);
    try {
      expect(source.rows.support!("n").group).toBe(true);
      expect(source.rows.support!("s").group).toBe(false);
      const answer = source.view!(groupedBy("s"), signal) as AppliedView<number, keyof Rec>;
      expect(answer.grouping).toBeNull();
      expect(source.rows.window(0, 1, signal)).toEqual([0]);
    } finally {
      source.destroy();
    }
  });

  test("Group Items have no cells, keep their row IDs through a regroup, and grouped rows refuse to move", () => {
    const rows: Rec[] = [
      { n: 1, s: "x", b: 0 },
      { n: 2, s: "y", b: 1 },
      { n: 3, s: "x", b: 2 },
    ];
    const source = fromRows(rows, COLUMNS);
    try {
      source.view!(groupedBy("s"), signal);
      const first = source.rows.window(0, 5, signal) as OutlinedWindow<number, keyof Rec>;
      expect(first.ids.filter((id) => id >= 0)).toEqual([0, 2, 1]);
      const [x, y] = first.ids.filter((id) => id < 0);
      expect(source.cells(first.ids.slice(0, 2), ["n", "b"], signal)).toEqual([
        [null, null],
        [1, 0],
      ]);
      source.view!(groupedBy("s", [{ id: "s", direction: "descending" }]), signal);
      const again = source.rows.window(0, 5, signal) as OutlinedWindow<number, keyof Rec>;
      expect(again.ids).toEqual([y, 1, x, 0, 2]);
      expect(source.rows.movable!()).toBe(false);
      expect(source.rows.move!([0], 3)).toEqual({ refused: expect.stringContaining("ungroup") });
    } finally {
      source.destroy();
    }
  });
});
