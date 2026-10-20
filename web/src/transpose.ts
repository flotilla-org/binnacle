// A Table Source on its side: the rows of `transpose(source)` are the
// source's columns and its columns the source's rows. Every request is passed
// on about the other axis and every answer swapped back, so a row-major
// source (`fromRows` over records, say) serves a column-major table, each
// record a column. It adds nothing: what the source does along one axis only,
// the turned table does along the other only.
//
// It is a pure axis swap and must stay one: it is not how a source supports
// its columns. `fromRows` supports sort, filter, hide and move on the column
// axis natively, tested against its own model; turning it only exercises
// those paths from the other side.

import type {
  AppliedView,
  FindMethods,
  FindQuery,
  FindStep,
  MaybePromise,
  TableSink,
  TableSource,
  Totals,
  ViewSpec,
} from "./source";

/** `spec` with its axes' parts swapped. */
export function transposeView<RowId, ColId>(spec: ViewSpec<RowId, ColId>): ViewSpec<ColId, RowId> {
  return { rows: spec.cols, cols: spec.rows };
}

function transposeTotals(totals: Totals): Totals {
  return {
    ...(totals.cols === undefined ? {} : { rows: totals.cols }),
    ...(totals.rows === undefined ? {} : { cols: totals.rows }),
  };
}

/** `view` with its axes' parts and totals swapped. */
export function transposeApplied<RowId, ColId>(view: AppliedView<RowId, ColId>): AppliedView<ColId, RowId> {
  return {
    rows: view.cols,
    cols: view.rows,
    ...(view.totals && { totals: transposeTotals(view.totals) }),
    ...(view.error === undefined ? {} : { error: view.error }),
  };
}

/**
 * `query` for the source under `transpose`, stepping along the other axis
 * first; `null` if it names the columns it searches, since that source can
 * search only its own columns, which are the rows here.
 */
export function transposeFind<RowId, ColId>(query: FindQuery<ColId>): FindQuery<RowId> | null {
  const { searches, ...rest } = query.query;
  if (searches !== "shown") return null;
  return { query: { ...rest, searches }, by: query.by === "rows" ? "cols" : "rows" };
}

/** `value` given to `f` at once if it is not a Promise, so a synchronous source stays synchronous. */
function then<T, U>(value: MaybePromise<T>, f: (v: T) => U): MaybePromise<U> {
  return value instanceof Promise ? value.then(f) : f(value);
}

/** The `rows` by `cols` matrix whose cell `[r][c]` is `cells[c][r]`. */
function transposeCells<V>(cells: V[][], rows: number, cols: number): V[][] {
  return Array.from({ length: rows }, (_, r) => Array.from({ length: cols }, (_, c) => cells[c]![r]!));
}

function transposeStep<RowId, ColId>(step: FindStep<ColId, RowId> | null): FindStep<RowId, ColId> | null {
  if (!step) return null;
  const { row, col, ...rest } = step;
  return { ...rest, row: col, col: row };
}

/**
 * `find` on its side. `asked` remembers which query each turned count query
 * stands for, so running counts reach the sink as the query asked.
 */
function transposeFindMethods<RowId, ColId>(
  find: FindMethods<ColId, RowId>,
  asked: WeakMap<FindQuery<RowId>, FindQuery<ColId>>,
): FindMethods<RowId, ColId> {
  const count = find.count;
  return {
    ...(find.regex === undefined ? {} : { regex: find.regex }),
    marks(rowIds, colIds, query, signal) {
      const turned = transposeFind<RowId, ColId>(query);
      if (!turned) return rowIds.map(() => colIds.map(() => false));
      return then(find.marks(colIds, rowIds, turned, signal), (m) => transposeCells(m, rowIds.length, colIds.length));
    },
    next(from, direction, query, signal) {
      const turned = transposeFind<RowId, ColId>(query);
      if (!turned) return null;
      return then(find.next(from && { row: from.col, col: from.row }, direction, turned, signal), transposeStep);
    },
    ...(count && {
      count(query, signal) {
        const turned = transposeFind<RowId, ColId>(query);
        if (!turned) return { total: 0 };
        asked.set(turned, query);
        return count.call(find, turned, signal);
      },
    }),
  };
}

/**
 * `source` on its side. A View Spec's Text Queries and Conditions name items
 * of the other axis, which turning leaves on the same side, so they pass on
 * as they are; a Find Query naming the columns it searches finds nothing.
 */
export function transpose<RowId, ColId, V>(source: TableSource<RowId, ColId, V>): TableSource<ColId, RowId, V> {
  const { view, renderer, find, attach, emptyMessage } = source;
  const asked = new WeakMap<FindQuery<ColId>, FindQuery<RowId>>();
  return {
    rows: source.cols,
    cols: source.rows,
    ...(view && {
      view: (spec: ViewSpec<ColId, RowId>, signal: AbortSignal) =>
        then(view.call(source, transposeView(spec), signal), transposeApplied),
    }),
    cells: (rowIds, colIds, signal) =>
      then(source.cells(colIds, rowIds, signal), (cells) => transposeCells(cells, rowIds.length, colIds.length)),
    ...(renderer && { renderer: (rowId: ColId, colId: RowId) => renderer(colId, rowId) }),
    ...(emptyMessage && { emptyMessage: () => emptyMessage.call(source) }),
    ...(find && { find: transposeFindMethods(find, asked) }),
    ...(attach && {
      attach: (sink: TableSink<ColId, RowId>) =>
        attach.call(source, {
          rows: sink.cols,
          cols: sink.rows,
          viewChanged: (v) => sink.viewChanged(transposeApplied(v)),
          findCounted: (q, count) => {
            const query = asked.get(q);
            if (query) sink.findCounted(query, count);
          },
        }),
    }),
  };
}
