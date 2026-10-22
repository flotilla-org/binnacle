// The shipments source: two thousand shipments through 2024, through
// `fromRows`, grouped by region as it opens, for grouping and comparison
// filters.
//
// Each region's Group Item heads its shipments, with a disclosure mark and
// its count in the row header, and the region column leaves the body while
// the rows group by it. A column's menu (its header's funnel, or
// Alt+ArrowDown) has "Ungroup rows" while grouped, and on any column "Group
// rows by this column" while not. Sorting by the grouped column orders the
// groups; any other column orders the shipments within their group. Focus
// stays on its shipment through grouping and ungrouping.
//
// The weight and crates columns hold numbers and the shipped column dates,
// so their column menus offer "Equal to" through "Between" on a number or
// date field beside the text condition and the checklist: a weight over
// 100, or the shipments of one quarter. Each comparison shows as a chip in
// the filter bar.

import { createColumnMenu, createFilterBar, fromRows, GridWidget, rowsBy, textRenderer } from "../../src";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { day, SHIPMENTS, shipments, type Shipment } from "./shipments-data";

const WIDTHS = { id: 64, region: 96, port: 128, shipped: 112, weight: 96, crates: 80 } as const;

export const shipmentsDemo: DemoSource = {
  name: "shipments",
  description: `${SHIPMENTS.toLocaleString("en")} shipments grouped by region: group by any column, compare numbers and dates`,
  showIds: true,
  filterBar: true,
  async mount(host, _toolbar, look) {
    const number = textRenderer<Shipment[keyof Shipment]>({ align: "end" });
    const source = fromRows(shipments, [
      { key: "id", filterable: false, renderer: number },
      { key: "region" },
      { key: "port" },
      // A Group Item's cells hold nothing.
      { key: "shipped", renderer: textRenderer({ format: (d: Date | null) => (d ? day(d) : "") }) },
      { key: "weight", renderer: number },
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
      initialColumnWidth: (col) => WIDTHS[col],
      ...look,
    });
    // As a saved view would come back: the column menu's "Group rows by"
    // waits until the source says the column groups, which it does once
    // the column is in view.
    const plain = { filter: { quick: null, conditions: [] }, sort: [], hidden: [] };
    widget.requestView({ rows: plain, cols: plain, grouping: rowsBy("region") });
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
