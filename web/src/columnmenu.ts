// The column menu: a part that mounts on a GridWidget, after which every
// column header whose column takes conditions draws a filter button. A press
// on the button, Alt+ArrowDown in the grid, or `open` drops a popover under
// the header: "Pin left" (or "Unpin" on a pinned column), "Hide column" and
// "Show hidden columns ▸" where the columns hide, then for a column that
// takes conditions, "Contains" or "Does not contain", a text field with case and
// regex toggles (regex only where the column supports it), then the
// checklist of the column's values (where it takes OneOf conditions) with a
// search, counts, "(Blanks)" and a note on whether the list is capped and
// narrowed, and Clear. A column that takes only OneOf shows just the
// checklist. Each change edits the column's conditions at once, the
// checklist's by the rules in `checklist.ts`, typing in the text field once
// the widget's typing pause has passed; a Text condition the Table Source did
// not apply shows red with its message under the field. Escape, a press
// outside, or Close shuts the menu. Drawn in the widget's Theme.
//
// It is the row menu too: a row header's button, Alt+ArrowRight or `openRow`
// drops the same popover beside the row header, filtering the columns by
// the row's values, with "Hide row" where the rows hide. It has no Pin:
// rows pin from the widget's API. "Show hidden ▸" opens a flyout listing
// the hidden items to show again (with a search past eight of them) and
// "Show all".

import {
  columnConditions,
  excluded,
  headTicked,
  replaced,
  tick,
  tickHead,
  ticked,
  type ColumnConditions,
  type Shown,
  type Written,
} from "./checklist";
import type { AxisName } from "./codes";
import type { Condition, Distinct, ListedValue, TextMatch } from "./source";
import type { FrameInfo, GridWidget, MenuAt } from "./widget";
import { createPause } from "./pause";

export interface ColumnMenu<ColId = unknown, RowId = unknown> {
  /** The popover while it is open, else `null`. */
  readonly element: HTMLElement | null;
  /** Opens the menu for `colId` (see `GridWidget.openColumnMenu`); `false` if the column takes no conditions and the columns do not hide. */
  open(colId: ColId, anchor?: DOMRect, at?: MenuAt): boolean;
  /**
   * Opens the menu for row `rowId`, filtering the columns (see
   * `GridWidget.openRowMenu`); `false` if they take no conditions on it and
   * the rows do not hide.
   */
  openRow(rowId: RowId, anchor?: DOMRect, at?: MenuAt): boolean;
  close(): void;
  /** Closes the menu and stops the headers drawing filter buttons. */
  destroy(): void;
}

/** How many values the checklist asks for; it says so when there are more. */
const LIMIT = 1000;

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
const SEPARATOR_STYLE = "height: 1px; margin: 8px -8px;";
const SEARCH_STYLE = FIELD_STYLE + " display: block; width: 100%; box-sizing: border-box;";
const LIST_HEIGHT = 220;
const LIST_MIN_HEIGHT = 96;
const LIST_STYLE = `max-height: ${LIST_HEIGHT}px; overflow: auto; margin: 6px 0 4px; padding: 2px 0; border-radius: 6px;`;
const ITEM_STYLE = "display: flex; align-items: center; gap: 6px; padding: 2px 8px; white-space: nowrap;";
const ITEM_TEXT_STYLE = "flex: 1; overflow: hidden; text-overflow: ellipsis;";
const COUNT_STYLE = "font-size: 11px; font-variant-numeric: tabular-nums;";
const NOTICE_STYLE = "font-size: 11px; min-height: 14px; margin: 0 2px;";
const ERROR_STYLE = "font-size: 11px; margin: 4px 2px 0;";
const ACTIONS_STYLE = "display: flex; flex-direction: column; gap: 2px;";
const MENU_ITEM_STYLE =
  "display: flex; align-items: center; gap: 8px; font: inherit; color: inherit; text-align: start; " +
  "padding: 4px 8px; border: 0; border-radius: 6px; background: transparent; cursor: pointer;";
const FLYOUT_STYLE = MENU_STYLE.replace("width: 300px", "width: 220px");
const HIDDEN_LIST_STYLE = "max-height: 240px; overflow: auto; display: flex; flex-direction: column; gap: 2px;";

/** How many hidden items the flyout lists before it offers a search. */
const SEARCH_PAST = 8;

/** Numbers each menu's error message, for `aria-describedby`. */
let errors = 0;

export function createColumnMenu<RowId, ColId, V>(widget: GridWidget<RowId, ColId, V>): ColumnMenu<ColId, RowId> {
  type Id = RowId | ColId;
  let menu: { element: HTMLElement; close(): void } | null = null;

  /** The conditions on `axis` as the latest frame lists them. */
  const listed = (axis: AxisName) => (axis === "rows" ? latest?.conditions : latest?.columnsFilter.conditions) ?? [];

  /**
   * Opens the menu for item `colId` of the other axis, filtering `axis` by
   * it: a column filters the rows, a row the columns. It hides the item on
   * its own axis.
   */
  const open = (axis: AxisName, colId: Id, anchor: DOMRect, at: MenuAt) => {
    menu?.close();
    const support = axis === "rows" ? widget.columnSupport(colId as ColId) : widget.rowSupport(colId as RowId);
    const own: AxisName = axis === "rows" ? "cols" : "rows";
    const hides = widget.canHide(own);
    const filters = !!support.text || support.oneOf;
    // A column's menu also pins it; a row's only filters the columns.
    const pinnable = axis === "rows";
    if (!filters && !pinnable && !hides) return;
    const regexAllowed = support.text?.regex ?? false;
    const returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    // The item's conditions as this menu last asked for them, which each
    // change replaces: the latest frame may not have caught up yet.
    const asked = listed(axis)
      .filter(({ state }) => state !== "removing")
      .map((c) => c.condition as Condition<Id>);
    let column: ColumnConditions<Id> = columnConditions(asked, colId);

    /** Asks the widget for `written` in place of the item's Text condition and OneOf. */
    const write = (written: Written<Id>) => {
      const pairs = [
        [column.text, written.text],
        [column.oneOf, written.oneOf],
      ] as const;
      for (const [from, to] of pairs) {
        if (from && to) {
          if (JSON.stringify(from) !== JSON.stringify(to)) widget.replaceCondition(from, to, axis);
        } else if (from) widget.removeCondition(from, axis);
        else if (to) widget.addCondition(to, axis);
      }
      column = columnConditions(replaced(column, written), colId);
    };

    const label = axis === "rows" ? widget.columnLabel(colId as ColId) : widget.rowLabel(colId as RowId);
    const element = el("div", "binnacle-column-menu", MENU_STYLE);
    element.setAttribute("role", "dialog");
    const noun = own === "cols" ? "column" : "row";
    const heading = !filters ? `${noun[0]!.toUpperCase()}${noun.slice(1)} ` : axis === "rows" ? "Filter " : "Filter columns by ";
    element.setAttribute("aria-label", heading + label);
    const title = el("div", "binnacle-column-menu-title", TITLE_STYLE);
    const name = el("b", "", "");
    name.textContent = label;
    title.append(heading, name);

    // Pin, Hide, and the hidden items to show again.
    const actions = el("div", "binnacle-column-menu-actions", ACTIONS_STYLE);
    // Inline styles would win over the `hidden` attribute.
    actions.style.display = hides || pinnable ? "flex" : "none";
    /** A menu item, lit while hovered or focused. */
    const lit = (b: HTMLButtonElement) => {
      const on = () => (b.style.background = b.disabled ? "transparent" : widget.theme.border);
      const off = () => (b.style.background = "transparent");
      b.addEventListener("pointerenter", on);
      b.addEventListener("pointerleave", () => b !== document.activeElement && off());
      b.addEventListener("focus", on);
      b.addEventListener("blur", off);
      return b;
    };
    // As of the latest frame, so it follows pins made while the menu is open.
    const pinned = () => pinnable && widget.pinned("cols").includes(colId as ColId);
    const pinButton = lit(menuItem("Pin left", "binnacle-column-menu-pin"));
    if (!pinnable) pinButton.style.display = "none";
    pinButton.addEventListener("click", () => {
      const unpin = pinned();
      close();
      if (unpin) widget.unpin("cols", colId as ColId);
      else widget.pin("cols", colId as ColId);
    });
    const hideButton = lit(menuItem(`Hide ${noun}`, "binnacle-column-menu-hide"));
    const showHidden = lit(menuItem("", "binnacle-column-menu-show-hidden"));
    if (!hides) hideButton.style.display = showHidden.style.display = "none";
    showHidden.setAttribute("aria-haspopup", "menu");
    showHidden.setAttribute("aria-expanded", "false");
    const actionsSeparator = el("div", "", SEPARATOR_STYLE);
    actionsSeparator.hidden = !filters;
    actions.append(pinButton, hideButton, showHidden, actionsSeparator);
    hideButton.addEventListener("click", () => {
      widget.hide([colId], own);
      close();
    });
    let flyout: { element: HTMLElement; search: HTMLInputElement; list: HTMLElement; listed: number } | null = null;
    const hiddenItems = () =>
      widget.hidden(own).map((id) => ({ id, label: own === "cols" ? widget.columnLabel(id as ColId) : widget.rowLabel(id as RowId) }));
    /** Lists the hidden items the flyout's search keeps, each a button that shows it, then Show all. */
    const listHidden = () => {
      if (!flyout) return;
      const items = hiddenItems();
      const { search: find, list: hiddenList } = flyout;
      const searching = items.length > SEARCH_PAST;
      // Inline styles would win over the `hidden` attribute.
      find.style.display = searching ? "block" : "none";
      const text = searching ? find.value.toLowerCase() : "";
      const kept = items.filter((i) => i.label.toLowerCase().includes(text));
      const at = hiddenList.contains(document.activeElement) ? [...hiddenList.children].indexOf(document.activeElement!) : -1;
      hiddenList.replaceChildren(
        ...kept.map(({ id, label: shown }) => {
          const b = lit(menuItem(shown, "binnacle-column-menu-hidden-item"));
          b.setAttribute("role", "menuitem");
          b.addEventListener("click", () => widget.show([id], own));
          return b;
        }),
      );
      if (items.length > 0) {
        const all = lit(menuItem("Show all", "binnacle-column-menu-show-all"));
        all.setAttribute("role", "menuitem");
        all.addEventListener("click", () => {
          widget.show(items.map((i) => i.id), own);
          closeFlyout(true);
        });
        hiddenList.append(all);
      } else {
        const none = el("div", "binnacle-column-menu-empty", "padding: 4px 8px;");
        none.textContent = `No hidden ${noun}s`;
        none.style.color = widget.theme.loading;
        hiddenList.append(none);
      }
      // Once the list empties it closes, and focus goes back to the menu; else focus stays in the list.
      const emptied = items.length === 0 && flyout.listed > 0;
      flyout.listed = items.length;
      if (emptied) {
        const lost = document.activeElement === document.body || flyout.element.contains(document.activeElement);
        closeFlyout(false);
        if (lost) hideButton.focus();
      } else if (at >= 0) (hiddenList.children[Math.min(at, hiddenList.children.length - 1)] as HTMLElement | undefined)?.focus();
    };
    const closeFlyout = (refocus: boolean) => {
      flyout?.element.remove();
      flyout = null;
      showHidden.setAttribute("aria-expanded", "false");
      if (refocus) showHidden.focus();
    };
    const openFlyout = () => {
      if (flyout) return;
      const fly = el("div", "binnacle-column-menu-flyout", FLYOUT_STYLE);
      fly.setAttribute("role", "menu");
      fly.setAttribute("aria-label", `Hidden ${noun}s`);
      const find = el("input", "binnacle-column-menu-hidden-search", SEARCH_STYLE + " margin-bottom: 6px;");
      find.type = "search";
      find.placeholder = `Search hidden ${noun}s`;
      find.setAttribute("aria-label", `Search hidden ${noun}s`);
      const hiddenList = el("div", "binnacle-column-menu-hidden", HIDDEN_LIST_STYLE);
      fly.append(find, hiddenList);
      find.addEventListener("input", listHidden);
      fly.addEventListener("keydown", (e) => {
        const items = [...hiddenList.querySelectorAll("button")];
        const i = items.indexOf(document.activeElement as HTMLButtonElement);
        if (e.key === "Escape" || e.key === "ArrowLeft") {
          if (e.key === "ArrowLeft" && e.target === find) return;
          e.preventDefault();
          e.stopPropagation();
          closeFlyout(true);
        } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
          e.preventDefault();
          const step = e.key === "ArrowDown" ? 1 : -1;
          items[i < 0 ? (step > 0 ? 0 : items.length - 1) : (i + step + items.length) % items.length]?.focus();
        }
      });
      flyout = { element: fly, search: find, list: hiddenList, listed: 0 };
      showHidden.setAttribute("aria-expanded", "true");
      listHidden();
      paint();
      document.body.append(fly);
      const box = element.getBoundingClientRect();
      const item = showHidden.getBoundingClientRect();
      const width = fly.offsetWidth;
      const left = box.right + 4 + width <= innerWidth - 8 ? box.right + 4 : Math.max(8, box.left - 4 - width);
      fly.style.left = `${left}px`;
      fly.style.top = `${Math.max(8, Math.min(item.top - 8, innerHeight - fly.offsetHeight - 8))}px`;
      (find.style.display === "none" ? hiddenList.querySelector("button") : find)?.focus();
    };
    showHidden.addEventListener("click", () => (flyout ? closeFlyout(true) : openFlyout()));
    showHidden.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowRight") return;
      e.preventDefault();
      openFlyout();
    });
    const markHidden = () => {
      const n = widget.hidden(own).length;
      showHidden.textContent = `Show hidden ${noun}s${n ? ` (${n.toLocaleString("en")})` : ""} ▸`;
      showHidden.disabled = n === 0;
      showHidden.style.opacity = n === 0 ? "0.5" : "";
      showHidden.style.cursor = n === 0 ? "default" : "pointer";
    };
    markHidden();

    const textRow = el("div", "binnacle-column-menu-text", ROW_STYLE);
    // Its inline display would beat the `hidden` attribute.
    if (!support.text) textRow.style.display = "none";
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
    const error = el("div", "binnacle-column-menu-error", ERROR_STYLE);
    error.id = `binnacle-column-menu-error-${++errors}`;
    error.setAttribute("role", "status");
    error.hidden = true;

    const values = el("div", "binnacle-column-menu-values", "");
    values.hidden = !support.oneOf;
    const separator = el("div", "", SEPARATOR_STYLE);
    separator.hidden = !support.text;
    const search = el("input", "binnacle-column-menu-search", SEARCH_STYLE);
    search.type = "search";
    search.placeholder = "Search values";
    search.setAttribute("aria-label", `Search ${label} values`);
    const list = el("div", "binnacle-column-menu-list", LIST_STYLE);
    list.setAttribute("role", "group");
    list.setAttribute("aria-label", `${label} values`);
    const notice = el("div", "binnacle-column-menu-notice", NOTICE_STYLE);
    notice.setAttribute("aria-live", "polite");
    values.append(separator, search, list, notice);

    const foot = el("div", "binnacle-column-menu-buttons", FOOT_STYLE);
    const clearButton = action("Clear");
    clearButton.hidden = !filters;
    const spacer = el("span", "", "flex: 1;");
    const closeButton = action("Close");
    foot.append(clearButton, spacer, closeButton);
    element.append(title, actions, textRow, error, values, foot);

    const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
    /** Shows the column's Text condition in the text row. */
    const showText = () => {
      const kind = column.text?.kind;
      const q = kind?.type === "Text" ? kind.query : null;
      how.value = column.text?.not ? "not" : "contains";
      field.value = q?.text ?? "";
      caseToggle.setAttribute("aria-pressed", String(q?.caseSensitive ?? false));
      regexToggle.setAttribute("aria-pressed", String((q?.regex ?? false) && regexAllowed));
    };
    showText();

    const pause = createPause(
      () => widget.typingPause,
      () => {
        const query: TextMatch = {
          text: field.value,
          caseSensitive: pressed(caseToggle),
          regex: regexAllowed && pressed(regexToggle),
        };
        const to: Condition<Id> = { id: colId, kind: { type: "Text", query }, not: how.value === "not" };
        write({ text: query.text === "" ? null : to, oneOf: column.oneOf });
        showValues();
        paint();
      },
    );
    const sendText = pause.now;
    for (const b of [caseToggle, regexToggle]) {
      b.addEventListener("click", () => {
        b.setAttribute("aria-pressed", String(!pressed(b)));
        paint();
        sendText();
      });
    }
    how.addEventListener("change", sendText);
    field.addEventListener("input", () => {
      pause.typed();
      paint();
    });
    clearButton.addEventListener("click", () => {
      widget.clearConditions(colId, axis);
      column = columnConditions([], colId);
      showText();
      showValues();
      (support.text ? field : search).focus();
    });

    // The checklist: the latest answer for the search typed, and the request in flight.
    let answer: Distinct | null = null;
    let asking: { abort(): void } | null = null;
    const ask = () => {
      asking?.abort();
      const searched = search.value;
      asking = widget.distinct(
        colId,
        searched,
        LIMIT,
        (d) => {
          asking = null;
          answer = d;
          showValues();
        },
        axis,
      );
      showValues();
    };
    search.addEventListener("input", ask);

    const shown = (): Shown => ({
      values: answer ? [...(answer.blanks ? [null] : []), ...answer.values.map((v) => v.text)] : [],
      partial: answer?.partial ?? false,
      search: search.value,
    });

    /** Draws the checklist from the latest answer and the column's conditions. */
    const showValues = () => {
      if (!support.oneOf) return;
      const t = widget.theme;
      const now = shown();
      const scroll = list.scrollTop;
      list.textContent = "";
      if (!answer) {
        notice.textContent = asking ? "Loading values…" : "";
        return;
      }
      const head = item(
        now.search ? `(Select all matching "${now.search}")` : "(Select all)",
        headTicked(column, now),
        undefined,
        (on) => write(tickHead(column, colId, on, now)),
      );
      head.classList.add("binnacle-column-menu-all");
      head.style.borderBottom = `1px solid ${t.border}`;
      list.append(head);
      const counts = new Map<ListedValue, number | undefined>(answer.values.map((v) => [v.text, v.count]));
      if (answer.blanks) counts.set(null, answer.blanks === true ? undefined : answer.blanks);
      for (const value of now.values) {
        const text = value === null ? "(Blanks)" : value === "" ? '""' : value;
        const row = item(text, ticked(column, value), counts.get(value), (on) =>
          write(tick(column, colId, value, on, now)),
        );
        if (excluded(column, value)) {
          row.querySelector("input")!.disabled = true;
          row.style.opacity = "0.45";
          row.title = "Excluded by the text condition";
        }
        list.append(row);
      }
      if (now.values.length === 0) {
        const empty = el("div", "binnacle-column-menu-empty", "padding: 4px 8px;");
        empty.textContent = "No values";
        empty.style.color = t.loading;
        list.append(empty);
      }
      list.scrollTop = scroll;
      const capped = answer.partial ? `First ${LIMIT.toLocaleString("en")} values; type to search. ` : "";
      notice.textContent = capped + (answer.narrowed ? "Narrowed by the other filters" : "Showing all values");
      notice.style.color = asking ? t.loading : t.rowNumber.color;
    };

    /** A checklist row: a checkbox, the value and its count; `change` runs with the new tick, then the list redraws. */
    const item = (text: string, on: boolean, count: number | undefined, change: (on: boolean) => void) => {
      const t = widget.theme;
      const row = el("label", "binnacle-column-menu-value", ITEM_STYLE);
      const box = el("input", "", `margin: 0; accent-color: ${t.focus.color};`);
      box.type = "checkbox";
      box.checked = on;
      box.addEventListener("change", () => {
        const at = [...list.children].indexOf(row);
        change(box.checked);
        showText();
        showValues();
        // The list was drawn again: keep focus on the same row.
        if (box === document.activeElement || document.activeElement === document.body) {
          list.children[at]?.querySelector("input")?.focus();
        }
      });
      const value = el("span", "", ITEM_TEXT_STYLE);
      value.textContent = text;
      row.append(box, value);
      if (count !== undefined) {
        const n = el("span", "binnacle-column-menu-count", COUNT_STYLE);
        n.textContent = count.toLocaleString("en");
        n.style.color = t.rowNumber.color;
        row.append(n);
      }
      return row;
    };

    const close = () => {
      if (menu?.element !== element) return;
      closeFlyout(false);
      menu = null;
      asking?.abort();
      if (pause.waiting) sendText();
      unwatch();
      document.removeEventListener("pointerdown", outside, true);
      element.remove();
      if (returnFocus?.isConnected) returnFocus.focus();
    };
    const outside = (e: PointerEvent) => {
      if (!element.contains(e.target as Node) && !flyout?.element.contains(e.target as Node)) close();
    };
    closeButton.addEventListener("click", close);
    element.addEventListener("keydown", (e) => {
      if (e.key === "Escape" || (e.key === "Enter" && (e.target === field || e.target === search))) {
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
      // Rewriting the same text swaps the node under a press, and WebKit
      // then drops the click.
      const pinText = pinned() ? "Unpin" : "Pin left";
      if (pinButton.textContent !== pinText) pinButton.textContent = pinText;
      for (const c of [how, field, search, clearButton, closeButton]) {
        c.style.background = t.body.background;
        c.style.border = `1px solid ${t.border}`;
      }
      actionsSeparator.style.background = t.border;
      markHidden();
      if (flyout) {
        const f = flyout.element;
        f.style.font = t.font;
        f.style.color = t.body.color;
        f.style.background = t.body.background;
        f.style.border = `1px solid ${t.border}`;
        flyout.search.style.background = t.body.background;
        flyout.search.style.border = `1px solid ${t.border}`;
      }
      // The column's Text condition, once nothing typed waits, as the latest frame has it.
      const text = column.text;
      const rejected =
        !pause.waiting &&
        text !== null &&
        listed(axis).some(
          ({ condition, state }) => state === "notApplied" && JSON.stringify(condition) === JSON.stringify(text),
        );
      field.style.borderColor = rejected ? t.error : t.focus.color;
      field.style.color = rejected ? t.error : "inherit";
      field.toggleAttribute("aria-invalid", rejected);
      if (rejected) field.setAttribute("aria-describedby", error.id);
      else field.removeAttribute("aria-describedby");
      error.hidden = !rejected;
      error.textContent = rejected ? (latest?.viewError ?? "Not applied") : "";
      error.style.color = t.error;
      list.style.border = `1px solid ${t.border}`;
      separator.style.background = t.border;
      for (const b of [caseToggle, regexToggle]) {
        const on = pressed(b);
        b.style.color = on ? t.focus.color : t.loading;
        b.style.borderColor = on ? t.focus.color : "transparent";
      }
    };
    paint();
    const unwatch = widget.subscribe(() => {
      paint();
      listHidden();
    });
    if (support.oneOf) ask();

    document.body.append(element);
    const width = element.offsetWidth;
    // Under a column header; beside a row header.
    const [left, top] = axis === "rows" ? [anchor.left, anchor.bottom + 4] : [anchor.right + 4, anchor.top];
    element.style.left = `${Math.max(8, Math.min(left, innerWidth - width - 8))}px`;
    // Room for the checklist once its values land.
    let height = Math.max(element.offsetHeight, support.oneOf ? 380 : 0);
    // Short of room below its header, the list gives way before the menu covers the header.
    const cut = Math.min(height - (innerHeight - top - 8), LIST_HEIGHT - LIST_MIN_HEIGHT);
    if (support.oneOf && cut > 0) {
      list.style.maxHeight = `${LIST_HEIGHT - cut}px`;
      height -= cut;
    }
    element.style.top = `${Math.max(8, Math.min(top, innerHeight - height - 8))}px`;
    document.addEventListener("pointerdown", outside, true);
    menu = { element, close };
    if (at === "hidden" && hides) return openFlyout();
    const first = support.text ? field : support.oneOf ? search : pinnable ? pinButton : hideButton;
    first.focus();
    if (first === field) field.select();
  };

  let latest: FrameInfo<RowId, ColId> | null = null;
  const unsubscribe = widget.subscribe((f) => {
    latest = f;
  });
  const detach = widget.attachColumnMenu((colId, anchor, at) => open("rows", colId, anchor, at));
  const detachRow = widget.attachRowMenu((rowId, anchor, at) => open("cols", rowId, anchor, at));

  return {
    get element() {
      return menu?.element ?? null;
    },
    open: (colId, anchor, at) => widget.openColumnMenu(colId, anchor, at),
    openRow: (rowId, anchor, at) => widget.openRowMenu(rowId, anchor, at),
    close: () => menu?.close(),
    destroy() {
      menu?.close();
      detach();
      detachRow();
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

/** A button that reads as one item of a menu's list. */
function menuItem(text: string, className: string): HTMLButtonElement {
  const b = el("button", className, MENU_ITEM_STYLE);
  b.type = "button";
  b.textContent = text;
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
