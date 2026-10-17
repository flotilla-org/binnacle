// The framework-neutral widget: mounts into a container, owns the frame loop
// (idle unless something changed), the native scroller and the painter, and
// frees the wasm Grid on destroy.

import { answerAutoSize } from "./autosize";
import { borderGaps, headersBorder, type BorderStyle } from "./border";
import { createCanvasPainter, type CanvasPainter } from "./canvas/painter";
import { PAD } from "./canvas/context";
import type { Effect } from "./effects";
import { createFeed, type Feed } from "./feed";
import { drainEffects, runFrame, type FrameParts } from "./frame";
import { createInput, type Input } from "./input";
import { createKeymap, type KeyBindings, type Platform } from "./keymap";
import { createScrollMapping, type ScrollMapping, type ScrollPort } from "./scroller";
import type { Snapshot, Span } from "./snapshot";
import type { TableSource } from "./source";
import { AXIS_CODE } from "./codes";
import { systemTheme, type ColorScheme, type Theme, type ThemeChoice } from "./theme";
import { bridge, loadBinnacle, type WebGrid } from "./wasm";

export interface GridOptions<V> {
  source: TableSource<number, number, V>;
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
  /** Called after every frame the widget runs. */
  onFrame?(frame: FrameInfo): void;
}

export interface FrameInfo {
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
}

const HEADER_HEIGHT = 28;
const ROOT_STYLE = "position: relative; width: 100%; height: 100%; overflow: hidden;";
const SCROLLER_STYLE = "position: absolute; inset: 0; overflow: auto; overscroll-behavior: none;";
const SIZER_STYLE = "position: relative;";
const LAYER_STYLE = "position: sticky; top: 0; left: 0; overflow: hidden;";

export class GridWidget<V = unknown> {
  readonly element: HTMLElement;
  private readonly scroller: HTMLDivElement;
  private readonly sizer: HTMLDivElement;
  private readonly layer: HTMLDivElement;
  private readonly painter: CanvasPainter;
  private readonly parts: FrameParts;
  private readonly scroll: ScrollMapping;
  private readonly feed: Feed;
  private readonly grid: WebGrid;
  private readonly input: Input;
  private last: Snapshot | null = null;
  private readonly cleanup: (() => void)[] = [];
  private raf = 0;
  private force = true;
  private dpr = 1;
  private size = { width: 0, height: 0 };
  private destroyed = false;
  private themeChoice: ThemeChoice;
  private readonly dark = matchMedia("(prefers-color-scheme: dark)");

  /** Loads the wasm module if needed, then mounts a grid into `container`. */
  static async create<V>(container: HTMLElement, options: GridOptions<V>): Promise<GridWidget<V>> {
    await loadBinnacle();
    return new GridWidget(container, options);
  }

  private constructor(
    container: HTMLElement,
    private readonly options: GridOptions<V>,
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
    this.layer.append(this.painter.element);
    this.sizer.append(this.layer);
    this.scroller.append(this.sizer);
    this.element.append(this.scroller);
    container.append(this.element);

    this.feed = createFeed(options.source);
    this.grid = this.createGrid();
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
    // The row-number band fits the widest default label, the row count.
    const band = Math.ceil(this.painter.measure.width(String(counts.rows), theme.font)) + 2 * PAD;
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
    const { snapshot, painted, again } = runFrame(this.parts, this.force);
    this.force = false;
    this.last = snapshot;
    this.input.place(snapshot);
    if (again) this.schedule();
    this.options.onFrame?.({
      painted,
      rows: span(snapshot, "rows"),
      cols: span(snapshot, "cols"),
      ...selection(snapshot),
    });
  }

  private dispatch(effect: Effect): void {
    const s = this.last;
    const sizing = {
      grid: this.grid,
      measure: this.painter.measure,
      theme: this.painter.theme,
      headerHeight: this.options.headerHeight ?? HEADER_HEIGHT,
    };
    if (answerAutoSize(effect, s, s && this.feed.content(s), sizing)) return;
  }

  private watchDpr(): void {
    const query = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    const changed = () => {
      this.invalidate();
      this.watchDpr();
    this.listen(this.dark, "change", () => {
      if (typeof this.themeChoice === "function") this.theme = this.themeChoice;
    });
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
