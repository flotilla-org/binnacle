// The huge source: 10^15 rows by 12 columns, position identity on both axes,
// values made up from the Position, like the ratatui demo's `--huge`. A "Go
// to row" box in the toolbar focuses a row by number; it lives outside the
// grid because typing in the grid starts an edit.
// Find works the matches out from the digits rather than visiting 10^16
// cells, and counts like a source that cannot count everything at once: it
// answers with the matches among the first thousand rows ("n of m+"), then
// pushes running counts through the sink as it gets ten times further each
// time, until the last says how many there are.

import { createFindBar, GridWidget, type FindCount, type FindQuery, type TableSink, type TableSource } from "../../src";
import type { DemoSource } from "./demo-source";

const ROWS = 1e15;
const COLS = 12;

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}

const text = (row: number, col: number) => `r${row}c${col}`;

/** The text a query looks for, as the cells spell it; `null` if no cell can match it. */
function sought(query: FindQuery<number>): string | null {
  const t = query.query.caseSensitive ? query.query.text : query.query.text.toLowerCase();
  return /^[rc0-9]*$/.test(t) ? t : null;
}

/** The columns a query searches. */
function searched(query: FindQuery<number>): number[] {
  const { searches } = query.query;
  return searches === "shown" ? range(0, COLS) : range(0, COLS).filter((c) => searches.includes(c));
}

/**
 * How many rows below `end` match `t` in column `col`: a walk of the digits
 * of every row number at once, tracking how much of `t` each has matched so
 * far (the Knuth-Morris-Pratt automaton, the full match absorbing).
 */
function matchingRows(t: string, col: number, end: number): number {
  const m = t.length;
  const fail = [0];
  for (let i = 1, k = 0; i < m; i++) {
    while (k > 0 && t[i] !== t[k]) k = fail[k - 1]!;
    if (t[i] === t[k]) k++;
    fail[i] = k;
  }
  const step = (state: number, ch: string) => {
    if (state === m) return m;
    let k = state;
    while (k > 0 && ch !== t[k]) k = fail[k - 1]!;
    return ch === t[k] ? k + 1 : k;
  };
  type Counts = number[];
  const zero = (): Counts => Array<number>(m + 1).fill(0);
  const feed = (from: Counts, chars: string): Counts =>
    [...chars].reduce((counts, ch) => {
      const next = zero();
      counts.forEach((n, s) => n && (next[step(s, ch)]! += n));
      return next;
    }, from);
  const digits = (from: Counts, lo: number, hi: number): Counts => {
    const next = zero();
    for (let d = lo; d < hi; d++) feed(from, String(d)).forEach((n, s) => (next[s]! += n));
    return next;
  };
  const free = (from: Counts, k: number) => Array.from({ length: k }).reduce<Counts>((c) => digits(c, 0, 10), from);
  const add = (a: Counts, b: Counts) => a.map((n, s) => n + b[s]!);
  const start = feed([1, ...Array<number>(m).fill(0)], "r");
  const bound = String(end);
  let total = zero();
  // Row numbers shorter than `end`'s, then those as long and below it.
  for (let l = 1; l < bound.length; l++) total = add(total, free(digits(start, l === 1 ? 0 : 1, 10), l - 1));
  let prefix = start;
  for (let i = 0; i < bound.length && end > 0; i++) {
    const d = Number(bound[i]);
    const lo = i === 0 && bound.length > 1 ? 1 : 0;
    total = add(total, free(digits(prefix, lo, d), bound.length - 1 - i));
    prefix = feed(prefix, bound[i]!);
  }
  return feed(total, `c${col}`)[m]!;
}

/** How many cells in rows below `end` match. */
function matchesBelow(query: FindQuery<number>, end: number): number {
  const t = sought(query);
  return t === null ? 0 : searched(query).reduce((n, c) => n + matchingRows(t, c, end), 0);
}

/** The first row at or after `from` (below `end`) where `has(start, end)` finds a match, by halving. */
function firstRow(from: number, end: number, has: (start: number, end: number) => boolean): number | null {
  if (from >= end || !has(from, end)) return null;
  let [lo, hi] = [from, end];
  while (hi - lo > 1) {
    const mid = lo + Math.floor((hi - lo) / 2);
    if (has(lo, mid)) hi = mid;
    else lo = mid;
  }
  return lo;
}

/** The last row before `end` (at or after `from`) where `has` finds a match. */
function lastRow(from: number, end: number, has: (start: number, end: number) => boolean): number | null {
  if (from >= end || !has(from, end)) return null;
  let [lo, hi] = [from, end];
  while (hi - lo > 1) {
    const mid = lo + Math.floor((hi - lo) / 2);
    if (has(mid, hi)) lo = mid;
    else hi = mid;
  }
  return lo;
}

/** The matches before a cell in the query's order. */
function before(query: FindQuery<number>, row: number, col: number): number {
  const t = sought(query)!;
  const cols = searched(query);
  if (query.by === "rows")
    return matchesBelow(query, row) + cols.filter((c) => c < col && text(row, c).includes(t)).length;
  return cols.reduce((n, c) => n + (c < col ? matchingRows(t, c, ROWS) : c === col ? matchingRows(t, c, row) : 0), 0);
}

/** The first match after (or before) `from` in the query's order, wrapping round. */
function step(
  query: FindQuery<number>,
  from: { row: number; col: number } | null,
  direction: "next" | "previous",
): { row: number; col: number; index: number; wrapped: boolean } | null {
  const t = sought(query);
  if (t === null) return null;
  const cols = searched(query);
  const forward = direction === "next";
  const hit = (row: number, col: number) => cols.includes(col) && text(row, col).includes(t);
  const found = (row: number, col: number, wrapped: boolean) => ({ row, col, index: before(query, row, col), wrapped });
  if (query.by === "rows") {
    const has = (a: number, b: number) => cols.some((c) => matchingRows(t, c, b) - matchingRows(t, c, a) > 0);
    const inRow = (row: number, after: number | null) => {
      const order = forward ? range(0, COLS) : range(0, COLS).reverse();
      return order.find((c) => (after === null || (forward ? c > after : c < after)) && hit(row, c));
    };
    if (from) {
      const c = inRow(from.row, from.col);
      if (c !== undefined) return found(from.row, c, false);
    }
    const row = forward ? firstRow(from ? from.row + 1 : 0, ROWS, has) : lastRow(0, from ? from.row : ROWS, has);
    if (row !== null) return found(row, inRow(row, null)!, false);
    const again = forward ? firstRow(0, ROWS, has) : lastRow(0, ROWS, has);
    return again === null ? null : found(again, inRow(again, null)!, from !== null);
  }
  const order = forward ? cols : [...cols].reverse();
  const inCol = (col: number, a: number, b: number) => {
    const has = (x: number, y: number) => matchingRows(t, col, y) - matchingRows(t, col, x) > 0;
    return forward ? firstRow(a, b, has) : lastRow(a, b, has);
  };
  if (from) {
    const row = forward ? inCol(from.col, from.row + 1, ROWS) : inCol(from.col, 0, from.row);
    if (row !== null && cols.includes(from.col)) return found(row, from.col, false);
    const later = order.filter((c) => (forward ? c > from.col : c < from.col));
    for (const c of later) {
      const r = inCol(c, 0, ROWS);
      if (r !== null) return found(r, c, false);
    }
  }
  for (const c of order) {
    const r = inCol(c, 0, ROWS);
    if (r !== null) return found(r, c, from !== null);
  }
  return null;
}

let sink: TableSink<number, number> | null = null;
/** The count pushing running totals, stopped when a newer one starts. */
let counting: ReturnType<typeof setTimeout> | null = null;

/** Counts the first thousand rows, then pushes running counts ten times further every 250 ms. */
function count(query: FindQuery<number>): FindCount {
  if (counting !== null) clearTimeout(counting);
  let digits = 3;
  const push = () => {
    digits++;
    const done = digits >= Math.log10(ROWS);
    sink?.findCounted(
      query,
      done ? { total: matchesBelow(query, ROWS) } : { total: matchesBelow(query, 10 ** digits), partial: true },
    );
    counting = done ? null : setTimeout(push, 250);
  };
  counting = setTimeout(push, 250);
  return { total: matchesBelow(query, 10 ** digits), partial: true };
}

const source: TableSource<number, number, string> = {
  rows: { identity: "position", count: () => ROWS, window: range },
  cols: { identity: "position", count: () => COLS, window: range, label: (id) => `col ${id}` },
  cells: (rows, cols) => rows.map((r) => cols.map((c) => text(r, c))),
  find: {
    marks: (rows, cols, query) => {
      const t = sought(query);
      const shown = new Set(searched(query));
      return rows.map((r) => cols.map((c) => t !== null && shown.has(c) && text(r, c).includes(t)));
    },
    next: (from, direction, query) => step(query, from, direction),
    count,
  },
  attach(s) {
    sink = s;
    return () => {
      sink = null;
      if (counting !== null) clearTimeout(counting);
    };
  },
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
    const frame = document.createElement("div");
    frame.className = "with-filter-bar";
    const gridBox = document.createElement("div");
    gridBox.className = "grid-box";
    frame.append(gridBox);
    host.append(frame);
    const widget = await GridWidget.create(gridBox, { source, columnWidth: 168, ...look });
    const findBar = createFindBar(widget);
    frame.prepend(findBar.element);
    const findButton = document.createElement("button");
    findButton.textContent = "Find";
    findButton.title = "Find in the table (Ctrl/Cmd-F in the grid)";
    findButton.addEventListener("click", () => findBar.open());
    const form = goToRow(widget);
    toolbar.append(form, findButton);
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
        findBar.destroy();
        widget.destroy();
        frame.remove();
        findButton.remove();
        form.remove();
      },
    };
  },
};
