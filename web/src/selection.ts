// Selection events: the focus, Selection and Picks of each frame by the
// Table Source's own IDs, handed on only when they differ from the last.

import type { Feed } from "./feed";
import { covers, stripOf, type AxisSnapshot, type Snapshot, type Span } from "./snapshot";

/** What `onSelectionChange` receives: the selection by the Table Source's own IDs. */
export interface SelectionChange<RowId = unknown, ColId = unknown> {
  /** The focused cell, if any. */
  focus: { rowId: RowId; colId: ColId } | null;
  /** The selected range, if any. */
  selection: { rows: AxisSelection<RowId>; cols: AxisSelection<ColId> } | null;
  /** The picked rows and columns, in no particular order. */
  picked: { rows: RowId[]; cols: ColId[] };
}

/**
 * One axis of the selected range. The grid holds IDs only for the items near
 * the viewport, so a range reaching beyond them gives its count and whichever
 * ends are known, and `ids` is `null`.
 */
export interface AxisSelection<Id> {
  count: number;
  first: Id | null;
  last: Id | null;
  /** Every ID in the range, in order, when all are known. */
  ids: Id[] | null;
}

type Ids<RowId, ColId> = Pick<Feed<RowId, ColId>, "rowId" | "colId">;

/** Returns a frame observer that calls `emit` when a frame's selection differs from the last one emitted. */
export function createSelectionEvents<RowId, ColId>(
  feed: Ids<RowId, ColId>,
  emit: (change: SelectionChange<RowId, ColId>) => void,
): (s: Snapshot) => void {
  let last: SelectionChange<RowId, ColId> = { focus: null, selection: null, picked: { rows: [], cols: [] } };
  return (s) => {
    const next = selectionChange(s, feed);
    if (same(last, next)) return;
    last = next;
    emit(next);
  };
}

function selectionChange<RowId, ColId>(s: Snapshot, feed: Ids<RowId, ColId>): SelectionChange<RowId, ColId> {
  const rowId = (key: bigint) => feed.rowId(key);
  const colId = (key: bigint) => feed.colId(key);
  const row = s.rows.focus?.key;
  const col = s.cols.focus?.key;
  const focusRow = row == null ? undefined : rowId(row);
  const focusCol = col == null ? undefined : colId(col);
  const range = s.selectionRange;
  return {
    focus: focusRow === undefined || focusCol === undefined ? null : { rowId: focusRow, colId: focusCol },
    selection: range && {
      rows: axisSelection(s.rows, range.rows, rowId),
      cols: axisSelection(s.cols, range.cols, colId),
    },
    picked: {
      rows: s.picked.rows.map(rowId).filter((id) => id !== undefined),
      cols: s.picked.cols.map(colId).filter((id) => id !== undefined),
    },
  };
}

function axisSelection<Id>(axis: AxisSnapshot, runs: readonly Span[], id: (key: bigint) => Id | undefined): AxisSelection<Id> {
  // The range is contiguous in drawn order, and the strip lists its items in
  // drawn order, so the known IDs come in order. Where the range starts in
  // drawn order is known when the item drawn before the first known one is
  // in the strip too (or there is none), likewise after the last, or when
  // the known ones span the whole count.
  const drawn = drawnIndices(axis);
  const known = new Map<number, Id>();
  for (const [i, item] of axis.strip.entries()) {
    if (!covers(runs, item.position) || item.key === null) continue;
    const it = id(item.key);
    if (it !== undefined) known.set(drawn[i]!, it);
  }
  const count = runs.reduce((n, r) => n + r.end - r.start, 0);
  const listed = new Set(drawn);
  const indices = [...known.keys()];
  const [head, tail] = [indices[0], indices.at(-1)];
  let start: number | null = null;
  if (head !== undefined && tail !== undefined) {
    if (head === 0 || listed.has(head - 1) || tail - head + 1 === count) start = head;
    else if (tail === axis.count - 1 || listed.has(tail + 1)) start = tail - count + 1;
  }
  const at = (index: number) => known.get(index) ?? null;
  const ids = known.size === count ? [...known.values()] : null;
  return { count, first: start === null ? null : at(start), last: start === null ? null : at(start + count - 1), ids };
}

/** The index in drawn order of each strip item: every Position is drawn once, pinned or scrolling. */
function drawnIndices(axis: AxisSnapshot): number[] {
  const scrollingEnd = axis.strip.length - axis.trailing;
  const pins = axis.strip.filter((_, i) => stripOf(axis, i) !== "scrolling").map((item) => item.position);
  return axis.strip.map((item, i) => {
    if (i < axis.pinned) return i;
    if (i >= scrollingEnd) return axis.count - axis.strip.length + i;
    return axis.pinned + item.position - pins.filter((p) => p < item.position).length;
  });
}

function same<RowId, ColId>(a: SelectionChange<RowId, ColId>, b: SelectionChange<RowId, ColId>): boolean {
  return (
    (a.focus === b.focus ||
      (a.focus !== null && b.focus !== null && Object.is(a.focus.rowId, b.focus.rowId) && Object.is(a.focus.colId, b.focus.colId))) &&
    (a.selection === b.selection ||
      (a.selection !== null &&
        b.selection !== null &&
        sameAxis(a.selection.rows, b.selection.rows) &&
        sameAxis(a.selection.cols, b.selection.cols))) &&
    sameList(a.picked.rows, b.picked.rows) &&
    sameList(a.picked.cols, b.picked.cols)
  );
}

function sameAxis<Id>(a: AxisSelection<Id>, b: AxisSelection<Id>): boolean {
  return (
    a.count === b.count &&
    Object.is(a.first, b.first) &&
    Object.is(a.last, b.last) &&
    (a.ids === b.ids || (a.ids !== null && b.ids !== null && sameList(a.ids, b.ids)))
  );
}

function sameList<T>(a: readonly T[], b: readonly T[]): boolean {
  return a.length === b.length && a.every((x, i) => Object.is(x, b[i]));
}
