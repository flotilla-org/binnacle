// The shipments source: two thousand shipments through 2024, through
// `fromRows`, grouped by region then port as it opens, for grouping and
// comparison filters.
//
// Each region's Group Item heads its ports, and each port's its shipments,
// with a disclosure mark and its count in the row header; the region and
// port columns leave the body while the rows group by them. Each group ends
// in a Subtotal carrying the group's total weight, and the Grand Total sits
// pinned at the bottom. A column's menu (its header's funnel, or
// Alt+ArrowDown) has "Ungroup rows" while grouped, and on any column "Group
// rows by this column" while not. Sorting by the grouped column orders the
// groups; any other column orders the shipments within their group. Focus
// stays on its shipment through grouping and ungrouping. Sorting by weight
// orders the groups at every level by their total weight.
//
// The weight and crates columns hold numbers and the shipped column dates,
// so their column menus offer "Equal to" through "Between" on a number or
// date field beside the text condition and the checklist: a weight over
// 100, or the shipments of one quarter. Each comparison shows as a chip in
// the filter bar.

import { createColumnMenu, createFilterBar, fromRows, GridWidget, LAYOUT_VERSION, rowsBy, textRenderer } from "../../src";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { day, SHIPMENTS, shipments, type Shipment } from "./shipments-data";

const WIDTHS = { id: 64, region: 96, port: 128, shipped: 112, weight: 96, crates: 80 } as const;
const HEADER = 28;

export const shipmentsDemo: DemoSource = {
  name: "shipments",
  description: `${SHIPMENTS.toLocaleString("en")} shipments by region then port, with Subtotals and a Grand Total: group by any column, compare numbers and dates`,
  showIds: true,
  filterBar: true,
  async mount(host, _toolbar, look) {
    const number = textRenderer<Shipment[keyof Shipment]>({ align: "end" });
    // A total weight to one decimal place, as each shipment's is.
    const weight = textRenderer({ align: "end", format: (w: number | null) => (w === null ? "" : String(Math.round(w * 10) / 10)) });
    const source = fromRows(shipments, [
      { key: "id", filterable: false, renderer: number },
      { key: "region" },
      { key: "port" },
      // A Summary Item's cells hold nothing but its weight.
      { key: "shipped", renderer: textRenderer({ format: (d: Date | null) => (d ? day(d) : "") }) },
      { key: "weight", renderer: weight },
      { key: "crates", renderer: number },
    ]);
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 120,
      headerHeight: HEADER,
      initialColumnWidth: (col) => WIDTHS[col],
      ...look,
    });
    // As a saved layout would come back, with a row band wide enough for
    // the Subtotals' labels: the column menu's "Group rows by" waits until
    // the source says the column groups, which it does once the column is
    // in view.
    const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    const byRegion = rowsBy<keyof Shipment>("region");
    const none = { leading: [], trailing: [] };
    widget.restoreLayout({
      version: LAYOUT_VERSION,
      pins: { rows: none, cols: none },
      sizes: { rows: [], cols: [] },
      bands: { rows: HEADER, cols: 200 },
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
          rowAxis: { ...byRegion.rowAxis, grandTotal: "after" },
        },
      },
    });
    const layout = layoutControls(widget, "shipments");
    const columnMenu = createColumnMenu(widget);
    const bar = createFilterBar(widget, { columnMenu });
    frame.prepend(bar.element);
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
        layout.destroy();
        bar.destroy();
        columnMenu.destroy();
        widget.destroy();
        frame.remove();
        source.destroy();
      },
    };
  },
};
