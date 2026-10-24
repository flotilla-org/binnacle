// The ARIA layer: a visually hidden `role="grid"` table of the visible
// window, kept beside the canvas for assistive technology. Elements are
// reused by Key, so a cell keeps its `id` (and a screen reader its place)
// while the table scrolls; only what changed is written.
//
// Rows grouped in outline form make it a `treegrid` (the WAI-ARIA treegrid
// pattern): each row carries its level, a Group Item whether it is
// expanded, and every row its place among its siblings, as the outline
// gives it. In header form each level is a column of row headers, each
// group one cell spanning its rows in view. Summary Items say their kind.

import { AXES, type AxisName } from "./codes";
import type { Content } from "./content";
import { cellText, plainText, type RendererChoice } from "./renderer";
import {
  cellState,
  itemSpan,
  type AxisSnapshot,
  type HeaderSpan,
  type KeyedOutline,
  type Snapshot,
  type StripItem,
} from "./snapshot";

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
  /** Reads `message` out in the live region, for news the table cannot show. */
  say(message: string): void;
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

/**
 * The visible items of an axis in drawn order, each with its index among the
 * drawn items and its place along the whole axis as drawn: the leading pins,
 * then the rest in Position order, then the trailing pins. A scrolling item
 * is hidden once it slides under a pinned Strip.
 */
function visible(axis: AxisSnapshot): { item: StripItem; index: number; place: number }[] {
  const { strip, pinned, trailing, count } = axis;
  const pins = [...strip.slice(0, pinned), ...strip.slice(strip.length - trailing)].map((item) => item.position);
  const place = (item: StripItem, index: number) => {
    if (index < pinned) return index;
    if (index >= strip.length - trailing) return count - (strip.length - index);
    return pinned + item.position - pins.filter((p) => p < item.position).length;
  };
  const out: { item: StripItem; index: number; place: number }[] = [];
  strip.forEach((item, index) => {
    const span = itemSpan(axis, index);
    if (item.offset + item.size > span.start && item.offset < span.end) out.push({ item, index, place: place(item, index) });
  });
  return out;
}

/** What a Summary Item's row is called in place of "row"; a Leaf's is a row. */
const KINDS: Record<Exclude<KeyedOutline["kind"], "leaf">, string> = {
  group: "group row",
  subtotal: "subtotal row",
  grandTotal: "grand total row",
};

/**
 * Header form's level columns as the visible rows meet them: for each row,
 * the span covering it at each tier, up to the first tier none covers (the
 * row's own header takes the rest of the band).
 */
function coverage(axis: AxisSnapshot, rows: { index: number }[], tiers: number): HeaderSpan[][] {
  const at = new Map(rows.map(({ index }, r) => [index, r]));
  const covers: (HeaderSpan | undefined)[][] = rows.map(() => []);
  for (const span of axis.spans) {
    if (span.tier >= tiers) continue;
    for (let i = span.first; i <= span.last; i++) {
      const r = at.get(i);
      if (r !== undefined) covers[r]![span.tier] = span;
    }
  }
  return covers.map((c) => {
    const k = Array.from({ length: tiers }, (_, t) => c[t]).findIndex((span) => span === undefined);
    return (k < 0 ? c : c.slice(0, k)) as HeaderSpan[];
  });
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
  /** The row band's column headers, one per tier where it crosses the corner. */
  let corners: HTMLTableCellElement[] = [corner];
  /** Header form's spanning cells, by tier and group Key. */
  let spans = new Map<string, HTMLTableCellElement>();
  /** Each axis's applied Sort Spec last seen, `null` before the first frame. */
  const sorted: Record<AxisName, string | null> = { rows: null, cols: null };
  /** The applied rows filter last seen, `null` before the first frame. */
  let filtered: string | null = null;
  /** The rows filter changed and its count is not announced yet. */
  let recount = false;
  /** The visible Keys as of the last frame. */
  let shown = "";
  /** The cell being edited as of the last frame, `""` if none. */
  let edited = "";
  /** A scroll's update waiting for the scroll to settle. */
  let deferred: { s: Snapshot; content: () => Content; since: number; timer: ReturnType<typeof setTimeout> } | null =
    null;
  /** The focused Key per axis as of the last frame. */
  const focus: Record<AxisName, bigint | null> = { rows: null, cols: null };
  /** The reason of the move refusal last seen, `null` while there is none. */
  let refused: string | null = null;
  /** Per axis, the focused Key being located, until the Grid stops locating. */
  const locating: Record<AxisName, bigint | null> = { rows: null, cols: null };

  const announce = (message: string) => {
    // The same words again must still change the text to be read again.
    status.textContent = message === status.textContent ? `${message}\u00a0` : message;
  };

  /**
   * Announces what the table itself cannot show: a sort applied, the rows
   * left once a filter change is applied and no other View Spec is pending,
   * a focused item gone, a move refused.
   */
  const watch = (s: Snapshot, content: () => Content) => {
    const messages: string[] = [];
    for (const axis of AXES) {
      const sort = s.appliedView[axis].sort.map((c) => `${c.key}:${c.direction}`).join(",");
      if (sorted[axis] !== null && sort !== sorted[axis]) messages.push(sortMessage(s, content(), axis));
      sorted[axis] = sort;
    }
    const filter = JSON.stringify(s.appliedView.rows.filter, (_, v: unknown) => (typeof v === "bigint" ? String(v) : v));
    recount ||= filtered !== null && filter !== filtered;
    filtered = filter;
    if (recount && s.requestedView === null) {
      recount = false;
      messages.push(rowCount(s.rows.count, s.rows.total));
    }
    const refusal = s.moveRefusal?.reason ?? null;
    if (refusal !== null && refusal !== refused) messages.push(refusal);
    refused = refusal;
    if (messages.length) announce(messages.join(". "));
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
    const primary = s.appliedView.rows.sort[0];
    const primaryRow = s.appliedView.cols.sort[0];
    const grouping = s.appliedView.grouping;
    const form = grouping && grouping.rowLevels.length > 0 ? grouping.rowAxis.form : null;
    const tree = form === "outline";
    set(table, "role", tree ? "treegrid" : "grid");
    // The row band's columns: one per level in header form, then the items' own headers.
    const tiers = form === "header" ? Math.max(0, s.rows.corner.length - 1) : 0;
    const band = tiers + 1;
    set(table, "aria-rowcount", String(s.rows.count + 1));
    set(table, "aria-colcount", String(s.cols.count + band));
    const cols = visible(s.cols);

    corners = Array.from({ length: band }, (_, t) => {
      const th = corners[t] ?? newCorner();
      set(th, "aria-colindex", String(t + 1));
      // A level column names its level; the items' own names every level in outline form.
      const levels = s.rows.corner[t]?.levels ?? [];
      text(th, levels.map((l) => content.levelName("rows", l.field)).join(", "));
      const sorts = primary !== undefined && levels.some((l) => l.field === primary.key);
      set(th, "aria-sort", sorts ? primary.direction : null);
      return th;
    });

    const nextHeaders = new Map<string, HTMLTableCellElement>();
    for (const { item, place } of cols) {
      const key = name(item);
      const th = headers.get(key) ?? document.createElement("th");
      nextHeaders.set(key, th);
      set(th, "role", "columnheader");
      set(th, "id", item.key === null ? null : `${prefix}-c${item.key}`);
      set(th, "aria-colindex", String(place + 1 + band));
      // Only the most significant sort column, as ARIA asks; the live region names the rest.
      set(th, "aria-sort", primary && primary.key === item.key ? primary.direction : null);
      text(th, content.label("cols", item));
    }
    headers = nextHeaders;
    arrange(headRow, [...corners, ...headers.values()]);

    const shownRows = visible(s.rows);
    const covers = coverage(s.rows, shownRows, tiers);
    const nextSpans = new Map<string, HTMLTableCellElement>();
    /** Per visible row, the spanning cell over it at each tier it is covered. */
    const over: HTMLTableCellElement[][] = shownRows.map(() => []);
    const nextRows = new Map<string, Row>();
    for (const [r, { item: rowItem, index: i, place: rowPlace }] of shownRows.entries()) {
      const key = name(rowItem);
      const row = rows.get(key) ?? newRow();
      nextRows.set(key, row);
      set(row.el, "aria-rowindex", String(rowPlace + 2));
      set(row.el, "id", rowItem.key === null ? null : `${prefix}-row${rowItem.key}`);
      const outline = rowItem.outline;
      const opens = tree && outline?.kind === "group" && outline.open !== "cantOpen";
      set(row.el, "aria-level", tree && outline ? String(outline.depth + 1) : null);
      set(row.el, "aria-expanded", opens ? String(outline.open === "open") : null);
      set(row.el, "aria-posinset", tree && outline?.set ? String(outline.set.index + 1) : null);
      set(row.el, "aria-setsize", tree && outline?.set ? String(outline.set.size) : null);
      set(row.el, "aria-roledescription", outline && outline.kind !== "leaf" ? KINDS[outline.kind] : null);

      // A span's cell sits in the first row of each run of rows it covers, spanning the run.
      const covered = covers[r]!;
      const starts: HTMLTableCellElement[] = [];
      covered.forEach((span, t) => {
        const above = covers[r - 1]?.[t];
        if (above?.key === span.key) {
          over[r]![t] = over[r - 1]![t]!;
          return;
        }
        let n = 1;
        while (covers[r + n]?.[t]?.key === span.key) n++;
        const id = `${t}:${span.key}`;
        const fresh = nextSpans.has(id);
        const th = (fresh ? undefined : spans.get(id)) ?? newSpan();
        nextSpans.set(fresh ? `${id}:${r}` : id, th);
        set(th, "id", fresh ? null : `${prefix}-t${t}-${span.key}`);
        set(th, "aria-colindex", String(t + 1));
        set(th, "rowspan", n > 1 ? String(n) : null);
        set(th, "aria-expanded", String(span.open === "open"));
        text(th, content.spanLabel("rows", span));
        over[r]![t] = th;
        starts.push(th);
      });
      set(row.header, "aria-colindex", String(covered.length + 1));
      set(row.header, "colspan", band - covered.length > 1 ? String(band - covered.length) : null);
      set(row.header, "id", rowItem.key === null ? null : `${prefix}-r${rowItem.key}`);
      set(row.header, "aria-sort", primaryRow && primaryRow.key === rowItem.key ? primaryRow.direction : null);
      text(row.header, content.label("rows", rowItem));
      const rowId = content.id("rows", rowItem);
      const cells = new Map<string, Cell>();
      for (const { item: colItem, index: j, place: colPlace } of cols) {
        const colKey = name(colItem);
        const cell = row.cells.get(colKey) ?? newCell();
        cells.set(colKey, cell);
        const { el } = cell;
        set(el, "id", rowItem.key === null || colItem.key === null ? null : `${prefix}-${rowItem.key}-${colItem.key}`);
        set(el, "aria-colindex", String(colPlace + 1 + band));
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
      arrange(row.el, [...starts, row.header, ...[...cells.values()].map((c) => c.el)]);
    }
    rows = nextRows;
    spans = nextSpans;
    arrange(body, [...rows.values()].map((r) => r.el));

    // While an edit is open the textarea is the editor, read as itself and
    // named for its cell; pointing away from it would hide the text.
    const { mode } = s;
    const row = mode.type === "Editing" ? mode.row : s.rows.focus?.key;
    const col = mode.type === "Editing" ? mode.col : s.cols.focus?.key;
    const focusedRow = row == null ? undefined : rows.get(String(row));
    // In the row band, focus is on a tree's row, or in header form on the level column's span over the row.
    let focused: HTMLElement | undefined;
    if (s.outlineFocused && focusedRow) {
      const r = shownRows.findIndex(({ item }) => item.key === row);
      focused = tree ? focusedRow.el : (over[r]?.[s.focusedTier] ?? focusedRow.header);
    } else if (col != null) {
      focused = focusedRow?.cells.get(String(col))?.el;
    }
    set(options.focus, "aria-activedescendant", mode.type === "Editing" ? null : focused?.id || null);
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
    say: announce,
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

const grouped = new Intl.NumberFormat("en");

/** The rows shown, of the total before any filter if known and different: "1,204 of 5,001 rows", or "5,001 rows". */
export function rowCount(count: number, total: number | null): string {
  const noun = (total ?? count) === 1 ? "row" : "rows";
  if (total === null || total === count) return `${grouped.format(count)} ${noun}`;
  return `${grouped.format(count)} of ${grouped.format(total)} ${noun}`;
}

/** What a change to `axis`'s sort says: the rows' as "Sorted by …", the columns' as "Columns sorted by …". */
function sortMessage(s: Snapshot, content: Content, axis: AxisName): string {
  const by = axis === "rows" ? "cols" : "rows";
  const sort = s.appliedView[axis].sort;
  if (sort.length === 0) return axis === "rows" ? "Not sorted" : "Columns not sorted";
  const items = sort.map(({ key, direction }) => {
    const item = s[by].strip.find((i) => i.key === key);
    const absent = by === "cols" ? "a column out of view" : "a row out of view";
    return `${item ? content.label(by, item) : absent}, ${direction}`;
  });
  return `${axis === "rows" ? "Sorted" : "Columns sorted"} by ${items.join(", then ")}`;
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

function newCorner(): HTMLTableCellElement {
  const th = document.createElement("th");
  th.setAttribute("role", "columnheader");
  return th;
}

function newSpan(): HTMLTableCellElement {
  const th = document.createElement("th");
  th.setAttribute("role", "rowheader");
  return th;
}

function newCell(): Cell {
  const el = document.createElement("td");
  el.setAttribute("role", "gridcell");
  return { el, value: undefined, renderer: undefined, failed: false };
}
