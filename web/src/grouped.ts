// groupedSource: a Table Source whose rows group through binnacle-group's
// grouping layer in wasm, over an aggregate backend the host writes in
// TypeScript and answers whenever it likes: a remote service, a worker, a
// database. The layer owns the structure (groups into items, Summary Item
// IDs from paths, Subtotals and the Grand Total, open and closed groups);
// the backend does the arithmetic and the filtering, one Aggregate Query
// at a time.
//
// A frame's queries go to the backend as one batch. It answers each by id,
// in any order, in parts, jointly (one scan for several), or with more than
// asked (a whole tree, a prefetched page); answers to a batch a newer View
// Spec superseded are dropped, and its AbortSignal aborts. While groups and
// Leaves are due, a Window waits for them and the widget draws them
// loading. The layer holds the open groups and a bounded cache of pages:
// memory grows with what the user opens, never with the rows.
//
// The base Table Source gives the columns, the Leaves' cells, and the rows
// while they are not grouped. Leaf IDs are its row IDs, non-negative
// integers; a Summary Item's row ID is a negative number, stable for the
// source's life (the Grand Total's is -1, its `grandTotal`), as fromRows
// names them. A Summary Item's cells hold its group's figure for each
// column a value aggregates. The level columns leave the columns while the
// rows group by them, through the base's hidden columns.

import { NOTICE_CODE } from "./codes";
import type {
  AggregateValue,
  AppliedView,
  AxisSink,
  ChangeNotice,
  Condition,
  FilterSpec,
  Grouping,
  ListedValue,
  Located,
  Outline,
  OutlinedWindow,
  Primitive,
  SortSpec,
  TableSink,
  TableSource,
  ViewSpec,
} from "./source";
import { summaryLabel } from "./source";
import type { KeyedCondition, KeyedGrouping, KeyedOutline, KeyedTextQuery, KeyedViewSpec } from "./snapshot";
import { bridge } from "./wasm";

/** A figure a backend computes: a number, text, or `null` for a blank. */
export type Figure = number | string | null;

/** A group's place: each level's field and the group's value there, from the outermost. */
export type GroupPath<ColId> = { field: ColId; value: ListedValue }[];

/**
 * What every query of a batch shares: the level fields (outermost first),
 * the values to aggregate, the slicers, and the rows' filter, hidden Leaves
 * and sort. The backend filters the Leaves by the slicers, the filter and
 * `hidden` before it groups and aggregates them, and orders siblings by
 * `sort`.
 */
export interface AggregateScope<ColId> {
  levels: ColId[];
  values: AggregateValue<ColId>[];
  slicers: Condition<ColId>[];
  filter: FilterSpec<ColId>;
  hidden: number[];
  sort: SortSpec<ColId>;
}

/**
 * One Aggregate Query: under the group at `path` (the whole axis when
 * empty), the groups of the next level from `start` to `end`, each with its
 * value, figures and child count; the Leaves of the group from `start` to
 * `end`; the figures of the whole group (`total`); or where the Leaf with a
 * row ID is (`locate`).
 */
export interface AggregateQuery<ColId> {
  id: number;
  path: GroupPath<ColId>;
  want: "groups" | "leaves" | "total" | { locate: number };
  start: number;
  end: number;
}

/** One frame's queries, for the View Spec `stamp` names. */
export interface AggregateBatch<ColId> {
  stamp: number;
  scope: AggregateScope<ColId>;
  queries: AggregateQuery<ColId>[];
}

/** One group: its value at its level, how many children it has (groups of the next level, or Leaves), and one figure per value. */
export interface GroupRow {
  value: ListedValue;
  children: number;
  figures: Figure[];
}

/**
 * What an answer holds: groups under its path from its `start` (and how
 * many there are in all), Leaf row IDs of its group from its `start`, where
 * a Leaf is (under its path at its `start`, unless not `found`), or the
 * figures of the whole group.
 */
export type AggregateBody =
  | { type: "groups"; total: number; groups: GroupRow[] }
  | { type: "leaves"; ids: number[] }
  | { type: "located"; id: number; found: boolean }
  | { type: "total"; figures: Figure[] };

/**
 * An answer, self-describing so it can come in any order, in parts or
 * unasked: the batch's stamp, the query it answers (`null` for data not
 * asked for) and where its body belongs. A query is done once an answer
 * names it, or once what was answered covers it.
 */
export interface AggregateAnswer<ColId> {
  stamp: number;
  id: number | null;
  path: GroupPath<ColId>;
  start: number;
  body: AggregateBody;
}

/**
 * What the grouping layer asks of the host: answers to each batch, given to
 * `reply` as they come, now or later. `signal` aborts when a newer View Spec
 * supersedes the batch; replies after it are dropped anyway.
 */
export interface AggregateBackend<ColId> {
  answer(
    batch: AggregateBatch<ColId>,
    reply: (answer: AggregateAnswer<ColId>) => void,
    signal: AbortSignal,
  ): void | Promise<void>;
}

export interface GroupedOptions<ColId, V> {
  /** The plain table: the columns, the Leaves' cells, and the rows while not grouped. */
  base: TableSource<number, ColId, V>;
  backend: AggregateBackend<ColId>;
  /** Whether the rows may group by a column; any column the base sorts by, by default. */
  groups?: (col: ColId) => boolean;
  /** How many closed groups and Leaves the layer caches besides those in view. */
  cache?: number;
  /** Called with a backend's failure (a rejected `answer`). */
  onError?: (error: unknown) => void;
}

/** A Summary Item of the grouped rows, as the layer holds it. */
export interface GroupedSummary<ColId> {
  kind: "group" | "subtotal" | "grandTotal";
  path: GroupPath<ColId>;
  children: number;
  figures: Figure[];
}

export interface GroupedSource<ColId, V> extends TableSource<number, ColId, V | Figure> {
  /** The Summary Item with this row ID, if the layer holds it. */
  summary(id: number): GroupedSummary<ColId> | undefined;
  /** How many groups and Leaves the layer holds. */
  held(): number;
  destroy(): void;
}

const SUMMARY = 1n << 63n;
const rowId = (key: bigint) => (key & SUMMARY ? -Number(key & (SUMMARY - 1n)) - 1 : Number(key));
const rowKey = (id: number) => (id < 0 ? SUMMARY | BigInt(-id - 1) : BigInt(id));

interface WireStep {
  field: bigint;
  value: ListedValue;
}

interface WireBatch {
  stamp: number;
  scope: {
    levels: bigint[];
    values: AggregateValue<bigint>[];
    slicers: KeyedCondition[];
    filter: { quick: KeyedTextQuery | null; conditions: KeyedCondition[] };
    hidden: bigint[];
    sort: { key: bigint; direction: "ascending" | "descending" }[];
  };
  requests: {
    id: number;
    path: WireStep[];
    want: "groups" | "leaves" | "total" | { locate: bigint };
    start: number;
    end: number;
  }[];
}

interface WireSummary {
  kind: "group" | "subtotal" | "grandTotal";
  path: WireStep[];
  children: number;
  aggregates: Figure[];
}

const known = <T>(x: T | undefined): x is T => x !== undefined;

/**
 * A Table Source whose rows group through the grouping layer over
 * `backend`, with `base` for everything else.
 */
export function groupedSource<ColId extends Primitive, V>(options: GroupedOptions<ColId, V>): GroupedSource<ColId, V> {
  const { base, backend } = options;
  const groups = options.groups ?? ((col: ColId) => base.rows.support?.(col).sort !== false);
  let layer: InstanceType<ReturnType<typeof bridge>["WebGroupingLayer"]> | undefined;
  const wasm = () => (layer ??= new (bridge().WebGroupingLayer)(options.cache ?? NaN));

  // Columns by Key: the order first seen.
  const colIds: ColId[] = [];
  const colKeys = new Map<ColId, bigint>();
  const colKey = (id: ColId) => {
    let key = colKeys.get(id);
    if (key === undefined) {
      key = BigInt(colIds.length);
      colKeys.set(id, key);
      colIds.push(id);
    }
    return key;
  };
  const colId = (key: bigint) => colIds[Number(key)];

  const sinks = new Set<TableSink<number, ColId>>();
  const tell = (notice: ChangeNotice, axis: "rows" | "cols" = "rows") => {
    for (const s of sinks) s[axis].notify(notice);
  };

  /** The grouping in force, by IDs, and its level columns. */
  let grouping: Grouping<ColId, number, ColId> | null = null;
  let levels: ColId[] = [];

  // Batches go out as the layer asks; each stamp's signal aborts once a
  // newer one supersedes it.
  let stamp = -1;
  let controller = new AbortController();
  const waiters = new Set<() => boolean>();
  const wake = () => {
    for (const w of [...waiters]) if (w()) waiters.delete(w);
  };
  const pump = () => {
    for (;;) {
      const wire = wasm().poll_batch() as WireBatch | null;
      if (!wire) return;
      if (wire.stamp !== stamp) {
        controller.abort();
        controller = new AbortController();
        stamp = wire.stamp;
      }
      const { signal } = controller;
      const batch = batchIds(wire);
      const reply = (answer: AggregateAnswer<ColId>) => {
        if (signal.aborted || !layer) return;
        layer.answer(answerKeys(answer));
        wake();
        pump();
      };
      Promise.resolve()
        .then(() => backend.answer(batch, reply, signal))
        .catch((error: unknown) => {
          if (!signal.aborted) options.onError?.(error);
        });
    }
  };
  /** `check()` once it gives an answer, asking the backend meanwhile; `undefined` if `signal` aborts first. */
  const until = <T>(check: () => T | undefined, signal?: AbortSignal): T | Promise<T | undefined> => {
    const now = check();
    // What `check` read may have asked for more.
    pump();
    if (now !== undefined) return now;
    return new Promise<T | undefined>((resolve) => {
      const w = () => {
        if (signal?.aborted) return resolve(undefined), true;
        const got = check();
        if (got === undefined) return false;
        resolve(got);
        return true;
      };
      waiters.add(w);
      signal?.addEventListener("abort", () => {
        waiters.delete(w);
        resolve(undefined);
      });
    });
  };

  const pathIds = (path: WireStep[]): GroupPath<ColId> =>
    path.map((s) => ({ field: colId(s.field)!, value: s.value }));
  const pathKeys = (path: GroupPath<ColId>): WireStep[] => path.map((s) => ({ field: colKey(s.field), value: s.value }));

  const conditionIds = (c: KeyedCondition): Condition<ColId> | undefined => {
    const id = colId(c.key);
    if (id === undefined) return undefined;
    switch (c.kind.type) {
      case "Text": {
        const { text, caseSensitive, regex } = c.kind.query;
        return { id, kind: { type: "Text", query: { text, caseSensitive, regex } }, not: c.not };
      }
      case "OneOf":
        return { id, kind: { type: "OneOf", values: c.kind.values }, not: c.not };
      case "Compare":
        return { id, kind: { type: "Compare", comparison: c.kind.comparison }, not: c.not };
      default:
        return undefined;
    }
  };
  const conditionKeys = (c: Condition<ColId>): KeyedCondition => {
    const key = colKey(c.id);
    if (c.kind.type === "Text") {
      const { text, caseSensitive, regex } = c.kind.query;
      return { key, kind: { type: "Text", query: { text, caseSensitive, regex, searches: [key] } }, not: c.not };
    }
    return { key, kind: c.kind, not: c.not };
  };

  const batchIds = (wire: WireBatch): AggregateBatch<ColId> => {
    const s = wire.scope;
    const quick = s.filter.quick;
    return {
      stamp: wire.stamp,
      scope: {
        levels: s.levels.map((k) => colId(k)!),
        values: s.values.map((v) => ({ ...v, field: v.field === null ? null : colId(v.field)! })),
        slicers: s.slicers.map(conditionIds).filter(known),
        filter: {
          quick: quick && {
            text: quick.text,
            caseSensitive: quick.caseSensitive,
            regex: quick.regex,
            searches: quick.searches === null ? "shown" : quick.searches.map((k) => colId(k)!),
          },
          conditions: s.filter.conditions.map(conditionIds).filter(known),
        },
        hidden: s.hidden.map(Number),
        sort: s.sort.map((c) => ({ id: colId(c.key)!, direction: c.direction })),
      },
      queries: wire.requests.map((r) => ({
        id: r.id,
        path: pathIds(r.path),
        want: typeof r.want === "string" ? r.want : { locate: Number(r.want.locate) },
        start: r.start,
        end: r.end,
      })),
    };
  };

  const answerKeys = (a: AggregateAnswer<ColId>) => {
    const b = a.body;
    const body =
      b.type === "groups"
        ? { type: "groups", total: b.total, groups: b.groups.map((g) => ({ value: g.value, children: g.children, aggregates: g.figures })) }
        : b.type === "leaves"
          ? { type: "leaves", keys: b.ids.map((id) => BigInt(id)) }
          : b.type === "located"
            ? { type: "located", key: BigInt(b.id), found: b.found }
            : { type: "total", aggregates: b.figures };
    return { stamp: a.stamp, id: a.id, path: pathKeys(a.path), start: a.start, body };
  };

  const specKeys = (spec: ViewSpec<number, ColId>, g: Grouping<ColId, number, ColId> | null): KeyedViewSpec => {
    const part = <Id>(p: ViewSpec<number, ColId>["rows"], ids: (id: Id) => bigint, cross: (id: ColId) => bigint) => ({
      filter: {
        quick: p.filter.quick && {
          text: p.filter.quick.text,
          caseSensitive: p.filter.quick.caseSensitive,
          regex: p.filter.quick.regex,
          searches: p.filter.quick.searches === "shown" ? null : p.filter.quick.searches.map(cross),
        },
        conditions: p.filter.conditions.map(conditionKeys),
      },
      sort: p.sort.map((c) => ({ key: cross(c.id), direction: c.direction })),
      hidden: (p.hidden as Id[]).map(ids),
    });
    const keyed: KeyedGrouping | null = g && {
      rowLevels: g.rowLevels.map((l) => ("field" in l ? { field: colKey(l.field), subtotal: l.subtotal } : l)),
      colLevels: [],
      values: g.values.map((v) => ({ ...v, field: v.field === null ? null : colKey(v.field) })),
      slicers: g.slicers.map(conditionKeys),
      rowAxis: { ...g.rowAxis, expand: { ...g.rowAxis.expand, except: g.rowAxis.expand.except.map(rowKey) } },
      colAxis: { ...g.colAxis, expand: { ...g.colAxis.expand, except: [] } },
    };
    return {
      rows: part<number>(spec.rows, rowKey, colKey),
      // The columns' part is the base's; the layer reads only the rows'.
      cols: { filter: { quick: null, conditions: [] }, sort: [], hidden: [] },
      grouping: keyed,
    };
  };

  const outlineIds = (o: KeyedOutline): Outline<number, ColId> => ({
    kind: o.kind,
    depth: o.depth,
    open: o.open,
    ancestors: o.ancestors.map(rowId),
    group: o.group ? { field: colId(o.group.field)!, value: o.group.value } : null,
  });

  const summary = (id: number): GroupedSummary<ColId> | undefined => {
    if (id >= 0 || !layer) return undefined;
    const s = layer.summary(rowKey(id)) as WireSummary | null;
    return s ? { kind: s.kind, path: pathIds(s.path), children: s.children, figures: s.aggregates } : undefined;
  };

  /**
   * The rows from `start` the layer holds, asking the backend for the rest:
   * all of them, or once it holds more than the last answer for the same
   * range gave. A short answer comes late, as any answer the widget waited
   * for, so the Grid asks again for the rest; the items still due draw
   * pending meanwhile.
   */
  let shown = { start: -1, end: -1, n: 0 };
  const window = (start: number, end: number, signal: AbortSignal) => {
    const before = shown.start === start && shown.end === end ? shown.n : 0;
    let short = false;
    const read = (): OutlinedWindow<number, ColId> | undefined => {
      const count = wasm().count();
      const want = Math.max(0, Math.min(end, Number.isNaN(count) ? 0 : count) - start);
      const items = wasm().window(start, end) as { key: bigint; outline: KeyedOutline }[];
      if (items.length < want && items.length <= before) return undefined;
      shown = { start, end, n: items.length };
      short = items.length < want;
      return { ids: items.map((i) => rowId(i.key)), outlines: items.map((i) => outlineIds(i.outline)) };
    };
    const got = until(read, signal) as OutlinedWindow<number, ColId> | Promise<OutlinedWindow<number, ColId>>;
    return short ? Promise.resolve(got) : got;
  };

  const notice = ([code, a, b]: [number, number, number]): ChangeNotice => {
    switch (code) {
      case NOTICE_CODE.Collapsed:
        return { type: "Collapsed", at: a, n: b };
      case NOTICE_CODE.Expanded:
        return { type: "Expanded", at: a, n: b };
      default:
        return { type: "Refiltered" };
    }
  };

  /** The `viewed` answers the layer has given, their notices told. */
  const drain = () => {
    let last: { applied: KeyedViewSpec; notices: [number, number, number][] } | undefined;
    for (let v; (v = wasm().poll_viewed()); ) {
      last = v as typeof last;
      for (const n of last!.notices) tell(notice(n));
    }
    return last;
  };

  // The base's news: its columns always, its rows only while not grouped.
  const relay: TableSink<number, ColId> = {
    rows: {
      notify: (n) => !wasm().grouped() && tell(n),
      supportChanged: (ids) => sinks.forEach((s) => s.rows.supportChanged(ids)),
      movableChanged: () => sinks.forEach((s) => s.rows.movableChanged()),
    } satisfies AxisSink<number, ColId>,
    cols: {
      notify: (n) => tell(n, "cols"),
      supportChanged: (ids) => sinks.forEach((s) => s.cols.supportChanged(ids)),
      movableChanged: () => sinks.forEach((s) => s.cols.movableChanged()),
    },
    viewChanged: (view) => sinks.forEach((s) => s.viewChanged(view)),
    findCounted: (query, count) => sinks.forEach((s) => s.findCounted(query, count)),
  };
  const detach = base.attach?.(relay);

  const figure = (id: number, col: ColId): Figure => {
    const at = grouping?.values.findIndex((v) => v.field === col) ?? -1;
    return at < 0 ? null : (summary(id)?.figures[at] ?? null);
  };

  return {
    rows: {
      count: () => {
        const n = layer?.count() ?? NaN;
        return Number.isNaN(n) ? base.rows.count() : n;
      },
      window: (start, end, signal) => (wasm().grouped() ? window(start, end, signal) : base.rows.window(start, end, signal)),
      support: (by) => ({ ...base.rows.support?.(by), group: groups(by) }),
      ...(base.rows.quickFilter ? { quickFilter: base.rows.quickFilter } : {}),
      locate: (id, signal) => {
        if (!wasm().grouped()) return base.rows.locate?.(id, signal) ?? { type: "Gone", suggest: null };
        const key = rowKey(id);
        const at = (): Located | undefined => {
          const [code, position] = wasm().locate(key);
          if (code === undefined) return undefined;
          return code === 0 ? { type: "Found", position: position! } : { type: "Gone", suggest: null };
        };
        return until(at, signal) as Located | Promise<Located>;
      },
      label: (id, position) => base.rows.label?.(id, position) ?? String(position + 1),
      groupLabel: (id, outline) => {
        const s = summary(id);
        if (!s) return summaryLabel(outline);
        const counted = grouping?.values.findIndex((v) => v.field === null && v.aggregate === "count") ?? -1;
        const count = counted < 0 ? s.children : s.figures[counted];
        return `${summaryLabel(outline)} (${count})`;
      },
      grandTotal: -1,
    },
    cols: base.cols,
    async view(asked, signal) {
      const wanted = (asked.grouping?.rowLevels ?? []).flatMap((l) => ("field" in l && groups(l.field) ? [l.field] : []));
      levels = [...new Set(wanted)];
      const was = wasm().grouped();
      grouping =
        levels.length === 0
          ? null
          : {
              ...asked.grouping!,
              rowLevels: levels.map((field) => asked.grouping!.rowLevels.find((l) => "field" in l && l.field === field)!),
              colLevels: [],
            };
      // The base filters and sorts the rows as asked, which it shows while
      // they are not grouped; the level columns leave the columns.
      const hidden = [...new Set([...asked.cols.hidden, ...levels])];
      drain();
      wasm().request_view(specKeys(asked, grouping));
      const plain = { ...asked, cols: { ...asked.cols, hidden }, grouping: null };
      const applied = base.view ? await base.view(plain, signal) : { rows: asked.rows, cols: plain.cols };
      const cols = { ...applied.cols, hidden: (applied.cols.hidden ?? []).filter((id) => !levels.includes(id) || asked.cols.hidden.includes(id)) };
      if (!wasm().grouped()) {
        drain();
        if (was) tell({ type: "Refiltered" });
        return { ...applied, cols, grouping: null };
      }
      const viewed = await until(drain, signal);
      if (!viewed) return { ...applied, cols, grouping };
      const except = (viewed.applied.grouping?.rowAxis.expand.except ?? []).map(rowId);
      grouping = { ...grouping!, rowAxis: { ...grouping!.rowAxis, expand: { ...grouping!.rowAxis.expand, except } } };
      return { ...applied, rows: asked.rows, cols, grouping } satisfies AppliedView<number, ColId>;
    },
    async cells(rowIds, colIds, signal) {
      const leaves = rowIds.filter((id) => id >= 0);
      const got = leaves.length ? await base.cells(leaves, colIds, signal) : [];
      const byId = new Map(leaves.map((id, i) => [id, got[i]!]));
      // A Summary Item's figures may land after the item itself.
      for (const id of rowIds) if (id < 0) await until(() => summary(id), signal);
      return rowIds.map((id) => (id < 0 ? colIds.map((c) => figure(id, c)) : byId.get(id)!));
    },
    ...(base.renderer ? { renderer: base.renderer as NonNullable<GroupedSource<ColId, V>["renderer"]> } : {}),
    attach(sink) {
      sinks.add(sink);
      return () => sinks.delete(sink);
    },
    summary,
    held: () => layer?.held() ?? 0,
    destroy() {
      controller.abort();
      detach?.();
      layer?.free();
      layer = undefined;
    },
  };
}
