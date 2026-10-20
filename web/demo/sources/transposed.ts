// The transposed source: the records on their side, as a pivoted or
// column-major table has them. Each record is a column headed with its id and
// each field a row; the Table Source is the records' own `fromRows` seen
// through `transpose`, so it adds and fixes nothing: what works along one
// axis only shows up here. Each field row draws its own way (the id and
// weight end-aligned, the crew as a bar). The filter box keeps the records
// whose name or port contains its text, as columns; find and editing work as
// in the records.

import {
  barRenderer,
  byRow,
  createColumnMenu,
  createFilterBar,
  createFindBar,
  fromRows,
  GridWidget,
  textRenderer,
  transpose,
  type CellRenderer,
} from "../../src";
import type { DemoSource } from "./demo-source";
import { parse, portPicker, type Value } from "./records";
import { matches, RECORDS, rows, type RecordRow } from "./records-data";

/** The renderers of the fields that do not draw as plain text. */
const FIELD_RENDERERS: Partial<Record<keyof RecordRow, CellRenderer<Value>>> = {
  id: textRenderer<Value>({ align: "end" }),
  weight: textRenderer<Value>({ align: "end" }),
  crew: barRenderer<Value>({ fraction: (v) => (v as number) / 12 }),
};

export const transposed: DemoSource = {
  name: "transposed",
  description: `the ${RECORDS.toLocaleString("en")} records on their side, each record a column`,
  showIds: true,
  async mount(host, toolbar, look) {
    const records = fromRows(rows, [
      { key: "id", filterable: false },
      { key: "name" },
      { key: "weight" },
      { key: "crew" },
      { key: "port" },
    ]);
    const source = transpose(records);
    const box = document.createElement("input");
    box.type = "search";
    box.placeholder = "Filter name or port";
    box.setAttribute("aria-label", "Filter records");
    box.addEventListener("input", () => {
      const text = box.value.trim();
      records.filter(text ? (r) => matches(r, text) : null);
    });
    const committed = document.createElement("output");
    committed.id = "committed";
    toolbar.append(box, committed);
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 112,
      renderer: byRow((field: keyof RecordRow) => FIELD_RENDERERS[field]),
      ...look,
      onCommit(field, id, value) {
        const parsed = parse(field, value);
        if (parsed === undefined) {
          committed.textContent = `${field} of ${id} kept`;
          return false;
        }
        (records.row(id) as Record<keyof RecordRow, Value>)[field] = parsed;
        committed.textContent = `${field} of ${id} = ${JSON.stringify(parsed)}`;
        records.edited(id);
        return true;
      },
      editor: (field) => (field === "port" ? portPicker() : null),
    });
    const columnMenu = createColumnMenu(widget);
    const bar = createFilterBar(widget, { columnMenu });
    const findBar = createFindBar(widget);
    frame.prepend(bar.element, findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.title = "Find in the table (Ctrl/Cmd-F in the grid)";
    findButton.addEventListener("click", () => findBar.open());
    toolbar.append(findButton);
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
        findBar.destroy();
        findButton.remove();
        bar.destroy();
        columnMenu.destroy();
        widget.destroy();
        frame.remove();
        records.destroy();
        box.remove();
        committed.remove();
      },
    };
  },
};
