// A small fixed table for the presentation screenshots: the Theme and Border
// Style come from the URL (`?theme=mono&border=grid+double`), text is in the
// bundled Inter so the pixels do not depend on the machine's fonts.

import { borderName, borderStyles, GridWidget, presets, type TableSource } from "../../src";

const NAMES = ["Ada", "Grace", "Edsger", "Barbara", "Donald"];
const HEADERS = ["name", "crew", "weight", "watch"];

const range = (start: number, end: number) => Array.from({ length: end - start }, (_, i) => start + i);
const source: TableSource<number, number, string> = {
  rows: { identity: "position", count: () => NAMES.length, window: range },
  cols: { identity: "position", count: () => HEADERS.length, window: range, label: (c) => HEADERS[c]! },
  cells: (rows, cols) =>
    rows.map((r) => cols.map((c) => [NAMES[r]!, String(r % 3), (61.5 + r * 4.25).toFixed(2), r % 2 ? "port" : "starboard"][c]!)),
};

const params = new URL(location.href).searchParams;
const preset = presets.find((t) => t.name === params.get("theme")) ?? presets[0]!;
const border = borderStyles.find((b) => borderName(b) === params.get("border")) ?? borderStyles[0]!;
const theme = { ...preset, font: "13px Inter", headerFont: "600 12px Inter" };

await Promise.all([document.fonts.load(theme.font), document.fonts.load(theme.headerFont)]);
const host = document.querySelector<HTMLElement>("#grid")!;
await GridWidget.create(host, {
  source,
  theme,
  border,
  columnWidth: 96,
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = "true";
  },
});
