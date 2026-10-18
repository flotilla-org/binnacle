// The filter bar: a part the embedder places beside a GridWidget, usually
// above it. It holds the rows' quick filter field with its case and regex
// toggles and a clear button, a chip for each column condition (× removes
// it), "+ Filter" to add one through a column menu, "Re-apply" while rows in
// view were left unmatched by the user's edits, and the row count. Each
// change to the field sets the widget's quick filter; the widget announces
// the count to assistive technology once the filter is applied. Drawn in the
// widget's Theme; hidden while the rows take no quick filter, have no
// conditions and no column menu is given.

import { rowCount } from "./aria";
import type { ColumnMenu } from "./columnmenu";
import type { Condition } from "./source";
import type { ConditionState } from "./snapshot";
import type { FrameInfo, GridWidget } from "./widget";

export interface FilterBarOptions<ColId = unknown> {
  /** What the empty field says; "Filter rows…" by default. */
  placeholder?: string;
  /** The column menu (`createColumnMenu`) that "+ Filter" and a press on a chip open; without one, neither does. */
  columnMenu?: ColumnMenu<ColId>;
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
const CHIPS_STYLE = "display: flex; flex-wrap: wrap; gap: 6px;";
const CHIP_STYLE = "display: inline-flex; align-items: center; border-radius: 999px; overflow: hidden;";
const CHIP_LABEL_STYLE =
  "font: inherit; color: inherit; border: 0; background: transparent; padding: 3px 4px 3px 10px; white-space: nowrap;";
const CHIP_REMOVE_STYLE =
  "font: inherit; color: inherit; border: 0; background: transparent; padding: 3px 8px 3px 4px; cursor: pointer;";
const ADD_STYLE =
  "font: inherit; border-radius: 999px; padding: 3px 10px; cursor: pointer; background: transparent; white-space: nowrap;";
const PICKER_STYLE =
  "position: fixed; z-index: 50; min-width: 160px; max-height: 280px; overflow: auto; padding: 4px 0; " +
  "border-radius: 8px; box-shadow: 0 10px 30px rgb(15 23 42 / 0.18);";
const PICK_STYLE =
  "display: block; width: 100%; text-align: left; font: inherit; color: inherit; border: 0; " +
  "background: transparent; padding: 4px 12px; cursor: pointer;";
const REAPPLY_STYLE =
  "font: inherit; border-radius: 999px; padding: 3px 10px; cursor: pointer; background: transparent; white-space: nowrap;";
const COUNT_STYLE = "margin-left: auto; font-variant-numeric: tabular-nums; white-space: nowrap;";

export function createFilterBar<RowId, ColId, V>(
  widget: GridWidget<RowId, ColId, V>,
  options: FilterBarOptions<ColId> = {},
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
  const chips = el("div", "binnacle-chips", CHIPS_STYLE);
  chips.setAttribute("role", "list");
  chips.setAttribute("aria-label", "Column filters");
  const menu = options.columnMenu;
  const add = el("button", "binnacle-add-filter", ADD_STYLE);
  add.type = "button";
  add.textContent = "+ Filter";
  add.setAttribute("aria-haspopup", "menu");
  add.hidden = !menu;
  const reapply = el("button", "binnacle-reapply", REAPPLY_STYLE);
  reapply.type = "button";
  reapply.textContent = "Re-apply";
  reapply.hidden = true;
  reapply.addEventListener("click", () => widget.reapplyView());
  const count = el("span", "binnacle-row-count", COUNT_STYLE);
  element.append(field, chips, add, reapply, count);

  let picker: HTMLElement | null = null;
  const closePicker = () => {
    picker?.remove();
    picker = null;
    document.removeEventListener("pointerdown", outsidePicker, true);
  };
  const outsidePicker = (e: PointerEvent) => {
    if (picker && !picker.contains(e.target as Node) && e.target !== add) closePicker();
  };
  /** Lists the columns that take conditions; picking one opens its menu. */
  const openPicker = async () => {
    const columns = (await widget.columns()).filter((c) => c.support.text || c.support.oneOf);
    closePicker();
    const t = widget.theme;
    const list = el("div", "binnacle-filter-picker", PICKER_STYLE);
    list.setAttribute("role", "menu");
    list.setAttribute("aria-label", "Filter column");
    list.style.font = t.font;
    list.style.color = t.body.color;
    list.style.background = t.body.background;
    list.style.border = `1px solid ${t.border}`;
    const items = columns.map((c) => {
      const b = el("button", "", PICK_STYLE);
      b.type = "button";
      b.setAttribute("role", "menuitem");
      b.textContent = c.label;
      b.addEventListener("pointerenter", () => b.focus());
      b.addEventListener("focus", () => (b.style.background = t.headerHovered.background));
      b.addEventListener("blur", () => (b.style.background = "transparent"));
      b.addEventListener("click", () => {
        const anchor = add.getBoundingClientRect();
        closePicker();
        menu?.open(c.id, anchor);
      });
      return b;
    });
    list.append(...items);
    list.addEventListener("keydown", (e) => {
      const at = items.indexOf(document.activeElement as HTMLButtonElement);
      const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
      if (step !== 0 && items.length > 0) {
        e.preventDefault();
        items[(at + step + items.length) % items.length]!.focus();
      } else if (e.key === "Escape") {
        e.preventDefault();
        closePicker();
        add.focus();
      }
    });
    const r = add.getBoundingClientRect();
    list.style.left = `${r.left}px`;
    list.style.top = `${r.bottom + 4}px`;
    document.body.append(list);
    document.addEventListener("pointerdown", outsidePicker, true);
    picker = list;
    items[0]?.focus();
  };
  add.addEventListener("click", () => (picker ? closePicker() : void openPicker()));

  /** One chip: "port: not Bergen", faded while pending; the label opens the column menu, × removes it. */
  const chip = (condition: Condition<ColId>, state: ConditionState) => {
    const t = widget.theme;
    const item = el("div", "binnacle-chip", CHIP_STYLE);
    item.setAttribute("role", "listitem");
    const text = chipText(widget.columnLabel(condition.id), condition);
    const label = el("button", "binnacle-chip-label", CHIP_LABEL_STYLE);
    label.type = "button";
    label.textContent = text;
    label.disabled = !menu || state === "removing";
    label.style.cursor = label.disabled ? "default" : "pointer";
    label.addEventListener("click", () => menu?.open(condition.id, item.getBoundingClientRect()));
    const remove = el("button", "binnacle-chip-remove", CHIP_REMOVE_STYLE);
    remove.type = "button";
    remove.textContent = "×";
    remove.setAttribute("aria-label", `Remove filter ${text}`);
    remove.hidden = state === "removing";
    remove.style.color = t.rowNumber.color;
    remove.addEventListener("click", () => widget.removeCondition(condition));
    item.append(label, remove);
    item.style.border = `1px solid ${t.sortMarker}`;
    item.style.background = t.picks.background;
    item.style.color = t.picks.color;
    if (state !== "applied") item.style.opacity = "0.55";
    if (state === "removing") label.style.textDecoration = "line-through";
    if (state !== "applied") item.setAttribute("aria-busy", "true");
    return item;
  };
  let shownChips = "";
  let shownQuick = "null";

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
    add.style.color = t.rowNumber.color;
    add.style.border = `1px dashed ${t.border}`;
    reapply.style.color = t.focus.color;
    reapply.style.border = `1px solid ${t.focus.color}`;
    for (const b of [caseToggle, regexToggle]) {
      const on = pressed(b);
      b.style.color = on ? t.focus.color : t.loading;
      b.style.borderColor = on ? t.focus.color : "transparent";
    }
  };

  const frame = (f: FrameInfo<RowId, ColId>) => {
    // Inline styles would win over the `hidden` attribute.
    element.style.display = f.quickFilter === null && f.conditions.length === 0 && !menu ? "none" : "flex";
    field.style.display = f.quickFilter === null ? "none" : "flex";
    regexAllowed = f.quickFilter?.regex ?? false;
    // Follows a quick filter set from elsewhere (the find bar's "Show only matching rows").
    const quick = JSON.stringify(f.quick);
    const changed = quick !== shownQuick;
    shownQuick = quick;
    if (changed && document.activeElement !== input && (f.quick?.text ?? "") !== input.value) {
      input.value = f.quick?.text ?? "";
      caseToggle.setAttribute("aria-pressed", String(f.quick?.caseSensitive ?? false));
      regexToggle.setAttribute("aria-pressed", String(f.quick?.regex ?? false));
      clearButton.hidden = input.value === "";
    }
    // Rebuilt only when they change, so a press on a chip is not lost to a repaint.
    const key = [
      widget.theme.name,
      ...f.conditions.map(({ condition, state }) => `${state} ${chipText(widget.columnLabel(condition.id), condition)}`),
    ].join("\n");
    if (key !== shownChips) {
      shownChips = key;
      chips.replaceChildren(...f.conditions.map(({ condition, state }) => chip(condition, state)));
    }
    regexToggle.hidden = !regexAllowed;
    const unmatched = f.rows.unmatched;
    reapply.hidden = unmatched === 0;
    const rows = unmatched === 1 ? "1 edited row no longer matches" : `${unmatched} edited rows no longer match`;
    reapply.title = `${rows} the filter: apply it again`;
    reapply.setAttribute("aria-label", `Re-apply the filter: ${rows}`);
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
      closePicker();
      element.remove();
    },
  };
}

function chipText<ColId>(label: string, c: Condition<ColId>): string {
  const not = c.not ? "not " : "";
  if (c.kind.type === "OneOf") return `${label}: ${not}${listedText(c.kind.values)}`;
  const q = c.kind.query;
  const text = q.regex ? `/${q.text}/` : q.text;
  return `${label}: ${not}${text}${q.caseSensitive ? " (Aa)" : ""}`;
}

/** A OneOf's values for a chip: up to three, then how many more; "(Blanks)" for a blank, "nothing" for none. */
export function listedText(values: (string | null)[]): string {
  if (values.length === 0) return "nothing";
  const named = values.slice(0, 3).map((v) => (v === null ? "(Blanks)" : v === "" ? '""' : v));
  const more = values.length - named.length;
  return named.join(", ") + (more > 0 ? ` +${more}` : "");
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
