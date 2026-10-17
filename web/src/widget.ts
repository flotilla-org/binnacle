// The framework-neutral widget: mounts into a container, owns the frame loop
// (idle unless something changed), the native scroller and the painter, and
// frees the wasm Grid on destroy.

import { answerAutoSize } from "./autosize";
import { borderGaps, headersBorder, type BorderStyle } from "./border";
import type { Editor } from "./editing";
import { createCanvasPainter, type CanvasPainter } from "./canvas/painter";
import { PAD } from "./canvas/context";
import type { Effect } from "./effects";
import { createFeed, type Feed } from "./feed";
import { drainEffects, runFrame, type FrameParts } from "./frame";
import { createInput, type Input } from "./input";
import { createKeymap, type KeyBindings, type Platform } from "./keymap";
import { createScrollMapping, type ScrollMapping, type ScrollPort } from "./scroller";
import { plainText, type RendererChoice } from "./renderer";
import type { Snapshot, Span } from "./snapshot";
import type { TableSource } from "./source";
import { AXIS_CODE } from "./codes";
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
   * answers for it again. Without `onCommit` the grid is read-only.
   */
  onCommit?(rowId: NoInfer<RowId>, colId: NoInfer<ColId>, value: NoInfer<V> | string): void;
  /** A custom editor for a cell, mounted over it in place of the textarea; `null` or `undefined` for the textarea. */
  editor?(rowId: NoInfer<RowId>, colId: NoInfer<ColId>): Editor<NoInfer<V>> | null | undefined;
  /** Called after every frame the widget runs. */
  onFrame?(frame: FrameInfo<NoInfer<RowId>, NoInfer<ColId>>): void;
}

export interface FrameInfo<RowId = unknown, ColId = unknown> {
  /** Whether the frame painted. */
  painted: boolean;
  /** The rows in view: the first and last visible Positions, and the count. */
  rows: { first: number; last: number; count: number };
  cols: { first: number; last: number; count: number };
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
}

const HEADER_HEIGHT = 28;
const ROOT_STYLE = "position: relative; width: 100%; height: 100%; overflow: hidden;";
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
  private last: Snapshot | null = null;
  private readonly cleanup: (() => void)[] = [];
  private raf = 0;
  private force = true;
  /** Paint next frame without Damage: the last paint asked, or a flash was marked. */
  private repaint = false;
  private dpr = 1;
  private size = { width: 0, height: 0 };
  private destroyed = false;
  private themeChoice: ThemeChoice;
  private readonly dark = matchMedia("(prefers-color-scheme: dark)");
  private readonly still = matchMedia("(prefers-reduced-motion: reduce)");

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
    this.painter = createCanvasPainter(this.resolveTheme());
    this.painter.border = options.border ?? headersBorder;
    const renderer = options.renderer ?? options.source.renderer;
    if (renderer) this.painter.renderer = renderer as RendererChoice<unknown, unknown, unknown>;
    this.painter.motion = !this.still.matches;
    this.layer.append(this.painter.element);
    this.sizer.append(this.layer);
    this.scroller.append(this.sizer);
    this.element.append(this.scroller);
    container.append(this.element);

    this.feed = createFeed(options.source, { schedule: () => this.schedule() });
    this.grid = this.createGrid();
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
          this.feed.edited(row, col, value);
          options.onCommit?.(ids.row, ids.col, value as V | string);
        },
        theme: () => this.painter.theme,
      },
    });

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
    this.feed.destroy();
    this.painter.destroy?.();
    this.grid.free();
    this.element.remove();
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

  /** The Theme the grid draws in, for the current colour scheme when set to one per scheme. */
  get theme(): Theme {
    return this.painter.theme;
  }

  set theme(choice: ThemeChoice) {
    this.themeChoice = choice;
    this.painter.theme = this.resolveTheme();
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
    const { snapshot, painted, again } = runFrame(this.parts, this.force || this.repaint);
    this.force = false;
    this.repaint = again;
    this.last = snapshot;
    this.input.place(snapshot);
    if (again) this.schedule();
    this.options.onFrame?.({
      painted,
      rows: span(snapshot, "rows"),
      cols: span(snapshot, "cols"),
      ...selection(snapshot),
      focusIds: this.focusIds(snapshot),
      loading: this.feed.loading(snapshot),
      animating: again,
      editing: snapshot.mode.type === "Editing" ? this.ids(snapshot.mode.row, snapshot.mode.col) : null,
    });
  }

  private dispatch(effect: Effect): void {
    const s = this.last;
    const sizing = {
      grid: this.grid,
      measure: this.painter.measure,
      theme: this.painter.theme,
      headerHeight: this.options.headerHeight ?? HEADER_HEIGHT,
      renderer: this.painter.renderer,
    };
    if (this.input.answer(effect)) return;
    if (answerAutoSize(effect, s, s && this.feed.content(s), sizing)) return;
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
  return { first: visible[0]?.position ?? 0, last: visible.at(-1)?.position ?? 0, count: s[axis].count };
}

function div(className: string, style: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = className;
  el.style.cssText = style;
  return el;
}
