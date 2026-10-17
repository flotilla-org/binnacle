// fromRows: the ready-made in-memory Table Source over an array of row
// objects, a typed front over binnacle-memory's In-Memory Table Source in
// the Grid's wasm module. The rows stay in JS: only the Remap lives in wasm,
// and a column's sort keys cross only when it is first sorted (numbers as a
// Float64Array, strings UTF-8 encoded once, so they sort by code point).
//
// Row IDs are the rows' indices in the array (their Logical Indices); column
// IDs are the property names.

import { byColumn, type CellRenderer } from "./renderer";
import type { AxisSink, ChangeNotice, Located, SortSpec, TableSource } from "./source";
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
    /** How its cells draw; plain text by default. */
    renderer?: CellRenderer<R[K]>;
  };
}[RowsColumnId<R>];

export type RowsColumnId<R> = keyof R & string;

export interface RowsSource<R> extends TableSource<number, RowsColumnId<R>, R[keyof R]> {
  /** Shows only the rows `keep` accepts, in the sort in force; `null` shows every row. */
  filter(keep: ((row: R) => boolean) | null): void;
  /** The row a row ID names. */
  row(id: number): R;
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

export function fromRows<R extends object>(rows: readonly R[], columns: readonly RowsColumn<R>[]): RowsSource<R> {
  type ColId = RowsColumnId<R>;
  const index = new Map<ColId, number>(columns.map((c, i) => [c.key, i]));
  const keys = new Map<ColId, SortKeys>();
  const sinks = new Set<{ rows: AxisSink<number>; cols: AxisSink<ColId> }>();
  let wasm: WebMemory | undefined;
  // Created on first use, once the widget has loaded the wasm module.
  const memory = () => (wasm ??= new (bridge().WebMemory)(rows.length, columns.length));
  const tell = (notice: ChangeNotice) => {
    for (const s of sinks) s.rows.notify(notice);
  };
  const located = ([code, position]: Float64Array): Located =>
    code === 0
      ? { type: "Found", position: position! }
      : { type: "Gone", suggest: Number.isNaN(position) ? null : position! };

  return {
    rows: {
      count: () => memory().count(ROWS),
      window: (start, end) => Array.from(memory().window(ROWS, start, end)),
      sort(spec: SortSpec<ColId>) {
        const m = memory();
        if (spec.some((c) => !index.has(c.id) || columns[index.get(c.id)!]!.sortable === false)) {
          throw new Error(`binnacle: fromRows cannot sort by ${spec.map((c) => c.id).join(", ")}`);
        }
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
        tell({ type: "Reordered" });
        return spec;
      },
      sortable: (by) => columns[index.get(by) ?? -1]?.sortable !== false,
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
    cells: (rowIds, colIds) => rowIds.map((r) => colIds.map((c) => rows[r]![c])),
    renderer: byColumn((id: ColId) => columns[index.get(id) ?? -1]?.renderer as CellRenderer<R[keyof R]> | undefined),
    attach(sink) {
      sinks.add(sink);
      return () => sinks.delete(sink);
    },
    filter(keep) {
      if (keep) memory().filter(Uint8Array.from(rows, (r) => (keep(r) ? 1 : 0)));
      else memory().clear_filter();
      tell({ type: "Reset" });
    },
    row: (id) => rows[id]!,
    destroy() {
      wasm?.free();
      wasm = undefined;
    },
  };
}
