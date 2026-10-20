// Plumbing between a Table Source and the Grid: counts, Window answers
// interned into Keys, the value cache, Change Notices from the source's
// sinks, and the answers to the Effects a Table Source owns (View Specs,
// Locate, move, find steps), and find's marks and count. Every answer goes
// through one `settle`.

import { AXES, AXIS_CODE, FIND, NOTICE_CODE, QUICK_FILTER, SUPPORT, other, type AxisName } from "./codes";
import { createValueCache, FAILED, PENDING } from "./cache";
import type { Content } from "./content";
import type { Effect } from "./effects";
import { createInterner, positionInterner, type KeyInterner } from "./interner";
import { createSettler, type Request } from "./settle";
import type {
  KeyedAxisView,
  KeyedCondition,
  KeyedFindQuery,
  KeyedTextQuery,
  KeyedViewSpec,
  Snapshot,
  SortColumn,
  StripItem,
} from "./snapshot";
import type {
  AppliedAxisView,
  AppliedView,
  AxisMethods,
  AxisView,
  ChangeNotice,
  Condition,
  Distinct,
  FailedRequest,
  FilterSpec,
  FindCount,
  FindQuery,
  Located,
  SortSpec,
  TableSource,
  TextQuery,
  Totals,
  ViewSpec,
} from "./source";
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
  /** A View Spec by Keys, issuing Keys for IDs that have none yet. */
  viewKeys(spec: AppliedView<RowId, ColId>): KeyedViewSpec;
  /** How the rows may be viewed by column `id`, as the source declares it with the defaults filled in. */
  columnSupport(id: ColId): ColumnSupport;
  /** How the columns may be viewed by row `id`, likewise; nothing without the source's `cols.support`. */
  rowSupport(id: RowId): ColumnSupport;
  /** A condition on `axis` (the rows by default) by Keys, issuing a Key for the item it reads if it has none yet. */
  conditionKey<A extends AxisName = "rows">(condition: Condition<CrossId<A, RowId, ColId>>, axis?: A): KeyedCondition;
  /** A condition on `axis` by the ID of the item it reads, if that item and the kind are known. */
  conditionId<A extends AxisName = "rows">(condition: KeyedCondition, axis?: A): Condition<CrossId<A, RowId, ColId>> | undefined;
  /**
   * Asks the source for up to `limit` distinct values of item `id` of the
   * other axis across `axis` (column `id` across the rows by default)
   * containing `search`, narrowed by the axis's applied filter less the
   * item's own conditions; `apply` gets the answer. `null` if the item takes
   * no OneOf conditions. Abort the request when its answer is no longer
   * wanted, such as when the search changes.
   */
  distinct<A extends AxisName = "rows">(
    id: CrossId<A, RowId, ColId>,
    search: string,
    limit: number,
    apply: (answer: Distinct) => void,
    axis?: A,
  ): Request | null;
  /** Where the item with `key` sits now: from the last painted Strip if it is there, else from the Table Source. */
  find(axis: AxisName, key: bigint): Promise<Located>;
  /** A Text Query on `axis` (the rows by default) by the IDs of the items it searches, if every one it names is known. */
  queryIds<A extends AxisName = "rows">(query: KeyedTextQuery, axis?: A): TextQuery<CrossId<A, RowId, ColId>> | undefined;
  /** A Find Query by Keys, issuing Keys for columns that have none yet. */
  findKeys(query: FindQuery<ColId>): KeyedFindQuery;
  /** A Find Query by IDs, if every column it names is known. */
  findIds(query: KeyedFindQuery): FindQuery<ColId> | undefined;
  /** Whether the item with `key` on `axis` is unmatched, as the Table Source last said. */
  unmatched(axis: AxisName, key: bigint): boolean;
  /** The count for the Find Query in force (`null` until the source gives one), and where the last step landed among the matches. */
  findState(): FindState;
  /**
   * The Table Source's message for the last View Spec it failed or answered
   * with part of the request left out; `null` once an answer leaves nothing
   * out.
   */
  viewError(): string | null;
  /** Detaches from the source and aborts outstanding requests. */
  destroy(): void;
}

/** What the Table Source last said about the Find Query in force. */
export interface FindState {
  count: FindCount | null;
  /** Whether the count is asked for and not answered yet. */
  counting: boolean;
  /** The last step's 0-based index among the matches (`null` if the source gave none) and whether it wrapped; `null` before a step lands. */
  step: { index: number | null; wrapped: boolean } | null;
  /** The Table Source's message if it failed a find request for the Find Query in force (an invalid regex, say). */
  error: string | null;
}

/** The IDs of the items an axis's View Spec part names: the columns for the rows, the rows for the columns. */
export type CrossId<A extends AxisName, RowId, ColId> = A extends "cols" ? RowId : ColId;

/**
 * How the rows may be viewed by one column (or the columns by one row),
 * every kind decided: sorting, Text conditions (with regexes or not, or
 * `null` for none), and OneOf conditions from a checklist of its values.
 */
export interface ColumnSupport {
  sort: boolean;
  text: { regex: boolean } | null;
  oneOf: boolean;
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
const BASE_VIEW: KeyedViewSpec = {
  rows: { filter: { quick: null, conditions: [] }, sort: [] },
  cols: { filter: { quick: null, conditions: [] }, sort: [] },
};

interface Axis<Id> {
  source: AxisMethods<Id, unknown>;
  interner: KeyInterner<Id>;
  position: boolean;
  /** The Window asked for and not answered yet: not asked again, and aborted once another is asked. */
  waiting: { asked: string; request: Request } | null;
  /** The Window the source last failed: not asked again until a Change Notice on the axis. */
  failed: { asked: string; start: number; end: number } | null;
  /** Whether each item is unmatched, by Key, as the source last said and the round it was asked in. */
  unmatched: Map<bigint, Flag>;
  /** Bumped when the flags may have changed (a Change Notice, a View Spec): each item shown is asked again. */
  flagRound: number;
  /** The flags asked for and not answered yet. */
  flagging: { keys: Set<bigint>; round: number; request: Request } | null;
}

/** An unmatched flag the source gave, and the round it was asked in: one from an earlier round is shown until it is asked again. */
interface Flag {
  unmatched: boolean;
  round: number;
}

function axisOf<Id>(source: AxisMethods<Id, unknown> & { idKey?: (id: Id) => string | number | bigint }): Axis<Id> {
  const position = source.identity === "position";
  return {
    source,
    interner: position ? (positionInterner as unknown as KeyInterner<Id>) : createInterner(source.idKey),
    position,
    waiting: null,
    failed: null,
    unmatched: new Map(),
    flagRound: 0,
    flagging: null,
  };
}

interface Block {
  rows: bigint[];
  cols: bigint[];
  request: Request;
}

const known = <T>(v: T | undefined): v is T => v !== undefined;

const NO_SUPPORT: ColumnSupport = { sort: false, text: null, oneOf: false };

const GONE: Located = { type: "Gone", suggest: null };

/** A mark the source gave, and the round it was asked in: one from an earlier round is shown until it is asked again. */
interface Mark {
  marked: boolean;
  round: number;
}

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
  /** The axis whose items `axis`'s View Spec part names. */
  const cross = (axis: AxisName) => axes[other(axis)];
  const cache = createValueCache<V>();
  /** Committed edits by Key pair, until a fresh answer for the cell lands. */
  const edits = new Map<string, unknown>();
  const pair = (row: bigint, col: bigint) => `${row}:${col}`;
  let grid: WebGrid | null = null;
  let detach: (() => void) | undefined;
  let shown: Snapshot | null = null;
  /** The View Spec last applied and its totals, to restore if a request fails. */
  let applied: KeyedViewSpec = BASE_VIEW;
  let totals: Totals = {};
  /** Counts that arrived before the Grid did. */
  const early: Partial<Record<AxisName, number>> = {};
  /** Cell blocks in flight, aborted once none of their rows or columns is in view. */
  const blocks = new Set<Block>();
  /** The View Spec request in flight, aborted when a newer one is asked for. */
  let viewing: Request | null = null;
  /** Locates in flight by axis and Key, each aborted when the same Key is asked for again. */
  const locating = new Map<string, Request>();
  const finder = source.find;
  /** The Find Query in force by IDs; `null` while find is closed or the query names a column not known. */
  let finding: FindQuery<ColId> | null = null;
  /** Bumped with each new Find Query, so a step for an older one leaves `findState` alone. */
  let findEpoch = 0;
  /** Find's marks by Key pair, for the query in force. */
  const marks = new Map<string, Mark>();
  /** Bumped when the marks may have changed (a Change Notice, a new View Spec): each visible cell is asked again. */
  let markRound = 0;
  /** Mark blocks in flight, aborted with the query or once out of view. */
  const markBlocks = new Set<Block>();
  const marking = new Set<string>();
  let counting: Request | null = null;
  /** Whether the count should be asked for again at the next frame (the rows or the View Spec changed). */
  let recount = false;
  let findState: FindState = { count: null, counting: false, step: null, error: null };
  let viewError: string | null = null;

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
    // A change to one axis's items can change which items of the other the
    // filter in force keeps, so both axes' flags are asked again.
    for (const a of AXES) reflag(a);
    // Items may come or go on either axis, except in a reorder.
    remark(notice.type !== "Reordered");
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
        // The same IDs, so values follow their Keys, unless the Keys are
        // Positions. Only the items shown keep theirs: one out of view may
        // have changed while filtered out, with no Position to say so.
        if (position) {
          cache.clear();
          edits.clear();
        } else {
          const { all } = visibleAt(axis, 0, 0);
          cache.retain(axis, (k) => all.has(k));
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

  /** Asks the source again how `axis` may be viewed by the items `ids` of the other axis, or by every one in view. */
  const supportChanged = (axis: AxisName, ids?: unknown[]) => {
    const by = cross(axis);
    const keys = ids
      ? ids.map((id) => by.interner.key(id))
      : (shown?.[other(axis)].strip ?? []).flatMap((i) => (i.key === null ? [] : [i.key]));
    markSupport(axis, BigUint64Array.from(keys));
    schedule();
  };

  const sinkFor = (axis: AxisName) => ({
    notify: (notice: ChangeNotice) => notify(axis, notice),
    supportChanged: (ids?: unknown[]) => supportChanged(axis, ids),
  });

  /** What a failure says, for the control it came from. */
  const message = (error: unknown) => (error instanceof Error ? error.message : String(error));

  const keysOf = <Id>(axis: Axis<Id>, ids: Id[]) => BigUint64Array.from(ids, (id) => axis.interner.key(id));

  /**
   * How `axis` may be viewed by item `id` of the other axis, defaults filled
   * in: nothing without `view`, and on the columns nothing without
   * `cols.support`, so a source that does not filter its columns offers no
   * controls for it.
   */
  const supportOf = (axis: AxisName, id: unknown): ColumnSupport => {
    const methods = axes[axis].source;
    if (source.view === undefined || (axis === "cols" && !methods.support)) return NO_SUPPORT;
    const support = methods.support?.(id) ?? {};
    const text = support.text ?? {};
    const listed = methods.distinct !== undefined;
    return {
      sort: support.sort ?? true,
      text: text ? { regex: text.regex ?? false } : null,
      oneOf: listed && (support.oneOf ?? true),
    };
  };
  const columnSupport = (id: ColId) => supportOf("rows", id);
  const rowSupport = (id: RowId) => supportOf("cols", id);

  /** Tells the Grid how `axis` may be viewed by the items of the other axis with `keys`. */
  const markSupport = (axis: AxisName, keys: BigUint64Array) => {
    const by = cross(axis);
    for (const key of keys) {
      const id = by.interner.id(key);
      const support = id === undefined ? NO_SUPPORT : supportOf(axis, id);
      const text = support.text ? SUPPORT.text | (support.text.regex ? SUPPORT.textRegex : 0) : 0;
      const oneOf = support.oneOf ? SUPPORT.oneOf : 0;
      grid?.set_support(AXIS_CODE[axis], key, (support.sort ? SUPPORT.sort : 0) | text | oneOf);
    }
  };

  const viewApplied = (spec: KeyedViewSpec, now: Totals = {}) => {
    applied = spec;
    totals = now;
    grid?.view_applied(spec, now.rows ?? NaN, now.cols ?? NaN);
    for (const axis of AXES) reflag(axis);
    remark(true);
  };

  /** The unmatched flags on `axis` may have changed: the items shown are asked again on the next paint, which this brings about. */
  const reflag = (axis: AxisName) => {
    axes[axis].flagRound++;
    if (axes[axis].source.unmatched) landed = true;
  };

  /** Asks which of the items shown on `axis` are unmatched, for those that lack a flag from this round. */
  const askUnmatched = (axis: AxisName, keys: bigint[]) => {
    const a = axes[axis];
    const ask = a.source.unmatched;
    if (!ask) return;
    const round = a.flagRound;
    const lacking = keys.filter((k) => a.unmatched.get(k)?.round !== round);
    const busy = a.flagging;
    if (!lacking.length || (busy?.round === round && lacking.every((k) => busy.keys.has(k)))) return;
    busy?.request.abort();
    a.flagging = null;
    const ids = lacking.map((k) => a.interner.id(k));
    if (!ids.every(known)) return;
    if (a.unmatched.size > 4 * Math.max(256, keys.length)) {
      const shownKeys = new Set(keys);
      for (const k of [...a.unmatched.keys()]) if (!shownKeys.has(k)) a.unmatched.delete(k);
    }
    let asking: Axis<unknown>["flagging"] = null;
    const done = () => {
      if (a.flagging === asking) a.flagging = null;
    };
    const request = settle(
      (signal) => ask.call(a.source, ids, signal),
      (answer) => {
        done();
        lacking.forEach((k, i) => a.unmatched.set(k, { unmatched: answer[i] === true, round }));
      },
      { fresh: true, otherwise: done, failed: report({ type: "unmatched", axis }) },
    );
    if (request.pending) a.flagging = asking = { keys: new Set(lacking), round, request };
  };

  /** The source's news: what it applies now, or new totals; answers due stay due. */
  const viewChanged = (view: AppliedView<RowId, ColId>) => {
    applied = { rows: partKeys(cols, view.rows), cols: partKeys(rows, view.cols) };
    totals = view.totals ?? totals;
    grid?.view_changed(applied, totals.rows ?? NaN, totals.cols ?? NaN);
    if (view.error !== undefined) viewError = view.error;
    remark(true);
    landed = true;
    schedule();
  };

  /** A failure of a find request for the query `epoch` names, shown on the find bar while that query is in force. */
  const findFailed = (request: "marks" | "next" | "count", epoch: number) => (error: unknown) => {
    report({ type: "find", request })(error);
    if (epoch === findEpoch) findState = { ...findState, error: message(error) };
  };

  /** Asks for the count of the Find Query in force, in place of any asked before. */
  const askFindCount = () => {
    recount = false;
    counting?.abort();
    counting = null;
    const query = finding;
    if (!finder?.count || !query) return;
    const request = settle(
      (signal) => finder.count!.call(finder, query, signal),
      (count) => {
        counting = null;
        findState = { ...findState, count, counting: false };
      },
      {
        otherwise: () => {
          if (counting === request) counting = null;
          findState = { ...findState, counting: counting !== null };
        },
        failed: findFailed("count", findEpoch),
      },
    );
    counting = request.pending ? request : null;
    findState = { ...findState, counting: counting !== null };
  };

  /** A running count pushed for `query`, kept while it is the query in force. */
  const findCounted = (query: FindQuery<ColId>, count: FindCount) => {
    if (query !== finding) return;
    findState = { ...findState, count };
    landed = true;
    schedule();
  };

  /**
   * The marks may have changed: each visible cell is asked again, its old
   * mark shown meanwhile; with `again`, the count too, once at the next
   * frame however many changes arrive before it.
   */
  const remark = (again: boolean) => {
    if (!finding) return;
    markRound++;
    if (again) recount = true;
  };

  /** A new Find Query, or find closed: the old marks go at once. */
  const answerFind = (query: KeyedFindQuery | null) => {
    for (const b of markBlocks) b.request.abort();
    marks.clear();
    markRound++;
    findEpoch++;
    finding = (query && findIds(query)) ?? null;
    findState = { count: null, counting: false, step: null, error: null };
    askFindCount();
    landed = true;
    schedule();
  };

  /** Answers a step, always exactly once: `null` if the source cannot step or fails. */
  const answerFindStep = (query: KeyedFindQuery, from: { row: bigint; col: bigint } | null, direction: "next" | "previous") => {
    const ids = findIds(query);
    const row = from && rows.interner.id(from.row);
    const col = from && cols.interner.id(from.col);
    const fromIds = row !== undefined && row !== null && col !== undefined && col !== null ? { row, col } : null;
    const answer = (found: { row: bigint; col: bigint } | null) => {
      grid?.find_stepped(found);
      schedule();
    };
    if (!finder || !ids) return answer(null);
    const epoch = findEpoch;
    settle(
      (signal) => finder.next.call(finder, fromIds, direction, ids, signal),
      (step) => {
        if (step && epoch === findEpoch) {
          findState = { ...findState, step: { index: step.index ?? null, wrapped: step.wrapped ?? false } };
        }
        answer(step && { row: rows.interner.key(step.row), col: cols.interner.key(step.col) });
      },
      { otherwise: () => answer(null), failed: findFailed("next", epoch) },
    );
  };

  /** Tells the Grid which axes take a quick filter: only with `view`. */
  const markQuickFilter = (g: WebGrid) => {
    for (const axis of AXES) {
      const quick = source.view ? axes[axis].source.quickFilter : undefined;
      const bits = quick ? QUICK_FILTER.on | (quick.regex ? QUICK_FILTER.regex : 0) : 0;
      g.set_quick_filter_support(AXIS_CODE[axis], bits);
    }
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

  /** A Text Query by Keys as IDs of `by`, or `undefined` if a column it names is not known. */
  const queryIds = <Id>(by: Axis<Id>, q: KeyedTextQuery): TextQuery<Id> | undefined => {
    const ids = q.searches?.map((k) => by.interner.id(k));
    if (ids && !ids.every(known)) return undefined;
    return { text: q.text, caseSensitive: q.caseSensitive, regex: q.regex, searches: ids ?? "shown" };
  };

  const queryKeys = <Id>(by: Axis<Id>, q: TextQuery<Id>): KeyedTextQuery => ({
    text: q.text,
    caseSensitive: q.caseSensitive,
    regex: q.regex,
    searches: q.searches === "shown" ? null : q.searches.map((id) => by.interner.key(id)),
  });

  const findIds = (q: KeyedFindQuery): FindQuery<ColId> | undefined => {
    const query = queryIds(cols, q.query);
    return query && { query, by: q.by };
  };

  /** A condition by its column's ID in `by`, or `undefined` if the column or the kind is not known. */
  const conditionId = <Id>(by: Axis<Id>, c: KeyedCondition): Condition<Id> | undefined => {
    const id = by.interner.id(c.key);
    if (id === undefined) return undefined;
    switch (c.kind.type) {
      case "Text": {
        const { text, caseSensitive, regex } = c.kind.query;
        return { id, kind: { type: "Text", query: { text, caseSensitive, regex } }, not: c.not };
      }
      case "OneOf":
        return { id, kind: { type: "OneOf", values: [...c.kind.values] }, not: c.not };
      default:
        return undefined;
    }
  };

  const conditionKey = <Id>(by: Axis<Id>, c: Condition<Id>): KeyedCondition => {
    const key = by.interner.key(c.id);
    if (c.kind.type === "OneOf") return { key, kind: { type: "OneOf", values: [...c.kind.values] }, not: c.not };
    const { text, caseSensitive, regex } = c.kind.query;
    return { key, kind: { type: "Text", query: { text, caseSensitive, regex, searches: [key] } }, not: c.not };
  };

  /** One axis's part by IDs of `by`, or `undefined` if an item it names is not known. */
  const partIds = <Id>(by: Axis<Id>, part: KeyedAxisView): AxisView<Id> | undefined => {
    const sort = sortIds(by, part.sort);
    const quick = part.filter.quick && queryIds(by, part.filter.quick);
    const conditions = part.filter.conditions.map((c) => conditionId(by, c));
    if (!sort || quick === undefined || !conditions.every(known)) return undefined;
    return { filter: { quick, conditions }, sort };
  };

  const partKeys = <Id>(by: Axis<Id>, part: AppliedAxisView<Id>): KeyedAxisView => {
    const quick = part.filter?.quick;
    const conditions = (part.filter?.conditions ?? []).map((c) => conditionKey(by, c));
    return { filter: { quick: quick ? queryKeys(by, quick) : null, conditions }, sort: sortKeys(by, part.sort) };
  };

  /** The applied filter of `axis` by IDs less the conditions on item `key`, leaving out parts naming unknown items. */
  const narrowing = (axis: AxisName, key: bigint): FilterSpec<unknown> => {
    const by = cross(axis);
    const filter = applied[axis].filter;
    const quick = filter.quick && queryIds(by, filter.quick);
    const conditions = filter.conditions.filter((c) => c.key !== key).map((c) => conditionId(by, c));
    return { quick: quick ?? null, conditions: conditions.filter(known) };
  };

  const distinct = (
    id: unknown,
    search: string,
    limit: number,
    apply: (answer: Distinct) => void,
    axis: AxisName = "rows",
  ): Request | null => {
    const methods = axes[axis].source;
    const ask = methods.distinct;
    if (!ask || !supportOf(axis, id).oneOf) return null;
    const narrow = narrowing(axis, cross(axis).interner.key(id));
    return settle((signal) => ask.call(methods, id, { search, limit, narrow }, signal), apply, {
      failed: report({ type: "distinct", axis }),
    });
  };

  const answerView = (requested: KeyedViewSpec) => {
    const view = source.view;
    if (!view) return viewApplied(BASE_VIEW);
    const [rowPart, colPart] = [partIds(cols, requested.rows), partIds(rows, requested.cols)];
    if (!rowPart || !colPart) return viewApplied(applied, totals);
    const failed = report({ type: "view" });
    const spec: ViewSpec<RowId, ColId> = { rows: rowPart, cols: colPart };
    const previous = { applied, totals };
    // The Grid still counts the superseded request's answer: aborting it
    // answers with the spec in force.
    viewing?.abort();
    viewing = settle(
      (signal) => view.call(source, spec, signal),
      (now) => {
        viewError = now.error ?? null;
        if (now.error !== undefined) failed(new Error(now.error));
        viewApplied({ rows: partKeys(cols, now.rows), cols: partKeys(rows, now.cols) }, now.totals ?? {});
      },
      {
        otherwise: () => viewApplied(previous.applied, previous.totals),
        failed: (error) => {
          viewError = message(error);
          failed(error);
        },
      },
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

  /** Asks for find's marks over the block `rowKeys` by `colKeys`. */
  const askMarks = (query: FindQuery<ColId>, rowKeys: bigint[], colKeys: bigint[]) => {
    const rowIds = rowKeys.map((k) => rows.interner.id(k));
    const colIds = colKeys.map((k) => cols.interner.id(k));
    if (!finder || !rowIds.every(known) || !colIds.every(known)) return;
    const round = markRound;
    const pairs = rowKeys.flatMap((r) => colKeys.map((c) => pair(r, c)));
    for (const p of pairs) marking.add(p);
    let block: Block | undefined;
    const done = () => {
      if (block) markBlocks.delete(block);
      for (const p of pairs) marking.delete(p);
    };
    const request = settle(
      (signal) => finder.marks.call(finder, rowIds, colIds, query, signal),
      (answer) => {
        done();
        if (query !== finding) return;
        rowKeys.forEach((r, i) =>
          colKeys.forEach((c, j) => marks.set(pair(r, c), { marked: answer[i]?.[j] === true, round })),
        );
      },
      { fresh: true, otherwise: done, failed: findFailed("marks", findEpoch) },
    );
    if (request.pending) markBlocks.add((block = { rows: rowKeys, cols: colKeys, request }));
  };

  /** Asks for the marks of the visible cells that lack one from this round, grouped as `request` groups values. */
  const requestMarks = (rowKeys: bigint[], colKeys: bigint[]) => {
    const query = finding;
    if (!query) return;
    const shownRows = new Set(rowKeys);
    const shownCols = new Set(colKeys);
    for (const b of markBlocks) {
      if (!b.rows.some((r) => shownRows.has(r)) || !b.cols.some((c) => shownCols.has(c))) b.request.abort();
    }
    const missing = new Map<string, { rows: bigint[]; cols: bigint[] }>();
    for (const r of rowKeys) {
      const lacking = colKeys.filter((c) => {
        const p = pair(r, c);
        return marks.get(p)?.round !== markRound && !marking.has(p);
      });
      if (!lacking.length) continue;
      const id = lacking.join(",");
      const block = missing.get(id);
      if (block) block.rows.push(r);
      else missing.set(id, { rows: [r], cols: lacking });
    }
    if (marks.size > 4 * Math.max(256, shownRows.size * shownCols.size)) {
      for (const p of [...marks.keys()]) {
        const [r, c] = p.split(":").map(BigInt) as [bigint, bigint];
        if (!shownRows.has(r) || !shownCols.has(c)) marks.delete(p);
      }
    }
    for (const block of missing.values()) askMarks(query, block.rows, block.cols);
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
    requestMarks(rowKeys, colKeys);
    askUnmatched("rows", rowKeys);
    askUnmatched("cols", colKeys);
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

  const unmatched = (axis: AxisName, key: bigint) => axes[axis].unmatched.get(key)?.unmatched === true;

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
    marked: (i, j) => {
      const row = shown?.rows.strip[i]?.key;
      const col = shown?.cols.strip[j]?.key;
      return finding !== null && row != null && col != null && marks.get(pair(row, col))?.marked === true;
    },
    unmatched: (axis, i) => {
      const key = shown?.[axis].strip[i]?.key;
      return key != null && unmatched(axis, key);
    },
  };

  return {
    counts: () => ({ rows: askCount("rows") ?? 0, cols: askCount("cols") ?? 0 }),
    connect(g) {
      grid = g;
      markQuickFilter(g);
      g.set_find_support(finder ? FIND.on | (finder.regex ? FIND.regex : 0) : 0);
      for (const axis of AXES) {
        const count = early[axis];
        if (count !== undefined) reset(axis, count);
      }
      detach = source.attach?.({ rows: sinkFor("rows"), cols: sinkFor("cols"), viewChanged, findCounted });
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
            markSupport(other(name), keys);
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
        case "FindRequested":
          answerFind(effect.query);
          return true;
        case "FindStepRequested":
          answerFindStep(effect.query, effect.from, effect.direction);
          return true;
        default:
          return false;
      }
    },
    content(s) {
      shown = s;
      request(s);
      if (recount) askFindCount();
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
    viewKeys: (spec) => ({ rows: partKeys(cols, spec.rows), cols: partKeys(rows, spec.cols) }),
    columnSupport,
    rowSupport,
    conditionKey: (c, axis) => conditionKey(cross(axis ?? "rows"), c as Condition<unknown>),
    conditionId: <A extends AxisName>(c: KeyedCondition, axis?: A) =>
      conditionId(cross(axis ?? "rows"), c) as Condition<CrossId<A, RowId, ColId>> | undefined,
    distinct: (id, search, limit, apply, axis) => distinct(id, search, limit, apply, axis),
    find(axis, key) {
      const item = shown?.[axis].strip.find((i) => i.key === key);
      if (item) return Promise.resolve({ type: "Found", position: item.position });
      return new Promise((resolve) => whereIs(axis, key, resolve, () => resolve(GONE)));
    },
    queryIds: <A extends AxisName>(q: KeyedTextQuery, axis?: A) =>
      queryIds(cross(axis ?? "rows"), q) as TextQuery<CrossId<A, RowId, ColId>> | undefined,
    findKeys: (q) => ({ query: queryKeys(cols, q.query), by: q.by }),
    findIds,
    findState: () => findState,
    unmatched,
    viewError: () => viewError,
    destroy() {
      detach?.();
      settler.close();
    },
  };
}
