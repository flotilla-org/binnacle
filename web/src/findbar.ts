// The find bar: a part the embedder places beside a GridWidget, usually
// above it, full width. Mod+F in the grid opens it (and `open` does). It holds
// the find field with its case and regex toggles, the order find steps in (by
// rows or by columns), the count ("3 of 120", "120 matches", "No matches"),
// a note when a step wraps, Previous and Next, "Show only matching rows"
// (which copies the query to the rows' quick filter) and a close button.
// Enter steps to the next match, Shift+Enter to the previous, Escape closes.
// Typing sets the Find Query once a pause has passed (`typingPause`); Enter,
// the toggles and the order at once. The count reads "finding…" until the
// source counts, and "No matches" in the error colour, with the source's
// message, when it fails. Closing clears the Find Query and its marks. Drawn in the widget's Theme;
// it never opens while the Table Source does not find.

import type { FindQuery } from "./source";
import type { FindInfo, FrameInfo, GridWidget } from "./widget";
import { createPause } from "./pause";

export interface FindBarOptions {
  /** What the empty field says; "Find in table" by default. */
  placeholder?: string;
}

export interface FindBar {
  readonly element: HTMLElement;
  /** Shows the bar and focuses its field, finding its text again if it has any. */
  open(): void;
  /** Hides the bar, closes find and gives DOM focus back to the grid. */
  close(): void;
  readonly opened: boolean;
  destroy(): void;
}

const BAR_STYLE = "display: none; align-items: center; gap: 6px; padding: 5px 10px; box-sizing: border-box;";
const LABEL_STYLE = "font-weight: 600; white-space: nowrap;";
const FIELD_STYLE = "display: flex; align-items: center; gap: 1px; padding: 0 3px 0 0; border-radius: 6px;";
const INPUT_STYLE =
  "font: inherit; color: inherit; width: 220px; padding: 4px 8px; border: 0; outline: none; background: transparent;";
const TOGGLE_STYLE =
  "min-width: 24px; height: 24px; padding: 0 5px; border: 1px solid transparent; border-radius: 4px; " +
  "background: transparent; color: inherit; cursor: pointer; font: 12px ui-monospace, Menlo, monospace;";
const SELECT_STYLE = "font: inherit; color: inherit; border-radius: 6px; padding: 3px 4px;";
const COUNT_STYLE = "font-variant-numeric: tabular-nums; white-space: nowrap; min-width: 6.5em; text-align: center;";
const NOTE_STYLE = "white-space: nowrap; font-size: 0.9em;";
const BUTTON_STYLE =
  "font: inherit; color: inherit; border-radius: 6px; padding: 3px 8px; cursor: pointer; white-space: nowrap;";
const ONLY_STYLE = `${BUTTON_STYLE} margin-left: auto;`;
const CLOSE_STYLE =
  "width: 24px; height: 24px; padding: 0; border: 0; border-radius: 4px; background: transparent; " +
  "color: inherit; cursor: pointer; font: inherit;";

export function createFindBar<RowId, ColId, V>(
  widget: GridWidget<RowId, ColId, V>,
  options: FindBarOptions = {},
): FindBar {
  const element = el("div", "binnacle-find-bar", BAR_STYLE);
  element.setAttribute("role", "search");
  element.setAttribute("aria-label", "Find");
  const label = el("span", "binnacle-find-label", LABEL_STYLE);
  label.textContent = "Find";
  const field = el("div", "binnacle-find-field", FIELD_STYLE);
  const input = el("input", "", INPUT_STYLE);
  input.type = "text";
  input.placeholder = options.placeholder ?? "Find in table";
  input.setAttribute("aria-label", "Find");
  const caseToggle = toggle("Aa", "Match case");
  const regexToggle = toggle(".*", "Use regular expression");
  field.append(input, caseToggle, regexToggle);
  const order = el("select", "binnacle-find-order", SELECT_STYLE);
  order.setAttribute("aria-label", "Find order");
  order.append(new Option("By rows", "rows"), new Option("By columns", "cols"));
  const count = el("span", "binnacle-find-count", COUNT_STYLE);
  count.setAttribute("aria-live", "polite");
  const note = el("span", "binnacle-find-wrapped", NOTE_STYLE);
  const previous = button("↑ Previous", "Previous match (Shift+Enter)");
  const next = button("↓ Next", "Next match (Enter)");
  const only = el("button", "binnacle-find-only", ONLY_STYLE);
  only.type = "button";
  only.textContent = "Show only matching rows";
  only.title = "Filter the rows by the find text";
  const closeButton = el("button", "binnacle-find-close", CLOSE_STYLE);
  closeButton.type = "button";
  closeButton.textContent = "×";
  closeButton.setAttribute("aria-label", "Close find (Escape)");
  closeButton.title = "Close find (Escape)";
  element.append(label, field, order, count, note, previous, next, only, closeButton);

  let opened = false;
  let info: FrameInfo<RowId, ColId> | null = null;
  /** Which way the last step went, for the wrap note. */
  let stepped: "next" | "previous" = "next";
  const pressed = (b: HTMLButtonElement) => b.getAttribute("aria-pressed") === "true";
  const regexAllowed = () => info?.find.support?.regex ?? false;

  const query = (): FindQuery<ColId> | null =>
    input.value === ""
      ? null
      : {
          query: {
            text: input.value,
            caseSensitive: pressed(caseToggle),
            regex: regexAllowed() && pressed(regexToggle),
            searches: "shown",
          },
          by: order.value === "cols" ? "cols" : "rows",
        };
  const pause = createPause(
    () => widget.typingPause,
    () => {
      widget.setFind(opened ? query() : null);
      if (info) frame(info);
    },
  );
  /** Sends the bar's Find Query now, dropping any wait. */
  const send = pause.now;
  const step = (direction: "next" | "previous") => {
    send();
    stepped = direction;
    if (direction === "next") widget.findNext();
    else widget.findPrevious();
  };

  const open = () => {
    if (info && !info.find.support) return;
    opened = true;
    element.style.display = "flex";
    send();
    paint();
    input.focus();
    input.select();
  };
  const close = () => {
    if (!opened) return;
    opened = false;
    element.style.display = "none";
    widget.setFind(null);
    widget.element.querySelector<HTMLElement>(".binnacle-input")?.focus({ preventScroll: true });
  };

  for (const b of [caseToggle, regexToggle]) {
    b.addEventListener("click", () => {
      b.setAttribute("aria-pressed", String(!pressed(b)));
      paint();
      send();
    });
  }
  input.addEventListener("input", () => {
    pause.typed();
    if (info) frame(info);
  });
  input.addEventListener("focus", () => paint());
  input.addEventListener("blur", () => paint());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      step(e.shiftKey ? "previous" : "next");
    } else if (e.key === "Escape") {
      e.preventDefault();
      close();
    }
  });
  order.addEventListener("change", send);
  previous.addEventListener("click", () => step("previous"));
  next.addEventListener("click", () => step("next"));
  only.addEventListener("click", () => {
    const q = query();
    if (q) widget.setQuickFilter(q.query);
  });
  closeButton.addEventListener("click", close);
  element.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && e.target !== input) {
      e.preventDefault();
      close();
    }
  });

  /** Colours from the widget's Theme, which may change at any frame. */
  const paint = () => {
    const t = widget.theme;
    element.style.font = t.font;
    element.style.color = t.header.color;
    element.style.background = `color-mix(in srgb, ${t.findMatch.background} 35%, ${t.header.background})`;
    element.style.borderBottom = `1px solid ${t.border}`;
    label.style.color = t.rowNumber.color;
    field.style.background = t.body.background;
    field.style.color = t.body.color;
    const lit = input.value !== "" || document.activeElement === input;
    field.style.border = `1px solid ${lit ? t.focus.color : t.border}`;
    for (const b of [caseToggle, regexToggle]) {
      const on = pressed(b);
      b.style.color = on ? t.focus.color : t.loading;
      b.style.borderColor = on ? t.focus.color : "transparent";
    }
    for (const b of [order, previous, next, only]) {
      b.style.background = t.body.background;
      b.style.border = `1px solid ${t.border}`;
    }
    const failed = !pause.waiting && info?.find.query != null && info.find.error !== null;
    const none = failed || (info?.find.count?.total === 0 && !info.find.count.partial);
    count.style.color = none ? t.error : t.rowNumber.color;
    note.style.color = failed ? t.error : t.findCurrent.color;
  };

  const frame = (f: FrameInfo<RowId, ColId>) => {
    info = f;
    regexToggle.hidden = !regexAllowed();
    // Copying the query needs a quick filter that takes it.
    only.hidden = f.quickFilter === null;
    const q = query();
    only.disabled = !q || (q.query.regex && !f.quickFilter?.regex);
    const finding = pause.waiting || (f.find.query !== null && f.find.count === null && f.find.counting);
    const failed = !finding && f.find.query !== null && f.find.error !== null;
    count.textContent = finding ? "finding…" : failed ? "No matches" : f.find.query ? countText(f.find) : "";
    count.toggleAttribute("aria-busy", finding);
    note.textContent = failed
      ? f.find.error!
      : f.find.query && f.find.step?.wrapped
        ? stepped === "next"
          ? "Wrapped to the start"
          : "Wrapped to the end"
        : "";
    if (opened && !f.find.support) close();
    paint();
  };
  const unsubscribe = widget.subscribe(frame);
  const detach = widget.attachFindBar(open);

  return {
    element,
    open,
    close,
    get opened() {
      return opened;
    },
    destroy() {
      unsubscribe();
      detach();
      pause.cancel();
      element.remove();
    },
  };
}

/** "3 of 120" once a step has landed, else "120 matches"; "No matches"; a partial count with "+". */
function countText(find: FindInfo): string {
  const c = find.count;
  if (c?.total === 0 && !c.partial) return "No matches";
  const total = c && `${c.total.toLocaleString("en")}${c.partial ? "+" : ""}`;
  const index = find.step?.index;
  if (index != null && find.current) return total ? `${(index + 1).toLocaleString("en")} of ${total}` : `Match ${index + 1}`;
  if (!c) return "";
  return `${total} ${c.total === 1 && !c.partial ? "match" : "matches"}`;
}

function toggle(text: string, label: string): HTMLButtonElement {
  const b = el("button", "binnacle-toggle", TOGGLE_STYLE);
  b.type = "button";
  b.textContent = text;
  b.title = label;
  b.setAttribute("aria-label", label);
  b.setAttribute("aria-pressed", "false");
  return b;
}

function button(text: string, title: string): HTMLButtonElement {
  const b = el("button", "binnacle-find-step", BUTTON_STYLE);
  b.type = "button";
  b.textContent = text;
  b.title = title;
  return b;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, style: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  e.style.cssText = style;
  return e;
}
