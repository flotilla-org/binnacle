// fromRows: the ready-made in-memory Table Source over an array of row
// objects, a typed front over binnacle-memory's In-Memory Table Source in
// the Grid's wasm module. The rows stay in JS: only the Remap lives in wasm,
// and a column's sort keys cross only when it is first sorted (numbers as a
// Float64Array, strings UTF-8 encoded once, so they sort by code point).
// The columns sort by rows' values the same way, a row's values across the
// columns taken as one sort key each.
//
// Rows are treated as immutable until `edited` or `changed` says one was
// changed in place. The user's edit leaves a row or column that stops
// matching its filter in view, unmatched, until the next View Spec; a change
// from the data is filtered at once.
//
// Each axis's quick filter and conditions match in JS (a `RegExp` for a
// regex, lowercased substrings otherwise, exact text for OneOf) against each
// cell's text as its column's renderer gives it, and hand binnacle-memory
// the items to keep: rows by their values across the columns, columns by
// theirs across the rows. A quick filter searching the shown items of the
// other axis searches those not hidden. Distinct values are counted in JS
// too, over the items not hidden.
//
// Both axes hide: binnacle-memory leaves the hidden items out whatever the
// filter keeps, and the answer prunes IDs the data lacks.
// Find matches the same way, once per Find Query, and walks the view's order
// for steps and the count; a pattern RegExp rejects fails both and marks
// nothing.
//
// Row IDs are the rows' indices in the array (their Logical Indices); column
// IDs are the property names.
//
// Rows and columns both move, read in the order shown with the filter in
// force. A move on a sorted axis keeps the order shown and drops the sort,
// unless `sortedMoves` refuses it.
//
// The rows group by any sortable column, through binnacle-group's grouping
// layer in wasm: the rows are sorted by the level columns first (the user's
// direction on one, else ascending), then by the user's sort, and grouped by
// each level column's cell text, nested outermost first, with Subtotals and
// a Grand Total where the grouping places them. Values are the standard
// aggregates: `count`, `distinct count` (of cell texts), and `sum`,
// `average`, `minimum` and `maximum` of numbers (dates count as their
// milliseconds); others are blank. Groups order by the one sort rule:
// sorted by a column a value aggregates, by that value; by a level column,
// by their text; else ascending. A Summary Item's row ID is a negative
// number, stable for the source's life (the Grand Total's is -1, its
// `grandTotal`); `group(id)` describes it, and its cells hold its group's
// figure for each column a value aggregates. The level columns leave the
// columns while grouped, and the rows don't move.

import { comparedBy, comparedOf, passes, type Compared } from "./compare";
import { byColumn, cellText, plainText, type CellRenderer } from "./renderer";
import type {
  AppliedView,
  ChangeNotice,
  Condition,
  Distinct,
  FilterSpec,
  FindMethods,
  FindQuery,
  Grouping,
  ListedValue,
  Located,
  Outline,
  SortSpec,
  TableSink,
  TableSource,
  TextMatch,
  TextQuery,
  ViewSpec,
} from "./source";
import { summaryLabel } from "./source";
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

export interface RowsOptions {
  /**
   * What a move on a sorted axis does: `"bake"` (the default) keeps the
   * order shown, places the moved items and drops the axis's sort;
   * `"refuse"` refuses it, and the axis is not movable while sorted.
   */
  sortedMoves?: "bake" | "refuse";
  /** The most rows and columns it hides at once, if fewer than the widget's own cap. */
  hideCap?: { rows?: number; cols?: number };
}

export interface RowsSource<R> extends TableSource<number, RowsColumnId<R>, R[keyof R]> {
  /** Find over each cell's text as its column's renderer gives it, regexes included. */
  find: FindMethods<number, RowsColumnId<R>>;
  /**
   * Shows only the rows `keep` accepts (and the quick filter keeps), in the
   * sort in force; `null` shows every row the quick filter keeps.
   */
  filter(keep: ((row: R) => boolean) | null): void;
  /** The row a Leaf's row ID names. */
  row(id: number): R;
  /** The Summary Item a negative row ID names, while it is in view; `undefined` for a Leaf. */
  group(id: number): RowsGroup<RowsColumnId<R>> | undefined;
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

/**
 * A Summary Item of the grouped rows: its kind; its level column and cell
 * text (`null` for blanks), or `null` for the Grand Total; how many rows it
 * holds; and its aggregates, one per value of the grouping (`null` for a
 * blank).
 */
export interface RowsGroup<ColId> {
  kind: "group" | "subtotal" | "grandTotal";
  field: ColId | null;
  value: ListedValue;
  count: number;
  aggregates: (number | null)[];
}

/** An item of a grouped Window as `WebMemory.grouped_window` gives it; a Summary Item's field is a column index. */
interface GroupedItem {
  key: bigint;
  item: number | null;
  outline: Outline<bigint, bigint>;
  aggregates: (number | null)[] | null;
  children: number | null;
}

/** The top bit of a Summary Item's Key, which its negative row ID leaves out. */
const SUMMARY = 1n << 63n;
const summaryId = (key: bigint) => -Number(key & (SUMMARY - 1n)) - 1;
const summaryKey = (id: number) => SUMMARY | BigInt(-id - 1);

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
  return encoded(values.map((v) => (v == null ? null : String(v))));
}

/** Strings UTF-8 encoded end to end, as binnacle-memory takes them; `null` for none. */
function encoded(texts: (string | null)[]): SortKeys & { type: "strings" } {
  const nulls = Uint8Array.from(texts, (t) => (t === null ? 1 : 0));
  const values = texts.map((t) => t ?? "");
  const encoder = new TextEncoder();
  const parts = values.map((v) => encoder.encode(v));
  const ends = new Uint32Array(parts.length);
  let end = 0;
  parts.forEach((p, i) => (ends[i] = end += p.length));
  const bytes = new Uint8Array(end);
  parts.forEach((p, i) => bytes.set(p, ends[i]! - p.length));
  return { type: "strings", bytes, ends, nulls };
}

const DIRECTION = { ascending: 0, descending: 1 } as const;

/** Why `RegExp` rejects `query`'s pattern, without its preamble; `undefined` if it takes it. */
function invalid(query: TextMatch): string | undefined {
  if (!query.regex) return undefined;
  try {
    new RegExp(query.text, "u");
    return undefined;
  } catch (e) {
    return String((e as Error).message).replace(/^Invalid regular expression: \/.*\/[a-z]*: /s, "");
  }
}

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

/** How a condition tests a cell's text (`null` for no value) or value, before `not`; `undefined` if its pattern is not a valid `RegExp`. */
function tester(kind: Condition<unknown>["kind"]): Test | undefined {
  if (kind.type === "OneOf") {
    const listed = new Set(kind.values);
    return (text) => listed.has(text);
  }
  if (kind.type === "Compare") {
    const { comparison } = kind;
    return (_, value) => passes(comparison, value);
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

/** The first of each ID of `ids` that `has` accepts, in order. */
function pruned<Id>(ids: readonly Id[], has: (id: Id) => boolean): Id[] {
  return [...new Set(ids)].filter(has);
}

/** Whether two lists without repeats hold the same items. */
function sameItems<T>(a: readonly T[], b: readonly T[]): boolean {
  return a.length === b.length && a.every((x) => b.includes(x));
}

function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

type Match = (text: string) => boolean;
type Test = (text: string | null, value: unknown) => boolean;

/** One axis's filter in force: its quick filter with its matcher, and its conditions each with its test. */
interface Filter<CrossId> {
  quick: TextQuery<CrossId> | null;
  match: Match | undefined;
  conditions: { condition: Condition<CrossId>; test: Test }[];
}

const NO_FILTER = { quick: null, match: undefined, conditions: [] };

/**
 * Whether a filter keeps an item whose text in item `id` of the other axis
 * is `text(id)`, and value `value(id)`; the quick filter searches `searched`.
 */
function keeps<CrossId>(
  { match, conditions }: Filter<CrossId>,
  searched: CrossId[],
  text: (id: CrossId) => string | null,
  value: (id: CrossId) => unknown,
) {
  return (
    (!match ||
      searched.some((id) => {
        const t = text(id);
        return t !== null && match(t);
      })) &&
    conditions.every(({ condition, test }) => test(text(condition.id), value(condition.id)) !== condition.not)
  );
}

/** What an axis's filter may test by an item of the other axis: whether it takes conditions, and what it compares. */
interface Takes<CrossId> {
  (id: CrossId): boolean;
  compared(id: CrossId): Compared | null;
}

/**
 * The conditions of `asked` a filter applies, each with its test: those on
 * items that take them, with a valid pattern, comparing the kind of value
 * the item holds.
 */
function testable<CrossId>(asked: Condition<CrossId>[], takes: Takes<CrossId>) {
  return asked.flatMap((condition) => {
    const { kind, id } = condition;
    const compares = kind.type !== "Compare" || comparedBy(kind.comparison) === (takes.compared(id) ?? "none");
    const test = takes(id) && compares ? tester(kind) : undefined;
    return test ? [{ condition, test }] : [];
  });
}

/**
 * `filter` with what `asked` asks for: a quick filter whose pattern is not
 * a valid RegExp leaves the one in force, and only testable conditions are
 * kept. Also whether that changed it, and why RegExp rejected the first
 * pattern it did, if any.
 */
function take<CrossId>(filter: Filter<CrossId>, asked: FilterSpec<CrossId> | undefined, takes: Takes<CrossId>) {
  const quick = asked?.quick ?? null;
  const match = quick && matcher(quick);
  let next = filter;
  if (!same(quick, filter.quick) && match !== undefined) next = { ...next, quick, match: match ?? undefined };
  const kept = testable(asked?.conditions ?? [], takes);
  if (!same(kept.map((k) => k.condition), filter.conditions.map((k) => k.condition))) next = { ...next, conditions: kept };
  const patterns = [quick, ...(asked?.conditions ?? []).map((c) => c.kind.type === "Text" && c.kind.query)];
  const error = patterns.flatMap((q) => (q ? [invalid(q)] : [])).find((e) => e !== undefined);
  return { filter: next, changed: next !== filter, error };
}

/** What a filter spec says of `filter`. */
function spec<CrossId>(filter: Filter<CrossId>): FilterSpec<CrossId> {
  return { quick: filter.quick, conditions: filter.conditions.map((k) => k.condition) };
}

export function fromRows<R extends object>(
  rows: readonly R[],
  columns: readonly RowsColumn<R>[],
  options: RowsOptions = {},
): RowsSource<R> {
  type ColId = RowsColumnId<R>;
  const index = new Map<ColId, number>(columns.map((c, i) => [c.key, i]));
  const keys = new Map<ColId, SortKeys>();
  /** Each row's values across the columns as sort keys, by row ID, for sorting the columns. */
  const rowKeys = new Map<number, SortKeys>();
  const sinks = new Set<TableSink<number, ColId>>();
  let wasm: WebMemory | undefined;
  let applied: SortSpec<ColId> = [];
  let colsApplied: SortSpec<number> = [];
  let filters: { rows: Filter<ColId>; cols: Filter<number> } = { rows: NO_FILTER, cols: NO_FILTER };
  let hidden: { rows: number[]; cols: ColId[] } = { rows: [], cols: [] };
  let keep: ((row: R) => boolean) | null = null;
  /** Each column's kind of value to compare, worked out on first use and again after a change. */
  const kinds = new Map<ColId, Compared | null>();
  /** The columns the rows group by, outermost first, and the grouping in force. */
  let levels: ColId[] = [];
  let grouping: Grouping<ColId, number, ColId> | null = null;
  /** The Summary Items seen in grouped Windows since the rows last grouped, by row ID. */
  const groups = new Map<number, RowsGroup<ColId>>();
  const groupable = (id: ColId) => index.has(id) && columns[index.get(id)!]!.sortable !== false;
  const filterable = (id: ColId) => index.has(id) && columns[index.get(id)!]!.filterable !== false;
  const compared = (id: ColId): Compared | null => {
    if (!filterable(id)) return null;
    let kind = kinds.get(id);
    if (kind === undefined) kinds.set(id, (kind = comparedOf(rows.map((r) => r[id]))));
    return kind;
  };
  /** What the columns compare by row `id`: the kind its values in the filterable columns share. */
  const comparedAcross = (id: number): Compared | null =>
    isRow(id) ? comparedOf(columns.filter((c) => c.filterable !== false).map((c) => rows[id]![c.key])) : null;
  const takesColumn: Takes<ColId> = Object.assign((id: ColId) => filterable(id), { compared });
  const isRow = (id: number) => Number.isInteger(id) && id >= 0 && id < rows.length;
  const takesRow: Takes<number> = Object.assign((id: number) => isRow(id), { compared: comparedAcross });
  // Created on first use, once the widget has loaded the wasm module.
  const memory = () => {
    if (wasm) return wasm;
    wasm = new (bridge().WebMemory)(rows.length, columns.length);
    wasm.refuse_sorted_moves(options.sortedMoves === "refuse");
    return wasm;
  };
  const tell = (notice: ChangeNotice, axis: "rows" | "cols" = "rows") => {
    for (const s of sinks) s[axis].notify(notice);
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

  /** Whether a row passes `filter` and `keep`. */
  const rowKeeper = (filter: Filter<ColId>) => {
    const { quick } = filter;
    const shownColumns = () => columns.map((c) => c.key).filter((id) => !hidden.cols.includes(id));
    const searched = (quick?.searches === "shown" ? shownColumns() : (quick?.searches ?? [])).filter((id) =>
      index.has(id),
    );
    return (row: R) =>
      keeps(
        filter,
        searched,
        (id) => text(row, id),
        (id) => row[id],
      ) && (keep?.(row) ?? true);
  };

  /** Whether the column at index `c` passes `filter`. */
  const columnKeeper = (filter: Filter<number>) => {
    const { quick } = filter;
    const shownRows = () => {
      const out = new Set(hidden.rows);
      return rows.map((_, i) => i).filter((i) => !out.has(i));
    };
    const searched = (quick?.searches === "shown" ? shownRows() : (quick?.searches ?? [])).filter(isRow);
    const key = (c: number) => columns[c]!.key;
    return (c: number) =>
      keeps(
        filter,
        searched,
        (r) => text(rows[r]!, key(c)),
        (r) => rows[r]?.[key(c)],
      );
  };

  /** A filter for `narrow`, as a checklist asks: its quick filter if valid, its testable conditions. */
  const narrowing = <C,>(narrow: FilterSpec<C>, takes: Takes<C>): Filter<C> => {
    const match = (narrow.quick && matcher(narrow.quick)) ?? undefined;
    return { quick: match ? narrow.quick : null, match, conditions: testable(narrow.conditions, takes) };
  };

  const columnKeeps = () => Uint8Array.from(columns, (_, c) => (columnKeeper(filters.cols)(c) ? 1 : 0));
  const filtering = (filter: Filter<unknown>) => filter.quick !== null || filter.conditions.length > 0;

  /** Which cells match a Find Query's text, by Logical Index then column index; worked out once per query. */
  let hits: { query: string; cells: Uint8Array } | undefined;
  const hitsFor = (find: FindQuery<ColId>): Uint8Array => {
    const id = JSON.stringify(find.query);
    if (hits?.query === id) return hits.cells;
    const error = invalid(find.query);
    if (error !== undefined) throw new Error(error);
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

  /** Hands binnacle-memory the rows the rows' filter and `keep` keep. */
  const refilter = () => {
    const m = memory();
    if (!keep && !filtering(filters.rows)) return void m.clear_filter(ROWS);
    const keeps = rowKeeper(filters.rows);
    m.filter(ROWS, Uint8Array.from(rows, (r) => (keeps(r) ? 1 : 0)));
  };

  /** Hands binnacle-memory the columns the columns' filter keeps. */
  const refilterColumns = () => {
    const m = memory();
    if (!filtering(filters.cols)) m.clear_filter(COLS);
    else m.filter(COLS, columnKeeps());
  };

  /**
   * The distinct texts of column `id` in the rows `keep` and `narrow` keep,
   * counted, containing `search` (ignoring case), in the column's sort order
   * (ties by text), up to `limit`; blanks counted unless searching.
   */
  const distinct = (id: ColId, search: string, limit: number, narrow: FilterSpec<ColId>): Distinct => {
    const keeps = rowKeeper(narrowing(narrow, takesColumn));
    const found = search ? matcher({ text: search, caseSensitive: false, regex: false })! : undefined;
    const values = rows.map((r) => r[id]);
    const numbers = values.every((v) => v == null || numeric(v) !== undefined);
    // Each text's count and least sort key.
    const seen = new Map<string, { count: number; rank: number | string }>();
    let blanks = 0;
    const out = new Set(hidden.rows);
    rows.forEach((row, i) => {
      if (!index.has(id) || !keeps(row) || out.has(i)) return;
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

  /**
   * The distinct texts of row `id` across the columns `narrow` keeps,
   * counted, containing `search` (ignoring case), in text order, up to
   * `limit`; blanks counted unless searching.
   */
  const distinctAcross = (id: number, search: string, limit: number, narrow: FilterSpec<number>): Distinct => {
    const keeps = columnKeeper(narrowing(narrow, takesRow));
    const found = search ? matcher({ text: search, caseSensitive: false, regex: false })! : undefined;
    const seen = new Map<string, number>();
    let blanks = 0;
    columns.forEach((c, i) => {
      if (!isRow(id) || !keeps(i) || hidden.cols.includes(c.key)) return;
      const t = text(rows[id]!, c.key);
      if (t === null) return void blanks++;
      if (!found || found(t)) seen.set(t, (seen.get(t) ?? 0) + 1);
    });
    const listed = [...seen].sort(([a], [b]) => byCodePoint(a, b));
    return {
      values: listed.slice(0, limit).map(([text, count]) => ({ text, count })),
      ...(blanks > 0 && !search ? { blanks } : {}),
      partial: listed.length > limit,
      narrowed: true,
    };
  };

  /** Sorts the rows by the level columns first, then by `spec`, which becomes the sort in force. */
  const sort = (spec: SortSpec<ColId>) => {
    const m = memory();
    const by: SortSpec<ColId> = [
      ...levels.map((id) => ({ id, direction: spec.find((c) => c.id === id)?.direction ?? ("ascending" as const) })),
      ...spec,
    ];
    for (const { id } of by) {
      let k = keys.get(id);
      if (!k) keys.set(id, (k = extract(rows.map((r) => r[id]))));
      if (k.type === "numbers") m.stage_numbers(k.values, k.nulls);
      else m.stage_strings(k.bytes, k.ends, k.nulls);
    }
    const moved = m.sort(
      ROWS,
      BigUint64Array.from(by, (c) => BigInt(index.get(c.id)!)),
      Uint8Array.from(by, (c) => DIRECTION[c.direction]),
    );
    applied = spec;
    return moved;
  };

  /**
   * Groups the rows in view as the grouping in force says, again, or
   * ungroups them if it has no levels: stages each column it reads, as cell
   * texts and as numbers, and the user's sort orders the groups.
   */
  const regroup = () => {
    const m = memory();
    groups.clear();
    if (levels.length === 0 || !grouping) return void (m.grouped() && m.ungroup());
    const read = new Set([...levels, ...grouping.values.flatMap((v) => (v.field === null ? [] : [v.field]))]);
    for (const id of read) {
      const { bytes, ends, nulls } = encoded(rows.map((r) => text(r, id)));
      const numbers = rows.map((r) => numeric(r[id]));
      m.stage_field(
        index.get(id)!,
        bytes,
        ends,
        nulls,
        Float64Array.from(numbers, (n) => n ?? 0),
        Uint8Array.from(numbers, (n) => (n === undefined ? 1 : 0)),
      );
    }
    const column = (id: ColId) => BigInt(index.get(id)!);
    const wire = {
      ...grouping,
      rowLevels: grouping.rowLevels.flatMap((l) => ("field" in l ? [{ ...l, field: column(l.field) }] : [])),
      values: grouping.values.map((v) => ({ ...v, field: v.field === null ? null : column(v.field) })),
      rowAxis: { ...grouping.rowAxis, expand: { ...grouping.rowAxis.expand, except: [] } },
      colAxis: { ...grouping.colAxis, expand: { ...grouping.colAxis.expand, except: [] } },
    };
    m.group(
      wire,
      Uint32Array.from(applied, (c) => index.get(c.id)!),
      Uint8Array.from(applied, (c) => DIRECTION[c.direction]),
    );
  };

  /** The index of the value that counts the rows, if the grouping has one. */
  const counted = () => grouping?.values.findIndex((v) => v.field === null && v.aggregate === "count") ?? -1;

  /** The grouped rows at Positions `start..end`, each Summary Item by its negative row ID. */
  const groupedWindow = (start: number, end: number) => {
    const items = memory().grouped_window(start, end) as GroupedItem[];
    const id = (item: GroupedItem) => item.item ?? summaryId(item.key);
    const outlines = items.map((item): Outline<number, ColId> => {
      const o = item.outline;
      const group = o.group ? { field: columns[Number(o.group.field)]!.key, value: o.group.value } : null;
      if (o.kind !== "leaf" && item.aggregates) {
        const count = item.aggregates[counted()] ?? item.children ?? 0;
        const summary = { kind: o.kind, field: group?.field ?? null, value: group?.value ?? null };
        groups.set(id(item), { ...summary, count, aggregates: item.aggregates });
      }
      return { kind: o.kind, depth: o.depth, open: o.open, ancestors: o.ancestors.map(summaryId), group };
    });
    return { ids: items.map(id), outlines };
  };

  /** A Summary Item's figure in column `col`: that of the first value aggregating it. */
  const figure = (id: number, col: ColId): number | null => {
    const at = grouping?.values.findIndex((v) => v.field === col) ?? -1;
    return at < 0 ? null : (groups.get(id)?.aggregates[at] ?? null);
  };

  /** Sorts the columns by the rows `spec` names, each row's values across the columns as its keys. */
  const sortColumns = (spec: SortSpec<number>) => {
    const m = memory();
    for (const { id } of spec) {
      let k = rowKeys.get(id);
      if (!k) rowKeys.set(id, (k = extract(columns.map((c) => rows[id]![c.key]))));
      if (k.type === "numbers") m.stage_numbers(k.values, k.nulls);
      else m.stage_strings(k.bytes, k.ends, k.nulls);
    }
    const moved = m.sort(
      COLS,
      BigUint64Array.from(spec, (c) => BigInt(c.id)),
      Uint8Array.from(spec, (c) => DIRECTION[c.direction]),
    );
    colsApplied = spec;
    return moved;
  };

  /** The View Spec in force, as `view` answers it. */
  const current = (): AppliedView<number, ColId> => ({
    rows: { filter: spec(filters.rows), sort: applied, hidden: hidden.rows },
    cols: { filter: spec(filters.cols), sort: colsApplied, hidden: hidden.cols },
    totals: { rows: rows.length, cols: columns.length },
    grouping,
  });

  /**
   * Moves the items with these Logical Indices on `axis`. Baking a sort
   * drops it, which the sinks hear before the widget gets the answer.
   */
  const move = (axis: typeof ROWS | typeof COLS, items: number[], to: number): boolean | { refused: string } => {
    const sorted = (axis === ROWS ? applied : colsApplied).length > 0;
    if (axis === ROWS && levels.length > 0) return { refused: "Can't move grouped rows: ungroup them first" };
    if (!memory().move_items(axis, Uint32Array.from(items), to)) {
      return { refused: "Can't move while sorted: clear the sort first" };
    }
    if (!sorted) return true;
    if (axis === ROWS) applied = [];
    else colsApplied = [];
    const view = current();
    for (const s of sinks) s.viewChanged(view);
    return true;
  };

  /**
   * The row `id` changed in place: told at its old Position, re-sorted, and
   * filtered again with the columns, at once or (with `defer`, for the
   * user's edit) only those out of view.
   */
  const update = (id: number, defer: boolean) => {
    const m = memory();
    const at = located(m.locate(ROWS, id));
    if (at.type === "Found") tell({ type: "ContentChanged", start: at.position, end: at.position + 1 });
    keys.clear();
    kinds.clear();
    rowKeys.delete(id);
    hits = undefined;
    let moved = false;
    if (keep || filtering(filters.rows)) {
      const kept = rowKeeper(filters.rows)(rows[id]!) ? 1 : 0;
      moved = defer ? m.defer(ROWS, id, kept) : m.refilter(ROWS, id, kept);
    }
    if (applied.length || levels.length) sort(applied);
    if (levels.length) {
      regroup();
      moved = true;
    }
    if (moved) tell({ type: "Refiltered" });
    else if (applied.length) tell({ type: "Reordered" });
    if (colsApplied.some((c) => c.id === id)) {
      sortColumns(colsApplied);
      tell({ type: "Reordered" }, "cols");
    }
    if (!filtering(filters.cols)) return;
    const kept = columnKeeps();
    const shifted = defer
      ? kept.reduce((any, k, c) => m.defer(COLS, c, k) || any, false)
      : m.refilter_all(COLS, kept);
    if (shifted) tell({ type: "Refiltered" }, "cols");
  };

  return {
    rows: {
      count: () => memory().count(ROWS),
      window: (start, end) => (levels.length ? groupedWindow(start, end) : Array.from(memory().window(ROWS, start, end))),
      quickFilter: { regex: true },
      hide: true,
      ...(options.hideCap?.rows === undefined ? {} : { hideCap: options.hideCap.rows }),
      support: (by) => {
        const column = columns[index.get(by) ?? -1];
        const filterable = column?.filterable !== false;
        return {
          sort: column?.sortable !== false,
          text: filterable && { regex: true },
          oneOf: filterable,
          compare: column ? compared(by) : null,
          group: column !== undefined && column.sortable !== false,
        };
      },
      distinct: (by, { search, limit, narrow }) => distinct(by, search, limit, narrow),
      locate: (id) => located(id < 0 ? memory().locate_summary(summaryKey(id)) : memory().locate(ROWS, id)),
      unmatched: (ids) =>
        Array.from(memory().unmatched(ROWS, Uint32Array.from(ids, (id) => (id < 0 ? rows.length : id))), (u) => u === 1),
      move: (ids, to) => move(ROWS, ids, to),
      movable: () => levels.length === 0 && memory().movable(ROWS),
      groupLabel: (id, outline) => {
        const group = groups.get(id);
        if (!group) return "";
        return counted() < 0 ? summaryLabel(outline) : `${summaryLabel(outline)} (${group.count})`;
      },
      grandTotal: -1,
    },
    cols: {
      count: () => memory().count(COLS),
      window: (start, end) => Array.from(memory().window(COLS, start, end), (i) => columns[i]!.key),
      quickFilter: { regex: true },
      hide: true,
      ...(options.hideCap?.cols === undefined ? {} : { hideCap: options.hideCap.cols }),
      support: (by) => ({ sort: true, text: { regex: true }, oneOf: true, compare: comparedAcross(by) }),
      distinct: (by, { search, limit, narrow }) => distinctAcross(by, search, limit, narrow),
      locate: (id) => located(memory().locate(COLS, index.get(id) ?? columns.length)),
      unmatched: (ids) =>
        Array.from(memory().unmatched(COLS, Uint32Array.from(ids, (id) => index.get(id) ?? columns.length)), (u) => u === 1),
      move: (ids, to) =>
        move(
          COLS,
          ids.map((id) => index.get(id)!),
          to,
        ),
      movable: () => memory().movable(COLS),
      label: (id) => columns[index.get(id) ?? -1]?.label ?? id,
    },
    // A quick filter whose pattern is not a valid RegExp leaves the one in
    // force; a condition with one, or on a column that takes none, is left
    // out, as is a sort term on an item the data lacks. The answer's error
    // says what RegExp made of the first pattern it rejected, the rows'
    // before the columns'.
    view(asked: ViewSpec<number, ColId>) {
      const by = asked.rows.sort.filter((c) => index.has(c.id));
      if (by.some((c) => columns[index.get(c.id)!]!.sortable === false)) {
        throw new Error(`binnacle: fromRows cannot sort by ${by.map((c) => c.id).join(", ")}`);
      }
      const byRows = asked.cols.sort.filter((c) => isRow(c.id));
      const forRows = take(filters.rows, asked.rows.filter, takesColumn);
      const forCols = take(filters.cols, asked.cols.filter, takesRow);
      filters = { rows: forRows.filter, cols: forCols.filter };
      const error = forRows.error ?? forCols.error;
      const was = hidden;
      hidden = { rows: pruned(asked.rows.hidden, isRow), cols: pruned(asked.cols.hidden, (id) => index.has(id)) };
      const m = memory();
      // The level columns leave the columns while the rows group by them.
      const wasGrouping = JSON.stringify(grouping);
      levels = [...new Set((asked.grouping?.rowLevels ?? []).flatMap((l) => ("field" in l && groupable(l.field) ? [l.field] : [])))];
      grouping =
        levels.length === 0
          ? null
          : {
              ...asked.grouping!,
              rowLevels: levels.map((field) => asked.grouping!.rowLevels.find((l) => "field" in l && l.field === field)!),
              colLevels: [],
              values: asked.grouping!.values.filter((v) => v.field === null || index.has(v.field)),
              slicers: [],
            };
      const regrouping = JSON.stringify(grouping) !== wasGrouping;
      const hid = {
        rows: m.hide(ROWS, Uint32Array.from(hidden.rows)),
        cols: m.hide(
          COLS,
          Uint32Array.from([...new Set([...hidden.cols, ...levels])], (id) => index.get(id)!),
        ),
      };
      // A quick filter searching the shown items of the other axis asks again when they change.
      const searchesShown = (filter: Filter<unknown>) => filter.quick?.searches === "shown";
      const shownChanged = {
        rows: !sameItems(was.cols, hidden.cols) && searchesShown(filters.rows),
        cols: !sameItems(was.rows, hidden.rows) && searchesShown(filters.cols),
      };
      const sorted = sort(by);
      // Any View Spec, even the one in force, filters the unmatched items.
      const refiltered = forRows.changed || shownChanged.rows || m.has_unmatched(ROWS);
      if (refiltered) refilter();
      const regrouped = regrouping || (levels.length > 0 && (refiltered || hid.rows || sorted));
      if (regrouped) regroup();
      // An axis left in the same order is told nothing.
      if (refiltered || hid.rows || sorted || regrouped) {
        tell({ type: refiltered || hid.rows || regrouped ? "Refiltered" : "Reordered" });
      }
      const resorted = (byRows.length > 0 || colsApplied.length > 0) && sortColumns(byRows);
      const refilteredCols = forCols.changed || shownChanged.cols || hid.cols || m.has_unmatched(COLS);
      if (refilteredCols) refilterColumns();
      if (resorted || refilteredCols) tell({ type: refilteredCols ? "Refiltered" : "Reordered" }, "cols");
      return { ...current(), ...(error === undefined ? {} : { error }) };
    },
    cells: (rowIds, colIds) =>
      rowIds.map((r) => colIds.map((c) => (r < 0 ? (figure(r, c) as R[keyof R]) : rows[r]![c]))),
    find: {
      regex: true,
      marks(rowIds, colIds, query) {
        if (invalid(query.query) !== undefined) return rowIds.map(() => colIds.map(() => false));
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
      if (levels.length) regroup();
      tell({ type: "Refiltered" });
    },
    row: (id) => rows[id]!,
    group: (id) => groups.get(id),
    edited: (id) => update(id, true),
    changed: (id) => update(id, false),
    destroy() {
      wasm?.free();
      wasm = undefined;
    },
  };
}
