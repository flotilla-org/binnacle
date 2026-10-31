// A Table Source over a Described Table (#304): the source itself runs in
// wasm (`WebDescribed`), sans-IO. The host fetches the watch it asks for
// and feeds back the bytes it reads; the source answers the widget as an
// In-Memory Table Source over the rows. Rows are named by the source's row
// Keys, columns by field name, as a Default View names them.

import type { WebDescribed } from "../wasm/binnacle_wasm.js";
import { AXIS_CODE, NOTICE_CODE, type AxisName } from "./codes";
import type { TypedValue, Zone } from "./format";
import type { Layout } from "./layout";
import { byColumn, type CellRenderer } from "./renderer";
import { valueRenderer } from "./renderers/value";
import type { KeyedCondition, KeyedTextQuery, KeyedViewSpec } from "./snapshot";
import type {
  AppliedAxisView,
  AppliedView,
  ChangeNotice,
  Condition,
  Distinct,
  FindCount,
  FindQuery,
  FindStep,
  Located,
  Support,
  TableSink,
  TableSource,
  TextQuery,
  Totals,
  ViewSpec,
} from "./source";
import { bridge } from "./wasm";

/** What a Described Table's rows are read within: a namespace, which every request needs, and fixed `where` clauses. */
export interface DescribedScope {
  namespace: string;
  wheres?: string[];
}

export interface DescribedOptions {
  /** The zone its dates draw in, and filters and find match them in; the runtime's local zone by default. */
  zone?: Zone;
}

/** The watch a Described Table asks its host to open: `GET {target}`. */
export interface DescribedRequest {
  table: string;
  path: string;
  query: string;
  target: string;
  limit: number;
}

/** One column of a Described Table, as its field describes it. */
export interface DescribedColumn {
  key: bigint;
  name: string;
  title: string | null;
  /** What its header shows: the title, else the name. */
  header: string;
  /** The field's type as the schema names it (`string`, `integer`, `object`, ...). */
  type: string;
  /** The kind of Typed Value its values are read as. */
  valueType: string;
  /** A UCUM code. */
  unit: string | null;
  nullable: boolean;
  roles: string[];
  /** The role the formatter draws it with (`time`, or `none`). */
  role: string;
  categories: { value: string; label?: string }[] | null;
  /** The table and fields its values refer to, from the schema's foreign keys. */
  reference: { table: string; fields: string[] } | null;
  /** Whether its values are JSON trees. */
  nested: boolean;
  /** How many of its values did not match its type. */
  mismatches: number;
}

/** How far a Described Table's rows have come, and how its stream has gone. */
export interface DescribedStatus {
  /** The `table` message has come. */
  described: boolean;
  /** A `synced` has come and no resync began since. */
  synced: boolean;
  /** The table has more rows than the watch asked for: only those that came are shown. */
  partial: boolean;
  /** The table's row count, if the server gave one. */
  count: { value: number; estimate: boolean } | null;
  incarnation: string | null;
  sequence: number | null;
  /** How many rows the source holds, filtered or not. */
  rows: number;
  /** How many values did not match their field's type. */
  mismatches: number;
  /** How many `resync` messages came. */
  resyncs: number;
  /** How many times the stream ended (calls to `ended`). */
  reconnects: number;
  /** How many messages did not decode, and were skipped. */
  decodeErrors: number;
}

/** A Described Table: its Table Source, and what its host feeds it. */
export interface DescribedTable {
  readonly source: TableSource<bigint, string, TypedValue>;
  /** The watch to open next, or `undefined`: first, and again after each `ended`. */
  pollRequest(): DescribedRequest | undefined;
  /** A response body begins, of its `Content-Type`; a type that is not a rows stream throws. */
  open(contentType: string): void;
  /** The body's next bytes, in pieces of any size. */
  feed(bytes: Uint8Array): void;
  /** The body ended: the source asks for its watch again, and the next one's rows replace these in place. */
  ended(): void;
  status(): DescribedStatus;
  columns(): DescribedColumn[];
  /** Property `name` of `field` as the schema sent it, or `undefined`. */
  property(field: string, name: string): unknown;
  /** The primary-key tuple of the row with this ID, or `undefined`. */
  rowKey(rowId: bigint): unknown[] | undefined;
  /** The Layout every Described Table starts from: Nested columns hidden, key and state columns pinned leading. */
  defaultLayout(): Layout<bigint, string>;
  /** Called after each `feed` and `ended`; returns how to unsubscribe. */
  onChange(listener: () => void): () => void;
  /** Frees the wasm side. */
  destroy(): void;
}

/** What `feed` and `ended` return across the wasm boundary. */
interface Fed {
  notices: Float64Array;
  view: { spec: KeyedViewSpec; totals: Totals } | null;
  columnsChanged: boolean;
  resyncs: number;
  errors: string[];
}

type WasmStatus = Omit<DescribedStatus, "resyncs" | "reconnects" | "decodeErrors">;

const AXIS_NAMES: readonly AxisName[] = ["rows", "cols"];

/** Change Notices as `[axis, code, a, b]` quads, end to end. */
function notices(quads: Float64Array): [AxisName, ChangeNotice][] {
  const out: [AxisName, ChangeNotice][] = [];
  for (let i = 0; i + 3 < quads.length; i += 4) {
    const [a, code, x, y] = [quads[i]!, quads[i + 1]!, quads[i + 2]!, quads[i + 3]!];
    out.push([AXIS_NAMES[a]!, notice(code, x, y)]);
  }
  return out;
}

function notice(code: number, a: number, b: number): ChangeNotice {
  switch (code) {
    case NOTICE_CODE.Reordered:
      return { type: "Reordered" };
    case NOTICE_CODE.Reset:
      return { type: "Reset" };
    case NOTICE_CODE.Inserted:
      return { type: "Inserted", at: a, n: b };
    case NOTICE_CODE.Removed:
      return { type: "Removed", at: a, n: b };
    case NOTICE_CODE.Refiltered:
      return { type: "Refiltered" };
    case NOTICE_CODE.Collapsed:
      return { type: "Collapsed", at: a, n: b };
    case NOTICE_CODE.Expanded:
      return { type: "Expanded", at: a, n: b };
    default:
      return { type: "ContentChanged", start: a, end: b };
  }
}

/**
 * One message's notices after another's, as the widget reads them: a
 * `Reset` or `Refiltered` asks for the count afresh, so the rest of its
 * axis's notices in the batch would count twice. Such an axis gets the one
 * (a `Reset` if any), after the other axis's.
 */
function batch(told: [AxisName, ChangeNotice][]): [AxisName, ChangeNotice][] {
  const whole = (axis: AxisName): ChangeNotice | undefined => {
    const types = told.filter(([a]) => a === axis).map(([, n]) => n.type);
    if (types.includes("Reset")) return { type: "Reset" };
    if (types.includes("Refiltered")) return { type: "Refiltered" };
    return undefined;
  };
  const wholes = AXIS_NAMES.map((axis) => [axis, whole(axis)] as const);
  const kept = told.filter(([axis]) => !wholes.some(([a, n]) => a === axis && n));
  return [...kept, ...wholes.flatMap(([axis, n]) => (n ? [[axis, n] as [AxisName, ChangeNotice]] : []))];
}

const located = ([code, position]: Float64Array): Located =>
  code === 0
    ? { type: "Found", position: position! }
    : { type: "Gone", suggest: Number.isNaN(position) ? null : position! };

/**
 * A Table Source over the Described Table `table` within `scope`: call it
 * after `loadBinnacle`. Its host opens the watch `pollRequest` asks for,
 * hands each response to `open`, `feed` and `ended`, and polls again after
 * each end. Each cell draws through the shared formatter with its
 * column's unit and role. An empty namespace throws.
 */
export function fromDescribed(table: string, scope: DescribedScope, options: DescribedOptions = {}): DescribedTable {
  const { zone } = options;
  let wasm: WebDescribed | undefined = new (bridge().WebDescribed)(table, scope.namespace, scope.wheres ?? [], zone);
  const live = (): WebDescribed => {
    if (!wasm) throw new Error("binnacle: this described table was destroyed");
    return wasm;
  };

  let byName = new Map<string, DescribedColumn>();
  let byKey = new Map<bigint, DescribedColumn>();
  /** One renderer per column, made again only when its type, unit or roles change. */
  const renderers = new Map<string, { made: string; renderer: CellRenderer<TypedValue> }>();
  const counts = { resyncs: 0, reconnects: 0, decodeErrors: 0 };
  const sinks = new Set<TableSink<bigint, string>>();
  const listeners = new Set<() => void>();

  const clock = zone && ((frameNow: number) => ({ now: performance.timeOrigin + frameNow, zone }));
  const renderOf = (c: DescribedColumn) => JSON.stringify([c.valueType, c.type, c.unit, c.roles]);
  const recolumn = () => {
    const columns = live().columns() as DescribedColumn[];
    byName = new Map(columns.map((c) => [c.name, c]));
    byKey = new Map(columns.map((c) => [c.key, c]));
    for (const name of renderers.keys()) if (!byName.has(name)) renderers.delete(name);
    for (const c of columns) {
      const made = renderOf(c);
      if (renderers.get(c.name)?.made === made) continue;
      const renderer = valueRenderer<TypedValue>({
        ...(c.unit === null ? {} : { unit: c.unit }),
        role: c.role,
        ...(clock ? { clock } : {}),
      });
      renderers.set(c.name, { made, renderer });
    }
  };

  const colKey = (name: string): bigint | undefined => byName.get(name)?.key;
  const colName = (key: bigint): string | undefined => byKey.get(key)?.name;
  const known = <T>(x: T | undefined): x is T => x !== undefined;

  const queryKeys = (q: TextQuery<string>): KeyedTextQuery => ({
    text: q.text,
    caseSensitive: q.caseSensitive,
    regex: q.regex,
    searches: q.searches === "shown" ? null : q.searches.map(colKey).filter(known),
  });
  const queryIds = (q: KeyedTextQuery): TextQuery<string> => ({
    text: q.text,
    caseSensitive: q.caseSensitive,
    regex: q.regex,
    searches: q.searches === null ? "shown" : q.searches.map(colName).filter(known),
  });
  const conditionKeys = (c: Condition<string>): KeyedCondition | undefined => {
    const key = colKey(c.id);
    if (key === undefined) return undefined;
    if (c.kind.type === "Text") {
      const { text, caseSensitive, regex } = c.kind.query;
      return { key, kind: { type: "Text", query: { text, caseSensitive, regex, searches: [key] } }, not: c.not };
    }
    return { key, kind: c.kind, not: c.not };
  };
  const conditionIds = (c: KeyedCondition): Condition<string> | undefined => {
    const id = colName(c.key);
    if (id === undefined) return undefined;
    switch (c.kind.type) {
      case "Text": {
        const { text, caseSensitive, regex } = c.kind.query;
        return { id, kind: { type: "Text", query: { text, caseSensitive, regex } }, not: c.not };
      }
      case "Other":
        return undefined;
      default:
        return { id, kind: c.kind, not: c.not };
    }
  };

  /** A View Spec by Keys, as the wasm side takes it: rows by their Keys, columns by theirs. Columns the source lacks are left out. */
  const specKeys = (spec: ViewSpec<bigint, string>): KeyedViewSpec => ({
    rows: {
      filter: {
        quick: spec.rows.filter.quick && queryKeys(spec.rows.filter.quick),
        conditions: spec.rows.filter.conditions.map(conditionKeys).filter(known),
      },
      sort: spec.rows.sort.flatMap((c) => {
        const key = colKey(c.id);
        return key === undefined ? [] : [{ key, direction: c.direction }];
      }),
      hidden: spec.rows.hidden,
    },
    // The columns are not sorted or filtered by rows.
    cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: spec.cols.hidden.map(colKey).filter(known) },
    grouping: null,
  });

  const specIds = (spec: KeyedViewSpec, totals: Totals): AppliedView<bigint, string> => {
    const rows: AppliedAxisView<string, bigint> = {
      filter: {
        quick: spec.rows.filter.quick && queryIds(spec.rows.filter.quick),
        conditions: spec.rows.filter.conditions.map(conditionIds).filter(known),
      },
      sort: spec.rows.sort.flatMap((c) => {
        const id = colName(c.key);
        return id === undefined ? [] : [{ id, direction: c.direction }];
      }),
      hidden: spec.rows.hidden,
    };
    const cols: AppliedAxisView<bigint, string> = { sort: [], hidden: spec.cols.hidden.map(colName).filter(known) };
    return { rows, cols, grouping: null, totals };
  };

  const tell = (quads: Float64Array) => {
    for (const [axis, n] of batch(notices(quads))) for (const s of sinks) s[axis].notify(n);
  };

  const fed = (r: Fed) => {
    counts.resyncs += r.resyncs;
    counts.decodeErrors += r.errors.length;
    if (r.columnsChanged) {
      recolumn();
      for (const s of sinks) s.rows.supportChanged();
    }
    if (r.view) {
      const view = specIds(r.view.spec, r.view.totals);
      for (const s of sinks) s.viewChanged(view);
    }
    tell(r.notices);
    for (const listener of [...listeners]) listener();
  };

  const findKeys = (q: FindQuery<string>) => ({ query: queryKeys(q.query), by: q.by });
  const keys = (ids: readonly bigint[]) => BigUint64Array.from(ids);
  /** Column Keys for names, with the places of those the source knows. */
  const colKeys = (names: readonly string[]) => {
    const at: number[] = [];
    const ks: bigint[] = [];
    names.forEach((name, i) => {
      const key = colKey(name);
      if (key === undefined) return;
      at.push(i);
      ks.push(key);
    });
    return { at, keys: BigUint64Array.from(ks) };
  };

  const axisMethods = (axis: AxisName) => {
    const a = AXIS_CODE[axis];
    const quickFilter = live().quickFilter(a) as { regex: boolean } | null;
    return {
      count: () => live().count(a),
      window: (start: number, end: number) => [...live().window(a, start, end)],
      ...(quickFilter ? { quickFilter } : {}),
      hide: live().hides(a),
    };
  };

  const source: TableSource<bigint, string, TypedValue> = {
    rows: {
      ...axisMethods("rows"),
      support: (by: string): Support => {
        const key = colKey(by);
        return key === undefined ? {} : (live().support(AXIS_CODE.rows, key) as Support);
      },
      distinct: (by, query): Distinct => {
        const key = colKey(by);
        if (key === undefined) return { values: [], partial: false, narrowed: true };
        const narrow = {
          quick: query.narrow.quick && queryKeys(query.narrow.quick),
          conditions: query.narrow.conditions.map(conditionKeys).filter(known),
        };
        const answer = live().distinct(AXIS_CODE.rows, key, { search: query.search, limit: query.limit, narrow });
        return (answer as Distinct | null) ?? { values: [], partial: false, narrowed: false };
      },
      locate: (id) => located(live().locate(AXIS_CODE.rows, id)),
    },
    cols: {
      ...axisMethods("cols"),
      window: (start, end) => [...live().window(AXIS_CODE.cols, start, end)].map(colName).filter(known),
      label: (id) => byName.get(id)?.header ?? id,
      locate: (id) => {
        const key = colKey(id);
        return key === undefined ? { type: "Gone", suggest: null } : located(live().locate(AXIS_CODE.cols, key));
      },
    },
    view(spec) {
      const viewed = live().view(specKeys(spec)) as { applied: KeyedViewSpec; totals: Totals; notices: Float64Array };
      tell(viewed.notices);
      return specIds(viewed.applied, viewed.totals);
    },
    cells(rowIds, colIds) {
      const { at, keys: ks } = colKeys(colIds);
      const got = live().cells(keys(rowIds), ks) as TypedValue[][];
      return got.map((row) => {
        const out: TypedValue[] = colIds.map(() => undefined);
        at.forEach((j, i) => (out[j] = row[i]));
        return out;
      });
    },
    renderer: byColumn((name: string) => renderers.get(name)?.renderer),
    find: {
      regex: (live().findSupport() as { regex: boolean } | null)?.regex ?? false,
      marks(rowIds, colIds, query) {
        const { at, keys: ks } = colKeys(colIds);
        const flat = live().findMarks(keys(rowIds), ks, findKeys(query));
        return rowIds.map((_, i) => {
          const row = colIds.map(() => false);
          at.forEach((j, k) => (row[j] = flat[i * at.length + k] === 1));
          return row;
        });
      },
      next(from, direction, query): FindStep<bigint, string> | null {
        const fromKeys = from && colKey(from.col) !== undefined ? { row: from.row, col: colKey(from.col)! } : null;
        const step = live().findNext(findKeys(query), fromKeys, direction) as {
          row: bigint;
          col: bigint;
          index?: number;
          wrapped: boolean;
        } | null;
        const col = step ? colName(step.col) : undefined;
        if (!step || col === undefined) return null;
        return { row: step.row, col, wrapped: step.wrapped, ...(step.index === undefined ? {} : { index: step.index }) };
      },
      count: (query): FindCount => live().findCount(findKeys(query)) as FindCount,
    },
    attach(sink) {
      sinks.add(sink);
      return () => sinks.delete(sink);
    },
  };

  return {
    source,
    pollRequest: () => (live().pollRequest() as DescribedRequest | undefined) ?? undefined,
    open: (contentType) => live().open(contentType),
    feed: (bytes) => fed(live().feed(bytes) as Fed),
    ended() {
      counts.reconnects++;
      fed(live().ended() as Fed);
    },
    status: () => ({ ...(live().status() as WasmStatus), ...counts }),
    columns: () => live().columns() as DescribedColumn[],
    property(field, name) {
      const key = colKey(field);
      return key === undefined ? undefined : (live().property(key, name) as unknown);
    },
    rowKey: (rowId) => live().rowTuple(rowId) as unknown[] | undefined,
    defaultLayout: () => live().defaultLayout() as Layout<bigint, string>,
    onChange(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    destroy() {
      wasm?.free();
      wasm = undefined;
      sinks.clear();
      listeners.clear();
    },
  };
}
