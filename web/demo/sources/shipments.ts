// The shipments source: two thousand shipments through 2024, through
// `fromRows`, for comparison filters. The weight and crates columns hold
// numbers and the shipped column dates, so their column menus offer "Equal
// to" through "Between" on a number or date field beside the text
// condition and the checklist: a weight over 100, or the shipments of one
// quarter. Each comparison shows as a chip in the filter bar.

import { createColumnMenu, createFilterBar, fromRows, GridWidget, textRenderer } from "../../src";
import type { DemoSource } from "./demo-source";
import { day, SHIPMENTS, shipments, type Shipment } from "./shipments-data";

const WIDTHS = { id: 64, port: 128, shipped: 112, weight: 96, crates: 80 } as const;

export const shipmentsDemo: DemoSource = {
  name: "shipments",
  description: `${SHIPMENTS.toLocaleString("en")} shipments: compare numbers and dates`,
  showIds: true,
  filterBar: true,
  async mount(host, _toolbar, look) {
    const number = textRenderer<Shipment[keyof Shipment]>({ align: "end" });
    const source = fromRows(shipments, [
      { key: "id", filterable: false, renderer: number },
      { key: "port" },
      { key: "shipped", renderer: textRenderer({ format: (d: Date) => day(d) }) },
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
        bar.destroy();
        columnMenu.destroy();
        widget.destroy();
        frame.remove();
        source.destroy();
      },
    };
  },
};
