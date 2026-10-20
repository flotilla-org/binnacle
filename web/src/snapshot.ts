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

/**
 * A condition as the Grid holds it: keeps the items whose value in the item
 * `key` names (of the other axis) satisfies `kind`, or with `not` those
 * whose value does not. A Text condition's query searches `key` alone; a
 * OneOf condition lists values, `null` for a blank. `Other` is a kind this
 * build does not know.
 */
export interface KeyedCondition {
  key: bigint;
  kind: { type: "Text"; query: KeyedTextQuery } | { type: "OneOf"; values: (string | null)[] } | { type: "Other" };
  not: boolean;
}

/** One axis's filter by Keys: the quick filter and the conditions, all of which must keep an item. */
export interface KeyedFilter {
  quick: KeyedTextQuery | null;
  conditions: KeyedCondition[];
}

/** One axis's part of a View Spec, by Keys of the other axis. */
export interface KeyedAxisView {
  filter: KeyedFilter;
  sort: SortColumn[];
}

/**
 * Where a condition stands: applied and still asked for, asked for and not
 * yet applied, applied and no longer asked for, or asked for and left out by
 * the Table Source with no answer due (rejected, failed, or no longer
 * supported).
 */
export type ConditionState = "applied" | "adding" | "removing" | "notApplied";

/**
 * A Find Query as the Grid holds it: the Text Query, and the axis find steps
 * along first (`rows`: each row's cells left to right, then the next row).
 */
export interface KeyedFindQuery {
  query: KeyedTextQuery;
  by: AxisName;
}

/** A cell find stepped to or from, by its row and column Keys. */
export interface FindMatch {
  row: bigint;
  col: bigint;
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
  /**
   * Whether the applied View Spec has a condition read in this item, whether
   * a request would change them, and whether one asked for is not applied.
   */
  filter: { filtered: boolean; pending: boolean; notApplied: boolean };
  /** What the Table Source supports when viewing the other axis by this item; all off while the Key is unknown. */
  support: { sort: boolean; text: { regex: boolean } | null; oneOf: boolean };
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
  /** Every drawn item: the leading pinned Strip's `pinned` items, then the scrolling Strip's. */
  strip: StripItem[];
  pinned: number;
  /** Where the scrolling Strip draws along the axis; its part-scrolled items reach outside it. */
  scrolling: { start: number; end: number };
  headers: HeaderPlacement[];
  /** The axis's count before any filter, as the Table Source last gave it; `null` if unknown. */
  total: number | null;
  /** Whether the axis has a quick filter, and whether it takes regexes; `null` if it has none. */
  quickFilter: { regex: boolean } | null;
  /** The axis's conditions as the filter's chips list them: the latest request's, then the applied ones it drops. */
  conditions: { condition: KeyedCondition; state: ConditionState }[];
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
  /** The picked Keys of each axis, in Key order. */
  picked: { rows: bigint[]; cols: bigint[] };
  movingKeys: bigint[];
  appliedView: KeyedViewSpec;
  /** The latest request while an answer is due (`viewPending`), then while the applied spec leaves part of it out. */
  requestedView: KeyedViewSpec | null;
  /** Whether an answer to a View Spec request is due. */
  viewPending: boolean;
  /** The Find Query, `null` while find is closed. */
  find: KeyedFindQuery | null;
  /** The match the last find step landed on, until its row or column leaves, a step for a new Find Query lands, or find closes. */
  findMatch: FindMatch | null;
  /** Whether the Table Source finds, and with regexes; `null` if it does not. */
  findSupport: { regex: boolean } | null;
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
  currentMatch: boolean;
}

const FOCUSED = 1;
const SELECTED = 2;
const EDITING = 4;
const CURRENT_MATCH = 8;

/** The state of the cell where the `row`th row item and `col`th column item cross. */
export function cellState(s: Snapshot, row: number, col: number): CellState {
  const bits = s.cells[row * s.cols.strip.length + col] ?? 0;
  return {
    focused: (bits & FOCUSED) !== 0,
    selected: (bits & SELECTED) !== 0,
    editing: (bits & EDITING) !== 0,
    currentMatch: (bits & CURRENT_MATCH) !== 0,
  };
}

const AXIS_NAMES: readonly AxisName[] = ["rows", "cols"];
const DIRECTIONS: readonly SortDirection[] = ["ascending", "descending"];
const CURSORS: readonly Cursor[] = ["default", "resizeCol", "resizeRow", "move"];
const CONDITION_STATES: readonly ConditionState[] = ["applied", "adding", "removing", "notApplied"];
/** `SUPPORT` bits in `codes.ts`, read here without importing the encoder's side. */
const SORT_BIT = 1;
/** `FIND` bits in `codes.ts`, likewise. */
const FIND_BIT = 1;
const FIND_REGEX_BIT = 2;
const TEXT_BIT = 2;
const REGEX_BIT = 4;
const ONE_OF_BIT = 8;

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

  condition(): KeyedCondition {
    const key = this.key();
    const not = this.flag();
    const code = this.num();
    const kind: KeyedCondition["kind"] =
      code === 0
        ? { type: "Text", query: this.textQuery() }
        : code === 2
          ? { type: "OneOf", values: this.list(() => (this.flag() ? null : this.text())) }
          : { type: "Other" };
    return { key, kind, not };
  }

  axisView(): KeyedAxisView {
    const quick = this.opt(() => this.textQuery());
    const conditions = this.list(() => this.condition());
    return { filter: { quick, conditions }, sort: this.sort() };
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
  const pinned = r.num();
  const scrolling = { start: r.num(), end: r.num() };
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
    filter: { filtered: r.flag(), pending: r.flag(), notApplied: r.flag() },
    support: supportOf(r.num()),
  }));
  const conditions = r.list(() => {
    const state = CONDITION_STATES[r.num()]!;
    return { condition: r.condition(), state };
  });
  const total = r.opt(() => r.num());
  const quickFilter = r.opt(() => ({ regex: r.flag() }));
  return {
    count,
    extent,
    header,
    gap,
    anchor,
    thumb,
    focus,
    locating,
    strip,
    pinned,
    scrolling,
    headers,
    total,
    quickFilter,
    conditions,
  };
}

function supportOf(bits: number): HeaderPlacement["support"] {
  return {
    sort: (bits & SORT_BIT) !== 0,
    text: (bits & TEXT_BIT) !== 0 ? { regex: (bits & REGEX_BIT) !== 0 } : null,
    oneOf: (bits & ONE_OF_BIT) !== 0,
  };
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
  const picked = { rows: r.list(() => r.key()), cols: r.list(() => r.key()) };
  const movingKeys = r.list(() => r.key());
  const appliedView = r.view();
  const requestedView = r.opt(() => r.view());
  const viewPending = r.flag();
  const find = r.opt(() => {
    const query = r.textQuery();
    return { query, by: r.axis() };
  });
  const findMatch = r.opt(() => ({ row: r.key(), col: r.key() }));
  const findBits = r.num();
  const findSupport = findBits & FIND_BIT ? { regex: (findBits & FIND_REGEX_BIT) !== 0 } : null;
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
    viewPending,
    find,
    findMatch,
    findSupport,
    cells,
  };
}
