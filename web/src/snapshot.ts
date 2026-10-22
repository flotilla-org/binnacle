// The typed Snapshot and its decoder: the only module that knows the flat
// layout `crates/binnacle-wasm/src/snapshot.rs` writes. Everything else reads
// the typed object. The agreement tests check the decoder against Rust.

import type { AxisName, PinEdge } from "./codes";
import type { Aggregate, AggregateValue, CompareOp, Comparison, Grouping, Level, Outline, Placement, Scalar } from "./source";

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
  /** Where the item sits in a grouped axis, as its Table Source sent it with its Window; `null` on a plain axis. */
  outline: KeyedOutline | null;
  /**
   * A Group Item whose disclosure mark is pending: a request still due opens
   * or closes it. Adapters draw the mark pending only after about 150 ms.
   */
  togglePending: boolean;
}

/** An outline by Keys: ancestors by Keys of the axis, the group's field by a Key of the other axis. */
export type KeyedOutline = Outline<bigint, bigint>;

/** The grouping part of a View Spec by Keys: fields are Keys of the columns. */
export type KeyedGrouping = Omit<Grouping<bigint, bigint, bigint>, "slicers"> & { slicers: KeyedCondition[] };

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
 * OneOf condition lists values, `null` for a blank; a Compare condition
 * compares as `Condition` in `source.ts` says; a Native condition's text is
 * the Table Source's own. `Other` is a kind this build does not know.
 */
export interface KeyedCondition {
  key: bigint;
  kind:
    | { type: "Text"; query: KeyedTextQuery }
    | { type: "OneOf"; values: (string | null)[] }
    | { type: "Compare"; comparison: Comparison }
    | { type: "Native"; text: string }
    | { type: "Other" };
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
  /** Keys of the axis's own items left out of view, apart from the filter. */
  hidden: bigint[];
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

/** A View Spec as the Grid holds it, by Keys: each axis's filter and sort, by items of the other axis, and its hidden items. */
export interface KeyedViewSpec {
  rows: KeyedAxisView;
  cols: KeyedAxisView;
  /** `null` for a plain table. */
  grouping: KeyedGrouping | null;
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
  support: {
    sort: boolean;
    text: { regex: boolean } | null;
    oneOf: boolean;
    compare: "numbers" | "dates" | null;
    group: boolean;
    native: boolean;
    pivot: boolean;
    subtotals: { before: boolean; after: boolean };
  };
  /** The aggregates the Table Source offers over this item while grouping the other axis; none while the Key is unknown. */
  aggregates: Required<Aggregate>[];
  /** Whether this item's move is requested and not yet answered. */
  movePending: boolean;
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
  /**
   * Every drawn item, in drawn order: the leading pinned Strip's `pinned`
   * items, the scrolling Strip's, then the trailing pinned Strip's `trailing`
   * items.
   */
  strip: StripItem[];
  pinned: number;
  trailing: number;
  /** Where the scrolling Strip draws along the axis; its part-scrolled items reach outside it. */
  scrolling: { start: number; end: number };
  /**
   * Where each pinned Strip draws along the axis. Too long to leave the
   * minimum scrolling region, a pinned Strip is clipped at its inside end
   * (the trailing one first); one clipped whole still lists its items.
   */
  regions: { leading: Span; trailing: Span };
  headers: HeaderPlacement[];
  /** The axis's count before any filter, as the Table Source last gave it; `null` if unknown. */
  total: number | null;
  /** Whether the axis has a quick filter, and whether it takes regexes; `null` if it has none. */
  quickFilter: { regex: boolean } | null;
  /** Whether the Table Source hides items of the axis. */
  hideSupport: boolean;
  /** The most hidden items the axis takes: past it a hide is refused, and the widget suggests a filter. */
  hideCap: number;
  /** The axis's conditions as the filter's chips list them: the latest request's, then the applied ones it drops. */
  conditions: { condition: KeyedCondition; state: ConditionState }[];
  /**
   * The axis's grouping levels as asked for: the latest request's, then the
   * applied ones it drops. A level applied with another Subtotal than the
   * one asked for is listed once, as asked, not applied.
   */
  levels: { level: Level<bigint>; state: ConditionState }[];
  /** Whether the Table Source can move the axis's items now, so dragging a header moves them. */
  movable: boolean;
  /** The Keys pinned to each edge, in pin order, dormant pins (their items gone for now) included. */
  pins: { leading: bigint[]; trailing: bigint[] };
  /** In header form, where each tier lies across the Header Band, outermost first; empty otherwise. */
  tiers: Span[];
  /** In header form, each group's spanning cell in each Strip it is drawn in. */
  spans: HeaderSpan[];
}

/**
 * A group drawn in header form: one spanning cell at `tier` (0 outermost)
 * over the items `first` to `last` of one Strip. `visible` is the part of
 * `rect` its Strip draws, where the label goes; `cutBefore` and `cutAfter`
 * say the group goes on past that side.
 */
export interface HeaderSpan {
  axis: AxisName;
  tier: number;
  key: bigint;
  field: bigint | null;
  open: "open" | "closed";
  first: number;
  last: number;
  rect: Rect;
  visible: Rect;
  cutBefore: boolean;
  cutAfter: boolean;
}

export type Mode =
  | { type: "Idle" }
  | { type: "DraggingSelection" }
  | { type: "Resizing"; axis: AxisName; key: bigint; startSize: number }
  | { type: "ResizingBand"; axis: AxisName; startThickness: number }
  | { type: "MovingHeaders"; axis: AxisName; key: bigint; target: DropTarget }
  | { type: "Editing"; row: bigint; col: bigint };

/**
 * What releasing dragged headers would do: `Move` asks the Table Source to
 * move them before Position `to` (unpinning the dragged item first if
 * `unpin`); `Unpin` unpins it back to its own Position `at`, the axis being
 * unmovable; `Pin` pins them to `edge` before the pinned `before` (`null`:
 * after the last), `reorder` when only the pins' order changes; `Stay`
 * drops nothing; `Refused` drops nothing either, as a move to before `to`
 * would be refused: `why` it is (between groups, or a Summary Item dragged).
 */
export type DropTarget =
  | { type: "Move"; to: number; unpin: boolean }
  | { type: "Unpin"; at: number }
  | { type: "Pin"; edge: PinEdge; before: bigint | null; reorder: boolean }
  | { type: "Stay" }
  | { type: "Refused"; to: number; why: MoveRefusal };

/** Why a drop would be refused: `other` for reasons added later. */
export type MoveRefusal = "betweenGroups" | "summary" | "other";

const REFUSALS: readonly MoveRefusal[] = ["betweenGroups", "summary", "other"];

/** A lit pin zone while a header is dragged: an empty pinned Strip's edge, `size` long from `offset`; `hot` where a drop would pin. */
export interface PinZone {
  axis: AxisName;
  edge: PinEdge;
  offset: number;
  size: number;
  hot: boolean;
}

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
  dropIndicator: { axis: AxisName; target: DropTarget; offset: number } | null;
  /** The lit pin zones while a header is dragged. */
  pinZones: PinZone[];
  hoveredHeader: { axis: AxisName; position: number } | null;
  /**
   * The Positions the range covers on each axis, as sorted runs apart from
   * each other: it is contiguous in drawn order, so with items pinned it may
   * be several runs.
   */
  selectionRange: { rows: Span[]; cols: Span[] } | null;
  selectionRects: Rect[];
  /** The picked Keys of each axis, in Key order. */
  picked: { rows: bigint[]; cols: bigint[] };
  movingKeys: bigint[];
  appliedView: KeyedViewSpec;
  /** The latest request while an answer is due (`viewPending`), then while the applied spec leaves part of it out. */
  requestedView: KeyedViewSpec | null;
  /** Whether an answer to a View Spec request is due. */
  viewPending: boolean;
  /** Focus is in the row Header Band of grouped rows: the outline column, or in header form a level column. */
  outlineFocused: boolean;
  /** In header form, the tier (level column) the focus is on while `outlineFocused`. */
  focusedTier: number;
  /** The move requested and not yet answered: its axis and Keys. No other move starts until it is answered. */
  pendingMove: { axis: AxisName; keys: bigint[] } | null;
  /** The last move the Table Source refused and why, until the next press. */
  moveRefusal: { axis: AxisName; reason: string } | null;
  /** The Find Query, `null` while find is closed. */
  find: KeyedFindQuery | null;
  /** The match the last find step landed on, until its row or column leaves, a step for a new Find Query lands, or find closes. */
  findMatch: FindMatch | null;
  /** Whether the Table Source finds, and with regexes; `null` if it does not. */
  findSupport: { regex: boolean } | null;
  /** The grouping's slicers, listed as the conditions are. */
  slicers: { condition: KeyedCondition; state: ConditionState }[];
  /** The grouping's values, listed as the conditions are. */
  values: { value: AggregateValue<bigint>; state: ConditionState }[];
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

/** Which Strip the `index`th drawn item of an axis is in. */
export function stripOf(axis: AxisSnapshot, index: number): "leading" | "scrolling" | "trailing" {
  if (index < axis.pinned) return "leading";
  return index < axis.strip.length - axis.trailing ? "scrolling" : "trailing";
}

/**
 * Where the `index`th drawn item of an axis may draw along it: a pinned item
 * in its pinned Strip's region, a scrolling item between the pinned Strips,
 * so it slides under them.
 */
export function itemSpan(axis: AxisSnapshot, index: number): Span {
  const strip = stripOf(axis, index);
  return strip === "scrolling" ? axis.scrolling : axis.regions[strip];
}

/** The index in drawn order of the focused item of an axis, or -1 if it is not drawn. */
export function focusIndex(axis: AxisSnapshot): number {
  const f = axis.focus;
  if (!f || f.key === null) return -1;
  return axis.strip.findIndex((i) => i.position === f.position && i.key === f.key);
}

/**
 * The drawn items of an axis the viewport shows, in drawn order: the leading
 * pinned ones, the scrolling ones not hidden under the pinned Strips (the
 * scrolling Strip includes overscan), then the trailing pinned ones.
 */
export function shownItems(axis: AxisSnapshot): StripItem[] {
  return axis.strip.filter((item, index) => {
    const span = itemSpan(axis, index);
    return item.offset + item.size > span.start && item.offset < span.end;
  });
}

/** Whether `runs` (a selection range's runs on one axis) covers Position `p`. */
export function covers(runs: readonly Span[], p: number): boolean {
  return runs.some((r) => r.start <= p && p < r.end);
}

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
const COMPARE_NUMBERS_BIT = 16;
const COMPARE_DATES_BIT = 32;
/** A comparison's code in the Snapshot, as `op`. */
const COMPARE_OPS: readonly CompareOp[] = ["equal", "notEqual", "less", "atMost", "greater", "atLeast"];
const BETWEEN = 6;
const GROUP_BIT = 64;
const NATIVE_BIT = 128;
const PIVOT_BIT = 256;
const SUBTOTAL_BEFORE_BIT = 512;
const SUBTOTAL_AFTER_BIT = 1024;
const PLACEMENTS: readonly Placement[] = ["before", "after", "off"];
const KINDS: readonly KeyedOutline["kind"][] = ["leaf", "group", "subtotal", "grandTotal"];
const OPEN_STATES: readonly KeyedOutline["open"][] = ["open", "closed", "cantOpen"];

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
          : code === 3
            ? { type: "Compare", comparison: this.comparison() }
            : code === 4
              ? { type: "Native", text: this.text() }
              : { type: "Other" };
    return { key, kind, not };
  }

  comparison(): Comparison {
    const op = this.num();
    if (op === BETWEEN) return { op: "between", low: this.scalar(), high: this.scalar() };
    return { op: COMPARE_OPS[op]!, value: this.scalar() };
  }

  scalar(): Scalar {
    const date = this.flag();
    return { type: date ? "date" : "number", value: this.num() };
  }

  axisView(): KeyedAxisView {
    const quick = this.opt(() => this.textQuery());
    const conditions = this.list(() => this.condition());
    const sort = this.sort();
    return { filter: { quick, conditions }, sort, hidden: this.list(() => this.key()) };
  }

  level(): Level<bigint> {
    switch (this.num()) {
      case 0:
        return { field: this.key(), subtotal: PLACEMENTS[this.num()]! };
      case 1:
        return { values: true };
      default:
        throw new Error("binnacle: unknown grouping level in Snapshot");
    }
  }

  value(): AggregateValue<bigint> {
    const field = this.optKey();
    const aggregate = this.text();
    const params = this.list(() => this.text());
    return { field, aggregate, ...(params.length > 0 ? { params } : {}) };
  }

  state(): ConditionState {
    return CONDITION_STATES[this.num()]!;
  }

  groupedAxis(): KeyedGrouping["rowAxis"] {
    const grandTotal = PLACEMENTS[this.num()]!;
    const form = this.num() === 0 ? "outline" : "header";
    const depth = this.opt(() => this.num()) ?? "all";
    return { grandTotal, form, expand: { depth, except: this.list(() => this.key()) } };
  }

  grouping(): KeyedGrouping {
    const rowLevels = this.list(() => this.level());
    const colLevels = this.list(() => this.level());
    const values = this.list(() => this.value());
    const slicers = this.list(() => this.condition());
    const rowAxis = this.groupedAxis();
    return { rowLevels, colLevels, values, slicers, rowAxis, colAxis: this.groupedAxis() };
  }

  outline(): KeyedOutline {
    const kind = KINDS[this.num()]!;
    const depth = this.num();
    const open = OPEN_STATES[this.num()]!;
    const ancestors = this.list(() => this.key());
    const groupOf = () => {
      const field = this.key();
      return { field, value: this.flag() ? this.text() : null };
    };
    const group = this.opt(groupOf);
    const span = this.num();
    const path = this.list(groupOf);
    return { kind, depth, open, ancestors, group, span, path };
  }

  view(): KeyedViewSpec {
    const rows = this.axisView();
    const cols = this.axisView();
    return { rows, cols, grouping: this.opt(() => this.grouping()) };
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
  const strip = r.list(() => ({
    position: r.num(),
    key: r.optKey(),
    offset: r.num(),
    size: r.num(),
    outline: r.opt(() => r.outline()),
    togglePending: r.flag(),
  }));
  const pinned = r.num();
  const trailing = r.num();
  const scrolling = { start: r.num(), end: r.num() };
  const regions = { leading: { start: r.num(), end: r.num() }, trailing: { start: r.num(), end: r.num() } };
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
    movePending: r.flag(),
    support: supportOf(r.num()),
    aggregates: r.list(() => ({ name: r.text(), params: r.list(() => r.text()) })),
  }));
  const conditions = r.list(() => {
    const state = r.state();
    return { condition: r.condition(), state };
  });
  const levels = r.list(() => {
    const state = r.state();
    return { level: r.level(), state };
  });
  const total = r.opt(() => r.num());
  const quickFilter = r.opt(() => ({ regex: r.flag() }));
  const movable = r.flag();
  const hideSupport = r.flag();
  const hideCap = r.num();
  const pins = { leading: r.list(() => r.key()), trailing: r.list(() => r.key()) };
  const tiers = r.list(() => ({ start: r.num(), end: r.num() }));
  const spans = r.list(() => ({
    axis: r.axis(),
    tier: r.num(),
    key: r.key(),
    field: r.optKey(),
    open: r.num() === 0 ? ("open" as const) : ("closed" as const),
    first: r.num(),
    last: r.num(),
    rect: r.rect(),
    visible: r.rect(),
    cutBefore: r.flag(),
    cutAfter: r.flag(),
  }));
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
    trailing,
    scrolling,
    regions,
    headers,
    total,
    quickFilter,
    hideSupport,
    hideCap,
    conditions,
    levels,
    movable,
    pins,
    tiers,
    spans,
  };
}

function supportOf(bits: number): HeaderPlacement["support"] {
  return {
    sort: (bits & SORT_BIT) !== 0,
    text: (bits & TEXT_BIT) !== 0 ? { regex: (bits & REGEX_BIT) !== 0 } : null,
    oneOf: (bits & ONE_OF_BIT) !== 0,
    compare: (bits & COMPARE_NUMBERS_BIT) !== 0 ? "numbers" : (bits & COMPARE_DATES_BIT) !== 0 ? "dates" : null,
    group: (bits & GROUP_BIT) !== 0,
    native: (bits & NATIVE_BIT) !== 0,
    pivot: (bits & PIVOT_BIT) !== 0,
    subtotals: { before: (bits & SUBTOTAL_BEFORE_BIT) !== 0, after: (bits & SUBTOTAL_AFTER_BIT) !== 0 },
  };
}

const EDGES: readonly PinEdge[] = ["leading", "trailing"];

function readTarget(r: Reader): DropTarget {
  switch (r.num()) {
    case 0:
      return { type: "Move", to: r.num(), unpin: r.flag() };
    case 1:
      return { type: "Unpin", at: r.num() };
    case 2:
      return { type: "Pin", edge: EDGES[r.num()]!, before: r.optKey(), reorder: r.flag() };
    case 3:
      return { type: "Stay" };
    case 4:
      return { type: "Refused", to: r.num(), why: REFUSALS[r.num()] ?? "other" };
    default:
      throw new Error("binnacle: unknown drop target in Snapshot");
  }
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
      return { type: "MovingHeaders", axis: r.axis(), key: r.key(), target: readTarget(r) };
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
  const dropIndicator = r.opt(() => ({ axis: r.axis(), target: readTarget(r), offset: r.num() }));
  const pinZones = r.list(() => ({
    axis: r.axis(),
    edge: EDGES[r.num()]!,
    offset: r.num(),
    size: r.num(),
    hot: r.flag(),
  }));
  const hoveredHeader = r.opt(() => ({ axis: r.axis(), position: r.num() }));
  const runs = () => r.list(() => ({ start: r.num(), end: r.num() }));
  const selectionRange = r.opt(() => ({ rows: runs(), cols: runs() }));
  const selectionRects = r.list(() => r.rect());
  const picked = { rows: r.list(() => r.key()), cols: r.list(() => r.key()) };
  const movingKeys = r.list(() => r.key());
  const appliedView = r.view();
  const requestedView = r.opt(() => r.view());
  const viewPending = r.flag();
  const outlineFocused = r.flag();
  const focusedTier = r.num();
  const pendingMove = r.opt(() => ({ axis: r.axis(), keys: r.list(() => r.key()) }));
  const moveRefusal = r.opt(() => ({ axis: r.axis(), reason: r.text() }));
  const find = r.opt(() => {
    const query = r.textQuery();
    return { query, by: r.axis() };
  });
  const findMatch = r.opt(() => ({ row: r.key(), col: r.key() }));
  const findBits = r.num();
  const findSupport = findBits & FIND_BIT ? { regex: (findBits & FIND_REGEX_BIT) !== 0 } : null;
  const slicers = r.list(() => {
    const state = r.state();
    return { condition: r.condition(), state };
  });
  const values = r.list(() => {
    const state = r.state();
    return { value: r.value(), state };
  });
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
    pinZones,
    hoveredHeader,
    selectionRange,
    selectionRects,
    picked,
    movingKeys,
    appliedView,
    requestedView,
    viewPending,
    outlineFocused,
    focusedTier,
    pendingMove,
    moveRefusal,
    find,
    findMatch,
    findSupport,
    slicers,
    values,
    cells,
  };
}
