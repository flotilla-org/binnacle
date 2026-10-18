// Effects as `WebGrid.poll_effects()` returns them: plain tagged objects with
// Keys as `bigint`. The union is hand-written against
// `crates/binnacle-wasm/src/effect.rs`; the agreement tests keep them in step.

import type { AxisName } from "./codes";
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
  | { type: "BandSizeChanged"; axis: AxisName; thickness: number }
  | { type: "BandAutoSizeRequested"; axis: AxisName }
  | { type: "MoveRequested"; axis: AxisName; keys: bigint[]; to: number }
  /** The Find Query changed, or find closed (`null`): fetch the marks and the count again. */
  | { type: "FindRequested"; query: KeyedFindQuery | null }
  /** Find the next or previous match of `query` from `from` (or the start or end) and answer `find_stepped`. */
  | { type: "FindStepRequested"; query: KeyedFindQuery; from: FindMatch | null; direction: "next" | "previous" }
>;

export type EffectType = Effect["type"];

type Field = "string" | "number" | "key" | "axis" | "keys" | "view" | "edit" | "find" | "find?" | "from" | "direction";

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

function isCondition(v: unknown): v is KeyedCondition {
  if (!isRecord(v) || typeof v.key !== "bigint" || typeof v.not !== "boolean" || !isRecord(v.kind)) return false;
  const listed = (values: unknown) =>
    Array.isArray(values) && values.every((t) => t === null || typeof t === "string");
  return (
    (v.kind.type === "Text" && isTextQuery(v.kind.query)) ||
    (v.kind.type === "OneOf" && listed(v.kind.values)) ||
    v.kind.type === "Other"
  );
}

function isAxisView(v: unknown): boolean {
  if (!isRecord(v) || !isRecord(v.filter)) return false;
  const quick = v.filter.quick === null || isTextQuery(v.filter.quick);
  const conditions = Array.isArray(v.filter.conditions) && v.filter.conditions.every(isCondition);
  return quick && conditions && Array.isArray(v.sort) && v.sort.every(isSortColumn);
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
    case "Replace":
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
    case "keys":
      return Array.isArray(v) && v.every((k) => typeof k === "bigint");
    case "view":
      return isRecord(v) && isAxisView(v.rows) && isAxisView(v.cols);
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
