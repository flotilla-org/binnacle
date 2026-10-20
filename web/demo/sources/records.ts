// The records source: a few thousand generated rows through `fromRows`,
// like the ratatui demo's records. Every column sorts (Shift-click adds a
// column), column and row headers drag to move columns and rows (a move on
// a sorted axis keeps the order shown and drops the sort, or with
// `?sortedMoves=refuse` is refused, the reason shown where it was
// dropped), and the filter box keeps the rows
// whose name or port contains its text. The filter bar above the grid holds
// the quick filter, which keeps the rows with its text in any column (with
// case and regex toggles), a chip for each column condition, "+ Filter" and
// the count. Every column but the id takes Text conditions: its header's
// funnel, or Alt+ArrowDown, opens the column menu, which also hides the
// column (or the picked or selected ones), shows only those, or shows hidden
// ones (Mod+Alt+0 hides the chosen columns, Mod+Alt+9 the rows; Shift with
// either lists the hidden ones); the filter bar counts what is left, "12 of
// 30 columns". "Choose columns…" in the menu, or Mod+Alt+O (Mod+Alt+Shift+O
// for the rows), opens the chooser, a checklist of every column or row to
// tick shown or hidden. The rows hide at most 100 at a time, so a hide past
// that says to filter instead. Cells edit in place: type,
// or press F2 or Enter, or double-click; the port column picks from a list.
// Ctrl/Cmd-F in the grid (or Find in the toolbar) opens the find bar under
// the filter bar: matches are marked, Enter and Shift+Enter (or F3 and
// Shift+F3 in the grid) step between them. Alt+Shift+ArrowLeft and
// Alt+Shift+ArrowRight pin the focused column left or right, and
// Alt+Shift+ArrowUp and Alt+Shift+ArrowDown its row to the top or bottom
// (again, or Alt+Shift+Backspace for the column, unpins it), as do the column
// and row menus' Pin buttons; "Freeze up to here" (or Alt+Shift+Home) pins
// every column from the first to the focused one. Pinned items stay by the
// edge, past a line, while the rest scrolls under them, through sorts and
// filters, and the arrow keys step through them in the order drawn.

import { createChooser, createColumnMenu, createFilterBar, createFindBar, fromRows, GridWidget, type Editor } from "../../src";
import type { DemoSource } from "./demo-source";
import { layoutControls } from "./layout";
import { matches, PORTS, RECORD_HIDE_CAP, RECORDS, rows, sortedMoves, WIDTHS, type RecordRow } from "./records-data";

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
  description: `${RECORDS.toLocaleString("en")} records: sort, filter, move columns and rows, edit`,
  showIds: true,
  filterBar: true,
  async mount(host, toolbar, look) {
    const source = fromRows(
      rows,
      [{ key: "id", filterable: false }, { key: "name" }, { key: "weight" }, { key: "crew" }, { key: "port" }],
      { ...sortedMoves(), hideCap: { rows: RECORD_HIDE_CAP } },
    );
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
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, {
      source,
      columnWidth: 140,
      initialColumnWidth: (col) => WIDTHS[col],
      ...look,
      onCommit(id, col, value) {
        const parsed = parse(col, value);
        if (parsed === undefined) {
          committed.textContent = `${col} of ${id} kept`;
          return false;
        }
        (source.row(id) as Record<keyof RecordRow, Value>)[col] = parsed;
        committed.textContent = `${col} of ${id} = ${JSON.stringify(parsed)}`;
        source.edited(id);
        return true;
      },
      editor: (_id, col) => (col === "port" ? portPicker() : null),
    });
    const layout = layoutControls(widget, "records");
    const columnMenu = createColumnMenu(widget);
    const chooser = createChooser(widget);
    const bar = createFilterBar(widget, { columnMenu });
    const findBar = createFindBar(widget);
    frame.prepend(bar.element, findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.title = "Find in the table (Ctrl/Cmd-F in the grid)";
    findButton.addEventListener("click", () => findBar.open());
    toolbar.append(findButton, ...layout.elements);
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
        source.destroy();
        box.remove();
        committed.remove();
      },
    };
  },
};
