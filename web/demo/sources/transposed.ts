// The transposed source: the records on their side, as a pivoted or
// column-major table has them. Each record is a column headed with its id and
// each field a row; the Table Source is the records' own `fromRows` seen
// through `transpose`, so it adds and fixes nothing: what works along one
// axis only shows up here. Each field row draws its own way (the id and
// weight end-aligned, the crew as a bar). A click on a field's row header
// sorts the records by it (Shift adds a field), as a column header click
// sorts the records upright, and a column header click selects the record.
// The filter box keeps the records whose name or port contains its text, as
// columns; the filter bar's quick filter and a field's row menu filter the
// records by their values too, and a record's column menu filters the
// fields. Find, editing, moves, pinning and hiding work as in the records,
// a field row's header dragging the field and a record's column header the
// record, so hiding a column hides a record, and the columns' chooser lists
// the records (at most 100 of them hide).

import {
  barRenderer,
  byRow,
  createChooser,
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
import { layoutControls } from "./layout";
import { parse, portPicker, type Value } from "./records";
import { matches, RECORD_HIDE_CAP, RECORDS, rows, sortedMoves, type RecordRow } from "./records-data";

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
  filterBar: true,
  async mount(host, toolbar, look) {
    const records = fromRows(
      rows,
      [{ key: "id", filterable: false }, { key: "name" }, { key: "weight" }, { key: "crew" }, { key: "port" }],
      { ...sortedMoves(), hideCap: { rows: RECORD_HIDE_CAP } },
    );
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
      headerClick: { rows: "sort", cols: "select" },
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
    const layout = layoutControls(widget, "transposed");
    const columnMenu = createColumnMenu(widget);
    const chooser = createChooser(widget);
    const bar = createFilterBar(widget, {
      columnMenu,
      axis: "cols",
      nouns: { rows: ["field", "fields"], cols: ["record", "records"] },
    });
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
        layout.destroy();
        findButton.remove();
        bar.destroy();
        columnMenu.destroy();
        chooser.destroy();
        widget.destroy();
        frame.remove();
        records.destroy();
        box.remove();
        committed.remove();
      },
    };
  },
};
