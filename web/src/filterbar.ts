// The filter bar: a part the embedder places beside a GridWidget, usually
// above it. It holds the rows' quick filter field (or the columns', with
// `axis`) with its case and regex toggles and a clear button, a chip for
// each column condition (× removes it), then after "Columns where" one for
// each condition on the columns, read in a row, "+ Filter" to add one
// through a column menu, "Re-apply" while items in view were left unmatched
// by the user's edits, and the row count, with the column count beside it
// while the columns are filtered ("5 of 12 fields", with `nouns`). Each
// change to the field sets the widget's quick filter, typing once a pause
// has passed (`typingPause`) and Enter, the toggles and × at once; the widget
// announces the count to assistive technology once the filter is applied.
// While a filter is pending the count reads "filtering…" and its chips fade;
// a quick filter or condition the Table Source did not apply turns red with
// its message, the text kept. Drawn in the widget's Theme; hidden while the
// axis takes no quick filter, neither axis has conditions and no column menu
// is given.

import { rowCount } from "./aria";
import { AXES, type AxisName } from "./codes";
import type { ColumnMenu } from "./columnmenu";
import type { Condition } from "./source";
import type { ConditionState } from "./snapshot";
import type { FrameInfo, GridWidget } from "./widget";
import { createPause } from "./pause";

export interface FilterBarOptions<ColId = unknown, RowId = unknown> {
  /** Which axis the field's quick filter filters; the rows by default. */
  axis?: AxisName;
  /** What the empty field says; "Filter rows…" (or "Filter columns…") by default. */
  placeholder?: string;
  /** The column menu (`createColumnMenu`) that "+ Filter" and a press on a chip open; without one, neither does. */
  columnMenu?: ColumnMenu<ColId, RowId>;
  /** What the counts call each axis's items, singular and plural; "row(s)" and "column(s)" by default. */
  nouns?: { rows?: [string, string]; cols?: [string, string] };
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
const ERROR_STYLE =
  "font-size: 0.9em; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 28em;";

/** Numbers each bar's error message, for `aria-describedby`. */
let errors = 0;

export function createFilterBar<RowId, ColId, V>(
  widget: GridWidget<RowId, ColId, V>,
  options: FilterBarOptions<ColId, RowId> = {},
): FilterBar {
  const axis = options.axis ?? "rows";
  const nouns = { rows: options.nouns?.rows ?? ["row", "rows"], cols: options.nouns?.cols ?? ["column", "columns"] };
  const element = el("div", "binnacle-filter-bar", BAR_STYLE);
  element.setAttribute("role", "search");
  const field = el("div", "binnacle-quick-filter", FIELD_STYLE);
  const input = el("input", "", INPUT_STYLE);
  // Text, not search: a search input's own clear button would duplicate the bar's.
  input.type = "text";
  input.placeholder = options.placeholder ?? `Filter ${nouns[axis][1]}…`;
  input.setAttribute("aria-label", `Filter ${nouns[axis][1]}`);
  const caseToggle = toggle("Aa", "Match case");
  const regexToggle = toggle(".*", "Use regular expression");
  const clearButton = el("button", "binnacle-clear", BUTTON_STYLE);
  clearButton.type = "button";
  clearButton.textContent = "×";
  clearButton.setAttribute("aria-label", "Clear filter");
  clearButton.hidden = true;
  field.append(input, caseToggle, regexToggle, clearButton);
  const error = el("span", "binnacle-filter-error", ERROR_STYLE);
  error.id = `binnacle-filter-error-${++errors}`;
  error.setAttribute("role", "status");
  error.hidden = true;
  const chips = el("div", "binnacle-chips", CHIPS_STYLE);
  chips.setAttribute("role", "list");
  chips.setAttribute("aria-label", "Column filters");
  const across = el("div", "binnacle-column-chips", CHIPS_STYLE + " align-items: center;");
  across.setAttribute("role", "list");
  across.setAttribute("aria-label", `Filters on the ${nouns.cols[1]}`);
  across.style.display = "none";
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
  const colCount = el("span", "binnacle-column-count", "font-variant-numeric: tabular-nums; white-space: nowrap;");
  colCount.hidden = true;
  // Why the last hide hid nothing: past the cap, a filter scales.
  const capped = el("span", "binnacle-hide-capped", ERROR_STYLE);
  capped.setAttribute("role", "status");
  capped.hidden = true;
  element.append(field, error, chips, across, add, reapply, count, colCount, capped);

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

  /** A condition's chip text: "port: not Bergen" on the rows, "3: Bergen" on the columns. */
  const chipOf = (on: AxisName, condition: Condition<RowId | ColId>) =>
    chipText(on === "rows" ? widget.columnLabel(condition.id as ColId) : widget.rowLabel(condition.id as RowId), condition);

  /**
   * One chip of a condition on `on`, faded while pending, red with
   * `message` when not applied; the label opens the menu of the item it
   * reads, × removes it.
   */
  const chip = (on: AxisName, condition: Condition<RowId | ColId>, state: ConditionState, message: string) => {
    const t = widget.theme;
    const item = el("div", "binnacle-chip", CHIP_STYLE);
    item.setAttribute("role", "listitem");
    const text = chipOf(on, condition);
    const label = el("button", "binnacle-chip-label", CHIP_LABEL_STYLE);
    label.type = "button";
    label.textContent = text;
    label.disabled = !menu || state === "removing";
    label.style.cursor = label.disabled ? "default" : "pointer";
    label.addEventListener("click", () => {
      const anchor = item.getBoundingClientRect();
      if (on === "rows") menu?.open(condition.id as ColId, anchor);
      else menu?.openRow(condition.id as RowId, anchor);
    });
    const remove = el("button", "binnacle-chip-remove", CHIP_REMOVE_STYLE);
    remove.type = "button";
    remove.textContent = "×";
    remove.setAttribute("aria-label", `Remove filter ${text}`);
    remove.hidden = state === "removing";
    remove.style.color = t.rowNumber.color;
    remove.addEventListener("click", () => widget.removeCondition(condition, on));
    item.append(label, remove);
    item.style.border = `1px solid ${t.sortMarker}`;
    item.style.background = t.picks.background;
    item.style.color = t.picks.color;
    if (state === "adding" || state === "removing") {
      item.style.opacity = "0.55";
      item.setAttribute("aria-busy", "true");
    }
    if (state === "removing") label.style.textDecoration = "line-through";
    if (state === "notApplied") {
      item.style.border = `1px solid ${t.error}`;
      item.style.color = t.error;
      item.style.background = t.body.background;
      item.title = `Not applied: ${message}`;
      label.setAttribute("aria-invalid", "true");
      label.setAttribute("aria-description", `Not applied: ${message}`);
    }
    return item;
  };
  let shownChips = "";
  const acrossLabel = el("span", "binnacle-column-chips-label", "white-space: nowrap;");
  acrossLabel.textContent = `${nouns.cols[1][0]!.toUpperCase()}${nouns.cols[1].slice(1)} where`;
  /** A frame's quick filter and conditions on `on` (the field's axis by default). */
  const quickOf = (f: FrameInfo<RowId, ColId> | null, on: AxisName = axis) =>
    f && (on === "rows" ? { quickFilter: f.quickFilter, quick: f.quick, quickState: f.quickState, conditions: f.conditions } : f.columnsFilter);
  let shownQuick = "null";

  let regexAllowed = false;
  /** The last frame, for repainting the field's state. */
  let info: FrameInfo<RowId, ColId> | null = null;
  const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
  const pause = createPause(
    () => widget.typingPause,
    () => {
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
        axis,
      );
      paint();
    },
  );
  /** Sends the field's quick filter now, dropping any wait. */
  const send = pause.now;
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
  input.addEventListener("input", () => {
    clearButton.hidden = input.value === "";
    pause.typed();
    paint();
  });
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
    const rejected = !pause.waiting && quickOf(info)?.quickState === "notApplied";
    field.style.border = `1px solid ${rejected ? t.error : lit ? t.focus.color : t.border}`;
    input.style.color = rejected ? t.error : "inherit";
    input.toggleAttribute("aria-invalid", rejected);
    if (rejected) input.setAttribute("aria-describedby", error.id);
    else input.removeAttribute("aria-describedby");
    error.hidden = !rejected;
    error.textContent = rejected ? (info?.viewError ?? "Not applied") : "";
    error.title = error.textContent;
    error.style.color = t.error;
    capped.style.color = t.error;
    count.style.color = t.rowNumber.color;
    colCount.style.color = t.rowNumber.color;
    acrossLabel.style.color = t.rowNumber.color;
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
    info = f;
    const q = quickOf(f)!;
    const conditions = { rows: f.conditions, cols: f.columnsFilter.conditions };
    // Inline styles would win over the `hidden` attribute.
    const bare = q.quickFilter === null && !conditions.rows.length && !conditions.cols.length && !menu;
    element.style.display = bare ? "none" : "flex";
    field.style.display = q.quickFilter === null ? "none" : "flex";
    regexAllowed = q.quickFilter?.regex ?? false;
    // Follows a quick filter set from elsewhere (the find bar's "Show only matching rows").
    const quick = JSON.stringify(q.quick);
    const changed = quick !== shownQuick;
    shownQuick = quick;
    if (changed && document.activeElement !== input && (q.quick?.text ?? "") !== input.value) {
      input.value = q.quick?.text ?? "";
      caseToggle.setAttribute("aria-pressed", String(q.quick?.caseSensitive ?? false));
      regexToggle.setAttribute("aria-pressed", String(q.quick?.regex ?? false));
      clearButton.hidden = input.value === "";
    }
    // Rebuilt only when they change, so a press on a chip is not lost to a repaint.
    const message = f.viewError ?? "the source did not apply it";
    const key = [
      widget.theme.name,
      message,
      ...AXES.flatMap((on) =>
        conditions[on].map(({ condition, state }) => `${on} ${state} ${chipOf(on, condition as Condition<RowId | ColId>)}`),
      ),
    ].join("\n");
    if (key !== shownChips) {
      shownChips = key;
      const chipsOf = (on: AxisName) =>
        conditions[on].map(({ condition, state }) => chip(on, condition as Condition<RowId | ColId>, state, message));
      chips.replaceChildren(...chipsOf("rows"));
      across.replaceChildren(...(conditions.cols.length ? [acrossLabel, ...chipsOf("cols")] : []));
      // Inline styles would win over the `hidden` attribute, and an empty list would still take a gap.
      across.style.display = conditions.cols.length ? "flex" : "none";
    }
    regexToggle.hidden = !regexAllowed;
    const edited = AXES.flatMap((on) => {
      const n = f[on].unmatched;
      return n === 0 ? [] : [`${n.toLocaleString("en")} edited ${nouns[on][n === 1 ? 0 : 1]}`];
    });
    reapply.hidden = edited.length === 0;
    const total = f.rows.unmatched + f.cols.unmatched;
    const what = `${edited.join(" and ")} no longer ${total === 1 ? "matches" : "match"}`;
    reapply.title = `${what} the filter: apply it again`;
    reapply.setAttribute("aria-label", `Re-apply the filter: ${what}`);
    const filtered = (on: AxisName) => f[on].total !== null && f[on].total !== f[on].count;
    const changing = (s: ConditionState | null) => s === "adding" || s === "removing";
    const filtering =
      f.viewPending &&
      [quickOf(f, "rows")!, quickOf(f, "cols")!].some(
        (a) => changing(a.quickState) || a.conditions.some((c) => changing(c.state)),
      );
    count.toggleAttribute("aria-busy", filtering);
    const [one, many] = nouns.rows;
    count.textContent = filtering
      ? "filtering…"
      : filtered("rows")
        ? `${f.rows.count.toLocaleString("en")} of ${f.rows.total!.toLocaleString("en")}`
        : options.nouns?.rows
          ? `${f.rows.count.toLocaleString("en")} ${f.rows.count === 1 ? one : many}`
          : rowCount(f.rows.count, null);
    colCount.hidden = filtering || !filtered("cols");
    colCount.textContent = filtered("cols")
      ? `${f.cols.count.toLocaleString("en")} of ${f.cols.total!.toLocaleString("en")} ${nouns.cols[f.cols.total === 1 ? 0 : 1]}`
      : "";
    capped.hidden = !f.hideCapped;
    capped.textContent = f.hideCapped?.hint ?? "";
    paint();
  };
  const unsubscribe = widget.subscribe(frame);

  return {
    element,
    clear,
    focus: () => input.focus(),
    destroy() {
      unsubscribe();
      pause.cancel();
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
