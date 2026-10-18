// The typed Snapshot and its decoder: the only module that knows the flat
// layout `crates/binnacle-wasm/src/snapshot.rs` writes. Everything else reads
// the typed object. The agreement tests check the decoder against Rust.

import type { AxisName } from "./codes";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface StripItem {
  position: number;
  /** `null` while the Table Source has not filled the Position. */
  key: bigint | null;
  offset: number;
  size: number;
}

export type SortDirection = "ascending" | "descending";

export interface SortColumn {
  key: bigint;
  direction: SortDirection;
}

/**
 * A Text Query as the Grid holds it: `searches` names the columns (Keys of
 * the other axis) it looks in, or is `null` for the shown ones.
 */
export interface KeyedTextQuery {
  text: string;
  caseSensitive: boolean;
  regex: boolean;
  searches: bigint[] | null;
}

/** One axis's part of a View Spec, by Keys of the other axis. */
export interface KeyedAxisView {
  filter: { quick: KeyedTextQuery | null };
  sort: SortColumn[];
}

/** A View Spec as the Grid holds it, by Keys: each axis's filter and sort, by items of the other axis. */
export interface KeyedViewSpec {
  rows: KeyedAxisView;
  cols: KeyedAxisView;
}

export interface HeaderPlacement {
  position: number;
  key: bigint | null;
  rect: Rect;
  sort: { applied: { direction: SortDirection; index: number } | null; pending: boolean };
  selected: boolean;
  hovered: boolean;
}

export interface AxisSnapshot {
  count: number;
  /** The viewport's length along the axis, header band included. */
  extent: number;
  /** The thickness of the header band leading the axis. */
  header: number;
  gap: number;
  anchor: { position: number; offset: number };
  thumb: { fraction: number; size: number };
  focus: { position: number; key: bigint | null } | null;
  locating: boolean;
  strip: StripItem[];
  headers: HeaderPlacement[];
  /** The axis's count before any filter, as the Table Source last gave it; `null` if unknown. */
  total: number | null;
  /** Whether the axis has a quick filter, and whether it takes regexes; `null` if it has none. */
  quickFilter: { regex: boolean } | null;
}

export type Mode =
  | { type: "Idle" }
  | { type: "DraggingSelection" }
  | { type: "Resizing"; axis: AxisName; key: bigint; startSize: number }
  | { type: "ResizingBand"; axis: AxisName; startThickness: number }
  | { type: "MovingHeaders"; axis: AxisName; key: bigint; to: number }
  | { type: "Editing"; row: bigint; col: bigint };

export type Cursor = "default" | "resizeCol" | "resizeRow" | "move";

export interface Snapshot {
  generation: number;
  damage: { layout: boolean; selection: boolean; positions: boolean };
  rows: AxisSnapshot;
  cols: AxisSnapshot;
  focusRect: Rect | null;
  editorRect: Rect | null;
  mode: Mode;
  capturing: boolean;
  cursor: Cursor;
  resizeGuide: { axis: AxisName; key: bigint | null; offset: number; size: number } | null;
  dropIndicator: { axis: AxisName; to: number; offset: number } | null;
  hoveredHeader: { axis: AxisName; position: number } | null;
  selectionRange: { rows: Span; cols: Span } | null;
  selectionRects: Rect[];
  picked: bigint[];
  movingKeys: bigint[];
  appliedView: KeyedViewSpec;
  requestedView: KeyedViewSpec | null;
  /** Cell states as bit flags, row-major over the Strips; read with `cellState`. */
  cells: Uint8Array;
}

export interface Span {
  start: number;
  end: number;
}

export interface CellState {
  focused: boolean;
  selected: boolean;
  editing: boolean;
}

const FOCUSED = 1;
const SELECTED = 2;
const EDITING = 4;

/** The state of the cell where the `row`th row item and `col`th column item cross. */
export function cellState(s: Snapshot, row: number, col: number): CellState {
  const bits = s.cells[row * s.cols.strip.length + col] ?? 0;
  return {
    focused: (bits & FOCUSED) !== 0,
    selected: (bits & SELECTED) !== 0,
    editing: (bits & EDITING) !== 0,
  };
}

const AXIS_NAMES: readonly AxisName[] = ["rows", "cols"];
const DIRECTIONS: readonly SortDirection[] = ["ascending", "descending"];
const CURSORS: readonly Cursor[] = ["default", "resizeCol", "resizeRow", "move"];

class Reader {
  private n = 0;
  private k = 0;

  constructor(
    private readonly numbers: Float64Array,
    private readonly keys: BigUint64Array,
  ) {}

  num(): number {
    const v = this.numbers[this.n++];
    if (v === undefined) throw new Error("binnacle: Snapshot numbers ended early");
    return v;
  }

  flag(): boolean {
    return this.num() !== 0;
  }

  key(): bigint {
    const v = this.keys[this.k++];
    if (v === undefined) throw new Error("binnacle: Snapshot keys ended early");
    return v;
  }

  optKey(): bigint | null {
    return this.flag() ? this.key() : null;
  }

  opt<T>(read: () => T): T | null {
    return this.flag() ? read() : null;
  }

  list<T>(read: () => T): T[] {
    const n = this.num();
    const out = new Array<T>(n);
    for (let i = 0; i < n; i++) out[i] = read();
    return out;
  }

  axis(): AxisName {
    return AXIS_NAMES[this.num()]!;
  }

  rect(): Rect {
    return { x: this.num(), y: this.num(), width: this.num(), height: this.num() };
  }

  sort(): SortColumn[] {
    return this.list(() => ({ key: this.key(), direction: DIRECTIONS[this.num()]! }));
  }

  /** Text as its UTF-16 code units, after their count. */
  text(): string {
    return String.fromCharCode(...this.list(() => this.num()));
  }

  textQuery(): KeyedTextQuery {
    const text = this.text();
    const caseSensitive = this.flag();
    const regex = this.flag();
    const searches = this.opt(() => this.list(() => this.key()));
    return { text, caseSensitive, regex, searches };
  }

  axisView(): KeyedAxisView {
    const quick = this.opt(() => this.textQuery());
    return { filter: { quick }, sort: this.sort() };
  }

  view(): KeyedViewSpec {
    const rows = this.axisView();
    return { rows, cols: this.axisView() };
  }

  done(): boolean {
    return this.n === this.numbers.length && this.k === this.keys.length;
  }
}

function readAxis(r: Reader): AxisSnapshot {
  const count = r.num();
  const extent = r.num();
  const header = r.num();
  const gap = r.num();
  const anchor = { position: r.num(), offset: r.num() };
  const thumb = { fraction: r.num(), size: r.num() };
  const focus = r.opt(() => ({ position: r.num(), key: r.optKey() }));
  const locating = r.flag();
  const strip = r.list(() => ({ position: r.num(), key: r.optKey(), offset: r.num(), size: r.num() }));
  const headers = r.list(() => ({
    position: r.num(),
    key: r.optKey(),
    rect: r.rect(),
    sort: {
      applied: r.opt(() => ({ direction: DIRECTIONS[r.num()]!, index: r.num() })),
      pending: r.flag(),
    },
    selected: r.flag(),
    hovered: r.flag(),
  }));
  const total = r.opt(() => r.num());
  const quickFilter = r.opt(() => ({ regex: r.flag() }));
  return { count, extent, header, gap, anchor, thumb, focus, locating, strip, headers, total, quickFilter };
}

function readMode(r: Reader): Mode {
  switch (r.num()) {
    case 0:
      return { type: "Idle" };
    case 1:
      return { type: "DraggingSelection" };
    case 2:
      return { type: "Resizing", axis: r.axis(), key: r.key(), startSize: r.num() };
    case 3:
      return { type: "ResizingBand", axis: r.axis(), startThickness: r.num() };
    case 4:
      return { type: "MovingHeaders", axis: r.axis(), key: r.key(), to: r.num() };
    case 5:
      return { type: "Editing", row: r.key(), col: r.key() };
    default:
      throw new Error("binnacle: unknown Mode in Snapshot");
  }
}

/** Decodes one frame's flat numbers and Keys into a typed Snapshot. */
export function decodeSnapshot(numbers: Float64Array, keys: BigUint64Array): Snapshot {
  const r = new Reader(numbers, keys);
  const generation = r.num();
  const damage = { layout: r.flag(), selection: r.flag(), positions: r.flag() };
  const rows = readAxis(r);
  const cols = readAxis(r);
  const focusRect = r.opt(() => r.rect());
  const editorRect = r.opt(() => r.rect());
  const mode = readMode(r);
  const capturing = r.flag();
  const cursor = CURSORS[r.num()]!;
  const resizeGuide = r.opt(() => ({ axis: r.axis(), key: r.optKey(), offset: r.num(), size: r.num() }));
  const dropIndicator = r.opt(() => ({ axis: r.axis(), to: r.num(), offset: r.num() }));
  const hoveredHeader = r.opt(() => ({ axis: r.axis(), position: r.num() }));
  const selectionRange = r.opt(() => ({
    rows: { start: r.num(), end: r.num() },
    cols: { start: r.num(), end: r.num() },
  }));
  const selectionRects = r.list(() => r.rect());
  const picked = r.list(() => r.key());
  const movingKeys = r.list(() => r.key());
  const appliedView = r.view();
  const requestedView = r.opt(() => r.view());
  const cells = new Uint8Array(rows.strip.length * cols.strip.length);
  for (let i = 0; i < cells.length; i++) cells[i] = r.num();
  if (!r.done()) throw new Error("binnacle: Snapshot has data past its end");
  return {
    generation,
    damage,
    rows,
    cols,
    focusRect,
    editorRect,
    mode,
    capturing,
    cursor,
    resizeGuide,
    dropIndicator,
    hoveredHeader,
    selectionRange,
    selectionRects,
    picked,
    movingKeys,
    appliedView,
    requestedView,
    cells,
  };
}
