// Effects as `WebGrid.poll_effects()` returns them: plain tagged objects with
// Keys as `bigint`. The union is hand-written against
// `crates/binnacle-wasm/src/effect.rs`; the agreement tests keep them in step.

import type { AxisName } from "./codes";
import type { SortColumn } from "./snapshot";

type Stamped<T> = T & { generation: number };

export type Effect = Stamped<
  | { type: "TextTyped"; text: string }
  | { type: "Activated"; row: bigint; col: bigint }
  | { type: "EditRequested"; row: bigint; col: bigint }
  | { type: "EditCancelled"; row: bigint; col: bigint }
  | { type: "FocusChanged"; row: bigint; col: bigint }
  | { type: "SelectionChanged" }
  | { type: "SortRequested"; spec: SortColumn[] }
  | { type: "Locate"; axis: AxisName; key: bigint }
  | { type: "SizeChanged"; axis: AxisName; key: bigint; size: number }
  | { type: "AutoSizeRequested"; axis: AxisName; key: bigint }
  | { type: "BandSizeChanged"; axis: AxisName; thickness: number }
  | { type: "BandAutoSizeRequested"; axis: AxisName }
  | { type: "MoveRequested"; axis: AxisName; keys: bigint[]; to: number }
>;

export type EffectType = Effect["type"];

type Field = "string" | "number" | "key" | "axis" | "keys" | "spec";

const FIELDS: { [T in EffectType]: Record<string, Field> } = {
  TextTyped: { text: "string" },
  Activated: { row: "key", col: "key" },
  EditRequested: { row: "key", col: "key" },
  EditCancelled: { row: "key", col: "key" },
  FocusChanged: { row: "key", col: "key" },
  SelectionChanged: {},
  SortRequested: { spec: "spec" },
  Locate: { axis: "axis", key: "key" },
  SizeChanged: { axis: "axis", key: "key", size: "number" },
  AutoSizeRequested: { axis: "axis", key: "key" },
  BandSizeChanged: { axis: "axis", thickness: "number" },
  BandAutoSizeRequested: { axis: "axis" },
  MoveRequested: { axis: "axis", keys: "keys", to: "number" },
};

function isSortColumn(v: unknown): boolean {
  if (typeof v !== "object" || v === null) return false;
  const c = v as Record<string, unknown>;
  return typeof c.key === "bigint" && (c.direction === "ascending" || c.direction === "descending");
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
    case "spec":
      return Array.isArray(v) && v.every(isSortColumn);
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
