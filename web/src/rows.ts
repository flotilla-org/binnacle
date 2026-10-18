// fromRows: the ready-made in-memory Table Source over an array of row
// objects, a typed front over binnacle-memory's In-Memory Table Source in
// the Grid's wasm module. The rows stay in JS: only the Remap lives in wasm,
// and a column's sort keys cross only when it is first sorted (numbers as a
// Float64Array, strings UTF-8 encoded once, so they sort by code point).
//
// Rows are treated as immutable until `changed` says one was edited.
//
// The quick filter and column conditions match in JS (a `RegExp` for a
// regex, lowercased substrings otherwise) against each cell's text as its
// column's renderer gives it, and hand binnacle-memory the rows to keep.
//
// Row IDs are the rows' indices in the array (their Logical Indices); column
// IDs are the property names.

import { byColumn, cellText, plainText, type CellRenderer } from "./renderer";
import type {
  AxisSink,
  ChangeNotice,
  Condition,
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
  /**
   * Shows only the rows `keep` accepts (and the quick filter keeps), in the
   * sort in force; `null` shows every row the quick filter keeps.
   */
  filter(keep: ((row: R) => boolean) | null): void;
  /** The row a row ID names. */
  row(id: number): R;
  /**
   * The row `id` names was changed in place: shows its new values and
   * re-sorts by the sort in force. The filter is not applied again, so a
   * changed row stays shown.
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

function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

type Match = (text: string) => boolean;

export function fromRows<R extends object>(rows: readonly R[], columns: readonly RowsColumn<R>[]): RowsSource<R> {
  type ColId = RowsColumnId<R>;
  const index = new Map<ColId, number>(columns.map((c, i) => [c.key, i]));
  const keys = new Map<ColId, SortKeys>();
  const sinks = new Set<{ rows: AxisSink<number>; cols: AxisSink<ColId> }>();
  let wasm: WebMemory | undefined;
  let applied: SortSpec<ColId> = [];
  let quick: TextQuery<ColId> | null = null;
  let quickMatch: Match | undefined;
  let conditions: { condition: Condition<ColId>; match: Match }[] = [];
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

  /** Hands binnacle-memory the rows the quick filter, every condition and `keep` keep. */
  const refilter = () => {
    const m = memory();
    if (!keep && !quick && !conditions.length) return void m.clear_filter();
    const match = quickMatch;
    // The shown columns are all of them: fromRows hides none.
    const searched = quick?.searches === "shown" ? columns.map((c) => c.key) : (quick?.searches ?? []);
    const named = searched.filter((id) => index.has(id));
    const quickKeeps = (row: R) =>
      !match ||
      named.some((id) => {
        const t = text(row, id);
        return t !== null && match(t);
      });
    // A blank never matches, so `not` keeps it.
    const conditionsKeep = (row: R) =>
      conditions.every(({ condition, match }) => {
        const t = text(row, condition.id);
        return (t !== null && match(t)) !== condition.not;
      });
    m.filter(Uint8Array.from(rows, (r) => (quickKeeps(r) && conditionsKeep(r) && (keep?.(r) ?? true) ? 1 : 0)));
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

  return {
    rows: {
      count: () => memory().count(ROWS),
      window: (start, end) => Array.from(memory().window(ROWS, start, end)),
      quickFilter: { regex: true },
      support: (by) => {
        const column = columns[index.get(by) ?? -1];
        return { sort: column?.sortable !== false, text: column?.filterable === false ? false : { regex: true } };
      },
      locate: (id) => located(memory().locate(ROWS, id)),
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
      const kept = (spec.rows.filter?.conditions ?? []).flatMap((condition) => {
        const match = filterable(condition.id) ? matcher(condition.kind.query) : undefined;
        return match ? [{ condition, match }] : [];
      });
      const conditionsChanged = !same(
        kept.map((k) => k.condition),
        conditions.map((k) => k.condition),
      );
      if (conditionsChanged) conditions = kept;
      sort(by);
      const refiltered = quickChanged || conditionsChanged;
      if (refiltered) refilter();
      tell({ type: refiltered ? "Refiltered" : "Reordered" });
      return {
        rows: { filter: { quick, conditions: conditions.map((k) => k.condition) }, sort: by },
        cols: { filter: { quick: null, conditions: [] }, sort: [] },
        totals: { rows: rows.length, cols: columns.length },
      };
    },
    cells: (rowIds, colIds) => rowIds.map((r) => colIds.map((c) => rows[r]![c])),
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
    changed(id) {
      const at = located(memory().locate(ROWS, id));
      if (at.type === "Found") tell({ type: "ContentChanged", start: at.position, end: at.position + 1 });
      keys.clear();
      if (applied.length) {
        sort(applied);
        tell({ type: "Reordered" });
      }
    },
    destroy() {
      wasm?.free();
      wasm = undefined;
    },
  };
}
