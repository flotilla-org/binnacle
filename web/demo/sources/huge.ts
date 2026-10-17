// The huge source: 10^15 rows by 12 columns, position identity on both axes,
// values made up from the Position, like the ratatui demo's `--huge`. A "Go
// to row" box in the toolbar focuses a row by number; it lives outside the
// grid because typing in the grid starts an edit.

import { GridWidget, type TableSource } from "../../src";
import type { DemoSource } from "./demo-source";

const ROWS = 1e15;
const COLS = 12;

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}

const source: TableSource<number, number, string> = {
  rows: { identity: "position", count: () => ROWS, window: range },
  cols: { identity: "position", count: () => COLS, window: range, label: (id) => `col ${id}` },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => `r${r}c${c}`)),
};

/** A row number as typed: digits with optional separators, or e-notation such as `1e12`. */
export function rowNumber(text: string): number | null {
  const plain = text.trim().replace(/[\s,_]/g, "");
  if (!/^(\d+|\d+(\.\d+)?e\d+)$/i.test(plain)) return null;
  const n = Number(plain);
  return Number.isSafeInteger(n) && n >= 1 && n <= ROWS ? n : null;
}

/** The "Go to row" box: Enter focuses the row at the top of the grid and hands it the keyboard. */
function goToRow(widget: GridWidget<number, number, string>): HTMLFormElement {
  const form = document.createElement("form");
  form.className = "go-to";
  const box = document.createElement("input");
  box.type = "text";
  box.inputMode = "numeric";
  box.placeholder = "Go to row";
  box.setAttribute("aria-label", "Go to row");
  box.title = "A row number from 1 to 10^15, such as 123,456,789 or 1e12; Enter to go";
  box.addEventListener("input", () => box.removeAttribute("aria-invalid"));
  form.append(box);
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const n = rowNumber(box.value);
    if (n === null) {
      box.setAttribute("aria-invalid", "true");
      return;
    }
    // Like a spreadsheet's Go To, the row lands at the top rather than just in view.
    widget.scrollTo(n - 1);
    void widget.focus(n - 1).then(() => widget.element.querySelector<HTMLElement>(".binnacle-input")?.focus());
  });
  return form;
}

export const huge: DemoSource = {
  name: "huge",
  description: "10^15 rows, position identity",
  async mount(host, toolbar, look) {
    const widget = await GridWidget.create(host, { source, columnWidth: 168, ...look });
    const form = goToRow(widget);
    toolbar.append(form);
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
        form.remove();
      },
    };
  },
};
