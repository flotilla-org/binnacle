// The slow source: the records, answered late, like the ratatui demo's
// `--slow`. Windows and cells arrive after a short delay, sorts and Locates
// after a longer one, so loading cells, a shimmering pending sort and a
// pulsing locate all show before the grid settles. Moves are answered as
// late as sorts, the moved headers shimmering meanwhile. Every request honours its
// AbortSignal: one the widget no longer wants is never answered, and the
// toolbar counts both. "Fail" makes Windows and cells fail instead: the
// failed cells show the error marker, and Retry asks again with a Reset.
// The filter bar holds a quick filter (regexes too) and column conditions,
// through the column menu: while one is pending the rows stay, the count
// reads "filtering…" and chips and header funnels fade until the late
// answer; typing more aborts the request in flight, so only the last answer
// paints. As the source declares no column support, conditions take contains
// but no regexes. An invalid regex comes back unapplied with RegExp's message,
// which turns the field red. Find's marks arrive late like cells, its steps
// and counts like sorts, the count reading "finding…" meanwhile; an invalid
// regex there reads "No matches" with the message.

import {
  createChooser,
  createColumnMenu,
  createFilterBar,
  createFindBar,
  fromRows,
  GridWidget,
  type AxisSink,
  type MaybePromise,
  type TableSource,
} from "../../src";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { matches, RECORDS, rows, sortedMoves, type RecordRow } from "./records-data";

/** How late the source answers, ms: Windows and cells, then sorts and Locates. */
export const PACES = {
  brisk: { data: 200, order: 600 },
  slow: { data: 600, order: 1500 },
  glacial: { data: 1500, order: 4000 },
} as const;

export type Pace = keyof typeof PACES;

export const slow: DemoSource = {
  name: "slow",
  description: `${RECORDS.toLocaleString("en")} records answered late: loading cells, a pending sort, a pending locate`,
  showIds: true,
  filterBar: true,
  async mount(host, toolbar, look) {
    const inner = fromRows(
      rows,
      [{ key: "id" }, { key: "name" }, { key: "weight" }, { key: "crew" }, { key: "port" }],
      sortedMoves(),
    );
    const url = new URL(location.href);
    let pace: Pace = (url.searchParams.get("pace") as Pace | null) ?? "slow";
    if (!(pace in PACES)) pace = "slow";
    let inFlight = 0;
    let aborted = 0;
    let failing = false;
    let sinks: { rows: AxisSink<number> } | null = null;

    const counter = document.createElement("span");
    counter.className = "requests";
    counter.setAttribute("aria-live", "off");
    const count = () => (counter.textContent = `${inFlight} in flight · ${aborted} aborted`);
    count();

    /** `answer()` after the pace's delay, or never if `signal` aborts first. */
    const later = <T,>(kind: "data" | "order", signal: AbortSignal, answer: () => MaybePromise<T>): Promise<T> =>
      new Promise<T>((resolve, reject) => {
        inFlight++;
        count();
        const timer = setTimeout(() => {
          signal.removeEventListener("abort", stop);
          inFlight--;
          count();
          if (failing && kind === "data") reject(new Error("the source is failing on purpose"));
          else {
            try {
              resolve(answer());
            } catch (error) {
              reject(error);
            }
          }
        }, PACES[pace][kind]);
        const stop = () => {
          clearTimeout(timer);
          inFlight--;
          aborted++;
          count();
          reject(signal.reason);
        };
        signal.addEventListener("abort", stop, { once: true });
      });

    const source: TableSource<number, keyof RecordRow, RecordRow[keyof RecordRow]> = {
      rows: {
        count: () => inner.rows.count(),
        window: (start, end, signal) => later("data", signal, () => inner.rows.window(start, end, signal)),
        locate: (id, signal) => later("order", signal, () => inner.rows.locate!(id, signal)),
        quickFilter: inner.rows.quickFilter!,
        move: (ids, to) => later("order", new AbortController().signal, () => inner.rows.move!(ids, to)),
        movable: () => inner.rows.movable!(),
      },
      cols: {
        ...inner.cols,
        move: (ids, to) => later("order", new AbortController().signal, () => inner.cols.move!(ids, to)),
      },
      view: (spec, signal) => later("order", signal, () => inner.view!(spec, signal)),
      cells: (rowIds, colIds, signal) => later("data", signal, () => inner.cells(rowIds, colIds, signal)),
      find: {
        regex: true,
        marks: (rowIds, colIds, query, signal) => later("data", signal, () => inner.find.marks(rowIds, colIds, query, signal)),
        next: (from, direction, query, signal) => later("order", signal, () => inner.find.next(from, direction, query, signal)),
        count: (query, signal) => later("order", signal, () => inner.find.count!(query, signal)),
      },
      emptyMessage: () => (box.value.trim() ? "No records match the filter" : undefined),
      attach: (sink) => {
        sinks = sink;
        return inner.attach!(sink);
      },
    };

    const speed = document.createElement("select");
    speed.setAttribute("aria-label", "How late the source answers");
    for (const [name, { data, order }] of Object.entries(PACES)) {
      speed.add(new Option(`${name}: ${data / 1000} s, sorts ${order / 1000} s`, name));
    }
    speed.value = pace;
    speed.addEventListener("change", () => {
      pace = speed.value as Pace;
      const next = new URL(location.href);
      next.searchParams.set("pace", pace);
      history.replaceState(null, "", next);
    });
    const box = document.createElement("input");
    box.type = "search";
    box.placeholder = "Filter name or port";
    box.setAttribute("aria-label", "Filter records");
    box.addEventListener("input", () => {
      const text = box.value.trim();
      inner.filter(text ? (r) => matches(r, text) : null);
    });
    const fail = document.createElement("label");
    const failBox = document.createElement("input");
    failBox.type = "checkbox";
    failBox.addEventListener("change", () => (failing = failBox.checked));
    fail.append(failBox, " Fail");
    const retry = document.createElement("button");
    retry.textContent = "Retry";
    retry.hidden = true;
    retry.addEventListener("click", () => {
      retry.hidden = true;
      sinks?.rows.notify({ type: "Reset" });
    });
    toolbar.append(speed, box, fail, retry, counter);
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 140,
      onError: (_error, request) => {
        if (request.type === "window" || request.type === "cells") retry.hidden = false;
      },
      ...look,
    });
    const layout = layoutControls(widget, "slow");
    const columnMenu = createColumnMenu(widget);
    const chooser = createChooser(widget);
    const bar = createFilterBar(widget, { columnMenu });
    const findBar = createFindBar(widget);
    frame.prepend(bar.element, findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.title = "Find in the table (Ctrl/Cmd-F in the grid)";
    findButton.addEventListener("click", () => findBar.open());
    toolbar.append(findButton, ...layout.elements);
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
        layout.destroy();
        findButton.remove();
        bar.destroy();
        columnMenu.destroy();
        chooser.destroy();
        widget.destroy();
        frame.remove();
        inner.destroy();
        speed.remove();
        box.remove();
        fail.remove();
        retry.remove();
        counter.remove();
      },
    };
  },
};
