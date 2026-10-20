// A small fixed table for the presentation screenshots: the Theme and Border
// Style come from the URL (`?theme=mono&border=grid+double`), text is in the
// bundled Inter so the pixels do not depend on the machine's fonts. With
// `pin=3` the columns are wider than the grid and column 3 is pinned left;
// `right`, `top` and `bottom` pin a column right and rows top and bottom, and
// `rows` sets the row count (the names repeat).

import { borderName, borderStyles, GridWidget, presets, type TableSource } from "../../src";

const NAMES = ["Ada", "Grace", "Edsger", "Barbara", "Donald"];
const HEADERS = ["name", "crew", "weight", "watch"];

const params = new URL(location.href).searchParams;
const rowCount = Number(params.get("rows") ?? NAMES.length);
const range = (start: number, end: number) => Array.from({ length: end - start }, (_, i) => start + i);
const source: TableSource<number, number, string> = {
  rows: { identity: "position", count: () => rowCount, window: range },
  cols: { identity: "position", count: () => HEADERS.length, window: range, label: (c) => HEADERS[c]! },
  cells: (rows, cols) =>
    rows.map((r) => cols.map((c) => [NAMES[r % NAMES.length]!, String(r % 3), (61.5 + r * 4.25).toFixed(2), r % 2 ? "port" : "starboard"][c]!)),
};

const preset = presets.find((t) => t.name === params.get("theme")) ?? presets[0]!;
const border = borderStyles.find((b) => borderName(b) === params.get("border")) ?? borderStyles[0]!;
const theme = { ...preset, font: "13px Inter", headerFont: "600 12px Inter" };
const pin = params.get("pin");

await Promise.all([document.fonts.load(theme.font), document.fonts.load(theme.headerFont)]);
const host = document.querySelector<HTMLElement>("#grid")!;
const widget = await GridWidget.create(host, {
  source,
  theme,
  border,
  columnWidth: pin === null && !params.has("right") ? 96 : 150,
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = "true";
  },
});
if (pin !== null) widget.pin("cols", Number(pin));
const at = (name: string) => (params.has(name) ? Number(params.get(name)) : null);
const [right, top, bottom] = [at("right"), at("top"), at("bottom")];
if (right !== null) widget.pin("cols", right, "trailing");
if (top !== null) widget.pin("rows", top);
if (bottom !== null) widget.pin("rows", bottom, "trailing");
