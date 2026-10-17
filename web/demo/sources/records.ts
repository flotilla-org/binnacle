// The records source: a few thousand generated rows through `fromRows`,
// like the ratatui demo's records. Every column sorts (Shift-click adds a
// column), headers drag to move columns, and the filter box keeps the rows
// whose name or port contains its text. Cells edit in place: type, or press
// F2 or Enter, or double-click; the port column picks from a list.

import { fromRows, GridWidget, type Editor } from "../../src";
import type { DemoSource } from "./demo-source";
import { matches, PORTS, RECORDS, rows, type RecordRow } from "./records-data";

export type Value = RecordRow[keyof RecordRow];

/** The new value for a column from the editor, or `undefined` if it does not fit the column. */
export function parse(col: keyof RecordRow, value: Value | string): Value | undefined {
  const text = typeof value === "string" ? value.trim() : value;
  switch (col) {
    case "id":
      return undefined;
    case "name":
      return text === "" || text === null ? undefined : String(text);
    case "port":
      return text === "" || text === null ? null : String(text);
    case "weight":
    case "crew": {
      if (text === "" || text === null) return col === "weight" ? null : undefined;
      const n = Number(text);
      return Number.isFinite(n) && (col === "weight" || Number.isInteger(n)) ? n : undefined;
    }
  }
}

/** A list of the ports, for the port column. */
export function portPicker(): Editor<Value> {
  const select = document.createElement("select");
  select.className = "port-picker";
  select.setAttribute("aria-label", "Port");
  return {
    mount(el, initial) {
      select.append(new Option("(none)", ""), ...PORTS.map((p) => new Option(p, p)));
      select.value = typeof initial === "string" ? initial : "";
      el.append(select);
    },
    value: () => select.value || null,
    focus: () => select.focus(),
    destroy: () => select.remove(),
  };
}

export const records: DemoSource = {
  name: "records",
  description: `${RECORDS.toLocaleString("en")} records: sort, filter, move columns, edit`,
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
    const committed = document.createElement("output");
    committed.id = "committed";
    toolbar.append(box, committed);
    const widget = await GridWidget.create(host, {
      source,
      columnWidth: 140,
      ...look,
      onCommit(id, col, value) {
        const parsed = parse(col, value);
        if (parsed === undefined) {
          committed.textContent = `${col} of ${id} kept`;
        } else {
          (source.row(id) as Record<keyof RecordRow, Value>)[col] = parsed;
          committed.textContent = `${col} of ${id} = ${JSON.stringify(parsed)}`;
        }
        // Either way the cell shows the source's value again.
        source.changed(id);
      },
      editor: (_id, col) => (col === "port" ? portPicker() : null),
    });
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
        committed.remove();
      },
    };
  },
};
