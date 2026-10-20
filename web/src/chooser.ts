// The chooser: a part that mounts on a GridWidget and lists an axis's own
// items as the find and filter checklist lists values, with a search and a
// tick for each item shown. A tick shows the item and an untick hides it, at
// once; "(Show all)" (or "(Show all matching …)" while searching) shows or
// hides every item listed. It lists the items shown in the Table Source's
// order (the first 1,000 of them), then the hidden ones, and keeps that
// order while open so nothing jumps as it is ticked. A hide past the axis's
// cap hides nothing and says to filter instead. Mod+Alt+O opens the columns'
// (Mod+Alt+Shift+O the rows'), as do the column and row menus' "Choose…"
// and `open`; Escape, a press outside, or Close shuts it. Drawn in the
// widget's Theme.

import type { AxisName } from "./codes";
import { capHint } from "./choose";
import {
  action,
  el,
  FOOT_STYLE,
  ITEM_STYLE,
  ITEM_TEXT_STYLE,
  LIST_STYLE,
  MENU_STYLE,
  NOTICE_STYLE,
  SEARCH_STYLE,
  TITLE_STYLE,
} from "./columnmenu";
import type { FrameInfo, GridWidget } from "./widget";

export interface Chooser {
  /** The popover while it is open, else `null`. */
  readonly element: HTMLElement | null;
  /** Opens the chooser of `axis` (the columns by default) beside `anchor`, else at the grid's top left; `false` if the axis does not hide. */
  open(axis?: AxisName, anchor?: DOMRect): boolean;
  close(): void;
  /** Closes it and unmounts it from the widget. */
  destroy(): void;
}

/** How many shown items it lists; it says so when there are more. */
const LIMIT = 1000;

const TAG_STYLE = "font-size: 11px;";

export function createChooser<RowId, ColId, V>(widget: GridWidget<RowId, ColId, V>): Chooser {
  type Id = RowId | ColId;
  let chooser: { element: HTMLElement; close(): void } | null = null;
  let latest: FrameInfo<RowId, ColId> | null = null;

  const show = (axis: AxisName, anchor: DOMRect | undefined) => {
    chooser?.close();
    const returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const nouns = axis === "cols" ? "columns" : "rows";
    const label = (id: Id) => (axis === "cols" ? widget.columnLabel(id as ColId) : widget.rowLabel(id as RowId));
    const element = el("div", "binnacle-chooser", MENU_STYLE.replace("width: 300px", "width: 280px"));
    element.setAttribute("role", "dialog");
    element.setAttribute("aria-label", `Choose ${nouns}`);
    const title = el("div", "binnacle-chooser-title", TITLE_STYLE);
    const name = el("b", "", "");
    name.textContent = nouns;
    title.append("Choose ", name);
    const search = el("input", "binnacle-chooser-search", SEARCH_STYLE);
    search.type = "search";
    search.placeholder = `Search ${nouns}`;
    search.setAttribute("aria-label", `Search ${nouns}`);
    const list = el("div", "binnacle-chooser-list", LIST_STYLE);
    list.setAttribute("role", "group");
    list.setAttribute("aria-label", nouns);
    const notice = el("div", "binnacle-chooser-notice", NOTICE_STYLE);
    notice.setAttribute("aria-live", "polite");
    const hint = el("div", "binnacle-chooser-hint", NOTICE_STYLE);
    hint.setAttribute("role", "status");
    hint.hidden = true;
    const foot = el("div", "", FOOT_STYLE);
    const spacer = el("span", "", "flex: 1;");
    const closeButton = action("Close");
    foot.append(spacer, closeButton);
    element.append(title, search, list, notice, hint, foot);

    // The items in the order the chooser opened with, `null` until listed.
    let items: { id: Id; label: string }[] | null = null;
    let partial = false;

    /** The listed items the search keeps, ignoring case. */
    const matching = () => {
      const text = search.value.toLowerCase();
      return (items ?? []).filter((i) => i.label.toLowerCase().includes(text));
    };
    const hiddenNow = () => new Set<Id>(widget.hidden(axis) as Id[]);

    const refuse = (hid: boolean) => {
      hint.hidden = hid;
      hint.textContent = hid ? "" : capHint(axis, widget.hideCap(axis));
    };
    const hide = (ids: Id[]) => refuse(widget.hide(ids, axis));
    const showIds = (ids: Id[]) => {
      widget.show(ids, axis);
      refuse(true);
    };

    /** A list row: a checkbox, the item and a tag; `change` runs with the new tick. */
    const row = (text: string, on: boolean | "mixed", tag: string, change: (on: boolean) => void) => {
      const t = widget.theme;
      const line = el("label", "binnacle-chooser-item", ITEM_STYLE);
      const box = el("input", "", `margin: 0; accent-color: ${t.focus.color};`);
      box.type = "checkbox";
      box.checked = on === true;
      box.indeterminate = on === "mixed";
      box.addEventListener("change", () => {
        const at = [...list.children].indexOf(line);
        change(box.checked);
        fill();
        // The list was drawn again: keep focus on the same row.
        list.children[at]?.querySelector("input")?.focus();
      });
      const value = el("span", "", ITEM_TEXT_STYLE);
      value.textContent = text;
      line.append(box, value);
      if (tag) {
        const tagged = el("span", "binnacle-chooser-tag", TAG_STYLE);
        tagged.textContent = tag;
        tagged.style.color = t.rowNumber.color;
        line.append(tagged);
      }
      return line;
    };

    /** Draws the list from the items and the hidden ones as of the latest frame. */
    const fill = () => {
      const t = widget.theme;
      const hidden = hiddenNow();
      // Items hidden elsewhere since it opened join at the end.
      if (items) {
        for (const id of hidden) if (!items.some((i) => i.id === id)) items.push({ id, label: label(id) });
      }
      const scroll = list.scrollTop;
      list.textContent = "";
      if (!items) {
        notice.textContent = `Listing ${nouns}…`;
        return;
      }
      const kept = matching();
      const shown = kept.filter((i) => !hidden.has(i.id)).length;
      const head = row(
        search.value ? `(Show all matching "${search.value}")` : "(Show all)",
        shown === kept.length && kept.length > 0 ? true : shown === 0 ? false : "mixed",
        "",
        (on) => (on ? showIds(kept.map((i) => i.id)) : hide(kept.filter((i) => !hidden.has(i.id)).map((i) => i.id))),
      );
      head.classList.add("binnacle-chooser-all");
      head.style.borderBottom = `1px solid ${t.border}`;
      list.append(head);
      const pinned = new Set<Id>(widget.pinned(axis as "cols") as Id[]);
      for (const i of kept) {
        const tag = pinned.has(i.id) ? "pinned" : "";
        list.append(row(i.label, !hidden.has(i.id), tag, (on) => (on ? showIds([i.id]) : hide([i.id]))));
      }
      if (kept.length === 0) {
        const empty = el("div", "binnacle-chooser-empty", "padding: 4px 8px;");
        empty.textContent = `No ${nouns} match`;
        empty.style.color = t.loading;
        list.append(empty);
      }
      list.scrollTop = scroll;
      const count = latest?.[axis];
      const of = count && count.total !== null ? ` of ${count.total.toLocaleString("en")}` : "";
      const capped = partial ? `First ${LIMIT.toLocaleString("en")} shown; search to narrow. ` : "";
      notice.textContent = `${capped}${(count?.count ?? 0).toLocaleString("en")}${of} ${nouns} shown`;
      notice.style.color = t.rowNumber.color;
    };
    search.addEventListener("input", fill);

    void widget.items(axis, LIMIT).then((listed) => {
      if (chooser?.element !== element) return;
      items = listed.ids.map((id) => ({ id: id as Id, label: label(id as Id) }));
      partial = listed.partial;
      fill();
    });

    const paint = () => {
      const t = widget.theme;
      element.style.font = t.font;
      element.style.color = t.body.color;
      element.style.background = t.body.background;
      element.style.border = `1px solid ${t.border}`;
      title.style.color = t.rowNumber.color;
      name.style.color = t.body.color;
      for (const c of [search, closeButton]) {
        c.style.background = t.body.background;
        c.style.border = `1px solid ${t.border}`;
      }
      list.style.border = `1px solid ${t.border}`;
      hint.style.color = t.error;
    };

    const close = () => {
      if (chooser?.element !== element) return;
      chooser = null;
      unwatch();
      document.removeEventListener("pointerdown", outside, true);
      element.remove();
      if (returnFocus?.isConnected) returnFocus.focus();
    };
    const outside = (e: PointerEvent) => {
      if (!element.contains(e.target as Node)) close();
    };
    closeButton.addEventListener("click", close);
    element.addEventListener("keydown", (e) => {
      if (e.key === "Escape" || (e.key === "Enter" && e.target === search)) {
        e.preventDefault();
        e.stopPropagation();
        close();
      } else if (e.key === "ArrowDown" && e.target === search) {
        e.preventDefault();
        list.querySelector("input")?.focus();
      }
    });
    paint();
    // Drawn again only when what it shows changed: a list redrawn under a
    // press makes WebKit drop the click.
    let drawn = "";
    const unwatch = widget.subscribe((f) => {
      latest = f;
      paint();
      const now = [
        widget.hidden(axis).map(String).join(","),
        f[axis].count,
        f[axis].total,
        widget.pinned(axis as "cols").join(","),
      ].join("|");
      if (now === drawn) return;
      drawn = now;
      const at = [...list.children].findIndex((c) => c.contains(document.activeElement));
      fill();
      if (at >= 0) list.children[at]?.querySelector("input")?.focus();
    });
    fill();

    document.body.append(element);
    const box = anchor ?? widget.element.getBoundingClientRect();
    const width = element.offsetWidth;
    const [left, top] = anchor ? [anchor.right + 4, anchor.top] : [box.left + 8, box.top + 8];
    element.style.left = `${Math.max(8, Math.min(left, innerWidth - width - 8))}px`;
    // Room for the list once the items land.
    const height = Math.max(element.offsetHeight, 360);
    element.style.top = `${Math.max(8, Math.min(top, innerHeight - height - 8))}px`;
    document.addEventListener("pointerdown", outside, true);
    chooser = { element, close };
    search.focus();
  };

  const detach = widget.attachChooser((axis, anchor) => show(axis, anchor));
  return {
    get element() {
      return chooser?.element ?? null;
    },
    open: (axis = "cols", anchor) => widget.openChooser(axis, anchor),
    close: () => chooser?.close(),
    destroy() {
      chooser?.close();
      detach();
    },
  };
}
