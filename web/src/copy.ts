// Copy: what a `CopyRequested` Effect names, gathered from the Table Source
// as the grid shows it and written as tab-separated text. Each copied row
// is the items its run of Positions holds, so a collapsed group's items,
// which hold no Position, are left out. On grouped rows each row leads with
// its label from the row Header Band: a Summary Item's label without its
// indent or disclosure mark, a Leaf's own label if the source gives one.
// An open Group Item whose Subtotal is placed copies no figures, as it
// draws none.

import type { Run } from "./effects";
import { cellText, type RendererChoice } from "./renderer";
import { windowIds } from "./feed";
import type { MaybePromise, Outline, TableSource } from "./source";

/** One copied row: its ID, its Position and, on a grouped axis, its outline. */
export interface CopiedRow<RowId, ColId> {
  id: RowId;
  position: number;
  outline: Outline<RowId, ColId> | null;
}

/** What a copy gathered, as the widget hands it to `onCopy`. */
export interface Copied<RowId, ColId> {
  rows: CopiedRow<RowId, ColId>[];
  cols: ColId[];
  /** Whether each row leads with its label, as on grouped rows. */
  rowLabels: boolean;
  /** The rows as tab-separated lines, as the grid shows them. */
  text: string;
}

/** The most cells one copy gathers; a bigger selection copies nothing. */
export const COPY_LIMIT = 1_000_000;

export interface CopyParts<RowId, ColId, V> {
  source: TableSource<RowId, ColId, V>;
  renderer: RendererChoice<RowId, ColId, V>;
  /** A row's label in the row Header Band. */
  label(row: CopiedRow<RowId, ColId>): string;
  /** Whether a row draws no figures (an open Group Item whose Subtotal is placed). */
  figureless(outline: Outline<RowId, ColId> | null): boolean;
  signal: AbortSignal;
}

function isPromise<T>(v: MaybePromise<T>): v is Promise<T> {
  return typeof (v as { then?: unknown } | null)?.then === "function";
}

/** `f` applied to `v`, at once unless `v` is a Promise. */
function then<T, U>(v: MaybePromise<T>, f: (t: T) => MaybePromise<U>): MaybePromise<U> {
  return isPromise(v) ? v.then(f) : f(v);
}

/** Every item in `runs` of an axis, in order, asking the axis a run at a time. */
function items<Item>(runs: Run[], ask: (run: Run) => MaybePromise<Item[]>): MaybePromise<Item[]> {
  const out: Item[] = [];
  const step = (i: number): MaybePromise<Item[]> => {
    if (i === runs.length) return out;
    return then(ask(runs[i]!), (got) => {
      out.push(...got);
      return step(i + 1);
    });
  };
  return step(0);
}

function count(runs: Run[]): number {
  return runs.reduce((n, r) => n + r.end - r.start, 0);
}

/** A field as a spreadsheet pastes it: quoted when it holds a tab, a line break or a quote. */
function field(s: string): string {
  return /[\t\n\r"]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/**
 * Gathers the cells `rows` and `cols` name from the source, at once if the
 * source answers at once. `null` past `COPY_LIMIT` cells.
 */
export function gatherCopy<RowId, ColId, V>(
  rows: Run[],
  cols: Run[],
  rowLabels: boolean,
  parts: CopyParts<RowId, ColId, V>,
): MaybePromise<Copied<RowId, ColId> | null> {
  if (count(rows) * count(cols) > COPY_LIMIT) return null;
  const { source, signal } = parts;
  const rowItems = items(rows, (r) =>
    then(source.rows.window(r.start, r.end, signal), (w) => {
      const ids = windowIds(w);
      const outlines = Array.isArray(w) ? [] : w.outlines;
      return ids.map((id, i) => ({ id, position: r.start + i, outline: outlines[i] ?? null }));
    }),
  );
  const colIds = items(cols, (r) => then(source.cols.window(r.start, r.end, signal), (w) => windowIds(w)));
  return then(rowItems, (rows) =>
    then(colIds, (cols) =>
      then(rows.length && cols.length ? source.cells(rows.map((r) => r.id), cols, signal) : [], (values) => {
        const lines = rows.map((row, i) => {
          const blank = parts.figureless(row.outline);
          const cells = cols.map((col, j) => {
            const value = values[i]?.[j];
            return blank || value === undefined ? "" : cellText(parts.renderer(row.id, col), value);
          });
          const label = rowLabels ? [parts.label(row)] : [];
          return [...label, ...cells].map(field).join("\t");
        });
        return { rows, cols, rowLabels, text: lines.join("\n") };
      }),
    ),
  );
}

/**
 * Puts `text` on the clipboard: through a `copy` event while the key that
 * asked is still being handled (which every browser allows), else through
 * the asynchronous Clipboard API.
 */
export function writeClipboard(text: string): Promise<void> {
  let wrote = false;
  const onCopy = (e: ClipboardEvent) => {
    if (!e.clipboardData) return;
    e.clipboardData.setData("text/plain", text);
    e.preventDefault();
    wrote = true;
  };
  document.addEventListener("copy", onCopy);
  try {
    document.execCommand("copy");
  } catch {
    // Some browsers throw outside a user gesture; the Clipboard API is next.
  } finally {
    document.removeEventListener("copy", onCopy);
  }
  if (wrote) return Promise.resolve();
  return navigator.clipboard?.writeText(text) ?? Promise.reject(new Error("binnacle: no clipboard"));
}
