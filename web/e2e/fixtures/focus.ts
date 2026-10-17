// A table for the focus and scroll API: 10,000 rows with string IDs by 8
// columns, wider and taller than the grid. `?async` answers Locate after a
// delay; `?unit=rows` selects by row. `window.fixture` reaches the widget,
// the last frame and selection change, reverses the order (a sort the app
// applied) and removes rows.

import { GridWidget, type AxisSink, type FrameInfo, type SelectionChange, type TableSource } from "../../src";

const ROWS = 10_000;
const COLS = ["a", "b", "c", "d", "e", "f", "g", "h"];
const params = new URL(location.href).searchParams;
const delay = <T>(value: T) => new Promise<T>((resolve) => setTimeout(() => resolve(value), 100));

let order = Array.from({ length: ROWS }, (_, i) => `row-${i}`);
let sink: AxisSink<string> | null = null;

const source: TableSource<string, string, string> = {
  rows: {
    count: () => order.length,
    window: (start, end) => order.slice(start, end),
    label: (id) => id,
    locate(id) {
      const at = order.indexOf(id);
      const answer = at < 0 ? ({ type: "Gone", suggest: null } as const) : ({ type: "Found", position: at } as const);
      return params.has("async") ? delay(answer) : answer;
    },
  },
  cols: {
    count: () => COLS.length,
    window: (start, end) => COLS.slice(start, end),
    locate: (id) => ({ type: "Found", position: COLS.indexOf(id) }),
  },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => `${r}.${c}`)),
  attach(s) {
    sink = s.rows;
    return () => (sink = null);
  },
};

const host = document.querySelector<HTMLElement>("#grid")!;
const fixture = {
  widget: null as GridWidget<string, string, string> | null,
  frame: null as FrameInfo<string, string> | null,
  change: null as SelectionChange<string, string> | null,
  reverse() {
    order = [...order].reverse();
    sink?.notify({ type: "Reordered" });
  },
  remove(id: string) {
    order = order.filter((r) => r !== id);
    sink?.notify({ type: "Reset" });
  },
};
(window as unknown as { fixture: typeof fixture }).fixture = fixture;

fixture.widget = await GridWidget.create(host, {
  source,
  columnWidth: 100,
  ...(params.get("unit") === "rows" ? { selectionUnit: "rows" as const } : {}),
  onFrame: (frame) => {
    fixture.frame = frame;
    if (frame.painted) host.dataset.painted = "true";
  },
  onSelectionChange: (change) => (fixture.change = change),
});
