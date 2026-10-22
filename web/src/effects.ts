// Effects as `WebGrid.poll_effects()` returns them: plain tagged objects with
// Keys as `bigint`. The union is hand-written against
// `crates/binnacle-wasm/src/effect.rs`; the agreement tests keep them in step.

import type { AxisName, PinEdge } from "./codes";
import type { FindMatch, KeyedCondition, KeyedFindQuery, KeyedTextQuery, KeyedViewSpec, SortColumn } from "./snapshot";

type Stamped<T> = T & { generation: number };

/** The cause of a requested View Spec, a hint: the spec alone is the request. `Other` is one this build does not know. */
export type KeyedViewEdit =
  | { type: "CycleSort"; axis: AxisName; key: bigint; multi: boolean }
  | { type: "SetQuickFilter"; axis: AxisName; query: KeyedTextQuery | null }
  /** The host asked for a whole View Spec. */
  | { type: "Replace" }
  /** Items of `axis` were shown or hidden, so a quick filter searching the shown ones asks again. */
  | { type: "ShownChanged"; axis: AxisName }
  | { type: "AddCondition"; axis: AxisName; condition: KeyedCondition }
  /** The first condition of `axis` equal to `condition` was removed. */
  | { type: "RemoveCondition"; axis: AxisName; condition: KeyedCondition }
  /** The first condition of `axis` equal to `from` became `to`, in place. */
  | { type: "ReplaceCondition"; axis: AxisName; from: KeyedCondition; to: KeyedCondition }
  /** The user asked for the same View Spec again, so rows their edits left unmatched are filtered. */
  | { type: "Reapply" }
  /** These items of `axis` were hidden, beside any hidden already. */
  | { type: "Hide"; axis: AxisName; keys: bigint[] }
  /** These hidden items of `axis` were shown again. */
  | { type: "Show"; axis: AxisName; keys: bigint[] }
  /** `axis` was grouped by the base field `field` as its innermost level. */
  | { type: "Group"; axis: AxisName; field: bigint }
  /** Every level of `axis` was removed. */
  | { type: "Ungroup"; axis: AxisName }
  /** The group of `axis` headed by the Group Item `key` was opened, or closed. */
  | { type: "Expand"; axis: AxisName; key: bigint }
  | { type: "Collapse"; axis: AxisName; key: bigint }
  /** The group headed by `key` was opened with every group under it the Grid knows. */
  | { type: "ExpandAll"; axis: AxisName; key: bigint }
  /** Every group of `axis` was opened to `depth` levels, its exceptions cleared. */
  | { type: "ExpandTo"; axis: AxisName; depth: number | "all" }
  | { type: "Other" };

export type Effect = Stamped<
  | { type: "TextTyped"; text: string }
  | { type: "Activated"; row: bigint; col: bigint }
  | { type: "EditRequested"; row: bigint; col: bigint }
  | { type: "EditCancelled"; row: bigint; col: bigint }
  | { type: "FocusChanged"; row: bigint; col: bigint }
  | { type: "SelectionChanged" }
  | { type: "ViewRequested"; spec: KeyedViewSpec; edit: KeyedViewEdit }
  | { type: "Locate"; axis: AxisName; key: bigint }
  | { type: "SizeChanged"; axis: AxisName; key: bigint; size: number }
  | { type: "AutoSizeRequested"; axis: AxisName; key: bigint }
  /** The Keys pinned to `edge` of `axis` are now `keys`, in pin order, dormant pins included. */
  | { type: "PinsChanged"; axis: AxisName; edge: PinEdge; keys: bigint[] }
  | { type: "BandSizeChanged"; axis: AxisName; thickness: number }
  | { type: "BandAutoSizeRequested"; axis: AxisName }
  | { type: "MoveRequested"; axis: AxisName; keys: bigint[]; to: number }
  /** The Find Query changed, or find closed (`null`): fetch the marks and the count again. */
  | { type: "FindRequested"; query: KeyedFindQuery | null }
  /** Find the next or previous match of `query` from `from` (or the start or end) and answer `find_stepped`. */
  | { type: "FindStepRequested"; query: KeyedFindQuery; from: FindMatch | null; direction: "next" | "previous" }
>;

export type EffectType = Effect["type"];

type Field = "string" | "number" | "key" | "axis" | "edge" | "keys" | "view" | "edit" | "find" | "find?" | "from" | "direction";

const FIELDS: { [T in EffectType]: Record<string, Field> } = {
  TextTyped: { text: "string" },
  Activated: { row: "key", col: "key" },
  EditRequested: { row: "key", col: "key" },
  EditCancelled: { row: "key", col: "key" },
  FocusChanged: { row: "key", col: "key" },
  SelectionChanged: {},
  ViewRequested: { spec: "view", edit: "edit" },
  Locate: { axis: "axis", key: "key" },
  SizeChanged: { axis: "axis", key: "key", size: "number" },
  AutoSizeRequested: { axis: "axis", key: "key" },
  PinsChanged: { axis: "axis", edge: "edge", keys: "keys" },
  BandSizeChanged: { axis: "axis", thickness: "number" },
  BandAutoSizeRequested: { axis: "axis" },
  MoveRequested: { axis: "axis", keys: "keys", to: "number" },
  FindRequested: { query: "find?" },
  FindStepRequested: { query: "find", from: "from", direction: "direction" },
};

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

function isSortColumn(v: unknown): v is SortColumn {
  return isRecord(v) && typeof v.key === "bigint" && (v.direction === "ascending" || v.direction === "descending");
}

function isTextQuery(v: unknown): v is KeyedTextQuery {
  if (!isRecord(v)) return false;
  const searches = v.searches === null || (Array.isArray(v.searches) && v.searches.every((k) => typeof k === "bigint"));
  return typeof v.text === "string" && typeof v.caseSensitive === "boolean" && typeof v.regex === "boolean" && searches;
}

function isAxis(v: unknown): v is AxisName {
  return v === "rows" || v === "cols";
}

function isFindQuery(v: unknown): v is KeyedFindQuery {
  return isRecord(v) && isTextQuery(v.query) && isAxis(v.by);
}

function isFindMatch(v: unknown): v is FindMatch {
  return isRecord(v) && typeof v.row === "bigint" && typeof v.col === "bigint";
}

const COMPARE_OPS = new Set(["equal", "notEqual", "less", "atMost", "greater", "atLeast"]);

function isScalar(v: unknown): boolean {
  return isRecord(v) && (v.type === "number" || v.type === "date") && typeof v.value === "number";
}

function isComparison(v: unknown): boolean {
  if (!isRecord(v)) return false;
  if (v.op === "between") return isScalar(v.low) && isScalar(v.high);
  return COMPARE_OPS.has(v.op as string) && isScalar(v.value);
}

function isCondition(v: unknown): v is KeyedCondition {
  if (!isRecord(v) || typeof v.key !== "bigint" || typeof v.not !== "boolean" || !isRecord(v.kind)) return false;
  const listed = (values: unknown) =>
    Array.isArray(values) && values.every((t) => t === null || typeof t === "string");
  return (
    (v.kind.type === "Text" && isTextQuery(v.kind.query)) ||
    (v.kind.type === "OneOf" && listed(v.kind.values)) ||
    (v.kind.type === "Compare" && isComparison(v.kind.comparison)) ||
    v.kind.type === "Other"
  );
}

function isAxisView(v: unknown): boolean {
  if (!isRecord(v) || !isRecord(v.filter)) return false;
  const quick = v.filter.quick === null || isTextQuery(v.filter.quick);
  const conditions = Array.isArray(v.filter.conditions) && v.filter.conditions.every(isCondition);
  const sorted = Array.isArray(v.sort) && v.sort.every(isSortColumn);
  return quick && conditions && sorted && isKeys(v.hidden);
}

function isKeys(v: unknown): v is bigint[] {
  return Array.isArray(v) && v.every((k) => typeof k === "bigint");
}

const PLACEMENT = ["before", "after", "off"];

function isLevel(v: unknown): boolean {
  if (!isRecord(v)) return false;
  return v.values === true || (typeof v.field === "bigint" && PLACEMENT.includes(v.subtotal as string));
}

function isGroupedAxis(v: unknown): boolean {
  if (!isRecord(v) || !isRecord(v.expand)) return false;
  const depth = v.expand.depth === "all" || typeof v.expand.depth === "number";
  const form = v.form === "outline" || v.form === "header";
  return PLACEMENT.includes(v.grandTotal as string) && form && depth && isKeys(v.expand.except);
}

function isGrouping(v: unknown): boolean {
  if (v === null) return true;
  if (!isRecord(v) || !Array.isArray(v.rowLevels) || !Array.isArray(v.colLevels)) return false;
  const values =
    Array.isArray(v.values) &&
    v.values.every(
      (a) =>
        isRecord(a) &&
        (a.field === null || typeof a.field === "bigint") &&
        typeof a.aggregate === "string" &&
        (a.params === undefined || (Array.isArray(a.params) && a.params.every((p) => typeof p === "string"))),
    );
  const slicers = Array.isArray(v.slicers) && v.slicers.every(isCondition);
  const levels = v.rowLevels.every(isLevel) && v.colLevels.every(isLevel);
  return levels && values && slicers && isGroupedAxis(v.rowAxis) && isGroupedAxis(v.colAxis);
}

function isEdit(v: unknown): v is KeyedViewEdit {
  if (!isRecord(v)) return false;
  switch (v.type) {
    case "CycleSort":
      return isAxis(v.axis) && typeof v.key === "bigint" && typeof v.multi === "boolean";
    case "SetQuickFilter":
      return isAxis(v.axis) && (v.query === null || isTextQuery(v.query));
    case "ShownChanged":
      return isAxis(v.axis);
    case "AddCondition":
    case "RemoveCondition":
      return isAxis(v.axis) && isCondition(v.condition);
    case "ReplaceCondition":
      return isAxis(v.axis) && isCondition(v.from) && isCondition(v.to);
    case "Hide":
    case "Show":
      return isAxis(v.axis) && isKeys(v.keys);
    case "Group":
      return isAxis(v.axis) && typeof v.field === "bigint";
    case "Ungroup":
      return isAxis(v.axis);
    case "Expand":
    case "Collapse":
    case "ExpandAll":
      return isAxis(v.axis) && typeof v.key === "bigint";
    case "ExpandTo":
      return isAxis(v.axis) && (v.depth === "all" || (typeof v.depth === "number" && Number.isInteger(v.depth) && v.depth >= 0));
    case "Replace":
    case "Reapply":
    case "Other":
      return true;
    default:
      return false;
  }
}

function fits(field: Field, v: unknown): boolean {
  switch (field) {
    case "string":
      return typeof v === "string";
    case "number":
      return typeof v === "number";
    case "key":
      return typeof v === "bigint";
    case "axis":
      return v === "rows" || v === "cols";
    case "edge":
      return v === "leading" || v === "trailing";
    case "keys":
      return isKeys(v);
    case "view":
      return isRecord(v) && isAxisView(v.rows) && isAxisView(v.cols) && isGrouping(v.grouping);
    case "edit":
      return isEdit(v);
    case "find":
      return isFindQuery(v);
    case "find?":
      return v === null || isFindQuery(v);
    case "from":
      return v === null || isFindMatch(v);
    case "direction":
      return v === "next" || v === "previous";
  }
}

/** Whether `v` is an Effect of a known type with every field it needs. */
export function isEffect(v: unknown): v is Effect {
  if (typeof v !== "object" || v === null) return false;
  const e = v as Record<string, unknown>;
  if (typeof e.type !== "string" || !Object.hasOwn(FIELDS, e.type)) return false;
  if (typeof e.generation !== "number") return false;
  const fields = FIELDS[e.type as EffectType];
  return Object.entries(fields).every(([name, field]) => fits(field, e[name]));
}
