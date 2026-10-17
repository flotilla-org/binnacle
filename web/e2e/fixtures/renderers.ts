// A small fixed table drawn by the built-in Cell Renderers, for screenshots
// per Theme (`?theme=mono`). Text is in the bundled Inter. The widget is left
// on `window.fixture` so tests can mark flashes.

import {
  barRenderer,
  byColumn,
  GridWidget,
  presets,
  sparklineRenderer,
  textRenderer,
  type CellRenderer,
  type TableSource,
} from "../../src";

type Value = string | number | readonly number[];

const NAMES = ["Ada", "Grace", "Edsger", "Barbara", "Donald"];
const HEADERS = ["name", "price", "trend", "load"];
const TRENDS: readonly number[][] = [
  [3, 4, 3.5, 5, 6, 5.5, 7],
  [9, 8, 8.5, 6, 5, 5.2, 4],
  [5, 5, 5, 5],
  [1, 6, NaN, 2, 7, 3, 8],
  [42],
];
const PRICES = [1234.5, 0.25, 99999.999, -12, 7];
const LOADS = [0.42, 0.9, 0, 1.4, NaN];

const range = (start: number, end: number) => Array.from({ length: end - start }, (_, i) => start + i);
const value = (r: number, c: number): Value => [NAMES[r]!, PRICES[r]!, TRENDS[r]!, LOADS[r]!][c]!;
const source: TableSource<number, number, Value> = {
  rows: { identity: "position", count: () => NAMES.length, window: range },
  cols: { identity: "position", count: () => HEADERS.length, window: range, label: (c) => HEADERS[c]! },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => value(r, c))),
};
const renderers: CellRenderer<Value>[] = [
  textRenderer(),
  textRenderer({ align: "end", format: new Intl.NumberFormat("en", { minimumFractionDigits: 2, maximumFractionDigits: 2 }) }),
  sparklineRenderer(),
  barRenderer(),
];

const params = new URL(location.href).searchParams;
const preset = presets.find((t) => t.name === params.get("theme")) ?? presets[0]!;
const theme = { ...preset, font: "13px Inter", headerFont: "600 12px Inter" };

await Promise.all([document.fonts.load(theme.font), document.fonts.load(theme.headerFont)]);
const host = document.querySelector<HTMLElement>("#grid")!;
let paints = 0;
const widget = await GridWidget.create(host, {
  source,
  theme,
  renderer: byColumn((c: number) => renderers[c]),
  columnWidth: 110,
  onFrame: (frame) => {
    if (!frame.painted) return;
    host.dataset.painted = "true";
    host.dataset.paints = String(++paints);
  },
});
(window as unknown as { fixture: unknown }).fixture = { widget };
