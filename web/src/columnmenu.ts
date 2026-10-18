// The column menu: a part that mounts on a GridWidget, after which every
// column header whose column takes Text conditions draws a filter button. A
// press on the button, Alt+ArrowDown in the grid, or `open` drops a popover
// under the header: "Contains" or "Does not contain", a text field with case
// and regex toggles (regex only where the column supports it), and Clear.
// Each change edits the column's Text condition at once; Escape, a press
// outside, or Close shuts the menu. Drawn in the widget's Theme.

import type { Condition, TextMatch } from "./source";
import type { FrameInfo, GridWidget } from "./widget";

export interface ColumnMenu<ColId = unknown> {
  /** The popover while it is open, else `null`. */
  readonly element: HTMLElement | null;
  /** Opens the menu for `colId` (see `GridWidget.openColumnMenu`); `false` if the column takes no conditions. */
  open(colId: ColId, anchor?: DOMRect): boolean;
  close(): void;
  /** Closes the menu and stops the headers drawing filter buttons. */
  destroy(): void;
}

const MENU_STYLE =
  "position: fixed; z-index: 50; width: 300px; box-sizing: border-box; padding: 8px; border-radius: 8px; " +
  "box-shadow: 0 10px 30px rgb(15 23 42 / 0.18), 0 2px 6px rgb(15 23 42 / 0.08);";
const TITLE_STYLE = "margin: 0 2px 6px; font-size: 12px;";
const ROW_STYLE = "display: flex; align-items: center; gap: 4px;";
const FIELD_STYLE =
  "font: inherit; color: inherit; flex: 1; min-width: 0; padding: 4px 8px; border-radius: 6px; outline: none;";
const SELECT_STYLE = "font: inherit; color: inherit; padding: 3px 2px; border-radius: 6px;";
const BUTTON_STYLE =
  "min-width: 24px; height: 24px; padding: 0 5px; border: 1px solid transparent; border-radius: 4px; " +
  "background: transparent; color: inherit; cursor: pointer; font: 12px ui-monospace, Menlo, monospace;";
const FOOT_STYLE = "display: flex; gap: 6px; margin-top: 8px;";
const ACTION_STYLE = "font: inherit; color: inherit; padding: 3px 12px; border-radius: 6px; cursor: pointer;";

export function createColumnMenu<RowId, ColId, V>(widget: GridWidget<RowId, ColId, V>): ColumnMenu<ColId> {
  let menu: { element: HTMLElement; close(): void } | null = null;

  const open = (colId: ColId, anchor: DOMRect) => {
    menu?.close();
    const support = widget.columnSupport(colId);
    if (!support.text) return;
    const regexAllowed = support.text.regex;
    const returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // The condition this menu edits: the column's first Text condition, as last asked for.
    let sent: Condition<ColId> | null =
      latest?.conditions.find(({ condition, state }) => state !== "removing" && condition.id === colId)?.condition ??
      null;

    const label = widget.columnLabel(colId);
    const element = el("div", "binnacle-column-menu", MENU_STYLE);
    element.setAttribute("role", "dialog");
    element.setAttribute("aria-label", `Filter ${label}`);
    const title = el("div", "binnacle-column-menu-title", TITLE_STYLE);
    const name = el("b", "", "");
    name.textContent = label;
    title.append("Filter ", name);

    const textRow = el("div", "binnacle-column-menu-text", ROW_STYLE);
    const how = el("select", "", SELECT_STYLE);
    how.setAttribute("aria-label", "Condition");
    how.append(new Option("Contains", "contains"), new Option("Does not contain", "not"));
    const field = el("input", "", FIELD_STYLE);
    field.type = "text";
    field.placeholder = regexAllowed ? "Text or regex" : "Text";
    field.setAttribute("aria-label", `${label} text`);
    const caseToggle = toggle("Aa", "Match case");
    const regexToggle = toggle(".*", "Use regular expression");
    regexToggle.hidden = !regexAllowed;
    textRow.append(how, field, caseToggle, regexToggle);
    // The OneOf checklist goes here, under the Text condition.
    const values = el("div", "binnacle-column-menu-values", "");
    values.hidden = true;
    const foot = el("div", "binnacle-column-menu-buttons", FOOT_STYLE);
    const clearButton = action("Clear");
    const spacer = el("span", "", "flex: 1;");
    const closeButton = action("Close");
    foot.append(clearButton, spacer, closeButton);
    element.append(title, textRow, values, foot);

    if (sent) {
      const q = sent.kind.query;
      how.value = sent.not ? "not" : "contains";
      field.value = q.text;
      caseToggle.setAttribute("aria-pressed", String(q.caseSensitive));
      regexToggle.setAttribute("aria-pressed", String(q.regex && regexAllowed));
    }

    const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
    const send = () => {
      const query: TextMatch = {
        text: field.value,
        caseSensitive: pressed(caseToggle),
        regex: regexAllowed && pressed(regexToggle),
      };
      const to: Condition<ColId> = { id: colId, kind: { type: "Text", query }, not: how.value === "not" };
      if (sent) widget.replaceCondition(sent, to);
      else widget.addCondition(to);
      sent = query.text === "" ? null : to;
    };
    for (const b of [caseToggle, regexToggle]) {
      b.addEventListener("click", () => {
        b.setAttribute("aria-pressed", String(!pressed(b)));
        paint();
        send();
      });
    }
    how.addEventListener("change", send);
    field.addEventListener("input", send);
    clearButton.addEventListener("click", () => {
      widget.clearConditions(colId);
      sent = null;
      field.value = "";
      field.focus();
    });

    const close = () => {
      if (menu?.element !== element) return;
      menu = null;
      document.removeEventListener("pointerdown", outside, true);
      element.remove();
      if (returnFocus?.isConnected) returnFocus.focus();
    };
    const outside = (e: PointerEvent) => {
      if (!element.contains(e.target as Node)) close();
    };
    closeButton.addEventListener("click", close);
    element.addEventListener("keydown", (e) => {
      if (e.key === "Escape" || (e.key === "Enter" && e.target === field)) {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    });

    const paint = () => {
      const t = widget.theme;
      element.style.font = t.font;
      element.style.color = t.body.color;
      element.style.background = t.body.background;
      element.style.border = `1px solid ${t.border}`;
      title.style.color = t.rowNumber.color;
      name.style.color = t.body.color;
      for (const c of [how, field, clearButton, closeButton]) {
        c.style.background = t.body.background;
        c.style.border = `1px solid ${t.border}`;
      }
      field.style.borderColor = t.focus.color;
      for (const b of [caseToggle, regexToggle]) {
        const on = pressed(b);
        b.style.color = on ? t.focus.color : t.loading;
        b.style.borderColor = on ? t.focus.color : "transparent";
      }
    };
    paint();

    document.body.append(element);
    const width = element.offsetWidth;
    element.style.left = `${Math.max(8, Math.min(anchor.left, innerWidth - width - 8))}px`;
    element.style.top = `${Math.max(8, Math.min(anchor.bottom + 4, innerHeight - element.offsetHeight - 8))}px`;
    document.addEventListener("pointerdown", outside, true);
    menu = { element, close };
    field.focus();
    field.select();
  };

  let latest: FrameInfo<RowId, ColId> | null = null;
  const unsubscribe = widget.subscribe((f) => {
    latest = f;
  });
  const detach = widget.attachColumnMenu(open);

  return {
    get element() {
      return menu?.element ?? null;
    },
    open: (colId, anchor) => widget.openColumnMenu(colId, anchor),
    close: () => menu?.close(),
    destroy() {
      menu?.close();
      detach();
      unsubscribe();
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

function action(text: string): HTMLButtonElement {
  const b = el("button", "", ACTION_STYLE);
  b.type = "button";
  b.textContent = text;
  return b;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, style: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  e.style.cssText = style;
  return e;
}
