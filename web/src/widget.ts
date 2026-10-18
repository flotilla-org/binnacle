// The framework-neutral widget: mounts into a container, owns the frame loop
// (idle unless something changed), the native scroller and the painter, and
// frees the wasm Grid on destroy.

import { createAria, type Aria } from "./aria";
import { answerAutoSize, fitWidth, type SizingParts } from "./autosize";
import { borderGaps, headersBorder, type BorderStyle } from "./border";
import type { Editor } from "./editing";
import { createCanvasPainter, type CanvasPainter } from "./canvas/painter";
import { PAD } from "./canvas/context";
import type { Effect } from "./effects";
import { filterButtonRect, hasFilterButton } from "./canvas/bands";
import { createFeed, type ColumnSupport, type Feed } from "./feed";
import { drainEffects, runFrame, type FrameParts } from "./frame";
import { createInput, type Input } from "./input";
import { createKeymap, type KeyBindings, type Platform } from "./keymap";
import { createScrollMapping, type ScrollMapping, type ScrollPort } from "./scroller";
import { plainText, type RendererChoice } from "./renderer";
import { createSelectionEvents, type SelectionChange } from "./selection";
import { createInitialSizes, type InitialSize, type InitialSizes } from "./sizes";
import type { AxisSnapshot, ConditionState, HeaderPlacement, Snapshot, Span } from "./snapshot";
import type { Condition, FailedRequest, Located, TableSource, TextQuery, ViewSpec } from "./source";
import { AXIS_CODE, SELECTION_UNIT, type AxisName } from "./codes";
import { systemTheme, type ColorScheme, type Theme, type ThemeChoice } from "./theme";
import { bridge, loadBinnacle, type WebGrid } from "./wasm";

export interface GridOptions<RowId, ColId, V = unknown> {
  source: TableSource<RowId, ColId, V>;
  /** Row height, CSS px. */
  rowHeight?: number;
  /** Default column width, CSS px. */
  columnWidth?: number;
  /** The column header band's height, CSS px. */
  headerHeight?: number;
  /**
   * Each column's starting width, by its ID: CSS px, `"fit"` to its header
   * and the values in view, `"fill"` for an equal share of the width the
   * other columns leave, or `undefined` for `columnWidth`. Asked once per
   * column, when it first arrives; a width the user sets wins.
   */
  initialColumnWidth?(colId: NoInfer<ColId>): InitialSize;
  /** Each row's starting height, by its ID, as `initialColumnWidth`; rows hold one line, so `"fit"` is `rowHeight`. */
  initialRowHeight?(rowId: NoInfer<RowId>): InitialSize;
  /** A Theme, or one per colour scheme; by default light or Catppuccin Mocha, following the system. */
  theme?: ThemeChoice;
  /** The lines drawn in the Gaps; the widget lays out Gaps that fit them. Default: handles in the header bands. */
  border?: BorderStyle;
  /** Key bindings merged over the defaults; `null` unbinds a chord (see `KeyBindings`). */
  keymap?: KeyBindings;
  /** Whose key conventions the defaults follow; detected from the browser if omitted. */
  platform?: Platform;
  /** Chooses each cell's Cell Renderer by the Table Source's IDs; the source's `renderer`, else plain text, by default. */
  renderer?: RendererChoice<NoInfer<RowId>, NoInfer<ColId>, NoInfer<V>>;
  /**
   * Called when an edit is committed, with the textarea's text or a custom
   * editor's value. The app updates its data and tells the source's sink
   * `ContentChanged`; the cell shows the committed value until the source
   * answers for it again. Returning `false` refuses the value: the cell keeps
   * the value it had, at once. Without `onCommit` the grid is read-only.
   */
  onCommit?(rowId: NoInfer<RowId>, colId: NoInfer<ColId>, value: NoInfer<V> | string): boolean | void;
  /** A custom editor for a cell, mounted over it in place of the textarea; `null` or `undefined` for the textarea. */
  editor?(rowId: NoInfer<RowId>, colId: NoInfer<ColId>): Editor<NoInfer<V>> | null | undefined;
  /** The grid's accessible name, read by screen readers. */
  label?: string;
  /**
   * Called when the focus, Selection or Picks change, after the frame that
   * shows the change, with the selection by the Table Source's own IDs.
   */
  onSelectionChange?(change: SelectionChange<NoInfer<RowId>, NoInfer<ColId>>): void;
  /** What the body says when an axis has no items, unless the source's `emptyMessage` says otherwise. Default "No rows". */
  emptyMessage?: string;
  /**
   * Called when the Table Source fails a request (a rejected Promise, or a
   * throw), with what it threw and which request it was. The cells of a
   * failed `window` or `cells` request show an error marker; the app retries
   * by sending a Change Notice (a `Reset`, or `ContentChanged` over them)
   * through the source's sink. Without `onError` failures are logged.
   */
  onError?(error: unknown, request: FailedRequest<NoInfer<RowId>, NoInfer<ColId>>): void;
  /** Called after every frame the widget runs. */
  onFrame?(frame: FrameInfo<NoInfer<RowId>, NoInfer<ColId>>): void;
  /**
   * What a click or a focus move selects besides the focused cell: nothing
   * (`"cells"`, the default) or its whole row (`"rows"`, for list-style views).
   */
  selectionUnit?: "cells" | "rows";
}

export interface FrameInfo<RowId = unknown, ColId = unknown> {
  /** Whether the frame painted. */
  painted: boolean;
  /**
   * The rows in view: the first and last visible Positions, the count, and
   * the count before any filter as the Table Source last gave it (`null` if
   * it gave none).
   */
  rows: { first: number; last: number; count: number; total: number | null };
  cols: { first: number; last: number; count: number; total: number | null };
  /** Whether the rows take a quick filter (the Table Source's `rows.quickFilter`), and with regexes; `null` if not. */
  quickFilter: { regex: boolean } | null;
  /** The focused cell's Positions, if any. */
  focus: { row: number; col: number } | null;
  /** The selection range as half-open Position spans, if any. */
  selection: { rows: Span; cols: Span } | null;
  /** How many rows are picked. */
  picked: number;
  /** The focused cell by the Table Source's own IDs, if any. */
  focusIds: { row: RowId; col: ColId } | null;
  /** Whether a cell in view still waits for the Table Source: its row or column, or its value. */
  loading: boolean;
  /** Whether the painter asked for another frame to animate a pending state. */
  animating: boolean;
  /** The cell being edited, by the Table Source's own IDs, if an editor is open. */
  editing: { row: RowId; col: ColId } | null;
  /**
   * The rows' column conditions as a filter's chips list them: the latest
   * request's, each applied or still being added, then the applied ones
   * being removed. Pending ones (`adding`, `removing`) are not applied yet.
   */
  conditions: { condition: Condition<ColId>; state: ConditionState }[];
  /** The column headers in view, by the Table Source's IDs: how each is filtered, and what it supports. */
  columns: ColumnHeader<ColId>[];
}

/** A column header in view, as `FrameInfo.columns` lists it. */
export interface ColumnHeader<ColId = unknown> {
  id: ColId;
  label: string;
  /** Whether an applied condition reads this column, and whether a request would change them. */
  filter: { filtered: boolean; pending: boolean };
  support: ColumnSupport;
}

/** A listed column, as `GridWidget.columns` gives them. */
export interface ColumnInfo<ColId = unknown> {
  id: ColId;
  label: string;
  support: ColumnSupport;
}

const HEADER_HEIGHT = 28;
// The grid's surfaces are not text: WebKit would tint a selected canvas. Custom
// editors and the ARIA table opt back in; the textarea is editable, so it stays selectable.
const ROOT_STYLE =
  "position: relative; width: 100%; height: 100%; overflow: hidden; user-select: none; -webkit-user-select: none;";
const SCROLLER_STYLE = "position: absolute; inset: 0; overflow: auto; overscroll-behavior: none;";
const SIZER_STYLE = "position: relative;";
const LAYER_STYLE = "position: sticky; top: 0; left: 0; overflow: hidden;";

export class GridWidget<RowId = unknown, ColId = unknown, V = unknown> {
  readonly element: HTMLElement;
  private readonly scroller: HTMLDivElement;
  private readonly sizer: HTMLDivElement;
  private readonly layer: HTMLDivElement;
  private readonly painter: CanvasPainter;
  private readonly parts: FrameParts;
  private readonly scroll: ScrollMapping;
  private readonly feed: Feed<RowId, ColId>;
  private readonly grid: WebGrid;
  private readonly input: Input;
  private readonly aria: Aria;
  private readonly sizes: InitialSizes | null;
  private last: Snapshot | null = null;
  private readonly selectionEvents: (s: Snapshot) => void;
  private readonly cleanup: (() => void)[] = [];
  private raf = 0;
  private force = true;
  /** Paint next frame without Damage: the last paint asked, or a flash was marked. */
  private repaint = false;
  private dpr = 1;
  private size = { width: 0, height: 0 };
  private destroyed = false;
  private themeChoice: ThemeChoice;
  private empty: string;
  private readonly dark = matchMedia("(prefers-color-scheme: dark)");
  private readonly still = matchMedia("(prefers-reduced-motion: reduce)");
  /** The `focus` call waiting for focus to land. */
  private focusing: Focusing | null = null;
  /** Waiting for the next frame to run. */
  private framed: (() => void)[] = [];
  private readonly listeners = new Set<(frame: FrameInfo<RowId, ColId>) => void>();
  /** The mounted column menu's opener, if one is mounted. */
  private columnMenu: ((colId: ColId, anchor: DOMRect) => void) | null = null;

  /** Loads the wasm module if needed, then mounts a grid into `container`. */
  static async create<RowId, ColId, V>(
    container: HTMLElement,
    options: GridOptions<RowId, ColId, V>,
  ): Promise<GridWidget<RowId, ColId, V>> {
    await loadBinnacle();
    return new GridWidget(container, options);
  }

  private constructor(
    container: HTMLElement,
    private readonly options: GridOptions<RowId, ColId, V>,
  ) {
    this.element = div("binnacle", ROOT_STYLE);
    this.scroller = div("binnacle-scroller", SCROLLER_STYLE);
    // The textarea is the widget's one Tab stop, not the scroller.
    this.scroller.tabIndex = -1;
    this.sizer = div("binnacle-sizer", SIZER_STYLE);
    this.layer = div("binnacle-layer", LAYER_STYLE);
    this.themeChoice = options.theme ?? systemTheme;
    this.empty = options.emptyMessage ?? "No rows";
    this.painter = createCanvasPainter(this.resolveTheme());
    this.painter.border = options.border ?? headersBorder;
    const renderer = options.renderer ?? options.source.renderer;
    if (renderer) this.painter.renderer = renderer as RendererChoice<unknown, unknown, unknown>;
    this.painter.motion = !this.still.matches;
    this.painter.element.setAttribute("aria-hidden", "true");
    this.layer.append(this.painter.element);
    this.sizer.append(this.layer);
    this.scroller.append(this.sizer);
    this.element.append(this.scroller);
    container.append(this.element);

    this.feed = createFeed(options.source, {
      schedule: () => this.schedule(),
      ...(options.onError ? { failed: (error, request) => options.onError?.(error, request) } : {}),
      located: (axis, key, answer) => this.located(axis, key, answer),
      windowed: (axis, keys) => this.sizes?.arrived(axis, keys),
    });
    this.grid = this.createGrid();
    if (options.selectionUnit) this.grid.set_selection_unit(SELECTION_UNIT[options.selectionUnit]);
    this.sizes = this.createSizes();
    this.selectionEvents = createSelectionEvents(this.feed, (change) => this.options.onSelectionChange?.(change));
    this.feed.connect(this.grid);
    const { scroller, sizer } = this;
    const port: ScrollPort = {
      get scrollTop() {
        return scroller.scrollTop;
      },
      set scrollTop(v) {
        scroller.scrollTop = v;
      },
      get scrollLeft() {
        return scroller.scrollLeft;
      },
      set scrollLeft(v) {
        scroller.scrollLeft = v;
      },
      get clientWidth() {
        return scroller.clientWidth;
      },
      get clientHeight() {
        return scroller.clientHeight;
      },
      setSize(width, height) {
        sizer.style.width = `${width}px`;
        sizer.style.height = `${height}px`;
      },
    };
    this.scroll = createScrollMapping(port);
    this.parts = {
      grid: this.grid,
      scroll: this.scroll,
      feed: this.feed,
      painter: this.painter,
      viewport: () => this.size,
      onEffect: (e) => this.dispatch(e),
      settle: (s) => this.sizes?.settle(s, this.feed.content(s)) ?? false,
    };

    this.input = createInput({
      grid: this.grid,
      layer: this.layer,
      keymap: createKeymap({
        ...(options.keymap ? { bindings: options.keymap } : {}),
        ...(options.platform ? { platform: options.platform } : {}),
      }),
      changed: () => {
        drainEffects(this.parts);
        this.schedule();
      },
      focusChanged: () => {
        this.painter.focused = this.input.focused;
        this.invalidate();
      },
      snapshot: () => this.last,
      menuButtonAt: (x, y) => this.menuButtonAt(x, y) !== undefined,
      menuPress: (x, y) => {
        const id = this.menuButtonAt(x, y);
        return id !== undefined && this.openColumnMenu(id);
      },
      openColumnMenu: () => {
        const key = this.last?.cols.focus?.key;
        const id = key == null ? undefined : this.feed.colId(key);
        return id !== undefined && this.openColumnMenu(id);
      },
      edits: {
        editable: options.onCommit !== undefined,
        value: (row, col) => this.feed.value(row, col),
        text: (row, col) => {
          const value = this.feed.value(row, col);
          const ids = this.ids(row, col);
          if (value === undefined || !ids) return "";
          const renderer = this.painter.renderer(ids.row, ids.col);
          return (renderer.text ?? plainText.text!)(value);
        },
        editor: (row, col) => {
          const ids = this.ids(row, col);
          return (ids && (options.editor?.(ids.row, ids.col) as Editor | null | undefined)) ?? null;
        },
        commit: (row, col, value) => {
          const ids = this.ids(row, col);
          if (!ids) return;
          if (options.onCommit?.(ids.row, ids.col, value as V | string) === false) return;
          this.feed.edited(row, col, value);
        },
        theme: () => this.painter.theme,
      },
    });
    this.aria = createAria({
      ...(options.label ? { label: options.label } : {}),
      focus: this.input.element,
      editable: options.onCommit !== undefined,
      renderer: () => this.painter.renderer,
    });
    this.element.append(this.aria.element);

    this.listen(this.scroller, "scroll", () => {
      if (this.scroll.take(this.grid)) {
        drainEffects(this.parts);
        this.schedule();
      }
    });
    const resize = new ResizeObserver(() => this.invalidate());
    resize.observe(this.element);
    this.cleanup.push(() => resize.disconnect());
    this.watchDpr();
    this.listen(this.dark, "change", () => {
      if (typeof this.themeChoice === "function") this.theme = this.themeChoice;
    });
    this.listen(this.still, "change", () => {
      this.painter.motion = !this.still.matches;
      this.invalidate();
    });
    const fonts = document.fonts;
    if (fonts) {
      this.listen(fonts, "loadingdone", () => {
        this.painter.measure.clear();
        this.invalidate();
      });
    }
    this.schedule();
  }

  /** Stops the frame loop, frees the wasm Grid and removes the widget's elements. Safe to call twice. */
  destroy(): void {
    if (this.destroyed) return;
    this.destroyed = true;
    cancelAnimationFrame(this.raf);
    for (const undo of this.cleanup) undo();
    this.input.destroy();
    this.aria.destroy();
    this.feed.destroy();
    this.listeners.clear();
    this.painter.destroy?.();
    this.grid.free();
    this.element.remove();
    this.focusing?.reject(new Error("binnacle: the grid was destroyed"));
    this.focusing = null;
    for (const waiting of this.framed.splice(0)) waiting();
  }

  /**
   * Moves focus to the cell at `rowId` and `colId` (by default the focused
   * column, else the first one in view), selecting as a focus move does, and
   * scrolls it into view. A row or column outside the Window is found with
   * the Table Source's `locate`. Settles after the frame that shows focus
   * there; rejected if the Table Source no longer has the row or column, or
   * if focus moves on first (another `focus` call, a key press).
   */
  async focus(rowId: RowId, colId?: ColId): Promise<void> {
    await this.started();
    const col = colId === undefined ? this.focusColumn() : this.feed.colKey(colId);
    if (col === null) throw new Error("binnacle: there is no column to focus");
    const row = this.feed.rowKey(rowId);
    const name = colId === undefined ? String(rowId) : `${String(rowId)} / ${String(colId)}`;
    this.focusing?.reject(new Error(`binnacle: focus moved on before it reached ${this.focusing.name}`));
    return new Promise((resolve, reject) => {
      this.focusing = { keys: { rows: row, cols: col }, name, gone: false, resolve, reject };
      this.grid.set_focus(row, col);
      drainEffects(this.parts);
      this.schedule();
    });
  }

  /**
   * Scrolls the row with `rowId`, and the column with `colId` if given, into
   * view without moving focus; one outside the Window is found with the Table
   * Source's `locate`. Settles after the frame that shows it; rejected if the
   * Table Source no longer has it.
   */
  async reveal(rowId: RowId, colId?: ColId): Promise<void> {
    await this.started();
    const wanted: [AxisName, bigint, unknown][] = [["rows", this.feed.rowKey(rowId), rowId]];
    if (colId !== undefined) wanted.push(["cols", this.feed.colKey(colId), colId]);
    const found = await Promise.all(wanted.map(([axis, key]) => this.feed.find(axis, key)));
    if (this.destroyed) throw new Error("binnacle: the grid was destroyed");
    wanted.forEach(([axis, , id], i) => {
      const at = found[i]!;
      if (at.type === "Gone") throw new Error(`binnacle: ${String(id)} is gone from the Table Source`);
      this.grid.reveal(AXIS_CODE[axis], at.position);
    });
    drainEffects(this.parts);
    await this.nextFrame();
  }

  /**
   * Scrolls so the row at Position `row`, and the column at `col` if given,
   * starts the body, or as near as the end allows. Focus does not move.
   * Throws a RangeError for a Position that is not a whole number from 0.
   */
  scrollTo(row: number, col?: number): void {
    const asked: [AxisName, number | undefined][] = [
      ["rows", row],
      ["cols", col],
    ];
    for (const [, p] of asked) {
      if (p !== undefined && !(Number.isSafeInteger(p) && p >= 0)) throw new RangeError(`binnacle: ${p} is not a Position`);
    }
    if (this.destroyed) return;
    for (const [axis, p] of asked) if (p !== undefined) this.grid.scroll_to_item(AXIS_CODE[axis], p);
    drainEffects(this.parts);
    this.schedule();
  }

  /**
   * Flashes the cell at `rowId` and `colId` (or the whole row, with no
   * column) in the Theme's flash style, fading over 800 ms: call it when the
   * host sees a value change.
   */
  flash(rowId: RowId, colId?: ColId): void {
    if (this.destroyed) return;
    const col = colId === undefined ? null : this.feed.colKey(colId);
    this.painter.flashes.mark(this.feed.rowKey(rowId), col, performance.now());
    this.repaint = true;
    this.schedule();
  }

  /**
   * Sets the rows' quick filter: keep the rows with `query`'s text in a
   * searched column. `null` or empty text clears it. The grid asks the Table
   * Source for the View Spec in force with this quick filter; setting the
   * one already asked for does nothing.
   */
  setQuickFilter(query: TextQuery<ColId> | null): void {
    if (this.destroyed) return;
    const keyed = query && this.feed.viewKeys({ rows: { filter: { quick: query }, sort: [] }, cols: { sort: [] } });
    this.grid.set_quick_filter(AXIS_CODE.rows, keyed ? keyed.rows.filter.quick : null);
    drainEffects(this.parts);
    this.schedule();
  }

  /**
   * Adds a condition on the rows: keep those whose value in the condition's
   * column matches (or with `not`, does not). A Text condition with no text
   * asks for nothing. The grid asks the Table Source for the View Spec in
   * force with it.
   */
  addCondition(condition: Condition<ColId>): void {
    this.editView(() => this.grid.add_condition(AXIS_CODE.rows, this.feed.conditionKey(condition)));
  }

  /** Removes the first rows condition equal to `condition`, if the View Spec in force (or asked for) has one. */
  removeCondition(condition: Condition<ColId>): void {
    this.editView(() => this.grid.remove_condition(AXIS_CODE.rows, this.feed.conditionKey(condition)));
  }

  /**
   * Puts `to` in place of the first rows condition equal to `from`: adds `to`
   * if there is none, and removes `from` if `to` asks for nothing.
   */
  replaceCondition(from: Condition<ColId>, to: Condition<ColId>): void {
    this.editView(() =>
      this.grid.replace_condition(AXIS_CODE.rows, this.feed.conditionKey(from), this.feed.conditionKey(to)),
    );
  }

  /** Removes every rows condition on column `colId`. */
  clearConditions(colId: ColId): void {
    const listed = this.last?.rows.conditions ?? [];
    const key = this.feed.colKey(colId);
    this.editView(() => {
      for (const { condition, state } of listed) {
        if (state !== "removing" && condition.key === key) this.grid.remove_condition(AXIS_CODE.rows, condition);
      }
    });
  }

  private editView(edit: () => void): void {
    if (this.destroyed) return;
    edit();
    drainEffects(this.parts);
    this.schedule();
  }

  /**
   * Mounts a column menu (see `createColumnMenu`): filterable column headers
   * draw its button, and a press on one, Alt+ArrowDown, or `openColumnMenu`
   * calls `open` with the column and a rectangle to place the menu under.
   * Returns how to unmount it; one menu is mounted at a time.
   */
  attachColumnMenu(open: (colId: ColId, anchor: DOMRect) => void): () => void {
    this.columnMenu = open;
    this.painter.filterButtons = true;
    this.invalidate();
    return () => {
      if (this.columnMenu !== open) return;
      this.columnMenu = null;
      this.painter.filterButtons = false;
      this.invalidate();
    };
  }

  /**
   * Opens the mounted column menu for `colId`, under its header if it is in
   * view, else under `anchor` (or the grid's top left). `false` if no menu is
   * mounted or the column cannot be filtered.
   */
  openColumnMenu(colId: ColId, anchor?: DOMRect): boolean {
    const open = this.columnMenu;
    if (!open || this.destroyed || !this.feed.columnSupport(colId).text) return false;
    const key = this.feed.colKey(colId);
    const header = this.last?.cols.headers.find((h) => h.key === key && this.inView(h));
    open(colId, header ? this.clientRect(header.rect) : (anchor ?? this.clientRect({ x: 0, y: 0, width: 0, height: 0 })));
    return true;
  }

  /** The columns of the table, up to `limit` (1,000 by default), with their labels and support, asked of the Table Source. */
  async columns(limit = 1000): Promise<ColumnInfo<ColId>[]> {
    const cols = this.options.source.cols;
    const count = await cols.count();
    const ids = await cols.window(0, Math.min(count, limit), new AbortController().signal);
    return ids.map((id) => ({ id, label: this.columnLabel(id), support: this.feed.columnSupport(id) }));
  }

  /** How the rows may be viewed by column `colId`: whether it sorts, and takes Text conditions (and regexes). */
  columnSupport(colId: ColId): ColumnSupport {
    return this.feed.columnSupport(colId);
  }

  /** A column's header text: the source's `cols.label` (given the column's Position if in view, else NaN), else its ID. */
  columnLabel(colId: ColId): string {
    const key = this.feed.colKey(colId);
    const position = this.last?.cols.strip.find((i) => i.key === key)?.position ?? Number.NaN;
    return this.options.source.cols.label?.(colId, position) ?? String(colId);
  }

  /** The column whose menu button is at (x, y), if a menu is mounted and there is one. */
  private menuButtonAt(x: number, y: number): ColId | undefined {
    const s = this.last;
    if (!this.columnMenu || !s || y > s.rows.header) return undefined;
    for (const h of s.cols.headers) {
      if (!hasFilterButton(true, h) || !this.inView(h)) continue;
      const r = filterButtonRect(h);
      if (x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height) return this.feed.colId(h.key!);
    }
    return undefined;
  }

  private inView(h: HeaderPlacement): boolean {
    const s = this.last;
    return s !== null && h.rect.x + h.rect.width > s.cols.header && h.rect.x < s.cols.extent;
  }

  /** A viewport rectangle in client coordinates. */
  private clientRect(r: { x: number; y: number; width: number; height: number }): DOMRect {
    const box = this.layer.getBoundingClientRect();
    return new DOMRect(box.left + r.x, box.top + r.y, r.width, r.height);
  }

  /** Asks the Table Source for the whole of `spec`, such as a saved view, in place of the View Spec in force. */
  requestView(spec: ViewSpec<RowId, ColId>): void {
    if (this.destroyed) return;
    this.grid.request_view(this.feed.viewKeys(spec));
    drainEffects(this.parts);
    this.schedule();
  }

  /** Calls `listener` after every frame, as `onFrame` is; returns how to stop. */
  subscribe(listener: (frame: FrameInfo<RowId, ColId>) => void): () => void {
    this.listeners.add(listener);
    if (this.last) listener(this.frameInfo(this.last, false, false));
    return () => this.listeners.delete(listener);
  }

  /** The Theme the grid draws in, for the current colour scheme when set to one per scheme. */
  get theme(): Theme {
    return this.painter.theme;
  }

  set theme(choice: ThemeChoice) {
    this.themeChoice = choice;
    this.painter.theme = this.resolveTheme();
    this.invalidate();
  }

  /** What the body says when an axis has no items, unless the source's `emptyMessage` says otherwise. */
  get emptyMessage(): string {
    return this.empty;
  }

  set emptyMessage(message: string) {
    this.empty = message;
    this.invalidate();
  }

  /** The lines drawn in the Gaps. Setting it lays the Gaps out to fit. */
  get border(): BorderStyle {
    return this.painter.border;
  }

  set border(style: BorderStyle) {
    this.painter.border = style;
    const gaps = borderGaps(style);
    this.grid.set_gap(AXIS_CODE.rows, gaps.rows);
    this.grid.set_gap(AXIS_CODE.cols, gaps.cols);
    this.invalidate();
  }

  private resolveTheme(): Theme {
    const choice = this.themeChoice;
    if (typeof choice !== "function") return choice;
    const scheme: ColorScheme = this.dark.matches ? "dark" : "light";
    return choice(scheme);
  }

  private createGrid(): WebGrid {
    const b = bridge();
    const counts = this.feed.counts();
    const o = this.options;
    const rowHeight = o.rowHeight ?? 24;
    const theme = this.painter.theme;
    const gaps = borderGaps(this.painter.border);
    // The row-number band fits the widest default label: as many digits as
    // the row count, each as wide as the widest digit.
    const digits = String(counts.rows).length;
    const digit = Math.max(...Array.from("0123456789", (d) => this.painter.measure.width(d, theme.font)));
    const band = Math.ceil(digit * digits) + 2 * PAD;
    const rows = new b.WebAxis(counts.rows, rowHeight).header(o.headerHeight ?? HEADER_HEIGHT).gap(gaps.rows).min_size(rowHeight);
    const cols = new b.WebAxis(counts.cols, o.columnWidth ?? 120).header(Math.max(40, band)).gap(gaps.cols).min_size(24);
    return new b.WebGrid(rows, cols);
  }

  /** Repaints the whole grid next frame (resize, DPR, fonts, theme). */
  private invalidate(): void {
    this.force = true;
    this.schedule();
  }

  private schedule(): void {
    if (!this.raf && !this.destroyed) this.raf = requestAnimationFrame(() => this.frame());
  }

  private frame(): void {
    this.raf = 0;
    const width = this.scroller.clientWidth;
    const height = this.scroller.clientHeight;
    const dpr = window.devicePixelRatio || 1;
    if (this.force || width !== this.size.width || height !== this.size.height || dpr !== this.dpr) {
      this.size = { width, height };
      this.dpr = dpr;
      this.layer.style.width = `${width}px`;
      this.layer.style.height = `${height}px`;
      this.painter.resize(width, height, dpr);
      this.force = true;
    }
    this.painter.empty = this.options.source.emptyMessage?.() ?? this.empty;
    const { snapshot, painted, again } = runFrame(this.parts, this.force || this.repaint);
    this.force = false;
    this.repaint = again;
    this.last = snapshot;
    this.input.place(snapshot);
    this.aria.frame(snapshot, painted, () => this.feed.content(snapshot));
    if (again) this.schedule();
    this.settleFocus(snapshot);
    for (const waiting of this.framed.splice(0)) waiting();
    const info = this.frameInfo(snapshot, painted, again);
    this.options.onFrame?.(info);
    for (const listener of this.listeners) listener(info);
    this.selectionEvents(snapshot);
  }

  private frameInfo(snapshot: Snapshot, painted: boolean, again: boolean): FrameInfo<RowId, ColId> {
    return {
      painted,
      rows: span(snapshot, "rows"),
      cols: span(snapshot, "cols"),
      quickFilter: snapshot.rows.quickFilter,
      ...selection(snapshot),
      focusIds: this.focusIds(snapshot),
      loading: this.feed.loading(snapshot),
      animating: again,
      editing: snapshot.mode.type === "Editing" ? this.ids(snapshot.mode.row, snapshot.mode.col) : null,
      conditions: snapshot.rows.conditions.flatMap(({ condition, state }) => {
        const c = this.feed.conditionId(condition);
        return c ? [{ condition: c, state }] : [];
      }),
      columns: visible(snapshot.cols).flatMap((item) => {
        const id = item.key === null ? undefined : this.feed.colId(item.key);
        const header = snapshot.cols.headers.find((h) => h.position === item.position);
        if (id === undefined || !header) return [];
        return [{ id, label: this.columnLabel(id), filter: header.filter, support: this.feed.columnSupport(id) }];
      }),
    };
  }

  /** Waits for a frame if none has run or one is due, so the Strips are those of what is in view. */
  private async started(): Promise<void> {
    if ((!this.last || this.raf) && !this.destroyed) await this.nextFrame();
    if (this.destroyed) throw new Error("binnacle: the grid was destroyed");
  }

  private nextFrame(): Promise<void> {
    return new Promise((resolve) => {
      this.framed.push(resolve);
      this.schedule();
    });
  }

  /** The focused column's Key, else the first one in view. */
  private focusColumn(): bigint | null {
    const cols = this.last?.cols;
    return cols?.focus?.key ?? (cols ? visible(cols).find((i) => i.key !== null)?.key : null) ?? null;
  }

  private located(axis: AxisName, key: bigint, answer: Located): void {
    if (answer.type === "Gone" && this.focusing?.keys[axis] === key) this.focusing.gone = true;
  }

  /** Settles a waiting `focus` once neither axis is locating: landed, or not. */
  private settleFocus(s: Snapshot): void {
    const f = this.focusing;
    if (!f || s.rows.locating || s.cols.locating) return;
    this.focusing = null;
    if (s.rows.focus?.key === f.keys.rows && s.cols.focus?.key === f.keys.cols) f.resolve();
    else if (f.gone) f.reject(new Error(`binnacle: ${f.name} is gone from the Table Source`));
    else f.reject(new Error(`binnacle: focus moved on before it reached ${f.name}`));
  }

  private createSizes(): InitialSizes | null {
    const { initialColumnWidth: cols, initialRowHeight: rows } = this.options;
    if (!cols && !rows) return null;
    return createInitialSizes(
      {
        ...(cols ? { cols: cols as (id: unknown) => InitialSize } : {}),
        ...(rows ? { rows: rows as (id: unknown) => InitialSize } : {}),
      },
      {
        grid: this.grid,
        id: (axis, key) => (axis === "rows" ? this.feed.rowId(key) : this.feed.colId(key)),
        fit: (s, content, key) => fitWidth(s, content, key, this.sizing(), true),
      },
    );
  }

  private sizing(): SizingParts {
    return {
      grid: this.grid,
      measure: this.painter.measure,
      theme: this.painter.theme,
      headerHeight: this.options.headerHeight ?? HEADER_HEIGHT,
      renderer: this.painter.renderer,
    };
  }

  private dispatch(effect: Effect): void {
    if (effect.type === "Locate") this.aria.locate(effect.axis, effect.key);
    const s = this.last;
    if (this.input.answer(effect)) return;
    if (answerAutoSize(effect, s, s && this.feed.content(s), this.sizing())) return;
    this.feed.answer(effect);
  }

  private focusIds(s: Snapshot): FrameInfo<RowId, ColId>["focusIds"] {
    const row = s.rows.focus?.key;
    const col = s.cols.focus?.key;
    return row == null || col == null ? null : this.ids(row, col);
  }

  /** The Table Source's IDs for a cell's Keys, if both are known. */
  private ids(row: bigint, col: bigint): { row: RowId; col: ColId } | null {
    const rowId = this.feed.rowId(row);
    const colId = this.feed.colId(col);
    return rowId === undefined || colId === undefined ? null : { row: rowId, col: colId };
  }

  private watchDpr(): void {
    const query = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    const changed = () => {
      this.invalidate();
      this.watchDpr();
    };
    query.addEventListener("change", changed, { once: true });
    this.cleanup.push(() => query.removeEventListener("change", changed));
  }

  private listen(target: EventTarget, type: string, handler: () => void): void {
    target.addEventListener(type, handler);
    this.cleanup.push(() => target.removeEventListener(type, handler));
  }
}

interface Focusing {
  keys: Record<AxisName, bigint>;
  /** The IDs asked for, for messages. */
  name: string;
  /** The Table Source answered a Locate for one of the Keys with `Gone`. */
  gone: boolean;
  resolve(): void;
  reject(error: Error): void;
}

/** The Strip's items the viewport shows (the Strip includes overscan). */
function visible(axis: AxisSnapshot) {
  return axis.strip.filter((i) => i.offset + i.size > axis.header && i.offset < axis.extent);
}

function selection(s: Snapshot): Pick<FrameInfo, "focus" | "selection" | "picked"> {
  const { rows, cols } = s;
  return {
    focus: rows.focus && cols.focus ? { row: rows.focus.position, col: cols.focus.position } : null,
    selection: s.selectionRange,
    picked: s.picked.length,
  };
}

function span(s: Snapshot, axis: "rows" | "cols"): FrameInfo["rows"] {
  const strip = s[axis].strip;
  // The Strip includes overscan; report what the viewport shows.
  const start = s[axis].header;
  const end = s[axis].extent;
  const visible = strip.filter((i) => i.offset + i.size > start && i.offset < end);
  return {
    first: visible[0]?.position ?? 0,
    last: visible.at(-1)?.position ?? 0,
    count: s[axis].count,
    total: s[axis].total,
  };
}

function div(className: string, style: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = className;
  el.style.cssText = style;
  return el;
}
