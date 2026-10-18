// The filter bar: a part the embedder places beside a GridWidget, usually
// above it. It holds the rows' quick filter field with its case and regex
// toggles and a clear button, and the row count. Each change to the field
// sets the widget's quick filter; the widget announces the count to
// assistive technology once the filter is applied. Drawn in the widget's
// Theme; hidden while the rows take no quick filter.

import { rowCount } from "./aria";
import type { FrameInfo, GridWidget } from "./widget";

export interface FilterBarOptions {
  /** What the empty field says; "Filter rows…" by default. */
  placeholder?: string;
}

export interface FilterBar {
  readonly element: HTMLElement;
  /** Clears the quick filter and focuses the field. */
  clear(): void;
  /** Moves DOM focus to the field. */
  focus(): void;
  destroy(): void;
}

const BAR_STYLE = "display: none; align-items: center; gap: 8px; padding: 6px 10px; box-sizing: border-box;";
const FIELD_STYLE = "display: flex; align-items: center; gap: 1px; padding: 0 3px 0 0; border-radius: 6px;";
const INPUT_STYLE =
  "font: inherit; color: inherit; width: 220px; padding: 4px 8px; border: 0; outline: none; background: transparent;";
const BUTTON_STYLE =
  "min-width: 24px; height: 24px; padding: 0 5px; border: 1px solid transparent; border-radius: 4px; " +
  "background: transparent; color: inherit; cursor: pointer; font: 12px ui-monospace, Menlo, monospace;";
const COUNT_STYLE = "margin-left: auto; font-variant-numeric: tabular-nums; white-space: nowrap;";

export function createFilterBar<RowId, ColId, V>(
  widget: GridWidget<RowId, ColId, V>,
  options: FilterBarOptions = {},
): FilterBar {
  const element = el("div", "binnacle-filter-bar", BAR_STYLE);
  element.setAttribute("role", "search");
  const field = el("div", "binnacle-quick-filter", FIELD_STYLE);
  const input = el("input", "", INPUT_STYLE);
  // Text, not search: a search input's own clear button would duplicate the bar's.
  input.type = "text";
  input.placeholder = options.placeholder ?? "Filter rows…";
  input.setAttribute("aria-label", "Filter rows");
  const caseToggle = toggle("Aa", "Match case");
  const regexToggle = toggle(".*", "Use regular expression");
  const clearButton = el("button", "binnacle-clear", BUTTON_STYLE);
  clearButton.type = "button";
  clearButton.textContent = "×";
  clearButton.setAttribute("aria-label", "Clear filter");
  clearButton.hidden = true;
  field.append(input, caseToggle, regexToggle, clearButton);
  const count = el("span", "binnacle-row-count", COUNT_STYLE);
  element.append(field, count);

  let regexAllowed = false;
  const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
  const send = () => {
    clearButton.hidden = input.value === "";
    widget.setQuickFilter(
      input.value === ""
        ? null
        : {
            text: input.value,
            caseSensitive: pressed(caseToggle),
            regex: regexAllowed && pressed(regexToggle),
            searches: "shown",
          },
    );
  };
  for (const b of [caseToggle, regexToggle]) {
    b.addEventListener("click", () => {
      b.setAttribute("aria-pressed", String(!pressed(b)));
      paint();
      send();
    });
  }
  const clear = () => {
    input.value = "";
    send();
    input.focus();
  };
  clearButton.addEventListener("click", clear);
  input.addEventListener("input", send);
  input.addEventListener("focus", () => paint());
  input.addEventListener("blur", () => paint());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      send();
    } else if (e.key === "Escape" && input.value !== "") {
      e.preventDefault();
      clear();
    }
  });

  /** Colours from the widget's Theme, which may change at any frame. */
  const paint = () => {
    const t = widget.theme;
    element.style.font = t.font;
    element.style.color = t.header.color;
    element.style.background = t.header.background;
    element.style.borderBottom = `1px solid ${t.border}`;
    field.style.background = t.body.background;
    field.style.color = t.body.color;
    // The field's border stands in for the input's focus ring.
    const lit = input.value !== "" || document.activeElement === input;
    field.style.border = `1px solid ${lit ? t.focus.color : t.border}`;
    count.style.color = t.rowNumber.color;
    for (const b of [caseToggle, regexToggle]) {
      const on = pressed(b);
      b.style.color = on ? t.focus.color : t.loading;
      b.style.borderColor = on ? t.focus.color : "transparent";
    }
  };

  const frame = (f: FrameInfo<RowId, ColId>) => {
    // Inline styles would win over the `hidden` attribute.
    element.style.display = f.quickFilter === null ? "none" : "flex";
    regexAllowed = f.quickFilter?.regex ?? false;
    regexToggle.hidden = !regexAllowed;
    const filtered = f.rows.total !== null && f.rows.total !== f.rows.count;
    count.textContent = filtered
      ? `${f.rows.count.toLocaleString("en")} of ${f.rows.total!.toLocaleString("en")}`
      : rowCount(f.rows.count, null);
    paint();
  };
  const unsubscribe = widget.subscribe(frame);

  return {
    element,
    clear,
    focus: () => input.focus(),
    destroy() {
      unsubscribe();
      element.remove();
    },
  };
}

function toggle(text: string, label: string): HTMLButtonElement {
  const b = el("button", "binnacle-toggle", BUTTON_STYLE);
  b.type = "button";
  b.textContent = text;
  b.title = label;
  b.setAttribute("aria-label", label);
  b.setAttribute("aria-pressed", "false");
  return b;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, style: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  e.style.cssText = style;
  return e;
}
