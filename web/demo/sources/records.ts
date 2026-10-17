// The records source: a few thousand generated rows through `fromRows`,
// like the ratatui demo's records. Every column sorts (Shift-click adds a
// column), headers drag to move columns, and the filter box keeps the rows
// whose name or port contains its text.

import { fromRows, GridWidget } from "../../src";
import type { DemoSource } from "./demo-source";
import { matches, RECORDS, rows } from "./records-data";

export const records: DemoSource = {
  name: "records",
  description: `${RECORDS.toLocaleString("en")} records: sort, filter, move columns`,
  showIds: true,
  async mount(host, toolbar, look) {
    const source = fromRows(rows, [
      { key: "id" },
      { key: "name" },
      { key: "weight" },
      { key: "crew" },
      { key: "port" },
    ]);
    const box = document.createElement("input");
    box.type = "search";
    box.placeholder = "Filter name or port";
    box.setAttribute("aria-label", "Filter records");
    box.addEventListener("input", () => {
      const text = box.value.trim();
      source.filter(text ? (r) => matches(r, text) : null);
    });
    toolbar.append(box);
    const widget = await GridWidget.create(host, { source, columnWidth: 140, ...look });
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
        widget.destroy();
        source.destroy();
        box.remove();
      },
    };
  },
};
