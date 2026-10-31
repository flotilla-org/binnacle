// A read-only status table for Row Menu Items: 20 rows with string IDs whose
// columns take no conditions and which do not hide, so a row's menu has
// nothing but the host's items. Every row but "row-1" has an "Open" item.
// `window.fixture` reaches the widget, the column menu and the last choice.

import { createColumnMenu, GridWidget, type TableSource } from "../../src";

const ROWS = Array.from({ length: 20 }, (_, i) => `row-${i}`);
const COLS = ["state", "since"];

const source: TableSource<string, string, string> = {
  rows: {
    count: () => ROWS.length,
    window: (start, end) => ROWS.slice(start, end),
    label: (id) => id,
    locate: (id) => ({ type: "Found", position: ROWS.indexOf(id) }),
  },
  cols: {
    count: () => COLS.length,
    window: (start, end) => COLS.slice(start, end),
    locate: (id) => ({ type: "Found", position: COLS.indexOf(id) }),
  },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => `${r}.${c}`)),
};

const host = document.querySelector<HTMLElement>("#grid")!;
const widget = await GridWidget.create(host, {
  source,
  rowHeight: 24,
  columnWidth: 100,
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = "true";
  },
});
const fixture = {
  widget,
  menu: createColumnMenu(widget, {
    rowItems: (rowId) => (rowId === "row-1" ? [] : [{ name: "open", label: "Open" }]),
    onRowItem: (choice) => (fixture.chosen = choice),
  }),
  chosen: null as { name: string; rowId: string } | null,
};
(window as unknown as { fixture: typeof fixture }).fixture = fixture;
