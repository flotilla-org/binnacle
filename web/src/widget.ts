// The framework-neutral widget: mounts into a container, owns the frame loop
// (idle unless something changed), the native scroller and the painter, and
// frees the wasm Grid on destroy.

import { createAria, type Aria } from "./aria";
import { answerAutoSize, bandWidth, fitWidth, type SizingParts } from "./autosize";
import { borderGaps, headersBorder, type BorderStyle } from "./border";
import type { Editor } from "./editing";
import { createCanvasPainter, type CanvasPainter } from "./canvas/painter";
import { PAD } from "./canvas/context";
import { gatherCopy, writeClipboard, type Copied } from "./copy";
import type { Effect, Run } from "./effects";
import { disclosureRect, filterButtonRect, hasFilterButton } from "./canvas/bands";
import { createFeed, windowIds, type ColumnSupport, type CrossId, type Feed, type FindState, type ItemId } from "./feed";
import { drainEffects, runFrame, type FrameParts } from "./frame";
import { createInput, type Input } from "./input";
import { createKeymap, type KeyBindings, type Platform } from "./keymap";
import { createScrollMapping, type ScrollMapping, type ScrollPort } from "./scroller";
import { plainText, type RendererChoice } from "./renderer";
import { createSelectionEvents, type SelectionChange } from "./selection";
import { createInitialSizes, type InitialSize, type InitialSizes } from "./sizes";
import type { Content } from "./content";
import {
  shownItems,
  stripOf,
  type AxisSnapshot,
  type ConditionState,
  type HeaderPlacement,
  type KeyedCondition,
  type KeyedTextQuery,
  type Snapshot,
  type Span,
} from "./snapshot";
import type {
  AggregateValue,
  AppliedAxisView,
  Condition,
  Distinct,
  FailedRequest,
  FindMatch,
  FindQuery,
  Level,
  Located,
  TableSource,
  TextQuery,
  ViewSpec,
} from "./source";
import { summaryLabel } from "./source";
import { AXES, AXIS_CODE, EDGE_CODE, HEADER_CLICK, SELECTION_UNIT, type AxisName, type PinEdge } from "./codes";
import { readLayout, restoreLayout, type Layout, type LayoutParts } from "./layout";
import { freezeFocused, pinFocused, pinnedKeys } from "./pin";
import {
  capHint,
  chosen,
  chosenCount,
  chosenHas,
  gaps,
  hiddenKeys,
  othersCount,
  overCap,
  type Chosen,
  type HideOutcome,
} from "./choose";
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
   * Called when the user activates a cell (Enter, or a double-click). On a
   * Summary Item's cell `drillDown` holds the conditions that pick out the
   * base rows under it (the slicers, then one OneOf per group on its path),
   * for the app to open, show beside the grid or ignore; such a cell never
   * starts an edit. Returning `true` keeps any other cell from starting one.
   */
  onActivate?(activation: Activation<NoInfer<RowId>, NoInfer<ColId>>): boolean | void;
  /**
   * Called when the user copies (Mod+C), with the cells gathered as the grid
   * shows them and their text. Return other text to copy that instead (only
   * Leaves, say, or the path as columns), or `false` to copy nothing; by
   * default the text goes on the clipboard.
   */
  onCopy?(copied: Copied<NoInfer<RowId>, NoInfer<ColId>>): string | false | void;
  /**
   * How long the fields that write a Text Query (the filter bar's quick
   * filter, the find bar, the column menu's contains field) wait after the
   * last key typed before asking the Table Source, in ms; 150 by default.
   * Enter, toggles, ticks and steps ask at once.
   */
  typingPause?: number;
  /**
   * What a click or a focus move selects besides the focused cell: nothing
   * (`"cells"`, the default), its whole row (`"rows"`, for list-style views)
   * or its whole column (`"cols"`, for column-major views). Ctrl or Cmd with
   * a click picks the cell's column under `"cols"`, else its row.
   */
  selectionUnit?: "cells" | "rows" | "cols";
  /**
   * What a click on a header does, per axis: `"sort"` sorts the other axis by
   * the item (Shift adds it to the sort), `"select"` selects its whole row or
   * column, `"none"` does nothing. Column headers sort and row headers
   * select by default; a column-major table swaps them.
   */
  headerClick?: { rows?: HeaderClick; cols?: HeaderClick };
}

/** An activated cell, as `GridOptions.onActivate` gets it. */
export interface Activation<RowId, ColId> {
  rowId: RowId;
  colId: ColId;
  /** On a Summary Item's cell, the conditions on the base fields that pick out the rows under it; `null` elsewhere. */
  drillDown: Condition<ColId>[] | null;
}

/** What a click on a header does: see `GridOptions.headerClick`. */
export type HeaderClick = keyof typeof HEADER_CLICK;

export interface FrameInfo<RowId = unknown, ColId = unknown> {
  /** Whether the frame painted. */
  painted: boolean;
  /** The rows in view. */
  rows: AxisInfo;
  cols: AxisInfo;
  /** Whether the rows take a quick filter (the Table Source's `rows.quickFilter`), and with regexes; `null` if not. */
  quickFilter: { regex: boolean } | null;
  /** The columns' filter, by rows, as `quickFilter`, `quick`, `quickState` and `conditions` give the rows'. */
  columnsFilter: AxisFilterInfo<RowId>;
  /** The rows' quick filter as last asked for: the request still unanswered (or not applied), else the one applied. */
  quick: TextQuery<ColId> | null;
  /**
   * Where `quick` stands: applied, being set (`adding`) or cleared
   * (`removing`) while the answer is due, or `notApplied` when the Table
   * Source answered without it (an invalid regex, say); `null` with no quick
   * filter asked for or applied.
   */
  quickState: ConditionState | null;
  /** Whether a View Spec request is unanswered: the count says "filtering…" while it changes the rows' filter. */
  viewPending: boolean;
  /** The Table Source's message for the part of the View Spec it did not apply, if any; shown on that part's control. */
  viewError: string | null;
  /** The focused cell's Positions, if any. */
  focus: { row: number; col: number } | null;
  /**
   * The selection range, if any, as each axis's Positions in sorted
   * half-open runs apart from each other (several when items are pinned).
   */
  selection: { rows: Span[]; cols: Span[] } | null;
  /** How many rows and columns are picked. */
  picked: { rows: number; cols: number };
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
  /** The grouping as asked for, each part where it stands, listed as `conditions` is. */
  grouping: GroupingInfo<ColId>;
  /** The column headers in view, by the Table Source's IDs: how each is filtered, and what it supports. */
  columns: ColumnHeader<ColId>[];
  /** Find: whether the Table Source finds, the Find Query in force, the current match, and what the source said of them. */
  find: FindInfo<RowId, ColId>;
  /**
   * The last hide refused because the axis's hidden items would pass its
   * cap, with the hint to show ("Can't hide over 1,000 rows: use a filter
   * instead"), until the next key or press; `null` otherwise.
   */
  hideCapped: { axis: AxisName; cap: number; hint: string } | null;
}

/**
 * The grouping as `FrameInfo` gives it: the rows' levels, the values and the
 * slicers, each the latest request's then the applied ones it drops. A part
 * the Table Source left out (a level on a column it can't group by, a
 * Subtotal it can't place, an aggregate it does not offer, a slicer it can't
 * apply) is `notApplied`; restoring a layout never fails on one.
 */
export interface GroupingInfo<ColId = unknown> {
  levels: { level: Level<ColId>; state: ConditionState }[];
  values: { value: AggregateValue<ColId>; state: ConditionState }[];
  slicers: { condition: Condition<ColId>; state: ConditionState }[];
}

/** One axis's filter as `FrameInfo` gives it, naming items of the other axis. */
export interface AxisFilterInfo<CrossId = unknown> {
  /** Whether the axis takes a quick filter, and with regexes; `null` if not. */
  quickFilter: { regex: boolean } | null;
  /** The quick filter as last asked for: the request still unanswered (or not applied), else the one applied. */
  quick: TextQuery<CrossId> | null;
  /** Where `quick` stands, as `FrameInfo.quickState` says of the rows'. */
  quickState: ConditionState | null;
  /** The axis's conditions as a filter's chips list them, as `FrameInfo.conditions` lists the rows'. */
  conditions: { condition: Condition<CrossId>; state: ConditionState }[];
}

/**
 * An axis in view, as `FrameInfo` gives it: the first and last visible
 * Positions, the count, the count before any filter as the Table Source last
 * gave it (`null` if it gave none), and how many visible items are unmatched
 * (kept after the user's edit though the filter drops them).
 */
export interface AxisInfo {
  first: number;
  last: number;
  count: number;
  total: number | null;
  unmatched: number;
  /** How many items are hidden, as last asked for (the Table Source's answer prunes those the data lacks). */
  hidden: number;
}

/** Where a mounted menu opens: at its top, or at the list of the axis's hidden items. */
export type MenuAt = "top" | "hidden";

/** Find as `FrameInfo.find` gives it. */
export interface FindInfo<RowId = unknown, ColId = unknown> extends FindState {
  /** Whether the Table Source finds, and with regexes; `null` if it does not. */
  support: { regex: boolean } | null;
  /** The Find Query in force; `null` while find is closed. */
  query: FindQuery<ColId> | null;
  /** The match the last step landed on, until its row or column leaves. */
  current: FindMatch<RowId, ColId> | null;
}

/** A column header in view, as `FrameInfo.columns` lists it. */
export interface ColumnHeader<ColId = unknown> {
  id: ColId;
  label: string;
  /** Whether an applied condition reads this column, whether a request would change them, and whether one asked for is not applied. */
  filter: { filtered: boolean; pending: boolean; notApplied: boolean };
  support: ColumnSupport;
}

/** A listed column, as `GridWidget.columns` gives them. */
export interface ColumnInfo<ColId = unknown> {
  id: ColId;
  label: string;
  support: ColumnSupport;
}

const HEADER_HEIGHT = 28;

/** How far in from each end a dragged header pins into an empty pinned Strip, in CSS px. */
const PIN_ZONE = { rows: 4, cols: 16 } as const;

/** The least length, CSS px, the pinned Strips leave the scrolling ones. */
const MIN_SCROLLING = 50;
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
  /** Whether the row-number band has been fitted to the first rows' labels. */
  private bandFitted = false;
  private last: Snapshot | null = null;
  private readonly selectionEvents: (s: Snapshot) => void;
  private readonly cleanup: (() => void)[] = [];
  private raf = 0;
  /** A frame is running, so its Snapshot is the latest. */
  private framing = false;
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
  private columnMenu: ((colId: ColId, anchor: DOMRect, at: MenuAt) => void) | null = null;
  /** The mounted row menu's opener, if one is mounted. */
  private rowMenu: ((rowId: RowId, anchor: DOMRect, at: MenuAt) => void) | null = null;
  /** The mounted find bar's opener, if one is mounted. */
  private findBar: (() => void) | null = null;
  private chooser: ((axis: AxisName, anchor?: DOMRect) => void) | null = null;
  /** The last hide refused past its axis's cap, until the next input. */
  private capped: { axis: AxisName; cap: number } | null = null;

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
    for (const axis of AXES) {
      const click = options.headerClick?.[axis];
      if (click) this.grid.set_header_click(AXIS_CODE[axis], HEADER_CLICK[click]);
    }
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
      settle: (s) => {
        const content = this.feed.content(s);
        const band = this.fitBand(s, content);
        return (this.sizes?.settle(s, content) ?? false) || band;
      },
    };

    this.input = createInput({
      grid: this.grid,
      layer: this.layer,
      keymap: createKeymap({
        ...(options.keymap ? { bindings: options.keymap } : {}),
        ...(options.platform ? { platform: options.platform } : {}),
      }),
      changed: () => {
        // Any input after a refused hide puts its hint away.
        if (this.capped) {
          this.capped = null;
          this.invalidate();
        }
        drainEffects(this.parts);
        this.schedule();
      },
      catchUp: () => this.catchUp(),
      focusChanged: () => {
        this.painter.focused = this.input.focused;
        this.invalidate();
      },
      snapshot: () => this.last,
      menuButtonAt: (x, y) => this.menuButtonAt(x, y) !== undefined,
      markPress: (x, y) => this.markPress(x, y),
      menuPress: (x, y) => {
        const at = this.menuButtonAt(x, y);
        if (!at) return false;
        return at.axis === "cols" ? this.openColumnMenu(at.id as ColId) : this.openRowMenu(at.id as RowId);
      },
      openColumnMenu: () => {
        if (!this.columnMenu) return false;
        // After the next frame, so a focus move just made has landed.
        this.framed.push(() => {
          const key = this.last?.cols.focus?.key;
          const id = key == null ? undefined : this.feed.colId(key);
          if (id !== undefined) this.openColumnMenu(id);
        });
        this.invalidate();
        return true;
      },
      openRowMenu: () => {
        if (!this.rowMenu) return false;
        this.framed.push(() => {
          const key = this.last?.rows.focus?.key;
          const id = key == null ? undefined : this.feed.rowId(key);
          if (id !== undefined) this.openRowMenu(id);
        });
        this.invalidate();
        return true;
      },
      pin: (axis, edge) => {
        // After the next frame, so a focus move just made has landed.
        this.framed.push(() => {
          if (this.last) this.editView(() => pinFocused(this.grid, this.last!, axis, edge));
        });
        this.invalidate();
      },
      hide: (axis) => {
        if (!this.canHide(axis)) return false;
        // After the next frame, so a focus move just made has landed.
        this.framed.push(() => void this.hideChosen(axis));
        this.invalidate();
        return true;
      },
      openChooser: (axis) => {
        if (!this.chooser || !this.canHide(axis)) return false;
        this.framed.push(() => this.openChooser(axis));
        this.invalidate();
        return true;
      },
      showHidden: (axis) => {
        if (!this.canHide(axis) || !(axis === "cols" ? this.columnMenu : this.rowMenu)) return false;
        this.framed.push(() => {
          const key = this.last?.[axis].focus?.key;
          if (key == null) return;
          if (axis === "cols") {
            const id = this.feed.colId(key);
            if (id !== undefined) this.openColumnMenu(id, undefined, "hidden");
          } else {
            const id = this.feed.rowId(key);
            if (id !== undefined) this.openRowMenu(id, undefined, "hidden");
          }
        });
        this.invalidate();
        return true;
      },
      freeze: (axis) => {
        this.framed.push(() => {
          if (this.last) this.editView(() => freezeFocused(this.grid, this.last!, axis));
        });
        this.invalidate();
      },
      find: (action) => {
        if (action === "open") return this.openFind();
        if (!this.last?.find) return false;
        if (action === "next") this.findNext();
        else this.findPrevious();
        return true;
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
   * one already asked for does nothing. With `axis` `"cols"`, sets the
   * columns' instead, searching rows.
   */
  setQuickFilter<A extends AxisName = "rows">(query: TextQuery<CrossId<A, RowId, ColId>> | null, axis?: A): void {
    if (this.destroyed) return;
    const on = axis ?? "rows";
    const part = { filter: { quick: query }, sort: [] };
    const keyed =
      query &&
      this.feed.viewKeys(
        on === "rows"
          ? { rows: part as AppliedAxisView<ColId, RowId>, cols: { sort: [] } }
          : { rows: { sort: [] }, cols: part as AppliedAxisView<RowId, ColId> },
      );
    this.grid.set_quick_filter(AXIS_CODE[on], keyed ? keyed[on].filter.quick : null);
    drainEffects(this.parts);
    this.schedule();
  }

  /**
   * Adds a condition on the rows: keep those whose value in the condition's
   * column matches (or with `not`, does not). A Text condition with no text
   * asks for nothing. The grid asks the Table Source for the View Spec in
   * force with it. With `axis` `"cols"`, a condition on the columns, read
   * in a row.
   */
  addCondition<A extends AxisName = "rows">(condition: Condition<CrossId<A, RowId, ColId>>, axis?: A): void {
    const on = axis ?? "rows";
    this.editView(() => this.grid.add_condition(AXIS_CODE[on], this.feed.conditionKey(condition, axis)));
  }

  /** Removes the first condition on `axis` (the rows by default) equal to `condition`, if the View Spec in force (or asked for) has one. */
  removeCondition<A extends AxisName = "rows">(condition: Condition<CrossId<A, RowId, ColId>>, axis?: A): void {
    const on = axis ?? "rows";
    this.editView(() => this.grid.remove_condition(AXIS_CODE[on], this.feed.conditionKey(condition, axis)));
  }

  /**
   * Puts `to` in place of the first condition on `axis` (the rows by
   * default) equal to `from`: adds `to` if there is none, and removes `from`
   * if `to` asks for nothing.
   */
  replaceCondition<A extends AxisName = "rows">(
    from: Condition<CrossId<A, RowId, ColId>>,
    to: Condition<CrossId<A, RowId, ColId>>,
    axis?: A,
  ): void {
    const on = axis ?? "rows";
    this.editView(() =>
      this.grid.replace_condition(AXIS_CODE[on], this.feed.conditionKey(from, axis), this.feed.conditionKey(to, axis)),
    );
  }

  /** Removes every rows condition on column `id`, or with `axis` `"cols"`, every columns condition on row `id`. */
  clearConditions<A extends AxisName = "rows">(id: CrossId<A, RowId, ColId>, axis?: A): void {
    const on = axis ?? "rows";
    const listed = this.last?.[on].conditions ?? [];
    const key = on === "rows" ? this.feed.colKey(id as ColId) : this.feed.rowKey(id as RowId);
    this.editView(() => {
      for (const { condition, state } of listed) {
        if (state !== "removing" && condition.key === key) this.grid.remove_condition(AXIS_CODE[on], condition);
      }
    });
  }

  /**
   * Adds a slicer: a condition on column `condition.id`, shown or not, that
   * filters the rows before they group and aggregate. A slicer outlives the
   * levels: with slicers and no levels the table is plain, sliced. A Text
   * condition with no text, or a Native one with none, asks for nothing.
   */
  addSlicer(condition: Condition<ColId>): void {
    this.editView(() => this.grid.add_slicer(this.feed.conditionKey(condition)));
  }

  /** Removes the first slicer equal to `condition`; with no slicers and no levels left the table is plain again. */
  removeSlicer(condition: Condition<ColId>): void {
    this.editView(() => this.grid.remove_slicer(this.feed.conditionKey(condition)));
  }

  /** Puts `to` in place of the first slicer equal to `from`, as `replaceCondition` does a condition. */
  replaceSlicer(from: Condition<ColId>, to: Condition<ColId>): void {
    this.editView(() => this.grid.replace_slicer(this.feed.conditionKey(from), this.feed.conditionKey(to)));
  }

  /** Removes every slicer on column `id`. */
  clearSlicers(id: ColId): void {
    const listed = this.last?.slicers ?? [];
    const key = this.feed.colKey(id);
    this.editView(() => {
      for (const { condition, state } of listed) {
        if (state !== "removing" && condition.key === key) this.grid.remove_slicer(condition);
      }
    });
  }

  /**
   * Whether the Table Source groups the rows at all, as of the latest frame:
   * a column in view it groups them by, or a level or slicer asked for or
   * applied. A source that groups nothing gets no grouping controls.
   */
  groups(): boolean {
    const s = this.last;
    if (!s) return false;
    const asked = (s.requestedView ?? s.appliedView).grouping;
    const listed = s.rows.levels.length > 0 || s.slicers.length > 0;
    return asked !== null || listed || s.cols.headers.some((h) => h.support.group);
  }

  /** Whether the Table Source hides items of `axis`: it has `view` and the axis has `hide`. */
  canHide(axis: AxisName): boolean {
    const source = this.options.source;
    return source.view !== undefined && source[axis].hide === true;
  }

  /**
   * Hides the items `ids` of `axis` (the rows by default), beside those
   * hidden already: the grid asks the Table Source to leave them out
   * whatever the filter keeps. Focus, the selection and picks follow as
   * when a filter drops them; their sizes stay. Does nothing unless the axis
   * hides (see `canHide`).
   */
  hide<A extends AxisName = "rows">(ids: ItemId<A, RowId, ColId>[], axis?: A): boolean {
    const on = axis ?? "rows";
    return this.hideKeys(on, this.itemKeys(on, ids)) !== "capped";
  }

  /** The most items `axis` (the rows by default) hides: past it, a hide is refused and the widget suggests a filter. */
  hideCap(axis: AxisName = "rows"): number {
    return this.last?.[axis].hideCap ?? Number.POSITIVE_INFINITY;
  }

  /**
   * Hides the chosen items of `axis` (the rows by default): its picks, else
   * the items the selection spans, else the focused item. With `around`, an
   * item a menu was opened on, only that item unless it is among the
   * chosen. Past the axis's hide cap nothing is hidden, the answer is
   * `"capped"` and `FrameInfo.hideCapped` carries the hint.
   */
  async hideChosen<A extends AxisName = "rows">(axis?: A, around?: ItemId<A, RowId, ColId>): Promise<HideOutcome> {
    const on = axis ?? "rows";
    const c = this.chosenAround(on, around);
    if (!c || !this.canHide(on)) return "nothing";
    if (overCap(this.last!, on, chosenCount(c))) return this.refuseHide(on);
    return this.hideKeys(on, await this.chosenKeys(on, c));
  }

  /**
   * Shows only the chosen items of `axis` (as `hideChosen` chooses them):
   * hides every other item shown at that moment, so items arriving later
   * are shown. Past the axis's hide cap nothing is hidden.
   */
  async showOnly<A extends AxisName = "rows">(axis?: A, around?: ItemId<A, RowId, ColId>): Promise<HideOutcome> {
    const on = axis ?? "rows";
    const c = this.chosenAround(on, around);
    if (!c || !this.canHide(on)) return "nothing";
    if (overCap(this.last!, on, othersCount(this.last!, on, c))) return this.refuseHide(on);
    const count = this.last![on].count;
    // A selection's others lie between its runs; picks are within the cap of all there is.
    const runs = c.type === "runs" ? gaps(c.runs, count) : [{ start: 0, end: count }];
    const keep = new Set(c.type === "keys" ? c.keys : []);
    const others = await this.chosenKeys(on, { type: "runs", runs });
    return this.hideKeys(on, BigUint64Array.from(others.filter((k) => !keep.has(k))));
  }

  /**
   * The items of `axis` the Table Source shows, in its order, up to `limit`,
   * then every hidden one: what a chooser lists. `partial` if more were shown.
   */
  async items<A extends AxisName>(axis: A, limit = 1000): Promise<{ ids: ItemId<A, RowId, ColId>[]; partial: boolean }> {
    const source = this.options.source[axis];
    const count = await source.count();
    const window = await source.window(0, Math.min(count, limit), new AbortController().signal);
    const shown = windowIds<unknown>(window) as ItemId<A, RowId, ColId>[];
    const hidden = this.hidden(axis).filter((id) => !shown.includes(id));
    return { ids: [...shown, ...hidden], partial: count > limit };
  }

  /** How many items `hideChosen` (with `around`) reaches on `axis`, as of the latest frame: 0 for none. */
  chosenCount<A extends AxisName = "rows">(axis?: A, around?: ItemId<A, RowId, ColId>): number {
    const c = this.chosenAround(axis ?? "rows", around);
    return c ? chosenCount(c) : 0;
  }

  /** What the chosen items of `on` are around `around`: the chosen if it is among them, else it alone. */
  private chosenAround(on: AxisName, around: unknown): Chosen | null {
    const s = this.last;
    if (!s) return null;
    const c = chosen(s, on);
    if (around === undefined) return c;
    const key = this.keyOf(on, around as RowId | ColId);
    const position = s[on].strip.find((i) => i.key === key)?.position;
    return c && chosenHas(c, key, position) ? c : { type: "keys", keys: [key] };
  }

  /** The Keys `c` names, asking the Table Source for a span's. */
  private async chosenKeys(on: AxisName, c: Chosen): Promise<bigint[]> {
    if (c.type === "keys") return c.keys;
    const source = this.options.source[on];
    const windows = await Promise.all(c.runs.map((r) => source.window(r.start, r.end, new AbortController().signal)));
    return Array.from(this.itemKeys(on, windows.flatMap((w) => windowIds<unknown>(w))));
  }

  private hideKeys(on: AxisName, keys: BigUint64Array | bigint[]): HideOutcome {
    if (this.destroyed || !this.canHide(on)) return "nothing";
    const fresh = keys.length > 0 && Array.from(keys).some((k) => !this.last || !hiddenKeys(this.last, on).includes(k));
    let hid = true;
    this.editView(() => (hid = this.grid.hide(AXIS_CODE[on], BigUint64Array.from(keys))));
    if (!hid) return this.refuseHide(on);
    return fresh ? "hidden" : "nothing";
  }

  private refuseHide(on: AxisName): HideOutcome {
    const cap = this.hideCap(on);
    this.capped = { axis: on, cap };
    this.aria.say(capHint(on, cap));
    this.invalidate();
    return "capped";
  }

  /** Shows the hidden items `ids` of `axis` (the rows by default) again, in their places. */
  show<A extends AxisName = "rows">(ids: ItemId<A, RowId, ColId>[], axis?: A): void {
    const on = axis ?? "rows";
    this.editView(() => this.grid.show(AXIS_CODE[on], this.itemKeys(on, ids)));
  }

  /** The hidden items of `axis` (the rows by default), in the order they were hidden, as of the latest frame. */
  hidden<A extends AxisName = "rows">(axis?: A): ItemId<A, RowId, ColId>[] {
    const on = axis ?? "rows";
    const s = this.last;
    const keys = s ? (s.requestedView ?? s.appliedView)[on].hidden : [];
    const id = (key: bigint) => (on === "rows" ? this.feed.rowId(key) : this.feed.colId(key));
    return keys.map(id).filter((i) => i !== undefined) as ItemId<A, RowId, ColId>[];
  }

  private itemKeys(axis: AxisName, ids: unknown[]): BigUint64Array {
    return BigUint64Array.from(ids, (id) => (axis === "rows" ? this.feed.rowKey(id as RowId) : this.feed.colKey(id as ColId)));
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
   * calls `open` with the column, a rectangle to place the menu under, and
   * where in the menu to open. Returns how to unmount it; one menu is
   * mounted at a time.
   */
  attachColumnMenu(open: (colId: ColId, anchor: DOMRect, at: MenuAt) => void): () => void {
    this.columnMenu = open;
    this.painter.filterButtons.cols = true;
    this.invalidate();
    return () => {
      if (this.columnMenu !== open) return;
      this.columnMenu = null;
      this.painter.filterButtons.cols = false;
      this.invalidate();
    };
  }

  /**
   * Mounts a row menu, to filter the columns by a row's values (see
   * `createColumnMenu`): row headers whose row the columns take conditions
   * on draw its button while hovered or filtering, and a press on one,
   * Alt+ArrowRight, or `openRowMenu` calls `open` with the row and a
   * rectangle to place the menu beside. Returns how to unmount it; one menu
   * is mounted at a time.
   */
  attachRowMenu(open: (rowId: RowId, anchor: DOMRect, at: MenuAt) => void): () => void {
    this.rowMenu = open;
    this.painter.filterButtons.rows = true;
    this.invalidate();
    return () => {
      if (this.rowMenu !== open) return;
      this.rowMenu = null;
      this.painter.filterButtons.rows = false;
      this.invalidate();
    };
  }

  /**
   * Mounts a chooser (see `createChooser`): Mod+Alt+O (Mod+Alt+Shift+O for
   * the rows), a menu's "Choose columns…" or `openChooser` calls `open` with
   * the axis and, from a menu, a rectangle to place it by. Returns how to
   * unmount it; one chooser is mounted at a time.
   */
  attachChooser(open: (axis: AxisName, anchor?: DOMRect) => void): () => void {
    this.chooser = open;
    return () => {
      if (this.chooser === open) this.chooser = null;
    };
  }

  /** Whether a chooser is mounted and `axis` hides, so it can be opened there. */
  canChoose(axis: AxisName): boolean {
    return this.chooser !== null && this.canHide(axis);
  }

  /** Opens the mounted chooser of `axis` (the columns by default): `false` if none is mounted or the axis does not hide. */
  openChooser(axis: AxisName = "cols", anchor?: DOMRect): boolean {
    const open = this.chooser;
    if (!open || this.destroyed || !this.canHide(axis)) return false;
    open(axis, anchor);
    return true;
  }

  /**
   * Opens the mounted row menu for `rowId`, beside its header if it is in
   * view, else at `anchor` (or the grid's top left), at `at`. `false` if no
   * menu is mounted, or the columns cannot be filtered by the row and the
   * rows do not hide.
   */
  openRowMenu(rowId: RowId, anchor?: DOMRect, at: MenuAt = "top"): boolean {
    const open = this.rowMenu;
    const support = this.feed.rowSupport(rowId);
    if (!open || this.destroyed || (!support.text && !support.oneOf && !support.compare && !this.canHide("rows"))) return false;
    const key = this.feed.rowKey(rowId);
    const header = this.last?.rows.headers.find((h) => h.key === key && this.inView("rows", h));
    open(rowId, header ? this.clientRect(header.rect) : (anchor ?? this.clientRect({ x: 0, y: 0, width: 0, height: 0 })), at);
    return true;
  }

  /**
   * Opens the mounted column menu for `colId`, under its header if it is in
   * view, else under `anchor` (or the grid's top left), at `at`. `false` if
   * no menu is mounted.
   */
  openColumnMenu(colId: ColId, anchor?: DOMRect, at: MenuAt = "top"): boolean {
    const open = this.columnMenu;
    if (!open || this.destroyed) return false;
    const key = this.feed.colKey(colId);
    const header = this.last?.cols.headers.find((h) => h.key === key && this.inView("cols", h));
    open(colId, header ? this.clientRect(header.rect) : (anchor ?? this.clientRect({ x: 0, y: 0, width: 0, height: 0 })), at);
    return true;
  }

  /**
   * Sets the Find Query, or closes find with `null` (or empty text). The
   * Table Source marks the matching cells in view and counts them; the old
   * marks go at once. Does nothing unless the source has `find`.
   */
  setFind(query: FindQuery<ColId> | null): void {
    if (this.destroyed || !this.options.source.find) return;
    this.grid.set_find(query && this.feed.findKeys(query));
    drainEffects(this.parts);
    this.schedule();
  }

  /**
   * Steps to the next match of the Find Query from the current one (or the
   * focused cell), wrapping round: focus moves there and it scrolls into
   * view; the selection stays. Does nothing while find is closed.
   */
  findNext(): void {
    this.editView(() => this.grid.find_next());
  }

  /** Steps to the previous match, as `findNext` does. */
  findPrevious(): void {
    this.editView(() => this.grid.find_previous());
  }

  /**
   * Mounts a find bar (see `createFindBar`): Mod+F in the grid calls `open`.
   * Returns how to unmount it; one find bar is mounted at a time.
   */
  attachFindBar(open: () => void): () => void {
    this.findBar = open;
    return () => {
      if (this.findBar === open) this.findBar = null;
    };
  }

  /** Opens the mounted find bar: `false` if none is mounted or the Table Source does not find. */
  openFind(): boolean {
    const open = this.findBar;
    if (!open || this.destroyed || !this.options.source.find) return false;
    open();
    return true;
  }

  /** The columns of the table, up to `limit` (1,000 by default), with their labels and support, asked of the Table Source. */
  async columns(limit = 1000): Promise<ColumnInfo<ColId>[]> {
    const cols = this.options.source.cols;
    const count = await cols.count();
    const ids = windowIds(await cols.window(0, Math.min(count, limit), new AbortController().signal));
    return ids.map((id) => ({ id, label: this.columnLabel(id), support: this.feed.columnSupport(id) }));
  }

  /**
   * Asks the Table Source for up to `limit` distinct values of column `id`
   * containing `search`, narrowed by the other filters applied, for a
   * checklist; `apply` gets the answer unless the request is aborted first.
   * `null` if the column takes no OneOf conditions. With `axis` `"cols"`,
   * the values of row `id` across the columns.
   */
  distinct<A extends AxisName = "rows">(
    id: CrossId<A, RowId, ColId>,
    search: string,
    limit: number,
    apply: (answer: Distinct) => void,
    axis?: A,
  ): { abort(): void } | null {
    if (this.destroyed) return null;
    return this.feed.distinct(id, search, limit, apply, axis);
  }

  /** How the rows may be viewed by column `colId`: whether it sorts, takes Text conditions (and regexes), and OneOf ones. */
  columnSupport(colId: ColId): ColumnSupport {
    return this.feed.columnSupport(colId);
  }

  /** How the columns may be viewed by row `rowId`, likewise; nothing unless the Table Source has `cols.support`. */
  rowSupport(rowId: RowId): ColumnSupport {
    return this.feed.rowSupport(rowId);
  }

  /** A row's header text: the source's `rows.label` (given the row's Position if in view, else NaN), else its Position + 1, else its ID. */
  rowLabel(rowId: RowId): string {
    const key = this.feed.rowKey(rowId);
    const position = this.last?.rows.strip.find((i) => i.key === key)?.position ?? Number.NaN;
    const label = this.options.source.rows.label?.(rowId, position);
    return label ?? (Number.isNaN(position) ? String(rowId) : String(position + 1));
  }

  /** A column's header text: the source's `cols.label` (given the column's Position if in view, else NaN), else its ID. */
  columnLabel(colId: ColId): string {
    const key = this.feed.colKey(colId);
    const position = this.last?.cols.strip.find((i) => i.key === key)?.position ?? Number.NaN;
    return this.options.source.cols.label?.(colId, position) ?? String(colId);
  }

  /**
   * Pins the column (or row) `id` to an edge of its axis, `leading` (the
   * left of the columns, the top of the rows) by default: it is drawn there,
   * after the items pinned before it, however the axis sorts or scrolls,
   * while the rest scrolls past. Pinning a pinned item moves it to the end of
   * the pins.
   */
  pin(axis: "cols", id: ColId, edge?: PinEdge): void;
  pin(axis: "rows", id: RowId, edge?: PinEdge): void;
  pin(axis: AxisName, id: RowId | ColId, edge: PinEdge = "leading"): void {
    this.editView(() => this.grid.pin(AXIS_CODE[axis], EDGE_CODE[edge], this.keyOf(axis, id)));
  }

  /** Unpins the column (or row) `id`; the scroll stays where it is. */
  unpin(axis: "cols", id: ColId): void;
  unpin(axis: "rows", id: RowId): void;
  unpin(axis: AxisName, id: RowId | ColId): void {
    this.editView(() => this.grid.unpin(AXIS_CODE[axis], this.keyOf(axis, id)));
  }

  /**
   * Pins every column (or row) from the start of its axis up to `id`, in
   * their current order, as many as fit with room left to scroll: on a
   * column pinned left, unpins the ones pinned left after it instead. The
   * axis scrolls to its start first if the grid does not hold all their IDs.
   */
  freeze(axis: "cols", id: ColId): void;
  freeze(axis: "rows", id: RowId): void;
  freeze(axis: AxisName, id: RowId | ColId): void {
    this.editView(() => this.grid.freeze(AXIS_CODE[axis], this.keyOf(axis, id)));
  }

  /**
   * The columns (or rows) drawn pinned to `edge` (the leading edge by
   * default), in pin order, as of the last frame.
   */
  pinned(axis: "cols", edge?: PinEdge): ColId[];
  pinned(axis: "rows", edge?: PinEdge): RowId[];
  pinned(axis: AxisName, edge: PinEdge = "leading"): (RowId | ColId)[] {
    const s = this.last;
    if (!s) return [];
    const id = (key: bigint) => (axis === "rows" ? this.feed.rowId(key) : this.feed.colId(key));
    return pinnedKeys(s, axis, edge).flatMap((key) => {
      const known = id(key);
      return known === undefined ? [] : [known];
    });
  }

  /**
   * Groups the rows by column `colId`, one level, each group headed by its
   * Group Item: the Table Source answers as it does any View Spec, and the
   * column leaves the body while the rows group by it. Nothing happens on a
   * column the source cannot group the rows by (`columnSupport(colId).group`).
   */
  groupBy(colId: ColId): void {
    this.editView(() => this.grid.group_by(AXIS_CODE.rows, this.feed.colKey(colId)));
  }

  /** Brings back the plain rows: no grouping. */
  ungroup(): void {
    this.editView(() => this.grid.ungroup(AXIS_CODE.rows));
  }

  /** The columns the rows group by, outermost first, as of the latest frame; empty when they are not grouped. */
  groupedBy(): ColId[] {
    const s = this.last;
    const grouping = s ? (s.requestedView ?? s.appliedView).grouping : null;
    return (grouping?.rowLevels ?? []).flatMap((level) => {
      const id = "field" in level ? this.feed.colId(level.field) : undefined;
      return id === undefined ? [] : [id];
    });
  }

  /**
   * The grid's layout by the Table Source's IDs, to save and give to
   * `restoreLayout` later: the View Spec last asked for, the pinned items
   * (those the source no longer has included), every size set by hand,
   * fitted or restored, and the header band thicknesses. The order of the
   * items is not in it: that is the source's. Runs a frame first if one is
   * due, so it is up to date.
   */
  layout(): Layout<RowId, ColId> {
    this.catchUp();
    return readLayout(this.last!, this.layoutParts());
  }

  /**
   * Puts back a layout from `layout`, before or after the data loads. Pins,
   * sizes and band thicknesses apply at once; the View Spec goes to the
   * Table Source as an ordinary request, so the rows in view stay until it
   * answers. It never fails on what the source lacks: a pinned item that
   * is missing waits, out of sight, and comes back if it appears; hidden
   * IDs the source does not know are dropped from its answer; a condition
   * on a missing column shows as not applied; a size waits for its item.
   * Returns `false`, changing nothing, for a layout of another version.
   */
  restoreLayout(layout: Layout<RowId, ColId>): boolean {
    if (this.destroyed || !restoreLayout(layout, this.layoutParts())) return false;
    // The restored row-number band stands: no fitting it to the labels later.
    this.bandFitted = true;
    drainEffects(this.parts);
    this.schedule();
    return true;
  }

  private keyOf(axis: AxisName, id: RowId | ColId): bigint {
    return axis === "rows" ? this.feed.rowKey(id as RowId) : this.feed.colKey(id as ColId);
  }

  /** A press on a Group Item's disclosure mark toggles its group. */
  private markPress(x: number, y: number): boolean {
    const s = this.last;
    if (!s || x >= s.cols.header || y < s.rows.header) return false;
    for (const [i, h] of s.rows.headers.entries()) {
      const outline = s.rows.strip[i]?.outline;
      const r = outline && h.key !== null && this.inView("rows", h) ? disclosureRect(outline, h.rect) : null;
      if (r && x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height) {
        this.grid.toggle(AXIS_CODE.rows, h.key!);
        return true;
      }
    }
    return false;
  }

  /** The column or row whose menu button is at (x, y), if its menu is mounted and there is one. */
  private menuButtonAt(x: number, y: number): { axis: AxisName; id: unknown } | undefined {
    const s = this.last;
    if (!s) return undefined;
    const band: AxisName | null =
      y < s.rows.header && x >= s.cols.header ? "cols" : x < s.cols.header && y >= s.rows.header ? "rows" : null;
    if (!band || !(band === "cols" ? this.columnMenu : this.rowMenu)) return undefined;
    for (const h of s[band].headers) {
      if (!hasFilterButton(true, h) || !this.inView(band, h)) continue;
      const r = filterButtonRect(band, h);
      if (x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height) {
        const id = band === "cols" ? this.feed.colId(h.key!) : this.feed.rowId(h.key!);
        return id === undefined ? undefined : { axis: band, id };
      }
    }
    return undefined;
  }

  private inView(axis: AxisName, h: HeaderPlacement): boolean {
    const s = this.last;
    if (s === null) return false;
    const [at, size] = axis === "cols" ? [h.rect.x, h.rect.width] : [h.rect.y, h.rect.height];
    return at + size > s[axis].header && at < s[axis].extent;
  }

  /** A viewport rectangle in client coordinates. */
  private clientRect(r: { x: number; y: number; width: number; height: number }): DOMRect {
    const box = this.layer.getBoundingClientRect();
    return new DOMRect(box.left + r.x, box.top + r.y, r.width, r.height);
  }

  /**
   * Asks the Table Source for the View Spec in force again, so rows the
   * user's edits left in view though they no longer match are filtered.
   */
  reapplyView(): void {
    if (this.destroyed) return;
    this.grid.reapply_view();
    drainEffects(this.parts);
    this.schedule();
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
  /** How long text fields wait after typing before asking the Table Source, in ms (`GridOptions.typingPause`). */
  get typingPause(): number {
    return this.options.typingPause ?? 150;
  }

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
    const gaps = borderGaps(this.painter.border);
    const band = Math.ceil(this.numberWidth()) + 2 * PAD;
    const rows = new b.WebAxis(counts.rows, rowHeight).header(o.headerHeight ?? HEADER_HEIGHT).gap(gaps.rows).min_size(rowHeight);
    const cols = new b.WebAxis(counts.cols, o.columnWidth ?? 120).header(Math.max(40, band)).gap(gaps.cols).min_size(24);
    const grid = new b.WebGrid(rows, cols);
    // Where a dragged header pins into an empty pinned Strip: past the bands
    // or the far ends, or this far in.
    grid.set_pin_zone(AXIS_CODE.rows, PIN_ZONE.rows);
    grid.set_pin_zone(AXIS_CODE.cols, PIN_ZONE.cols);
    // Pinned Strips longer than this leaves to scroll are clipped.
    grid.set_min_scrolling(AXIS_CODE.rows, MIN_SCROLLING);
    grid.set_min_scrolling(AXIS_CODE.cols, MIN_SCROLLING);
    return grid;
  }

  /** Repaints the whole grid next frame (resize, DPR, fonts, theme). */
  private invalidate(): void {
    this.force = true;
    this.schedule();
  }

  /** Runs the frame now if one is due, so what follows sees the Grid as of the input so far. */
  private catchUp(): void {
    if (!this.framing && (this.raf || !this.last) && !this.destroyed) {
      cancelAnimationFrame(this.raf);
      this.frame();
    }
  }

  private schedule(): void {
    if (!this.raf && !this.destroyed) this.raf = requestAnimationFrame(() => this.frame());
  }

  private frame(): void {
    this.raf = 0;
    this.framing = true;
    try {
      this.paintFrame();
    } finally {
      this.framing = false;
    }
  }

  private paintFrame(): void {
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
      rows: span(snapshot, "rows", (key) => this.feed.unmatched("rows", key)),
      cols: span(snapshot, "cols", (key) => this.feed.unmatched("cols", key)),
      quickFilter: snapshot.rows.quickFilter,
      quick: this.quick(snapshot, "rows"),
      quickState: quickState(snapshot, "rows"),
      columnsFilter: {
        quickFilter: snapshot.cols.quickFilter,
        quick: this.quick(snapshot, "cols"),
        quickState: quickState(snapshot, "cols"),
        conditions: snapshot.cols.conditions.flatMap(({ condition, state }) => {
          const c = this.feed.conditionId(condition, "cols");
          return c ? [{ condition: c, state }] : [];
        }),
      },
      viewPending: snapshot.viewPending,
      viewError: this.feed.viewError(),
      ...selection(snapshot),
      focusIds: this.focusIds(snapshot),
      loading: this.feed.loading(snapshot),
      animating: again,
      editing: snapshot.mode.type === "Editing" ? this.ids(snapshot.mode.row, snapshot.mode.col) : null,
      conditions: snapshot.rows.conditions.flatMap(({ condition, state }) => {
        const c = this.feed.conditionId(condition);
        return c ? [{ condition: c, state }] : [];
      }),
      grouping: this.groupingInfo(snapshot),
      columns: visible(snapshot.cols).flatMap((item) => {
        const id = item.key === null ? undefined : this.feed.colId(item.key);
        const header = snapshot.cols.headers.find((h) => h.position === item.position);
        if (id === undefined || !header) return [];
        return [{ id, label: this.columnLabel(id), filter: header.filter, support: this.feed.columnSupport(id) }];
      }),
      find: {
        support: snapshot.findSupport,
        query: snapshot.find ? (this.feed.findIds(snapshot.find) ?? null) : null,
        current: snapshot.findMatch && this.ids(snapshot.findMatch.row, snapshot.findMatch.col),
        ...this.feed.findState(),
      },
      hideCapped: this.capped && { ...this.capped, hint: capHint(this.capped.axis, this.capped.cap) },
    };
  }

  /** The grouping's listed parts by the Table Source's IDs, leaving out those naming a column not known. */
  private groupingInfo(s: Snapshot): GroupingInfo<ColId> {
    const col = (key: bigint | null) => (key === null ? null : this.feed.colId(key));
    return {
      levels: s.rows.levels.flatMap(({ level, state }): GroupingInfo<ColId>["levels"] => {
        if ("values" in level) return [{ level, state }];
        const field = col(level.field);
        return field === undefined || field === null ? [] : [{ level: { field, subtotal: level.subtotal }, state }];
      }),
      values: s.values.flatMap(({ value, state }) => {
        const field = col(value.field);
        return field === undefined ? [] : [{ value: { ...value, field }, state }];
      }),
      slicers: s.slicers.flatMap(({ condition, state }) => {
        const c = this.feed.conditionId(condition);
        return c ? [{ condition: c, state }] : [];
      }),
    };
  }

  private quick<A extends AxisName>(s: Snapshot, axis: A): TextQuery<CrossId<A, RowId, ColId>> | null {
    const q = (s.requestedView ?? s.appliedView)[axis].filter.quick;
    return (q && this.feed.queryIds(q, axis)) ?? null;
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

  private layoutParts(): LayoutParts<RowId, ColId> {
    const sizes = this.sizes;
    return { grid: this.grid, feed: this.feed, ...(sizes ? { computed: sizes.computed } : {}) };
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

  /**
   * The widest default row label: as many digits as the row count, each as
   * wide as the widest digit.
   */
  private numberWidth(): number {
    const digits = String(this.feed.counts().rows).length;
    const font = this.painter.theme.font;
    return digits * Math.max(...Array.from("0123456789", (d) => this.painter.measure.width(d, font)));
  }

  /**
   * Widens the row-number band once, when the first rows arrive, to fit their
   * labels (a column-major table's field names, say, or the widest row
   * number) beside a row menu's button or a filter mark and, where a row
   * header click sorts, a sort marker; returns whether it did.
   */
  private fitBand(s: Snapshot, content: Content): boolean {
    if (this.bandFitted || s.rows.strip.length === 0) return false;
    const numbers = this.options.source.rows.label ? 0 : this.numberWidth();
    const width = bandWidth(s, content, this.sizing(), true, this.options.headerClick?.rows === "sort", numbers);
    if (width === null) return false;
    this.bandFitted = true;
    if (Math.ceil(width) <= s.cols.header) return false;
    this.grid.set_header(AXIS_CODE.cols, Math.ceil(width));
    return true;
  }

  private sizing(): SizingParts {
    return {
      grid: this.grid,
      measure: this.painter.measure,
      theme: this.painter.theme,
      headerHeight: this.options.headerHeight ?? HEADER_HEIGHT,
      rowMenu: this.painter.filterButtons.rows,
      renderer: this.painter.renderer,
    };
  }

  private dispatch(effect: Effect): void {
    if (effect.type === "Locate") this.aria.locate(effect.axis, effect.key);
    if (effect.type === "CopyRequested") return this.copy(effect.rows, effect.cols, effect.rowLabels);
    if (effect.type === "Activated" && this.activated(effect.row, effect.col, effect.drillDown)) return;
    const s = this.last;
    if (this.input.answer(effect)) return;
    if (answerAutoSize(effect, s, s && this.feed.content(s), this.sizing())) return;
    this.feed.answer(effect);
  }

  /** Tells the app of an activation; whether that was all it does (no edit starts). */
  private activated(row: bigint, col: bigint, drillDown: KeyedCondition[] | null): boolean {
    const ids = this.ids(row, col);
    if (!ids) return drillDown !== null;
    const conditions = drillDown?.flatMap((c) => {
      const condition = this.feed.conditionId(c);
      return condition ? [condition] : [];
    });
    const handled = this.options.onActivate?.({ rowId: ids.row, colId: ids.col, drillDown: conditions ?? null });
    return drillDown !== null || handled === true;
  }

  /** Gathers what a copy names and puts it on the clipboard, at once if the source answers at once. */
  private copy(rows: Run[], cols: Run[], rowLabels: boolean): void {
    const source = this.options.source;
    const levels = this.last?.appliedView.grouping?.rowLevels ?? [];
    const controller = new AbortController();
    const gathered = gatherCopy(rows, cols, rowLabels, {
      source,
      renderer: this.painter.renderer as RendererChoice<RowId, ColId, V>,
      signal: controller.signal,
      label: ({ id, position, outline }) => {
        if (outline && outline.kind !== "leaf") return source.rows.groupLabel?.(id, outline) ?? summaryLabel(outline);
        return source.rows.label?.(id, position) ?? "";
      },
      figureless: (outline) => {
        if (outline?.kind !== "group" || outline.open !== "open") return false;
        const level = levels[outline.depth];
        return level !== undefined && "field" in level && level.subtotal !== "off";
      },
    });
    const write = (copied: Copied<RowId, ColId> | null) => {
      if (!copied || this.destroyed) return;
      const text = this.options.onCopy?.(copied);
      if (text === false) return;
      writeClipboard(typeof text === "string" ? text : copied.text).catch((e: unknown) => console.warn(e));
    };
    if (gathered instanceof Promise) gathered.then(write, (e: unknown) => console.warn(e));
    else write(gathered);
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

const visible = shownItems;

function selection(s: Snapshot): Pick<FrameInfo, "focus" | "selection" | "picked"> {
  const { rows, cols } = s;
  return {
    focus: rows.focus && cols.focus ? { row: rows.focus.position, col: cols.focus.position } : null,
    selection: s.selectionRange,
    picked: { rows: s.picked.rows.length, cols: s.picked.cols.length },
  };
}

function span(s: Snapshot, axis: "rows" | "cols", unmatched: (key: bigint) => boolean): AxisInfo {
  const visible = shownItems(s[axis]);
  // What scrolls: the pinned items stay whatever the scroll.
  const scrolling = s[axis].strip.filter((item, i) => stripOf(s[axis], i) === "scrolling" && visible.includes(item));
  return {
    first: scrolling[0]?.position ?? 0,
    last: scrolling.at(-1)?.position ?? 0,
    count: s[axis].count,
    total: s[axis].total,
    unmatched: visible.filter((i) => i.key !== null && unmatched(i.key)).length,
    hidden: (s.requestedView ?? s.appliedView)[axis].hidden.length,
  };
}

function div(className: string, style: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = className;
  el.style.cssText = style;
  return el;
}

/** Where an axis's quick filter stands between the latest request and the applied View Spec. */
function quickState(s: Snapshot, axis: AxisName): ConditionState | null {
  const applied = s.appliedView[axis].filter.quick;
  const asked = s.requestedView ? s.requestedView[axis].filter.quick : applied;
  if (sameQuery(asked, applied)) return asked ? "applied" : null;
  if (!s.viewPending) return "notApplied";
  return asked ? "adding" : "removing";
}

function sameQuery(a: KeyedTextQuery | null, b: KeyedTextQuery | null): boolean {
  if (!a || !b) return a === b;
  const keys = (q: KeyedTextQuery) => q.searches?.join(",") ?? null;
  return a.text === b.text && a.caseSensitive === b.caseSensitive && a.regex === b.regex && keys(a) === keys(b);
}
