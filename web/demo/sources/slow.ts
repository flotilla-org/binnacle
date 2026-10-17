// The slow source: the records, answered late, like the ratatui demo's
// `--slow`. Windows and cells arrive after a short delay, sorts and Locates
// after a longer one, so loading cells, a shimmering pending sort and a
// pulsing locate all show before the grid settles. Every request honours its
// AbortSignal: one the widget no longer wants is never answered, and the
// toolbar counts both.

import { fromRows, GridWidget, type MaybePromise, type TableSource } from "../../src";
import type { DemoSource } from "./demo-source";
import { matches, RECORDS, rows, type RecordRow } from "./records-data";

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
  async mount(host, toolbar, look) {
    const inner = fromRows(rows, [
      { key: "id" },
      { key: "name" },
      { key: "weight" },
      { key: "crew" },
      { key: "port" },
    ]);
    const url = new URL(location.href);
    let pace: Pace = (url.searchParams.get("pace") as Pace | null) ?? "slow";
    if (!(pace in PACES)) pace = "slow";
    let inFlight = 0;
    let aborted = 0;

    const counter = document.createElement("span");
    counter.className = "requests";
    counter.setAttribute("aria-live", "off");
    const count = () => (counter.textContent = `${inFlight} in flight · ${aborted} aborted`);
    count();

    /** `answer()` after the pace's delay, or never if `signal` aborts first. */
    const later = <T,>(kind: "data" | "order", signal: AbortSignal, answer: () => MaybePromise<T>) =>
      new Promise<T>((resolve, reject) => {
        inFlight++;
        count();
        const timer = setTimeout(() => {
          signal.removeEventListener("abort", stop);
          inFlight--;
          count();
          resolve(answer());
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
        sort: (spec, signal) => later("order", signal, () => inner.rows.sort!(spec, signal)),
        locate: (id, signal) => later("order", signal, () => inner.rows.locate!(id, signal)),
      },
      cols: inner.cols,
      cells: (rowIds, colIds, signal) => later("data", signal, () => inner.cells(rowIds, colIds, signal)),
      attach: (sink) => inner.attach!(sink),
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
    toolbar.append(speed, box, counter);
    const widget = await GridWidget.create(host, { source, columnWidth: 140, ...look });
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
        widget.destroy();
        inner.destroy();
        speed.remove();
        box.remove();
        counter.remove();
      },
    };
  },
};
