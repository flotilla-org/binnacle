// The ARIA layer: a visually hidden `role="grid"` table of the visible
// window, kept beside the canvas for assistive technology. Elements are
// reused by Key, so a cell keeps its `id` (and a screen reader its place)
// while the table scrolls; only what changed is written.

import { AXES, type AxisName } from "./codes";
import type { Content } from "./content";
import { cellText, plainText, type RendererChoice } from "./renderer";
import { cellState, type AxisSnapshot, type Snapshot, type StripItem } from "./snapshot";

export interface AriaOptions {
  /** The grid's accessible name. */
  label?: string;
  /** The element holding DOM focus, which names the focused cell as its active descendant. */
  focus: HTMLElement;
  /** Whether cells can be edited; the grid is `aria-readonly` otherwise. */
  editable: boolean;
  /** The Cell Renderers, whose `text` is a cell's accessible text. */
  renderer(): RendererChoice<unknown, unknown, unknown>;
}

export interface Aria {
  /** The hidden table; the widget places it beside the canvas. */
  readonly element: HTMLElement;
  /** Follows a frame. `painted` says whether anything may have changed; `content` is asked only if the table updates. */
  frame(s: Snapshot, painted: boolean, content: () => Content): void;
  /** The Grid asked where `key` went on `axis` (a Locate Effect). */
  locate(axis: AxisName, key: bigint): void;
  destroy(): void;
}

const HIDDEN_STYLE = [
  "position: absolute",
  "left: 0",
  "top: 0",
  "width: 1px",
  "height: 1px",
  "overflow: hidden",
  "clip-path: inset(50%)",
  "white-space: nowrap",
  "user-select: text",
  "-webkit-user-select: text",
].join("; ");

/** While only the visible Keys change (a scroll), the table waits for this long a pause, ms... */
const SETTLE = 100;
/** ...but no longer than this since the scroll began. */
const MOST = 500;

let widgets = 0;

interface Cell {
  el: HTMLTableCellElement;
  value: unknown;
  renderer: unknown;
  failed: boolean;
}

interface Row {
  el: HTMLTableRowElement;
  header: HTMLTableCellElement;
  cells: Map<string, Cell>;
}

/** The visible items of an axis with their places in the Strip. */
function visible(axis: AxisSnapshot): { item: StripItem; index: number }[] {
  const out: { item: StripItem; index: number }[] = [];
  axis.strip.forEach((item, index) => {
    if (item.offset + item.size > axis.header && item.offset < axis.extent) out.push({ item, index });
  });
  return out;
}

/** A visible item's name among its axis's elements: its Key, or its Position while the Key is unknown. */
const name = (item: StripItem) => (item.key === null ? `p${item.position}` : String(item.key));

function set(el: Element, attribute: string, value: string | null): void {
  if (value === null) {
    if (el.hasAttribute(attribute)) el.removeAttribute(attribute);
  } else if (el.getAttribute(attribute) !== value) {
    el.setAttribute(attribute, value);
  }
}

function text(el: Element, s: string): void {
  if (el.textContent !== s) el.textContent = s;
}

/** Makes `parent`'s children `wanted`, in order, moving only those out of place. */
function arrange(parent: Element, wanted: Element[]): void {
  wanted.forEach((el, i) => {
    if (parent.children[i] !== el) parent.insertBefore(el, parent.children[i] ?? null);
  });
  while (parent.children.length > wanted.length) parent.lastElementChild!.remove();
}

export function createAria(options: AriaOptions): Aria {
  const prefix = `binnacle${++widgets}`;
  // A table grows to fit its cells whatever its width, so a div hides it.
  const element = document.createElement("div");
  element.className = "binnacle-aria";
  element.style.cssText = HIDDEN_STYLE;
  const table = document.createElement("table");
  element.append(table);
  table.setAttribute("role", "grid");
  if (options.label) table.setAttribute("aria-label", options.label);
  set(options.focus, "aria-label", options.label ?? null);
  table.setAttribute("aria-multiselectable", "true");
  set(table, "aria-readonly", options.editable ? null : "true");
  const head = table.createTHead();
  head.setAttribute("role", "rowgroup");
  const headRow = head.insertRow();
  headRow.setAttribute("role", "row");
  headRow.setAttribute("aria-rowindex", "1");
  // The row headers' column, empty, so that tables laid out by the DOM (WebKit's) line each header up with its cells.
  const corner = document.createElement("th");
  corner.setAttribute("role", "columnheader");
  corner.setAttribute("aria-colindex", "1");
  const body = table.createTBody();
  body.setAttribute("role", "rowgroup");
  const status = document.createElement("div");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  status.setAttribute("aria-atomic", "true");
  element.append(status);

  let headers = new Map<string, HTMLTableCellElement>();
  let rows = new Map<string, Row>();
  /** The applied Sort Spec last seen, `null` before the first frame. */
  let sorted: string | null = null;
  /** The visible Keys as of the last frame. */
  let shown = "";
  /** The cell being edited as of the last frame, `""` if none. */
  let edited = "";
  /** A scroll's update waiting for the scroll to settle. */
  let deferred: { s: Snapshot; content: () => Content; since: number; timer: ReturnType<typeof setTimeout> } | null =
    null;
  /** The focused Key per axis as of the last frame. */
  const focus: Record<AxisName, bigint | null> = { rows: null, cols: null };
  /** Per axis, the focused Key being located, until the Grid stops locating. */
  const locating: Record<AxisName, bigint | null> = { rows: null, cols: null };

  const announce = (message: string) => {
    // The same words again must still change the text to be read again.
    status.textContent = message === status.textContent ? `${message}\u00a0` : message;
  };

  /** Announces what the table itself cannot show: a sort applied, a focused item gone. */
  const watch = (s: Snapshot, content: () => Content) => {
    const sort = s.appliedSort.map((c) => `${c.key}:${c.direction}`).join(",");
    if (sorted !== null && sort !== sorted) announce(sortMessage(s, content()));
    sorted = sort;
    for (const axis of AXES) {
      const key = locating[axis];
      // A Locate that ends `Found` keeps the focused Key; `Gone` moves focus off it.
      if (key !== null && !s[axis].locating) {
        locating[axis] = null;
        if (s[axis].focus?.key !== key) announce(`The focused ${axis === "rows" ? "row" : "column"} is no longer in the table`);
      }
      focus[axis] = s[axis].focus?.key ?? null;
    }
  };

  const update = (s: Snapshot, content: Content) => {
    const choose = options.renderer();
    const primary = s.appliedSort[0];
    set(table, "aria-rowcount", String(s.rows.count + 1));
    set(table, "aria-colcount", String(s.cols.count + 1));
    const cols = visible(s.cols);

    const nextHeaders = new Map<string, HTMLTableCellElement>();
    for (const { item } of cols) {
      const key = name(item);
      const th = headers.get(key) ?? document.createElement("th");
      nextHeaders.set(key, th);
      set(th, "role", "columnheader");
      set(th, "id", item.key === null ? null : `${prefix}-c${item.key}`);
      set(th, "aria-colindex", String(item.position + 2));
      // Only the most significant sort column, as ARIA asks; the live region names the rest.
      set(th, "aria-sort", primary && primary.key === item.key ? primary.direction : null);
      text(th, content.label("cols", item));
    }
    headers = nextHeaders;
    arrange(headRow, [corner, ...headers.values()]);

    const nextRows = new Map<string, Row>();
    for (const { item: rowItem, index: i } of visible(s.rows)) {
      const key = name(rowItem);
      const row = rows.get(key) ?? newRow();
      nextRows.set(key, row);
      set(row.el, "aria-rowindex", String(rowItem.position + 2));
      set(row.header, "id", rowItem.key === null ? null : `${prefix}-r${rowItem.key}`);
      text(row.header, content.label("rows", rowItem));
      const rowId = content.id("rows", rowItem);
      const cells = new Map<string, Cell>();
      for (const { item: colItem, index: j } of cols) {
        const colKey = name(colItem);
        const cell = row.cells.get(colKey) ?? newCell();
        cells.set(colKey, cell);
        const { el } = cell;
        set(el, "id", rowItem.key === null || colItem.key === null ? null : `${prefix}-${rowItem.key}-${colItem.key}`);
        set(el, "aria-colindex", String(colItem.position + 2));
        set(el, "aria-selected", String(cellState(s, i, j).selected));
        const value = content.value(i, j);
        const failed = content.failed(i, j);
        set(el, "aria-busy", value === undefined && !failed ? "true" : null);
        const colId = content.id("cols", colItem);
        const renderer = rowId === undefined || colId === undefined ? plainText : choose(rowId, colId);
        if (value !== cell.value || renderer !== cell.renderer || failed !== cell.failed) {
          cell.value = value;
          cell.renderer = renderer;
          cell.failed = failed;
          text(el, failed ? "Failed to load" : value === undefined ? "Loading" : cellText(renderer, value));
        }
      }
      row.cells = cells;
      arrange(row.el, [row.header, ...[...cells.values()].map((c) => c.el)]);
    }
    rows = nextRows;
    arrange(body, [...rows.values()].map((r) => r.el));

    // While an edit is open the textarea is the editor, read as itself and
    // named for its cell; pointing away from it would hide the text.
    const { mode } = s;
    const row = mode.type === "Editing" ? mode.row : s.rows.focus?.key;
    const col = mode.type === "Editing" ? mode.col : s.cols.focus?.key;
    const focused = row == null || col == null ? null : rows.get(String(row))?.cells.get(String(col));
    set(options.focus, "aria-activedescendant", mode.type === "Editing" ? null : focused?.el.id || null);
    const label = (axis: "rows" | "cols", key: bigint) => {
      const item = s[axis].strip.find((i) => i.key === key);
      return item ? content.label(axis, item) : "";
    };
    set(
      options.focus,
      "aria-label",
      mode.type === "Editing" ? `Edit ${label("cols", mode.col)}, ${label("rows", mode.row)}` : (options.label ?? null),
    );
  };

  return {
    element,
    frame(s, painted, content) {
      const before = shown;
      shown = visibleKeys(s);
      const moved = shown !== before;
      const was = edited;
      edited = s.mode.type === "Editing" ? `${s.mode.row},${s.mode.col}` : "";
      const refocused =
        (s.rows.focus?.key ?? null) !== focus.rows || (s.cols.focus?.key ?? null) !== focus.cols || edited !== was;
      watch(s, content);
      if (!painted && !moved && !refocused) return;
      // A frame that only moved the window (a scroll) waits for a pause;
      // anything else, a focus move or values landing, updates now.
      const scroll = moved && !refocused && !s.damage.selection;
      const now = performance.now();
      if (scroll && (deferred === null || now - deferred.since < MOST)) {
        const since = deferred?.since ?? now;
        if (deferred) clearTimeout(deferred.timer);
        const timer = setTimeout(() => {
          const last = deferred!;
          deferred = null;
          update(last.s, last.content());
        }, SETTLE);
        deferred = { s, content, since, timer };
        return;
      }
      if (deferred) clearTimeout(deferred.timer);
      deferred = null;
      update(s, content());
    },
    locate(axis, key) {
      // Locates for other Keys (an edit's cell) say nothing about focus.
      if (focus[axis] === key) locating[axis] = key;
    },
    destroy() {
      if (deferred) clearTimeout(deferred.timer);
      element.remove();
    },
  };
}

function visibleKeys(s: Snapshot): string {
  const keys = (axis: AxisSnapshot) => visible(axis).map(({ item }) => name(item)).join(",");
  return `${keys(s.rows)};${keys(s.cols)}`;
}

function sortMessage(s: Snapshot, content: Content): string {
  if (s.appliedSort.length === 0) return "Not sorted";
  const columns = s.appliedSort.map(({ key, direction }) => {
    const item = s.cols.strip.find((i) => i.key === key);
    return `${item ? content.label("cols", item) : "a column out of view"}, ${direction}`;
  });
  return `Sorted by ${columns.join(", then ")}`;
}

function newRow(): Row {
  const el = document.createElement("tr");
  el.setAttribute("role", "row");
  const header = document.createElement("th");
  header.setAttribute("role", "rowheader");
  header.setAttribute("aria-colindex", "1");
  el.append(header);
  return { el, header, cells: new Map() };
}

function newCell(): Cell {
  const el = document.createElement("td");
  el.setAttribute("role", "gridcell");
  return { el, value: undefined, renderer: undefined, failed: false };
}
