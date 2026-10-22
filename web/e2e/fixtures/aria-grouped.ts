// A small grouped table for the ARIA layer's tests: seven shipments by
// region then port through `fromRows`, each group with a Subtotal after it
// and a Grand Total after all carrying the total weight, the regions open and the ports closed.
// `?form=header` draws the levels as columns of the row band.
// `window.fixture` reaches the widget.

import { fromRows, GridWidget, LAYOUT_VERSION, rowsBy, textRenderer } from "../../src";

type Shipment = { id: number; region: string; port: string; weight: number };

const shipments: Shipment[] = [
  { id: 1, region: "North", port: "Aberdeen", weight: 10 },
  { id: 2, region: "North", port: "Bergen", weight: 20 },
  { id: 3, region: "North", port: "Aberdeen", weight: 30 },
  { id: 4, region: "South", port: "Cadiz", weight: 40 },
  { id: 5, region: "South", port: "Cadiz", weight: 50 },
  { id: 6, region: "South", port: "Dover", weight: 60 },
  { id: 7, region: "North", port: "Bergen", weight: 70 },
];

const form = new URL(location.href).searchParams.get("form") === "header" ? "header" : "outline";
const source = fromRows(shipments, [
  { key: "id", renderer: textRenderer({ align: "end" }) },
  { key: "region" },
  { key: "port" },
  { key: "weight", renderer: textRenderer({ align: "end" }) },
]);
const host = document.querySelector<HTMLElement>("#grid")!;
const widget = await GridWidget.create(host, {
  source,
  label: "Shipments",
  columnWidth: 90,
  onFrame: (frame) => {
    if (frame.painted) host.dataset.painted = "true";
  },
});
const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
const byRegion = rowsBy<keyof Shipment>("region");
const none = { leading: [], trailing: [] };
widget.restoreLayout({
  version: LAYOUT_VERSION,
  pins: { rows: none, cols: none },
  sizes: { rows: [], cols: [] },
  bands: { rows: 28, cols: 200 },
  view: {
    rows: plain,
    cols: plain,
    grouping: {
      ...byRegion,
      rowLevels: [
        { field: "region", subtotal: "after" },
        { field: "port", subtotal: "after" },
      ],
      values: [{ field: "weight", aggregate: "sum" }, ...byRegion.values],
      rowAxis: { ...byRegion.rowAxis, grandTotal: "after", form, expand: { depth: 1, except: [] } },
    },
  },
});

(window as unknown as { fixture: unknown }).fixture = { widget, source };
