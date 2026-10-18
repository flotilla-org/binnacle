// Plumbing between a Table Source and the Grid: counts, Window answers
// interned into Keys, the value cache, Change Notices from the source's
// sinks, and the answers to the Effects a Table Source owns (View Specs,
// Locate, move). Every answer goes through one `settle`.

import { AXES, AXIS_CODE, NOTICE_CODE, SUPPORT, type AxisName } from "./codes";
import { createValueCache, FAILED, PENDING } from "./cache";
import type { Content } from "./content";
import type { Effect } from "./effects";
import { createInterner, positionInterner, type KeyInterner } from "./interner";
import { createSettler, type Request } from "./settle";
import type { KeyedViewSpec, Snapshot, SortColumn, StripItem } from "./snapshot";
import type { AxisMethods, ChangeNotice, FailedRequest, Located, SortSpec, TableSource, ViewSpec } from "./source";
import type { WebGrid } from "./wasm";

export interface Feed<RowId = unknown, ColId = unknown> {
  /** The counts to create the Grid with; 0 for one still on the way, which lands as a Reset. */
  counts(): { rows: number; cols: number };
  /** Takes Change Notices for `grid` from now on and attaches the source's sinks. */
  connect(grid: WebGrid): void;
  /** Answers the Windows `begin_frame` asked for (`[rowStart, rowEnd, colStart, colEnd]`, `NaN` for none). */
  fill(grid: WebGrid, requests: Float64Array): void;
  /** Answers the Effects the Table Source owns (View Specs, Locate, move); `false` for the rest. */
  answer(effect: Effect): boolean;
  /** The values and labels for the Snapshot's visible items, asking for missing values. */
  content(s: Snapshot): Content;
  /** Whether a cell in the viewport still waits for its Keys or its value (one whose request failed does not). */
  loading(s: Snapshot): boolean;
  /** The value shown for the cell with these Keys: an edit not yet answered for, or the cache's; `undefined` if neither. */
  value(row: bigint, col: bigint): unknown;
  /** A value committed by an edit, shown for its cell until the source answers for the cell again. */
  edited(row: bigint, col: bigint, value: unknown): void;
  /** Whether values landed since the last call, so the frame should paint. */
  takeLanded(): boolean;
  /** The user's ID for a Key, if known. */
  rowId(key: bigint): RowId | undefined;
  colId(key: bigint): ColId | undefined;
  /** The Key for a user's ID, issuing one if it has none yet. */
  rowKey(id: RowId): bigint;
  colKey(id: ColId): bigint;
  /** Where the item with `key` sits now: from the last painted Strip if it is there, else from the Table Source. */
  find(axis: AxisName, key: bigint): Promise<Located>;
  /** Detaches from the source and aborts outstanding requests. */
  destroy(): void;
}

export interface FeedHooks<RowId = unknown, ColId = unknown> {
  /** A late answer landed or a Change Notice arrived: run a frame. */
  schedule?(): void;
  /** The Table Source failed a request; by default it is logged. */
  failed?(error: unknown, request: FailedRequest<RowId, ColId>): void;
  /** The Table Source answered a Locate the Grid asked for. */
  located?(axis: AxisName, key: bigint, answer: Located): void;
  /** Keys arrived in a Window on `axis` and the Grid has them. */
  windowed?(axis: AxisName, keys: BigUint64Array): void;
}

/** Every Change Notice type has its code. */
const NOTICE: Record<ChangeNotice["type"], number> = NOTICE_CODE;

/** The base view: nothing filtered, unsorted. */
const BASE_VIEW: KeyedViewSpec = { rows: { sort: [] }, cols: { sort: [] } };

interface Axis<Id> {
  source: AxisMethods<Id, unknown>;
  interner: KeyInterner<Id>;
  position: boolean;
  /** The Window asked for and not answered yet: not asked again, and aborted once another is asked. */
  waiting: { asked: string; request: Request } | null;
  /** The Window the source last failed: not asked again until a Change Notice on the axis. */
  failed: { asked: string; start: number; end: number } | null;
}

function axisOf<Id>(source: AxisMethods<Id, unknown> & { idKey?: (id: Id) => string | number | bigint }): Axis<Id> {
  const position = source.identity === "position";
  return {
    source,
    interner: position ? (positionInterner as unknown as KeyInterner<Id>) : createInterner(source.idKey),
    position,
    waiting: null,
    failed: null,
  };
}

interface Block {
  rows: bigint[];
  cols: bigint[];
  request: Request;
}

const known = <T>(v: T | undefined): v is T => v !== undefined;

const GONE: Located = { type: "Gone", suggest: null };

export function createFeed<RowId, ColId, V>(
  source: TableSource<RowId, ColId, V>,
  hooks: FeedHooks<RowId, ColId> = {},
): Feed<RowId, ColId> {
  const schedule = () => hooks.schedule?.();
  /** Reports a failure of `request` to the hook, or the console. */
  const report = (request: FailedRequest<RowId, ColId>) => (error: unknown) => {
    if (hooks.failed) hooks.failed(error, request);
    else console.error("binnacle: a Table Source request failed", request, error);
  };
  let landed = false;
  const settler = createSettler(
    () => {
      landed = true;
      schedule();
    },
    (error) => console.error("binnacle: a Table Source request failed", error),
  );
  const { settle } = settler;
  const rows = axisOf<RowId>(source.rows as AxisMethods<RowId, unknown>);
  const cols = axisOf<ColId>(source.cols as AxisMethods<ColId, unknown>);
  const axes: Record<AxisName, Axis<unknown>> = { rows: rows as Axis<unknown>, cols: cols as Axis<unknown> };
  const cache = createValueCache<V>();
  /** Committed edits by Key pair, until a fresh answer for the cell lands. */
  const edits = new Map<string, unknown>();
  const pair = (row: bigint, col: bigint) => `${row}:${col}`;
  let grid: WebGrid | null = null;
  let detach: (() => void) | undefined;
  let shown: Snapshot | null = null;
  /** The View Spec last applied, to restore if a request fails. */
  let applied: KeyedViewSpec = BASE_VIEW;
  /** Counts that arrived before the Grid did. */
  const early: Partial<Record<AxisName, number>> = {};
  /** Cell blocks in flight, aborted once none of their rows or columns is in view. */
  const blocks = new Set<Block>();
  /** The View Spec request in flight, aborted when a newer one is asked for. */
  let viewing: Request | null = null;
  /** Locates in flight by axis and Key, each aborted when the same Key is asked for again. */
  const locating = new Map<string, Request>();

  /** Tells the Grid the count on `axis`, with a Reset unless another counted notice is given. */
  const reset = (axis: AxisName, count: number, notice: "Reset" | "Refiltered" = "Reset") => {
    if (grid) grid.notify(AXIS_CODE[axis], NOTICE[notice], count, NaN);
    else early[axis] = count;
    schedule();
  };

  /** The count now if the source answers at once; a late one lands as a Reset. */
  const askCount = (axis: AxisName): number | undefined => {
    let now: number | undefined;
    let late = false;
    settle(
      () => axes[axis].source.count(),
      (count) => {
        if (late) reset(axis, count);
        else now = count;
      },
      { failed: report({ type: "count", axis }) },
    );
    late = true;
    return now;
  };

  /** Positions to the visible Keys at them, on `axis`, as the last painted Snapshot had them. */
  const visibleAt = (axis: AxisName, start: number, end: number) => {
    const keys = new Set<bigint>();
    const all = new Set<bigint>();
    for (const item of shown?.[axis].strip ?? []) {
      if (item.key === null) continue;
      all.add(item.key);
      if (item.position >= start && item.position < end) keys.add(item.key);
    }
    return { keys, all };
  };

  /** Applies a Change Notice on `axis`: the cache, the interner, then the Grid. */
  const notify = (axis: AxisName, notice: ChangeNotice) => {
    // Aborts the Windows and cells in flight; their `otherwise` forgets them.
    settler.bump();
    const { interner, position } = axes[axis];
    axes[axis].failed = null;
    switch (notice.type) {
      case "Reset":
        cache.clear();
        interner.rebuild();
        settle(() => axes[axis].source.count(), (count) => reset(axis, count), {
          fresh: true,
          failed: report({ type: "count", axis }),
        });
        return;
      case "Refiltered":
        // The same IDs, so values follow their Keys, unless the Keys are Positions.
        if (position) {
          cache.clear();
          edits.clear();
        }
        settle(() => axes[axis].source.count(), (count) => reset(axis, count, "Refiltered"), {
          fresh: true,
          failed: report({ type: "count", axis }),
        });
        return;
      case "ContentChanged": {
        // Visible items outside the range keep their values; the rest, which
        // cannot be placed, are asked for again when they come into view.
        const { keys, all } = visibleAt(axis, notice.start, notice.end);
        cache.retain(axis, (k) => all.has(k) && !keys.has(k));
        break;
      }
      default:
        // Values follow Keys through a reorder, unless the Keys are Positions.
        if (position) {
          cache.clear();
          edits.clear();
        }
    }
    const [a, b] =
      notice.type === "Inserted" || notice.type === "Removed"
        ? [notice.at, notice.n]
        : notice.type === "ContentChanged"
          ? [notice.start, notice.end]
          : [NaN, NaN];
    grid?.notify(AXIS_CODE[axis], NOTICE[notice.type], a, b);
    schedule();
  };

  const sinkFor = (axis: AxisName) => ({ notify: (notice: ChangeNotice) => notify(axis, notice) });

  const keysOf = <Id>(axis: Axis<Id>, ids: Id[]) => BigUint64Array.from(ids, (id) => axis.interner.key(id));

  /** Tells the Grid how the rows may be viewed by the columns with `keys`. */
  const markSupport = (keys: BigUint64Array) => {
    const view = source.view;
    for (const key of keys) {
      const id = cols.interner.id(key);
      const sort = view !== undefined && id !== undefined && (source.rows.support?.(id).sort ?? true);
      grid?.set_support(AXIS_CODE.rows, key, sort ? SUPPORT.sort : 0);
    }
  };

  const viewApplied = (spec: KeyedViewSpec) => {
    applied = spec;
    grid?.view_applied(spec);
  };

  /** A sort by Keys as IDs of `by`, or `undefined` if one is not known. */
  const sortIds = <Id>(by: Axis<Id>, sort: SortColumn[]): SortSpec<Id> | undefined => {
    const spec: SortSpec<Id> = [];
    for (const c of sort) {
      const id = by.interner.id(c.key);
      if (id === undefined) return undefined;
      spec.push({ id, direction: c.direction });
    }
    return spec;
  };

  const sortKeys = <Id>(by: Axis<Id>, sort: SortSpec<Id>): SortColumn[] =>
    sort.map((c) => ({ key: by.interner.key(c.id), direction: c.direction }));

  const answerView = (requested: KeyedViewSpec) => {
    const view = source.view;
    if (!view) return viewApplied(BASE_VIEW);
    const [rowSort, colSort] = [sortIds(cols, requested.rows.sort), sortIds(rows, requested.cols.sort)];
    if (!rowSort || !colSort) return viewApplied(applied);
    const spec: ViewSpec<RowId, ColId> = { rows: { sort: rowSort }, cols: { sort: colSort } };
    const previous = applied;
    // The Grid still counts the superseded request's answer: aborting it
    // answers with the spec in force.
    viewing?.abort();
    viewing = settle(
      (signal) => view.call(source, spec, signal),
      (now) =>
        viewApplied({
          rows: { sort: sortKeys(cols, now.rows.sort) },
          cols: { sort: sortKeys(rows, now.cols.sort) },
        }),
      { otherwise: () => viewApplied(previous), failed: report({ type: "view" }) },
    );
  };

  const located = (axis: AxisName, key: bigint, answer: Located) => {
    const [code, position] = answer.type === "Found" ? [0, answer.position] : [1, answer.suggest ?? NaN];
    grid?.located(AXIS_CODE[axis], key, code, position);
  };

  /** Asks where `key` sits now; `unanswered` runs if the source cannot say (aborted, failed, no such ID). */
  const whereIs = (
    axis: AxisName,
    key: bigint,
    answer: (located: Located) => void,
    unanswered: () => void,
  ): Request | null => {
    const { source: s, interner, position } = axes[axis];
    const id = interner.id(key);
    if (id === undefined) {
      unanswered();
      return null;
    }
    if (s.locate) {
      return settle((signal) => s.locate!(id, signal), answer, {
        otherwise: unanswered,
        failed: report({ type: "locate", axis }),
      });
    }
    // Like the Rust IdentitySource: a Position identity is where it says.
    const count = shown?.[axis].count ?? 0;
    answer(position && Number(key) < count ? { type: "Found", position: Number(key) } : GONE);
    return null;
  };

  const answerLocate = (axis: AxisName, key: bigint) => {
    // The Grid acts on the last answer it is due for this Key, so a
    // superseded request is answered at once, and ignored.
    const at = `${axis}:${key}`;
    locating.get(at)?.abort();
    const request = whereIs(
      axis,
      key,
      (answer) => {
        locating.delete(at);
        hooks.located?.(axis, key, answer);
        located(axis, key, answer);
        // The Grid may scroll at the next frame's start (a Refiltered keeping its place).
        schedule();
      },
      () => (locating.delete(at), located(axis, key, GONE)),
    );
    if (request?.pending) locating.set(at, request);
  };

  const answerMove = (axis: AxisName, keys: bigint[], to: number) => {
    const { source: s, interner } = axes[axis];
    const ids = keys.map((k) => interner.id(k));
    if (!s.move || !ids.every(known)) return;
    settle(
      () => s.move!(ids, to),
      (moved) => {
        if (moved) notify(axis, { type: "Reordered" });
      },
      { failed: report({ type: "move", axis }) },
    );
  };

  /** Asks for the block `rowKeys` by `colKeys`, marking it pending until it lands. */
  const ask = (rowKeys: bigint[], colKeys: bigint[]) => {
    const rowIds = rowKeys.map((k) => rows.interner.id(k));
    const colIds = colKeys.map((k) => cols.interner.id(k));
    if (!rowIds.every(known) || !colIds.every(known)) return;
    for (const r of rowKeys) for (const c of colKeys) cache.set(r, c, PENDING);
    let block: Block | undefined;
    const done = () => block && blocks.delete(block);
    const failed = report({ type: "cells", rowIds, colIds });
    const request = settle(
      (signal) => source.cells(rowIds, colIds, signal),
      (block) => {
        done();
        rowKeys.forEach((r, i) => {
          const values = block[i] ?? [];
          colKeys.forEach((c, j) => {
            if (cache.get(r, c) === PENDING) cache.set(r, c, values[j] as V);
            edits.delete(pair(r, c));
          });
        });
      },
      {
        fresh: true,
        otherwise: () => {
          done();
          for (const r of rowKeys) for (const c of colKeys) cache.unpend(r, c);
        },
        failed: (error) => {
          for (const r of rowKeys) for (const c of colKeys) if (cache.get(r, c) === PENDING) cache.set(r, c, FAILED);
          failed(error);
        },
      },
    );
    if (request.pending) blocks.add((block = { rows: rowKeys, cols: colKeys, request }));
  };

  /**
   * Asks for the visible cells the cache lacks, one block per set of missing
   * columns: scrolling down asks for new rows by every column, across for
   * every row by new columns, diagonally for both.
   */
  const request = (s: Snapshot) => {
    const rowKeys = s.rows.strip.flatMap((i) => (i.key === null ? [] : [i.key]));
    const colKeys = s.cols.strip.flatMap((i) => (i.key === null ? [] : [i.key]));
    const shownRows = new Set(rowKeys);
    const shownCols = new Set(colKeys);
    // A block none of whose rows (or columns) is in view any more is not wanted.
    for (const b of blocks) {
      if (!b.rows.some((r) => shownRows.has(r)) || !b.cols.some((c) => shownCols.has(c))) b.request.abort();
    }
    const missing = new Map<string, { rows: bigint[]; cols: bigint[] }>();
    for (const r of rowKeys) {
      const lacking = colKeys.filter((c) => cache.get(r, c) === undefined);
      if (!lacking.length) continue;
      const id = lacking.join(",");
      const block = missing.get(id);
      if (block) block.rows.push(r);
      else missing.set(id, { rows: [r], cols: lacking });
    }
    cache.prune(shownRows, shownCols);
    for (const block of missing.values()) ask(block.rows, block.cols);
  };

  const valueAt = (row: bigint, col: bigint): unknown => {
    const p = pair(row, col);
    if (edits.has(p)) return edits.get(p);
    const v = cache.get(row, col);
    return v === PENDING || v === FAILED ? undefined : v;
  };

  /** Whether the Window covering `item` failed, while its Key is missing. */
  const windowFailed = (axis: AxisName, item: StripItem) => {
    const failed = axes[axis].failed;
    return item.key === null && failed !== null && item.position >= failed.start && item.position < failed.end;
  };

  const failedAt = (row: StripItem | undefined, col: StripItem | undefined): boolean => {
    if (!row || !col) return false;
    if (windowFailed("rows", row) || windowFailed("cols", col)) return true;
    return row.key !== null && col.key !== null && cache.get(row.key, col.key) === FAILED;
  };

  const value = (row: StripItem | undefined, col: StripItem | undefined): unknown =>
    row?.key == null || col?.key == null ? undefined : valueAt(row.key, col.key);

  const content: Content = {
    value: (i, j) => value(shown?.rows.strip[i], shown?.cols.strip[j]),
    failed: (i, j) => failedAt(shown?.rows.strip[i], shown?.cols.strip[j]),
    label: (axis, item) => {
      const { source: s, interner } = axes[axis];
      const id = item.key === null ? undefined : interner.id(item.key);
      const label = id === undefined ? undefined : s.label?.(id, item.position);
      if (label !== undefined) return label;
      return axis === "rows" || id === undefined ? String(item.position + 1) : String(id);
    },
    id: (axis, item) => (item.key === null ? undefined : axes[axis].interner.id(item.key)),
  };

  return {
    counts: () => ({ rows: askCount("rows") ?? 0, cols: askCount("cols") ?? 0 }),
    connect(g) {
      grid = g;
      for (const axis of AXES) {
        const count = early[axis];
        if (count !== undefined) reset(axis, count);
      }
      detach = source.attach?.({ rows: sinkFor("rows"), cols: sinkFor("cols") });
    },
    fill(g, requests) {
      for (const name of AXES) {
        const i = AXIS_CODE[name];
        const start = requests[i * 2]!;
        const end = requests[i * 2 + 1]!;
        if (Number.isNaN(start)) continue;
        const axis = axes[name];
        const asked = `${start}:${end}`;
        if (axis.waiting?.asked === asked || axis.failed?.asked === asked) continue;
        axis.waiting?.request.abort();
        const request = settle(
          (signal) => axis.source.window(start, end, signal),
          (ids) => {
            axis.waiting = null;
            axis.failed = null;
            const keys = keysOf(axis, ids);
            (grid ?? g).set_window(i, start, keys);
            if (name === "cols") markSupport(keys);
            hooks.windowed?.(name, keys);
          },
          {
            fresh: true,
            otherwise: () => (axis.waiting = null),
            failed: (error) => {
              axis.failed = { asked, start, end };
              report({ type: "window", axis: name, start, end })(error);
            },
          },
        );
        if (request.pending) axis.waiting = { asked, request };
      }
    },
    answer(effect) {
      switch (effect.type) {
        case "ViewRequested":
          answerView(effect.spec);
          return true;
        case "Locate":
          answerLocate(effect.axis, effect.key);
          return true;
        case "MoveRequested":
          answerMove(effect.axis, effect.keys, effect.to);
          return true;
        default:
          return false;
      }
    },
    content(s) {
      shown = s;
      request(s);
      return content;
    },
    loading(s) {
      // The Strips include overscan; only what the viewport shows counts.
      const inView = (axis: AxisName) =>
        s[axis].strip.filter((i) => i.offset + i.size > s[axis].header && i.offset < s[axis].extent);
      const cols = inView("cols");
      return inView("rows").some((r) => cols.some((c) => value(r, c) === undefined && !failedAt(r, c)));
    },
    value: valueAt,
    edited(row, col, value) {
      edits.set(pair(row, col), value);
      landed = true;
    },
    takeLanded() {
      const was = landed;
      landed = false;
      return was;
    },
    rowId: (key) => rows.interner.id(key),
    colId: (key) => cols.interner.id(key),
    rowKey: (id) => rows.interner.key(id),
    colKey: (id) => cols.interner.key(id),
    find(axis, key) {
      const item = shown?.[axis].strip.find((i) => i.key === key);
      if (item) return Promise.resolve({ type: "Found", position: item.position });
      return new Promise((resolve) => whereIs(axis, key, resolve, () => resolve(GONE)));
    },
    destroy() {
      detach?.();
      settler.close();
    },
  };
}
