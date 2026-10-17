// A small table for the ARIA layer's tests: named rows, a text, number,
// sparkline and bar column, sortable by every column; editable with
// `?editable`. `window.fixture` lets
// tests hold cell values back (loading), remove a row (its Locate ends
// `Gone`) and reach the widget.

import {
  barRenderer,
  byColumn,
  GridWidget,
  sparklineRenderer,
  textRenderer,
  type AxisSink,
  type CellRenderer,
  type SortSpec,
  type TableSource,
} from "../../src";

type Col = "name" | "price" | "trend" | "load";
type Value = string | number | readonly number[];
type Row = Record<Col, Value> & { id: string };

const all: Row[] = [
  { id: "ada", name: "Ada", price: 12.5, trend: [3, 4, 3.5, 5], load: 0.42 },
  { id: "grace", name: "Grace", price: 3, trend: [9, 8, 6, 4], load: 0.9 },
  { id: "edsger", name: "Edsger", price: 7.25, trend: [5, 5, 5], load: 0 },
  { id: "barbara", name: "Barbara", price: 40, trend: [1, 6, 2], load: 1 },
  { id: "donald", name: "Donald", price: 0.5, trend: [42], load: 0.1 },
];
const COLS: Col[] = ["name", "price", "trend", "load"];
const LABELS: Record<Col, string> = { name: "Name", price: "Price", trend: "Trend", load: "Load" };

let order = all.map((r) => r.id);
let spec: SortSpec<Col> = [];
let sink: { rows: AxisSink<string>; cols: AxisSink<Col> } | null = null;
let held: (() => void)[] | null = null;
const byId = (id: string) => all.find((r) => r.id === id)!;

function sortOrder() {
  order.sort((a, b) => {
    for (const { id, direction } of spec) {
      const x = byId(a)[id];
      const y = byId(b)[id];
      const c = x < y ? -1 : x > y ? 1 : 0;
      if (c) return direction === "ascending" ? c : -c;
    }
    return 0;
  });
}

const source: TableSource<string, Col, Value> = {
  rows: {
    count: () => order.length,
    window: (start, end) => order.slice(start, end),
    label: (id) => byId(id).name as string,
    sort(next) {
      spec = next;
      sortOrder();
      queueMicrotask(() => sink?.rows.notify({ type: "Reordered" }));
      return next;
    },
    locate: (id) => {
      const at = order.indexOf(id);
      return at < 0 ? { type: "Gone", suggest: null } : { type: "Found", position: at };
    },
  },
  cols: {
    count: () => COLS.length,
    window: (start, end) => COLS.slice(start, end),
    label: (c) => LABELS[c],
  },
  cells(rows, cols) {
    const values = rows.map((r) => cols.map((c) => byId(r)[c]));
    if (!held) return values;
    return new Promise((resolve) => held!.push(() => resolve(values)));
  },
  attach(s) {
    sink = s;
    return () => (sink = null);
  },
};

const renderers: Record<Col, CellRenderer<Value>> = {
  name: textRenderer(),
  price: textRenderer({ align: "end", format: new Intl.NumberFormat("en", { minimumFractionDigits: 2 }) }),
  trend: sparklineRenderer() as CellRenderer<Value>,
  load: barRenderer() as CellRenderer<Value>,
};

const host = document.querySelector<HTMLElement>("#grid")!;
const widget = await GridWidget.create(host, {
  source,
  label: "People",
  renderer: byColumn((c: Col) => renderers[c]),
  columnWidth: 100,
  ...(new URL(location.href).searchParams.has("editable") ? { onCommit: () => {} } : {}),
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = "true";
  },
});

(window as unknown as { fixture: unknown }).fixture = {
  widget,
  /** Holds every cell answer back until `release`. */
  hold() {
    held = [];
  },
  release() {
    const waiting = held ?? [];
    held = null;
    for (const answer of waiting) answer();
  },
  /** Removes a row; the Reset makes the widget locate the focused one. */
  remove(id: string) {
    order = order.filter((r) => r !== id);
    sink?.rows.notify({ type: "Reset" });
  },
};
