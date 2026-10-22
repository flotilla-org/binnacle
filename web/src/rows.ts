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
// columns while grouped. Leaves move within their group only (on a sorted
// axis the order shown is kept and the sort on the Leaves dropped, unless
// `sortedMoves` refuses it); a drop between groups is refused. Find walks
// the grouped rows as if every group were open, a Group Item's label (its
// level column's text) before its cells, and counts the matches closed
// groups hide; a step into one carries the groups to open. The user's edit
// leaves every row where it is, its group's figures worked out again at
// once, and a Leaf whose level value changed unmatched until the next View
// Spec groups the rows again. Each column offers the aggregates it can take
// (`count` and `distinct count` of any, the rest of numeric ones); a value
// asking for another is left out, listed as not applied.
//
// Slicers filter the rows before they group, on any column, shown or not,
// grouped or not; with slicers and no levels the table is plain, sliced.
// While grouped, a comparison on a column a value aggregates leaves out the
// groups whose figure (the first such value's) fails it, with everything
// under them, rather than the rows; the other conditions filter the rows.
//
// Column levels, or the values pseudo-level on the rows, make a pivot: the
// columns come from the data too, each pivot column by a negative column ID
// stable for the source's life (the column Grand Total's is -1), and a cell
// holds the figure where its row's and its column's groups cross. Each
// axis's innermost groups are its items, so no row of the data shows. The
// columns are told every change as `Refiltered`, don't move, and order by
// value ascending, or by the Grand Total row as the columns' sort asks.

import { GRAND_TOTAL, NOTICE_CODE, VALUES_FIELD } from "./codes";
import { comparedBy, comparedOf, passes, type Compared } from "./compare";
import { byColumn, cellText, plainText, type CellRenderer } from "./renderer";
import { textRenderer } from "./renderers/text";
import type {
  Aggregate,
  AggregateValue,
  AppliedView,
  ChangeNotice,
  Condition,
  Distinct,
  FilterSpec,
  FindMethods,
  FindQuery,
  Grouping,
  Level,
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
import { summaryLabel, valueName } from "./source";
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

/**
 * A column ID of `fromRows`: a property of the rows, or while it pivots a
 * pivot column's, a negative number as a Summary Item's row ID is.
 */
export type RowsColId<R> = RowsColumnId<R> | number;

export interface RowsOptions {
  /**
   * What a move on a sorted axis does: `"bake"` (the default) keeps the
   * order shown, places the moved items and drops the axis's sort;
   * `"refuse"` refuses it, and the axis is not movable while sorted.
   */
  sortedMoves?: "bake" | "refuse";
  /** The most rows and columns it hides at once, if fewer than the widget's own cap. */
  hideCap?: { rows?: number; cols?: number };
  /**
   * Whether it pivots, for column levels or the values pseudo-level on the
   * rows; `false` by default, leaving both out. Its column IDs then take in
   * the pivot columns' negative numbers.
   */
  pivots?: boolean;
}

/** A `fromRows` source; its column IDs are `RowsColId<R>` if it pivots. */
export interface RowsSource<R, ColId extends RowsColId<R> = RowsColumnId<R>> extends TableSource<number, ColId, R[keyof R]> {
  /** Find over each cell's text as its column's renderer gives it, regexes included; nothing while it pivots. */
  find: FindMethods<number, ColId>;
  /**
   * Shows only the rows `keep` accepts (and the quick filter keeps), in the
   * sort in force; `null` shows every row the quick filter keeps.
   */
  filter(keep: ((row: R) => boolean) | null): void;
  /** The row a Leaf's row ID names. */
  row(id: number): R;
  /** The Summary Item a negative row ID names, while it is in view; `undefined` for a Leaf. */
  group(id: number): RowsGroup<RowsColumnId<R>> | undefined;
  /** The pivot column a negative column ID names, while it is in view; `undefined` for a column of the rows. */
  pivotColumn(id: number): RowsGroup<RowsColumnId<R>> | undefined;
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
 * A Summary Item of the grouped rows, or a pivot column: its kind; its
 * level column and cell text (`null` for blanks), or `null` for the Grand
 * Total; how many rows it holds; and its aggregates, one per value of the
 * grouping (`null` for a blank). A group of the values pseudo-level, and a
 * Grand Total split by value, has the field `null` and the value's name.
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
  /** The index of the value it stands for, if one does. */
  value: number | null;
}

/** An item of the grouped rows as `WebMemory.grouped_items` gives it, every group open. */
interface FoundItem {
  key: bigint;
  item: number | null;
  hidden: boolean;
  ancestors: bigint[];
  field: bigint | null;
  label: string | null;
}

/** A match of grouped find: its place in the query's order, its cell, whether a closed group hides it, and the groups above it. */
interface GroupedMatch<ColId> {
  place: number;
  row: number;
  col: ColId;
  hidden: boolean;
  ancestors: number[];
}

/** A Summary Item as `WebMemory.summary` reads it, by its Key: its group's field (a column index) and value. */
interface SummaryOf {
  kind: "group" | "subtotal" | "grandTotal";
  group: { field: bigint; value: string | null } | null;
  aggregates: (number | null)[];
  children: number;
  value: number | null;
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

/** A count's cells in a pivot: a number, at the end. */
const COUNTED = textRenderer<number | null>({ align: "end" });

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

/** The aggregates any column takes, and those a column of numbers (or dates) takes too. */
const COUNTS: readonly Aggregate[] = [{ name: "count" }, { name: "distinct count" }];
const NUMBERS: readonly Aggregate[] = [...COUNTS, ...["sum", "average", "minimum", "maximum"].map((name) => ({ name }))];

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

/**
 * How a condition tests a cell's text (`null` for no value) or value, before
 * `not`; `undefined` if its pattern is not a valid `RegExp`, or it is a
 * Native condition, which no query language here reads.
 */
function tester(kind: Condition<unknown>["kind"]): Test | undefined {
  if (kind.type === "Native") return undefined;
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
  options: RowsOptions & { pivots: true },
): RowsSource<R, RowsColId<R>>;
export function fromRows<R extends object>(
  rows: readonly R[],
  columns: readonly RowsColumn<R>[],
  options?: RowsOptions,
): RowsSource<R>;
export function fromRows<R extends object>(
  rows: readonly R[],
  columns: readonly RowsColumn<R>[],
  options: RowsOptions = {},
): RowsSource<R, RowsColId<R>> | RowsSource<R> {
  type ColId = RowsColumnId<R>;
  type Col = RowsColId<R>;
  const index = new Map<Col, number>(columns.map((c, i) => [c.key, i]));
  /** Whether `id` names a column of the rows, not a pivot column. */
  const isBase = (id: Col): id is ColId => typeof id === "string" && index.has(id);
  const keys = new Map<ColId, SortKeys>();
  /** Each row's values across the columns as sort keys, by row ID, for sorting the columns. */
  const rowKeys = new Map<number, SortKeys>();
  const sinks = new Set<TableSink<number, Col>>();
  let wasm: WebMemory | undefined;
  let applied: SortSpec<ColId> = [];
  let colsApplied: SortSpec<number> = [];
  let filters: { rows: Filter<Col>; cols: Filter<number> } = { rows: NO_FILTER, cols: NO_FILTER };
  let hidden: { rows: number[]; cols: ColId[] } = { rows: [], cols: [] };
  let keep: ((row: R) => boolean) | null = null;
  /** Each column's kind of value to compare, worked out on first use and again after a change. */
  const kinds = new Map<ColId, Compared | null>();
  /** The columns the rows group by, outermost first, and the grouping in force. */
  let levels: ColId[] = [];
  let grouping: Grouping<ColId, number, Col> | null = null;
  /** Whether the grouping in force pivots, and the columns' sort while it does: by the Grand Total row alone. */
  let pivot = false;
  let pivotSort: SortSpec<number> = [];
  /** Whether the rows go through the grouping layer: grouped, or a pivot's. */
  const grouped = () => levels.length > 0 || pivot;
  /** The slicers in force, each with its test. */
  let slicers: Filter<Col>["conditions"] = [];
  /** The Summary Items seen in grouped Windows since the rows last grouped, by row ID, and the pivot columns by column ID. */
  const groups = new Map<number, RowsGroup<ColId>>();
  const pivotColumns = new Map<number, RowsGroup<ColId>>();
  /** Each pivot column's value, by column ID, for its renderer. */
  const columnValues = new Map<number, number>();
  /** Whether the user edited a row since the rows last grouped: the next View Spec groups them again. */
  let stale = false;
  const groupable = (id: Col) => isBase(id) && columns[index.get(id)!]!.sortable !== false;
  const filterable = (id: Col) => isBase(id) && columns[index.get(id)!]!.filterable !== false;
  const compared = (id: Col): Compared | null => {
    if (!filterable(id) || !isBase(id)) return null;
    let kind = kinds.get(id);
    if (kind === undefined) kinds.set(id, (kind = comparedOf(rows.map((r) => r[id]))));
    return kind;
  };
  /** What the columns compare by row `id`: the kind its values in the filterable columns share. */
  const comparedAcross = (id: number): Compared | null =>
    isRow(id) ? comparedOf(columns.filter((c) => c.filterable !== false).map((c) => rows[id]![c.key])) : null;
  const takesColumn: Takes<Col> = Object.assign((id: Col) => filterable(id), { compared });
  /** The aggregates column `id` takes: every one of a column of numbers or dates. */
  const aggregatesOf = (id: ColId): readonly Aggregate[] => {
    if (!index.has(id)) return [];
    return rows.every((r) => r[id] == null || numeric(r[id]) !== undefined) ? NUMBERS : COUNTS;
  };
  /** Whether a condition of the rows acts on the groups, after aggregating: a comparison on a column a value aggregates, while grouped. */
  const later = (c: Condition<Col>) =>
    levels.length > 0 && c.kind.type === "Compare" && (grouping?.values.some((v) => v.field === c.id) ?? false);
  /** The filter the rows themselves pass: the rows' conditions but those acting later, then the slicers. */
  const leafFilter = (): Filter<Col> => ({
    ...filters.rows,
    conditions: [...filters.rows.conditions.filter((k) => !later(k.condition)), ...slicers],
  });
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

  /** A cell's text, as its column's renderer gives it; `null` for no value or a pivot column. */
  const text = (row: R, id: Col): string | null => {
    if (!isBase(id)) return null;
    const value = row[id];
    if (value == null) return null;
    const renderer = (columns[index.get(id)!]!.renderer ?? plainText) as CellRenderer<unknown>;
    return cellText(renderer, value);
  };

  /** Whether a row passes `filter` and `keep`. */
  const rowKeeper = (filter: Filter<Col>) => {
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
        (id) => (isBase(id) ? row[id] : undefined),
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
  const hitsFor = (find: FindQuery<Col>): Uint8Array => {
    const id = JSON.stringify(find.query);
    if (hits?.query === id) return hits.cells;
    const error = invalid(find.query);
    if (error !== undefined) throw new Error(error);
    const match = matcher(find.query);
    const n = columns.length;
    const cells = new Uint8Array(rows.length * n);
    const { searches } = find.query;
    const searched = searches === "shown" ? columns.map((c) => c.key) : searches.filter(isBase);
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

  /**
   * Every match in the grouped rows, in the query's order as if every
   * group were open (by rows, each item's label before its cells; by
   * columns, the labels first), and the place of the cell `from`.
   */
  const groupedMatches = (query: FindQuery<Col>, from: { row: number; col: Col } | null) => {
    const cells = hitsFor(query);
    const match = matcher(query.query);
    const n = columns.length;
    const items = memory().grouped_items() as FoundItem[];
    const { colsAt } = shown();
    const slots = colsAt.length + 1;
    const place = (i: number, slot: number) => (query.by === "rows" ? i * slots + slot : slot * items.length + i);
    const found: GroupedMatch<ColId>[] = [];
    items.forEach((item, i) => {
      const at = { hidden: item.hidden, ancestors: item.ancestors.map(summaryId) };
      if (item.field !== null && item.label !== null && match?.(item.label)) {
        found.push({ place: place(i, 0), row: summaryId(item.key), col: columns[Number(item.field)]!.key, ...at });
      }
      if (item.item === null) return;
      colsAt.forEach((c, j) => {
        if (cells[item.item! * n + c] === 1) found.push({ place: place(i, j + 1), row: item.item!, col: columns[c]!.key, ...at });
      });
    });
    found.sort((a, b) => a.place - b.place);
    let start: number | null = null;
    if (from) {
      const key = from.row < 0 ? summaryKey(from.row) : BigInt(from.row);
      const i = items.findIndex((item) => item.key === key);
      if (i >= 0) start = place(i, colsAt.indexOf(index.get(from.col) ?? -1) + 1);
    }
    return { found, start };
  };

  /** Hands binnacle-memory the rows the rows' filter and `keep` keep. */
  const refilter = () => {
    const m = memory();
    const filter = leafFilter();
    if (!keep && !filtering(filter)) return void m.clear_filter(ROWS);
    const keeps = rowKeeper(filter);
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
  const distinct = (id: ColId, search: string, limit: number, narrow: FilterSpec<Col>): Distinct => {
    const narrowed = narrowing(narrow, takesColumn);
    const keeps = rowKeeper({ ...narrowed, conditions: [...narrowed.conditions, ...slicers] });
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

  /** The base fields of `all`, leaving out the values pseudo-level. */
  const fieldsOf = (all: Level<ColId>[]) => all.flatMap((l) => ("field" in l ? [l.field] : []));

  /**
   * Groups the rows in view as the grouping in force says, again, or
   * ungroups them if it has no levels: stages each column it reads, as cell
   * texts and as numbers, and the user's sort orders the groups.
   */
  const regroup = () => {
    const m = memory();
    groups.clear();
    pivotColumns.clear();
    columnValues.clear();
    stale = false;
    if (!grouped() || !grouping) return void (m.grouped() && m.ungroup());
    stage(grouping);
    const column = (id: Col) => BigInt(index.get(id)!);
    const level = (l: Level<ColId>) => ("field" in l ? { ...l, field: column(l.field) } : l);
    const wire = {
      ...grouping,
      rowLevels: grouping.rowLevels.map(level),
      colLevels: grouping.colLevels.map(level),
      values: grouping.values.map((v) => ({ ...v, field: v.field === null ? null : column(v.field) })),
      rowAxis: { ...grouping.rowAxis, expand: { ...grouping.rowAxis.expand, except: groupKeys(grouping) } },
      colAxis: { ...grouping.colAxis, expand: { ...grouping.colAxis.expand, except: [] } },
    };
    const having = filters.rows.conditions
      .filter((k) => later(k.condition))
      .map(({ condition: c }) => ({ key: column(c.id), kind: c.kind, not: c.not }));
    m.group(
      { ...wire, slicers: [] },
      Uint32Array.from(applied, (c) => index.get(c.id)!),
      Uint8Array.from(applied, (c) => DIRECTION[c.direction]),
      having,
      BigUint64Array.from(pivotSort, () => GRAND_TOTAL),
      Uint8Array.from(pivotSort, (c) => DIRECTION[c.direction]),
    );
    // The Group Items whose paths no longer fit the levels are pruned.
    const except = Array.from(m.except(), summaryId);
    grouping = { ...grouping, rowAxis: { ...grouping.rowAxis, expand: { ...grouping.rowAxis.expand, except } } };
  };

  /** Stages each column `g` reads, as cell texts and as numbers. */
  const stage = (g: Grouping<ColId, number, Col>) => {
    const m = memory();
    const read = new Set([
      ...fieldsOf(g.rowLevels),
      ...fieldsOf(g.colLevels),
      ...g.values.flatMap((v) => (v.field === null ? [] : [v.field])),
    ]);
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
  };

  /** The Keys of the Group Items in the rows' `expand.except`. */
  const groupKeys = (g: Grouping<ColId, number, Col>) =>
    g.rowAxis.expand.except.flatMap((id) => (id < 0 ? [summaryKey(id)] : []));

  /** Opens and closes the groups as the grouping in force says, over the rows as they are grouped. */
  const reopen = (): ChangeNotice | null => {
    const { depth } = grouping!.rowAxis.expand;
    const [code, at, n] = memory().expand(depth === "all" ? -1 : depth, BigUint64Array.from(groupKeys(grouping!)));
    if (code === undefined) return null;
    if (code === NOTICE_CODE.Collapsed) return { type: "Collapsed", at: at!, n: n! };
    if (code === NOTICE_CODE.Expanded) return { type: "Expanded", at: at!, n: n! };
    return { type: "Refiltered" };
  };

  /**
   * After a regroup, told after its Refiltered: the Summary Items keep their
   * row IDs, but their figures may have changed with the rows under them.
   * The range reaches past any count, so a pinned Grand Total at its old
   * Position is in it too.
   */
  const refigured = () => tell({ type: "ContentChanged", start: 0, end: Number.MAX_SAFE_INTEGER });

  /** The index of the value that counts the rows, if the grouping has one. */
  const counted = () => grouping?.values.findIndex((v) => v.field === null && v.aggregate === "count") ?? -1;

  /** A column's header text, its key if it has none. */
  const columnLabel = (id: Col) => (isBase(id) ? (columns[index.get(id)!]!.label ?? id) : "");

  /** The name of the grouping's value at `i`, as the caption names one: `sum(weight)`. */
  const valueLabel = (i: number | null): string | null => {
    const value = i === null ? undefined : grouping?.values[i];
    return value ? valueName(value, columnLabel) : null;
  };

  /**
   * The group a Summary Item's outline names, by the source's IDs: a level
   * column and its cell text, or for the values pseudo-level `null` and the
   * value's name (`value` its index; without one, the name as given).
   */
  const groupOf = (group: { field: bigint | null; value: ListedValue }, value: number | null) =>
    group.field === null || group.field === VALUES_FIELD
      ? { field: null, value: value === null ? group.value : valueLabel(value) }
      : { field: columns[Number(group.field)]!.key, value: group.value };

  /** The items of grouped `axis` at Positions `start..end`, each Summary Item by its negative ID. */
  const groupedWindow = (axis: typeof ROWS | typeof COLS, start: number, end: number) => {
    const items = memory().grouped_window(axis, start, end) as GroupedItem[];
    const id = (item: GroupedItem) => item.item ?? summaryId(item.key);
    const outlines = items.map((item): Outline<number, ColId> => {
      const o = item.outline;
      const group = o.group ? groupOf(o.group, item.value) : null;
      if (o.kind !== "leaf" && item.aggregates) {
        const count = item.aggregates[counted()] ?? item.children ?? 0;
        const summary = { kind: o.kind, field: group?.field ?? null, value: group?.value ?? null };
        (axis === ROWS ? groups : pivotColumns).set(id(item), { ...summary, count, aggregates: item.aggregates });
      }
      if (axis === COLS && item.value !== null) columnValues.set(id(item), item.value);
      const path = (o.path ?? []).map((g) => groupOf(g, null));
      return { kind: o.kind, depth: o.depth, open: o.open, ancestors: o.ancestors.map(summaryId), group, span: o.span ?? 0, path, set: o.set ?? null };
    });
    return { ids: items.map(id), outlines };
  };

  /** A Summary Item of `axis` as last listed, else read where it sits now (a pinned Grand Total out of view). */
  const summaryOf = (id: number, axis: typeof ROWS | typeof COLS = ROWS): RowsGroup<ColId> | undefined => {
    const seen = axis === ROWS ? groups : pivotColumns;
    if (!seen.has(id) && memory().grouped()) {
      // A group in header form has no Position, but the layer knows it.
      const known = memory().summary(summaryKey(id)) as SummaryOf | undefined;
      if (known) {
        const group = known.group && groupOf(known.group, known.value);
        const count = known.aggregates[counted()] ?? known.children;
        const value = group?.value ?? null;
        seen.set(id, { kind: known.kind, field: group?.field ?? null, value, count, aggregates: known.aggregates });
        return seen.get(id);
      }
      const [code, position] = memory().locate_summary(axis, summaryKey(id));
      if (code === 0) groupedWindow(axis, position!, position! + 1);
    }
    return seen.get(id);
  };

  /** A Summary Item's figure in column `col`: that of the first value aggregating it. */
  const figure = (id: number, col: Col): number | null => {
    const at = grouping?.values.findIndex((v) => v.field === col) ?? -1;
    return at < 0 ? null : (summaryOf(id)?.aggregates[at] ?? null);
  };

  /** The figure where a pivot's row and column cross. */
  const crossed = (row: number, col: number): number | null =>
    row < 0 && pivot ? (memory().cell(summaryKey(row), summaryKey(col)) ?? null) : null;

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
  const current = (): AppliedView<number, Col> => ({
    rows: { filter: spec(filters.rows), sort: applied, hidden: hidden.rows },
    cols: { filter: spec(filters.cols), sort: pivot ? pivotSort : colsApplied, hidden: hidden.cols },
    totals: pivot ? { rows: rows.length } : { rows: rows.length, cols: columns.length },
    grouping,
  });

  /**
   * Moves the items with these Logical Indices on `axis`. Baking a sort
   * drops it, which the sinks hear before the widget gets the answer.
   */
  const move = (axis: typeof ROWS | typeof COLS, items: number[], to: number): boolean | { refused: string } => {
    const sorted = (axis === ROWS ? applied : colsApplied).length > 0;
    if (axis === ROWS && grouped()) return moveGrouped(items, to);
    if (axis === COLS && pivot) return { refused: "Can't move pivot columns: they follow the column sort" };
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

  /** The user's sort on the Leaves: its columns the rows don't group by. */
  const leafSort = () => applied.filter((c) => !levels.includes(c.id));

  /**
   * Moves grouped Leaves within their group. On Leaves sorted by a column
   * the rows don't group by, the order shown is kept and that sort dropped,
   * which the sinks hear before the widget gets the answer.
   */
  const moveGrouped = (ids: number[], to: number): boolean | { refused: string } => {
    const sorted = leafSort().length > 0;
    const keys = BigUint64Array.from(ids, (id) => (id < 0 ? summaryKey(id) : BigInt(id)));
    if (sorted && options.sortedMoves === "refuse" && !ids.some((id) => id < 0)) {
      return { refused: "Can't move while sorted: clear the sort first" };
    }
    const code = memory().move_grouped(keys, to);
    if (code === 1) return { refused: "Can't move between groups" };
    if (code === 2) return { refused: "Group rows and totals can't be moved" };
    if (!sorted) return true;
    applied = applied.filter((c) => levels.includes(c.id));
    regroup();
    const view = current();
    for (const s of sinks) s.viewChanged(view);
    return true;
  };

  /**
   * The user edited row `id` while the rows are grouped: every row stays
   * where it is, the groups' figures are worked out again at once, and the
   * next View Spec groups the rows again.
   */
  const editGrouped = (id: number) => {
    const m = memory();
    const at = located(m.locate(ROWS, id));
    if (at.type === "Found") tell({ type: "ContentChanged", start: at.position, end: at.position + 1 });
    keys.clear();
    kinds.clear();
    rowKeys.delete(id);
    hits = undefined;
    if (keep || filtering(filters.rows)) m.defer(ROWS, id, rowKeeper(filters.rows)(rows[id]!) ? 1 : 0);
    stage(grouping!);
    const changed = m.refigure();
    groups.clear();
    for (let i = 0; i < changed.length; i += 2) tell({ type: "ContentChanged", start: changed[i]!, end: changed[i + 1]! });
    stale = true;
  };

  /**
   * The row `id` changed in place: told at its old Position, re-sorted, and
   * filtered again with the columns, at once or (with `defer`, for the
   * user's edit) only those out of view.
   */
  const update = (id: number, defer: boolean) => {
    if (defer && levels.length > 0) return editGrouped(id);
    const m = memory();
    const at = located(m.locate(ROWS, id));
    if (at.type === "Found") tell({ type: "ContentChanged", start: at.position, end: at.position + 1 });
    keys.clear();
    kinds.clear();
    rowKeys.delete(id);
    hits = undefined;
    let moved = false;
    if (keep || filtering(leafFilter())) {
      const kept = rowKeeper(leafFilter())(rows[id]!) ? 1 : 0;
      moved = defer ? m.defer(ROWS, id, kept) : m.refilter(ROWS, id, kept);
    }
    if (applied.length || levels.length) sort(applied);
    if (grouped()) {
      regroup();
      moved = true;
    }
    if (moved) tell({ type: "Refiltered" });
    else if (applied.length) tell({ type: "Reordered" });
    if (grouped()) refigured();
    // A pivot's columns may come or go with any row.
    if (pivot) return tell({ type: "Refiltered" }, "cols");
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

  /**
   * The levels of `asked` the source groups by: each column it can group
   * by, once, and the values pseudo-level.
   */
  const levelsOf = (asked: Level<Col>[] = []): Level<ColId>[] => {
    const seen = new Set<Col>();
    return asked.flatMap((l): Level<ColId>[] => {
      if ("values" in l) return [l];
      if (!isBase(l.field) || !groupable(l.field) || seen.has(l.field)) return [];
      seen.add(l.field);
      return [{ field: l.field, subtotal: l.subtotal }];
    });
  };

  /** How a pivot column's cells draw: as the field its value aggregates does, or a count as a number. */
  const pivotRenderer = (id: number) => {
    const value = grouping?.values[columnValues.get(id) ?? 0];
    if (!value) return undefined;
    if (value.field === null) return COUNTED as CellRenderer<R[keyof R]>;
    return columns[index.get(value.field)!]!.renderer as CellRenderer<R[keyof R]> | undefined;
  };

  const source: RowsSource<R, RowsColId<R>> = {
    rows: {
      count: () => memory().count(ROWS),
      window: (start, end) => (grouped() ? groupedWindow(ROWS, start, end) : Array.from(memory().window(ROWS, start, end))),
      quickFilter: { regex: true },
      hide: true,
      ...(options.hideCap?.rows === undefined ? {} : { hideCap: options.hideCap.rows }),
      support: (by) => {
        const column = isBase(by) ? columns[index.get(by)!] : undefined;
        const filterable = column?.filterable !== false;
        const group = column !== undefined && column.sortable !== false;
        return {
          sort: column?.sortable !== false,
          text: filterable && { regex: true },
          oneOf: filterable,
          compare: column ? compared(by) : null,
          group,
          pivot: group && options.pivots === true,
          subtotals: { before: true, after: true },
          aggregates: isBase(by) ? [...aggregatesOf(by)] : [],
        };
      },
      distinct: (by, { search, limit, narrow }) =>
        isBase(by) ? distinct(by, search, limit, narrow) : { values: [], partial: false, narrowed: true },
      locate: (id) => located(id < 0 ? memory().locate_summary(ROWS, summaryKey(id)) : memory().locate(ROWS, id)),
      unmatched: (ids) =>
        Array.from(memory().unmatched(ROWS, Uint32Array.from(ids, (id) => (id < 0 ? rows.length : id))), (u) => u === 1),
      move: (ids, to) => move(ROWS, ids, to),
      movable: () =>
        !pivot && (levels.length === 0 ? memory().movable(ROWS) : options.sortedMoves !== "refuse" || leafSort().length === 0),
      spanLabel: (id) => {
        const group = summaryOf(id);
        if (!group) return "";
        const label = group.value ?? "(blank)";
        return counted() < 0 ? label : `${label} (${group.count})`;
      },
      groupLabel: (id, outline) => {
        const group = summaryOf(id);
        if (!group) return "";
        const label = summaryLabel({ ...outline, group: outline.group ? { ...outline.group, value: group.value } : null });
        return counted() < 0 ? label : `${label} (${group.count})`;
      },
      grandTotal: -1,
    },
    cols: {
      count: () => memory().count(COLS),
      window: (start, end) =>
        pivot ? groupedWindow(COLS, start, end) : Array.from(memory().window(COLS, start, end), (i) => columns[i]!.key),
      quickFilter: { regex: true },
      hide: true,
      ...(options.hideCap?.cols === undefined ? {} : { hideCap: options.hideCap.cols }),
      support: (by) => ({ sort: true, text: { regex: true }, oneOf: true, compare: comparedAcross(by) }),
      distinct: (by, { search, limit, narrow }) => distinctAcross(by, search, limit, narrow),
      locate: (id) =>
        located(
          typeof id === "number"
            ? memory().locate_summary(COLS, summaryKey(id))
            : memory().locate(COLS, index.get(id) ?? columns.length),
        ),
      unmatched: (ids) =>
        Array.from(memory().unmatched(COLS, Uint32Array.from(ids, (id) => index.get(id) ?? columns.length)), (u) => u === 1),
      move: (ids, to) =>
        move(
          COLS,
          ids.map((id) => index.get(id) ?? columns.length),
          to,
        ),
      movable: () => !pivot && memory().movable(COLS),
      label: (id) => String(columnLabel(id)),
      // A Grand Total split by value is "All" for that value.
      groupLabel: (id, outline) => {
        const known = typeof id === "number" ? summaryOf(id, COLS) : undefined;
        const value = known?.value ?? outline.group?.value ?? null;
        if (outline.kind === "grandTotal") return value === null ? "All" : `All: ${value}`;
        return summaryLabel({ ...outline, group: outline.group ? { ...outline.group, value } : null });
      },
      spanLabel: (id) => (typeof id === "number" ? summaryOf(id, COLS)?.value : undefined) ?? "(blank)",
      grandTotal: -1,
    },
    // A quick filter whose pattern is not a valid RegExp leaves the one in
    // force; a condition with one, or on a column that takes none, is left
    // out, as is a sort term on an item the data lacks. The answer's error
    // says what RegExp made of the first pattern it rejected, the rows'
    // before the columns'.
    view(asked: ViewSpec<number, Col>) {
      const by = asked.rows.sort.filter((c): c is { id: ColId; direction: typeof c.direction } => isBase(c.id));
      if (by.some((c) => columns[index.get(c.id)!]!.sortable === false)) {
        throw new Error(`binnacle: fromRows cannot sort by ${by.map((c) => c.id).join(", ")}`);
      }
      const byRows = asked.cols.sort.filter((c) => isRow(c.id));
      const leavesBefore = JSON.stringify(spec(leafFilter()));
      const forRows = take(filters.rows, asked.rows.filter, takesColumn);
      const forCols = take(filters.cols, asked.cols.filter, takesRow);
      filters = { rows: forRows.filter, cols: forCols.filter };
      const error = forRows.error ?? forCols.error;
      const was = hidden;
      hidden = { rows: pruned(asked.rows.hidden, isRow), cols: [...new Set(asked.cols.hidden.filter(isBase))] };
      const m = memory();
      // The level columns leave the columns while the rows group by them.
      const before = grouping;
      const wasPivot = pivot;
      const sortedBefore = pivotSort;
      const rowLevels = levelsOf(asked.grouping?.rowLevels).filter((l) => options.pivots === true || "field" in l);
      const colLevels = options.pivots === true ? levelsOf(asked.grouping?.colLevels) : [];
      levels = fieldsOf(rowLevels);
      pivot = options.pivots === true && (colLevels.length > 0 || rowLevels.some((l) => "values" in l));
      pivotSort = pivot ? asked.cols.sort.filter((c) => c.id === -1) : [];
      slicers = testable(asked.grouping?.slicers ?? [], takesColumn);
      const offered = (v: AggregateValue<Col>): v is AggregateValue<ColId> =>
        v.field === null
          ? v.aggregate === "count"
          : isBase(v.field) && aggregatesOf(v.field).some((a) => a.name === v.aggregate);
      grouping =
        !grouped() && slicers.length === 0
          ? null
          : {
              ...asked.grouping!,
              rowLevels,
              colLevels,
              values: asked.grouping!.values.filter(offered),
              slicers: slicers.map((k) => k.condition as Condition<ColId>),
            };
      // Opening and closing groups regroups nothing.
      const shape = (g: typeof grouping) => JSON.stringify(g && { ...g, rowAxis: { ...g.rowAxis, expand: null } });
      const regrouping = shape(grouping) !== shape(before) || !same(pivotSort, sortedBefore);
      const reopening = JSON.stringify(grouping?.rowAxis.expand) !== JSON.stringify(before?.rowAxis.expand);
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
      const leavesChanged = JSON.stringify(spec(leafFilter())) !== leavesBefore;
      const refiltered = forRows.changed || leavesChanged || shownChanged.rows || m.has_unmatched(ROWS);
      if (refiltered) refilter();
      const regrouped = regrouping || (grouped() && (refiltered || hid.rows || sorted || stale));
      if (regrouped) regroup();
      const reopened = !regrouped && reopening && grouped() ? reopen() : null;
      // An axis left in the same order is told nothing.
      if (refiltered || hid.rows || sorted || regrouped) {
        tell({ type: refiltered || hid.rows || regrouped ? "Refiltered" : "Reordered" });
        if (regrouped && grouped()) refigured();
      } else if (reopened) tell(reopened);
      const resorted = (byRows.length > 0 || colsApplied.length > 0) && sortColumns(byRows);
      const refilteredCols = forCols.changed || shownChanged.cols || hid.cols || m.has_unmatched(COLS);
      if (refilteredCols) refilterColumns();
      // A pivot's columns change only as a whole: never moved or removed one by one.
      if (pivot || wasPivot) {
        if (pivot !== wasPivot || (pivot && regrouped)) tell({ type: "Refiltered" }, "cols");
      } else if (resorted || refilteredCols) tell({ type: refilteredCols ? "Refiltered" : "Reordered" }, "cols");
      return { ...current(), ...(error === undefined ? {} : { error }) };
    },
    cells: (rowIds, colIds) =>
      rowIds.map((r) =>
        colIds.map((c) => {
          if (!isBase(c)) return (typeof c === "number" ? crossed(r, c) : null) as R[keyof R];
          return r < 0 ? (figure(r, c) as R[keyof R]) : rows[r]![c];
        }),
      ),
    find: {
      regex: true,
      marks(rowIds, colIds, query) {
        if (pivot || invalid(query.query) !== undefined) return rowIds.map(() => colIds.map(() => false));
        const cells = hitsFor(query);
        const n = columns.length;
        return rowIds.map((r) => colIds.map((c) => isBase(c) && cells[r * n + index.get(c)!] === 1));
      },
      next(from, direction, query) {
        if (pivot) return null;
        if (levels.length > 0) {
          const { found, start } = groupedMatches(query, from);
          if (found.length === 0) return null;
          const forward = direction === "next";
          // The next match past `start` either way, if one is.
          const past = (f: GroupedMatch<ColId>) => start !== null && (forward ? f.place > start : f.place < start);
          let i = forward ? found.findIndex(past) : found.map(past).lastIndexOf(true);
          const wrapped = start !== null && i < 0;
          if (i < 0) i = forward ? 0 : found.length - 1;
          const f = found[i]!;
          return { row: f.row, col: f.col, index: i, wrapped, ancestors: { rows: f.ancestors } };
        }
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
        if (pivot) return { total: 0 };
        if (levels.length > 0) {
          const { found } = groupedMatches(query, null);
          return { total: found.length, hidden: found.filter((f) => f.hidden).length };
        }
        const cells = hitsFor(query);
        const n = columns.length;
        const { rowsAt, colsAt } = shown();
        let total = 0;
        for (const r of rowsAt) for (const c of colsAt) total += cells[r * n + c]!;
        return { total };
      },
    },
    renderer: byColumn((id: Col) =>
      typeof id === "number"
        ? pivotRenderer(id)
        : (columns[index.get(id) ?? -1]?.renderer as CellRenderer<R[keyof R]> | undefined),
    ),
    attach(sink) {
      sinks.add(sink);
      return () => sinks.delete(sink);
    },
    filter(next) {
      keep = next;
      refilter();
      if (grouped()) regroup();
      tell({ type: "Refiltered" });
      if (grouped()) refigured();
      if (pivot) tell({ type: "Refiltered" }, "cols");
    },
    row: (id) => rows[id]!,
    group: (id) => summaryOf(id),
    pivotColumn: (id) => (pivot ? summaryOf(id, COLS) : undefined),
    edited: (id) => update(id, true),
    changed: (id) => update(id, false),
    destroy() {
      wasm?.free();
      wasm = undefined;
    },
  };
  return source;
}
