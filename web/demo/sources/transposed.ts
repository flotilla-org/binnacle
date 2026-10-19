// PROTOTYPE (#138, branch `prototype/column-major`, throwaway): the records
// on their side, each record a column and each field a row. The Table
// Source is the records' own `fromRows` seen through `transpose`, which
// passes every request on about the other axis and swaps every answer back.
// It adds and fixes nothing, so whatever the widget, the Grid or `fromRows`
// does along one axis only shows up here as a gap. See `NOTES.md` at the top
// of the branch.

import {
  createColumnMenu,
  createFilterBar,
  createFindBar,
  fromRows,
  GridWidget,
  type AppliedView,
  type FindQuery,
  type FindStep,
  type TableSink,
  type TableSource,
  type ViewSpec,
} from "../../src";
import type { DemoSource } from "./demo-source";
import { parse, portPicker, type Value } from "./records";
import { matches, RECORDS, rows, type RecordRow } from "./records-data";

function flipView<R, C>(spec: ViewSpec<R, C>): ViewSpec<C, R> {
  return { rows: spec.cols, cols: spec.rows };
}

function flipApplied<R, C>(view: AppliedView<R, C>): AppliedView<C, R> {
  return {
    rows: view.cols,
    cols: view.rows,
    ...(view.totals && {
      totals: {
        ...(view.totals.cols === undefined ? {} : { rows: view.totals.cols }),
        ...(view.totals.rows === undefined ? {} : { cols: view.totals.rows }),
      },
    }),
    ...(view.error === undefined ? {} : { error: view.error }),
  };
}

// A Text Query names items of the other axis, which flipping does not
// change for "shown"; named ones are passed on as they are, wrongly typed.
function flipFind<R, C>(query: FindQuery<C>): FindQuery<R> {
  return { query: query.query as unknown as FindQuery<R>["query"], by: query.by === "rows" ? "cols" : "rows" };
}

function flipStep<R, C>(step: FindStep<C, R> | null): FindStep<R, C> | null {
  return step && { ...step, row: step.col, col: step.row };
}

/** `f` of `value`, at once if it is not a Promise, so a synchronous source stays synchronous. */
function then<T, U>(value: T | Promise<T>, f: (v: T) => U): U | Promise<U> {
  return value instanceof Promise ? value.then(f) : f(value);
}

function transposeCells<V>(cells: V[][], rows: number, cols: number): V[][] {
  return Array.from({ length: rows }, (_, r) => Array.from({ length: cols }, (_, c) => cells[c]![r]!));
}

/** `source` with its axes swapped. */
export function transpose<R, C, V>(source: TableSource<R, C, V>): TableSource<C, R, V> {
  const find = source.find;
  const view = source.view;
  return {
    rows: source.cols,
    cols: source.rows,
    ...(view && {
      view: (spec, signal) => then(view.call(source, flipView(spec), signal), flipApplied),
    }),
    cells: (rowIds, colIds, signal) =>
      then(source.cells(colIds, rowIds, signal), (cells) => transposeCells(cells, rowIds.length, colIds.length)),
    ...(source.renderer && { renderer: (row: C, col: R) => source.renderer!(col, row) }),
    ...(find && {
      find: {
        ...(find.regex === undefined ? {} : { regex: find.regex }),
        marks: (rowIds, colIds, query, signal) =>
          then(find.marks(colIds, rowIds, flipFind(query), signal), (m) => transposeCells(m, rowIds.length, colIds.length)),
        next: (from, direction, query, signal) =>
          then(find.next(from && { row: from.col, col: from.row }, direction, flipFind(query), signal), flipStep),
        ...(find.count && { count: (query, signal) => find.count!(flipFind(query), signal) }),
      },
    }),
    attach: (sink: TableSink<C, R>) =>
      source.attach?.({
        rows: sink.cols,
        cols: sink.rows,
        viewChanged: (v) => sink.viewChanged(flipApplied(v)),
        findCounted: (q, count) => sink.findCounted(flipFind(q), count),
      }) ?? (() => {}),
  };
}

export const transposed: DemoSource = {
  name: "transposed",
  description: `PROTOTYPE: the ${RECORDS.toLocaleString("en")} records with each record a column`,
  showIds: true,
  async mount(host, toolbar, look) {
    const records = fromRows(rows, [
      { key: "id", filterable: false },
      { key: "name" },
      { key: "weight" },
      { key: "crew" },
      { key: "port" },
    ]);
    const source = transpose(records);
    const box = document.createElement("input");
    box.type = "search";
    box.placeholder = "Filter name or port";
    box.setAttribute("aria-label", "Filter records");
    box.addEventListener("input", () => {
      const text = box.value.trim();
      records.filter(text ? (r) => matches(r, text) : null);
    });
    const committed = document.createElement("output");
    committed.id = "committed";
    toolbar.append(box, committed);
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 112,
      ...look,
      onCommit(field, id, value) {
        const parsed = parse(field, value);
        if (parsed === undefined) {
          committed.textContent = `${field} of ${id} kept`;
          return false;
        }
        (records.row(id) as Record<keyof RecordRow, Value>)[field] = parsed;
        committed.textContent = `${field} of ${id} = ${JSON.stringify(parsed)}`;
        records.edited(id);
        return true;
      },
      editor: (field) => (field === "port" ? portPicker() : null),
    });
    const columnMenu = createColumnMenu(widget);
    const bar = createFilterBar(widget, { columnMenu });
    const findBar = createFindBar(widget);
    frame.prepend(bar.element, findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.addEventListener("click", () => findBar.open());
    toolbar.append(findButton);
    return {
      get theme() {
        return widget.theme;
      },
      set theme(choice) {
        widget.theme = choice;
      },
      get border() {
        return widget.border;
      },
      set border(style) {
        widget.border = style;
      },
      destroy() {
        findBar.destroy();
        findButton.remove();
        bar.destroy();
        columnMenu.destroy();
        widget.destroy();
        frame.remove();
        records.destroy();
        box.remove();
        committed.remove();
      },
    };
  },
};
