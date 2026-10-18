// fromRows: the ready-made in-memory Table Source over an array of row
// objects, a typed front over binnacle-memory's In-Memory Table Source in
// the Grid's wasm module. The rows stay in JS: only the Remap lives in wasm,
// and a column's sort keys cross only when it is first sorted (numbers as a
// Float64Array, strings UTF-8 encoded once, so they sort by code point).
//
// Rows are treated as immutable until `edited` or `changed` says one was
// changed in place. The user's edit leaves a row that stops matching the
// filter in view, unmatched, until the next View Spec; a change from the
// data is filtered at once.
//
// The quick filter and column conditions match in JS (a `RegExp` for a
// regex, lowercased substrings otherwise, exact text for OneOf) against each
// cell's text as its column's renderer gives it, and hand binnacle-memory
// the rows to keep. Distinct values are counted in JS too.
// Find matches the same way, once per Find Query, and walks the view's order
// for steps and the count.
//
// Row IDs are the rows' indices in the array (their Logical Indices); column
// IDs are the property names.

import { byColumn, cellText, plainText, type CellRenderer } from "./renderer";
import type {
  AxisSink,
  ChangeNotice,
  Condition,
  Distinct,
  FilterSpec,
  FindMethods,
  FindQuery,
  Located,
  SortSpec,
  TableSource,
  TextMatch,
  TextQuery,
  ViewSpec,
} from "./source";
import { bridge } from "./wasm";

type WebMemory = InstanceType<ReturnType<typeof bridge>["WebMemory"]>;

/** A column of `fromRows`: a property of the rows. */
export type RowsColumn<R> = {
  [K in RowsColumnId<R>]: {
    key: K;
    /** Header text; the property name by default. */
    label?: string;
    /** Whether its header sorts; `true` by default. */
    sortable?: boolean;
    /** Whether it takes column conditions; `true` by default. */
    filterable?: boolean;
    /** How its cells draw; plain text by default. */
    renderer?: CellRenderer<R[K]>;
  };
}[RowsColumnId<R>];

export type RowsColumnId<R> = keyof R & string;

export interface RowsSource<R> extends TableSource<number, RowsColumnId<R>, R[keyof R]> {
  /** Find over each cell's text as its column's renderer gives it, regexes included. */
  find: FindMethods<number, RowsColumnId<R>>;
  /**
   * Shows only the rows `keep` accepts (and the quick filter keeps), in the
   * sort in force; `null` shows every row the quick filter keeps.
   */
  filter(keep: ((row: R) => boolean) | null): void;
  /** The row a row ID names. */
  row(id: number): R;
  /**
   * The user edited the row `id` names in place: shows its new values and
   * re-sorts by the sort in force. If it no longer matches the filter it
   * stays shown, unmatched (drawn dimmed), until the next View Spec.
   */
  edited(id: number): void;
  /**
   * The row `id` names changed in place in the data itself, as from a live
   * feed: shows its new values, re-sorts, and applies the filter to it at
   * once, so it may come into view or leave.
   */
  changed(id: number): void;
  /** Frees the wasm side. */
  destroy(): void;
}

type SortKeys =
  | { type: "numbers"; values: Float64Array; nulls: Uint8Array }
  | { type: "strings"; bytes: Uint8Array; ends: Uint32Array; nulls: Uint8Array };

const ROWS = 0;
const COLS = 1;

/** The number a value sorts by, or `undefined` if it sorts as a string. */
function numeric(v: unknown): number | undefined {
  if (typeof v === "number") return v;
  if (typeof v === "bigint" || typeof v === "boolean") return Number(v);
  if (v instanceof Date) return v.getTime();
  return undefined;
}

/** A column sorts as numbers if every value that is not null does, else as strings. */
function extract(values: unknown[]): SortKeys {
  const nulls = Uint8Array.from(values, (v) => (v == null ? 1 : 0));
  if (values.every((v) => v == null || numeric(v) !== undefined)) {
    return { type: "numbers", values: Float64Array.from(values, (v) => (v == null ? 0 : numeric(v)!)), nulls };
  }
  const encoder = new TextEncoder();
  const parts = values.map((v) => encoder.encode(v == null ? "" : String(v)));
  const ends = new Uint32Array(parts.length);
  let end = 0;
  parts.forEach((p, i) => (ends[i] = end += p.length));
  const bytes = new Uint8Array(end);
  parts.forEach((p, i) => bytes.set(p, ends[i]! - p.length));
  return { type: "strings", bytes, ends, nulls };
}

const DIRECTION = { ascending: 0, descending: 1 } as const;

/** Whether some text matches `query`, or `undefined` if its pattern is not a valid `RegExp`. */
function matcher(query: TextMatch): ((text: string) => boolean) | undefined {
  if (query.regex) {
    let pattern: RegExp;
    try {
      pattern = new RegExp(query.text, query.caseSensitive ? "u" : "iu");
    } catch {
      return undefined;
    }
    return (text) => pattern.test(text);
  }
  if (query.caseSensitive) return (text) => text.includes(query.text);
  const lower = query.text.toLowerCase();
  return (text) => text.toLowerCase().includes(lower);
}

/** How a condition tests a cell's text (`null` for no value), before `not`; `undefined` if its pattern is not a valid `RegExp`. */
function tester(kind: Condition<unknown>["kind"]): Test | undefined {
  if (kind.type === "OneOf") {
    const listed = new Set(kind.values);
    return (text) => listed.has(text);
  }
  // A blank never matches, so `not` keeps it.
  const match = matcher(kind.query);
  return match && ((text) => text !== null && match(text));
}

/** Orders numbers with NaN after every number, as binnacle-memory does. */
function byNumber(a: number, b: number): number {
  if (Number.isNaN(a) || Number.isNaN(b)) return Number(Number.isNaN(a)) - Number(Number.isNaN(b));
  return a - b;
}

/** Orders strings by code point, as binnacle-memory orders UTF-8. */
function byCodePoint(a: string, b: string): number {
  const [x, y] = [[...a], [...b]];
  for (let i = 0; i < Math.min(x.length, y.length); i++) {
    const d = x[i]!.codePointAt(0)! - y[i]!.codePointAt(0)!;
    if (d !== 0) return d;
  }
  return x.length - y.length;
}

function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

type Match = (text: string) => boolean;
type Test = (text: string | null) => boolean;

export function fromRows<R extends object>(rows: readonly R[], columns: readonly RowsColumn<R>[]): RowsSource<R> {
  type ColId = RowsColumnId<R>;
  const index = new Map<ColId, number>(columns.map((c, i) => [c.key, i]));
  const keys = new Map<ColId, SortKeys>();
  const sinks = new Set<{ rows: AxisSink<number>; cols: AxisSink<ColId> }>();
  let wasm: WebMemory | undefined;
  let applied: SortSpec<ColId> = [];
  let quick: TextQuery<ColId> | null = null;
  let quickMatch: Match | undefined;
  let conditions: { condition: Condition<ColId>; test: Test }[] = [];
  let keep: ((row: R) => boolean) | null = null;
  const filterable = (id: ColId) => index.has(id) && columns[index.get(id)!]!.filterable !== false;
  // Created on first use, once the widget has loaded the wasm module.
  const memory = () => (wasm ??= new (bridge().WebMemory)(rows.length, columns.length));
  const tell = (notice: ChangeNotice) => {
    for (const s of sinks) s.rows.notify(notice);
  };
  const located = ([code, position]: Float64Array): Located =>
    code === 0
      ? { type: "Found", position: position! }
      : { type: "Gone", suggest: Number.isNaN(position) ? null : position! };

  /** A cell's text, as its column's renderer gives it; `null` for no value. */
  const text = (row: R, id: ColId): string | null => {
    const value = row[id];
    if (value == null) return null;
    const renderer = (columns[index.get(id)!]!.renderer ?? plainText) as CellRenderer<unknown>;
    return cellText(renderer, value);
  };

  /** Whether a row passes the quick filter `query` (matching with `match`) and every one of `tested`, and `keep`. */
  const keeper = (
    query: TextQuery<ColId> | null,
    match: Match | undefined,
    tested: { condition: Condition<ColId>; test: Test }[],
  ) => {
    // The shown columns are all of them: fromRows hides none.
    const searched = query?.searches === "shown" ? columns.map((c) => c.key) : (query?.searches ?? []);
    const named = searched.filter((id) => index.has(id));
    return (row: R) =>
      (!match ||
        named.some((id) => {
          const t = text(row, id);
          return t !== null && match(t);
        })) &&
      tested.every(({ condition, test }) => test(text(row, condition.id)) !== condition.not) &&
      (keep?.(row) ?? true);
  };

  /** The conditions of `asked` this source applies, each with its test: those on filterable columns, with a valid pattern. */
  const testable = (asked: Condition<ColId>[]) =>
    asked.flatMap((condition) => {
      const test = filterable(condition.id) ? tester(condition.kind) : undefined;
      return test ? [{ condition, test }] : [];
    });

  /** Which cells match a Find Query's text, by Logical Index then column index; worked out once per query. */
  let hits: { query: string; cells: Uint8Array } | undefined;
  const hitsFor = (find: FindQuery<ColId>): Uint8Array => {
    const id = JSON.stringify(find.query);
    if (hits?.query === id) return hits.cells;
    const match = matcher(find.query);
    const n = columns.length;
    const cells = new Uint8Array(rows.length * n);
    const { searches } = find.query;
    const searched = searches === "shown" ? columns.map((c) => c.key) : searches.filter((c) => index.has(c));
    if (match) {
      rows.forEach((row, r) => {
        for (const c of searched) {
          const t = text(row, c);
          if (t !== null && match(t)) cells[r * n + index.get(c)!] = 1;
        }
      });
    }
    hits = { query: id, cells };
    return cells;
  };

  /** The view: its rows' Logical Indices and its columns' indices, each in Position order. */
  const shown = () => {
    const m = memory();
    return { rowsAt: Array.from(m.window(ROWS, 0, m.count(ROWS))), colsAt: Array.from(m.window(COLS, 0, columns.length)) };
  };

  /** Hands binnacle-memory the rows the quick filter, every condition and `keep` keep. */
  const refilter = () => {
    const m = memory();
    if (!keep && !quick && !conditions.length) return void m.clear_filter();
    const keeps = keeper(quick, quickMatch, conditions);
    m.filter(Uint8Array.from(rows, (r) => (keeps(r) ? 1 : 0)));
  };

  /**
   * The distinct texts of column `id` in the rows `keep` and `narrow` keep,
   * counted, containing `search` (ignoring case), in the column's sort order
   * (ties by text), up to `limit`; blanks counted unless searching.
   */
  const distinct = (id: ColId, search: string, limit: number, narrow: FilterSpec<ColId>): Distinct => {
    const asked = narrow.quick;
    const match = (asked && matcher(asked)) ?? undefined;
    const keeps = keeper(match ? asked : null, match, testable(narrow.conditions));
    const found = search ? matcher({ text: search, caseSensitive: false, regex: false })! : undefined;
    const values = rows.map((r) => r[id]);
    const numbers = values.every((v) => v == null || numeric(v) !== undefined);
    // Each text's count and least sort key.
    const seen = new Map<string, { count: number; rank: number | string }>();
    let blanks = 0;
    rows.forEach((row, i) => {
      if (!index.has(id) || !keeps(row)) return;
      const t = text(row, id);
      if (t === null) return void blanks++;
      if (found && !found(t)) return;
      const rank = numbers ? numeric(values[i])! : String(values[i]);
      const entry = seen.get(t);
      if (!entry) return void seen.set(t, { count: 1, rank });
      entry.count++;
      const less = numbers ? byNumber(rank as number, entry.rank as number) : byCodePoint(rank as string, entry.rank as string);
      if (less < 0) entry.rank = rank;
    });
    const order = ([a, x]: [string, { rank: number | string }], [b, y]: [string, { rank: number | string }]) =>
      (numbers ? byNumber(x.rank as number, y.rank as number) : byCodePoint(x.rank as string, y.rank as string)) ||
      byCodePoint(a, b);
    const listed = [...seen].sort(order);
    return {
      values: listed.slice(0, limit).map(([text, { count }]) => ({ text, count })),
      ...(blanks > 0 && !search ? { blanks } : {}),
      partial: listed.length > limit,
      narrowed: true,
    };
  };

  const sort = (spec: SortSpec<ColId>) => {
    const m = memory();
    for (const { id } of spec) {
      let k = keys.get(id);
      if (!k) keys.set(id, (k = extract(rows.map((r) => r[id]))));
      if (k.type === "numbers") m.stage_numbers(k.values, k.nulls);
      else m.stage_strings(k.bytes, k.ends, k.nulls);
    }
    m.sort(
      BigUint64Array.from(spec, (c) => BigInt(index.get(c.id)!)),
      Uint8Array.from(spec, (c) => DIRECTION[c.direction]),
    );
    applied = spec;
  };

  /**
   * The row `id` changed in place: told at its old Position, re-sorted, and
   * filtered again, at once or (with `defer`, for the user's edit) only if
   * it was out of view.
   */
  const update = (id: number, defer: boolean) => {
    const m = memory();
    const at = located(m.locate(ROWS, id));
    if (at.type === "Found") tell({ type: "ContentChanged", start: at.position, end: at.position + 1 });
    keys.clear();
    hits = undefined;
    let moved = false;
    if (keep || quick || conditions.length) {
      const kept = keeper(quick, quickMatch, conditions)(rows[id]!) ? 1 : 0;
      moved = defer ? m.defer_row(id, kept) : m.refilter_row(id, kept);
    }
    if (applied.length) sort(applied);
    if (moved) tell({ type: "Refiltered" });
    else if (applied.length) tell({ type: "Reordered" });
  };

  return {
    rows: {
      count: () => memory().count(ROWS),
      window: (start, end) => Array.from(memory().window(ROWS, start, end)),
      quickFilter: { regex: true },
      support: (by) => {
        const column = columns[index.get(by) ?? -1];
        const filterable = column?.filterable !== false;
        return { sort: column?.sortable !== false, text: filterable && { regex: true }, oneOf: filterable };
      },
      distinct: (by, { search, limit, narrow }) => distinct(by, search, limit, narrow),
      locate: (id) => located(memory().locate(ROWS, id)),
      unmatched: (ids) => Array.from(memory().unmatched(Uint32Array.from(ids)), (u) => u === 1),
    },
    cols: {
      count: () => columns.length,
      window: (start, end) => Array.from(memory().window(COLS, start, end), (i) => columns[i]!.key),
      locate: (id) => located(memory().locate(COLS, index.get(id) ?? columns.length)),
      move(ids, to) {
        memory().move_columns(Uint32Array.from(ids, (id) => index.get(id)!), to);
        return true;
      },
      label: (id) => columns[index.get(id) ?? -1]?.label ?? id,
    },
    // Columns never sort or filter by rows, so their part of the spec is
    // dropped. A quick filter whose pattern is not a valid RegExp leaves the
    // one in force; a condition with one, or on a column that takes none, is
    // left out.
    view(spec: ViewSpec<number, ColId>) {
      const by = spec.rows.sort;
      if (by.some((c) => !index.has(c.id) || columns[index.get(c.id)!]!.sortable === false)) {
        throw new Error(`binnacle: fromRows cannot sort by ${by.map((c) => c.id).join(", ")}`);
      }
      const asked = spec.rows.filter?.quick ?? null;
      const match = asked && matcher(asked);
      const quickChanged = !same(asked, quick) && match !== undefined;
      if (quickChanged) {
        quick = asked;
        quickMatch = match ?? undefined;
      }
      const kept = testable(spec.rows.filter?.conditions ?? []);
      const conditionsChanged = !same(
        kept.map((k) => k.condition),
        conditions.map((k) => k.condition),
      );
      if (conditionsChanged) conditions = kept;
      sort(by);
      // Any View Spec, even the one in force, filters the unmatched rows.
      const refiltered = quickChanged || conditionsChanged || memory().has_unmatched();
      if (refiltered) refilter();
      tell({ type: refiltered ? "Refiltered" : "Reordered" });
      return {
        rows: { filter: { quick, conditions: conditions.map((k) => k.condition) }, sort: by },
        cols: { filter: { quick: null, conditions: [] }, sort: [] },
        totals: { rows: rows.length, cols: columns.length },
      };
    },
    cells: (rowIds, colIds) => rowIds.map((r) => colIds.map((c) => rows[r]![c])),
    find: {
      regex: true,
      marks(rowIds, colIds, query) {
        const cells = hitsFor(query);
        const n = columns.length;
        return rowIds.map((r) => colIds.map((c) => index.has(c) && cells[r * n + index.get(c)!] === 1));
      },
      next(from, direction, query) {
        const cells = hitsFor(query);
        const n = columns.length;
        const { rowsAt, colsAt } = shown();
        const [nr, nc] = [rowsAt.length, colsAt.length];
        const size = nr * nc;
        // The cell at place `i` of the query's order, as Positions.
        const at = (i: number): [number, number] =>
          query.by === "rows" ? [Math.floor(i / nc), i % nc] : [i % nr, Math.floor(i / nr)];
        const hit = (i: number) => {
          const [r, c] = at(i);
          return cells[rowsAt[r]! * n + colsAt[c]!] === 1;
        };
        let start: number | null = null;
        if (from) {
          const r = rowsAt.indexOf(from.row);
          const c = colsAt.indexOf(index.get(from.col) ?? -1);
          if (r >= 0 && c >= 0) start = query.by === "rows" ? r * nc + c : c * nr + r;
        }
        const forward = direction === "next";
        const first = start === null ? (forward ? 0 : size - 1) : start + (forward ? 1 : -1);
        for (let k = 0; k < size; k++) {
          const place = forward ? first + k : first - k;
          const i = ((place % size) + size) % size;
          if (!hit(i)) continue;
          let before = 0;
          for (let j = 0; j < i; j++) if (hit(j)) before++;
          const [r, c] = at(i);
          const wrapped = start !== null && (place >= size || place < 0);
          return { row: rowsAt[r]!, col: columns[colsAt[c]!]!.key, index: before, wrapped };
        }
        return null;
      },
      count(query) {
        const cells = hitsFor(query);
        const n = columns.length;
        const { rowsAt, colsAt } = shown();
        let total = 0;
        for (const r of rowsAt) for (const c of colsAt) total += cells[r * n + c]!;
        return { total };
      },
    },
    renderer: byColumn((id: ColId) => columns[index.get(id) ?? -1]?.renderer as CellRenderer<R[keyof R]> | undefined),
    attach(sink) {
      sinks.add(sink);
      return () => sinks.delete(sink);
    },
    filter(next) {
      keep = next;
      refilter();
      tell({ type: "Refiltered" });
    },
    row: (id) => rows[id]!,
    edited: (id) => update(id, true),
    changed: (id) => update(id, false),
    destroy() {
      wasm?.free();
      wasm = undefined;
    },
  };
}
