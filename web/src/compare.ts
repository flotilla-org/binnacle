// Comparison conditions in JS: how fromRows tests a value against one, and
// how the filter bar and the column menu write one.

import type { CompareOp, Comparison, Scalar } from "./source";

/** The kind of value a column compares: numbers, or dates. */
export type Compared = "numbers" | "dates";

/** The scalar a value compares as: a number or bigint as a number, a `Date` as a date; else none. */
export function scalarOf(value: unknown): Scalar | undefined {
  if (typeof value === "number") return { type: "number", value };
  if (typeof value === "bigint") return { type: "number", value: Number(value) };
  if (value instanceof Date) return { type: "date", value: value.getTime() };
  return undefined;
}

/** The kind every value that is not null shares, if there is one and it compares; `null` otherwise. */
export function comparedOf(values: Iterable<unknown>): Compared | null {
  let kind: Compared | null = null;
  for (const v of values) {
    if (v == null) continue;
    const s = scalarOf(v);
    if (!s) return null;
    const k = s.type === "number" ? "numbers" : "dates";
    if (kind && kind !== k) return null;
    kind = k;
  }
  return kind;
}

/** The kind of value `c` compares with, or `undefined` for a between mixing numbers and dates. */
export function comparedBy(c: Comparison): Compared | undefined {
  const kind = (s: Scalar): Compared => (s.type === "number" ? "numbers" : "dates");
  if (c.op !== "between") return kind(c.value);
  return kind(c.low) === kind(c.high) ? kind(c.low) : undefined;
}

/** `a` against `b`: -1, 0 or 1, or `undefined` for a NaN, or a number against a date. */
function order(a: Scalar, b: Scalar): number | undefined {
  if (a.type !== b.type || Number.isNaN(a.value) || Number.isNaN(b.value)) return undefined;
  return a.value < b.value ? -1 : a.value > b.value ? 1 : 0;
}

const WANTED: Record<CompareOp, number[]> = {
  equal: [0],
  notEqual: [-1, 1],
  less: [-1],
  atMost: [-1, 0],
  greater: [1],
  atLeast: [0, 1],
};

/** Whether `value` passes `c` (before `not`): never a null, a NaN or a value of another kind. */
export function passes(c: Comparison, value: unknown): boolean {
  const v = scalarOf(value);
  if (!v) return false;
  const is = (s: Scalar, wanted: number[]) => wanted.includes(order(v, s) ?? Number.NaN);
  if (c.op === "between") return is(c.low, WANTED.atLeast) && is(c.high, WANTED.atMost);
  return is(c.value, WANTED[c.op]);
}

/** A scalar as a chip or an input writes it: a number as JS does, a date as `YYYY-MM-DD`, with its time in UTC if not midnight. */
export function scalarText(s: Scalar): string {
  if (s.type === "number") return String(s.value);
  const date = new Date(s.value);
  if (Number.isNaN(date.getTime())) return "invalid date";
  const iso = date.toISOString();
  if (s.value % 86_400_000 === 0) return iso.slice(0, 10);
  return iso.replace(/:00\.000Z$/, "Z").replace(/\.000Z$/, "Z");
}

/** Each comparison's sign, as chips show it. */
export const OP_SIGNS: Record<CompareOp | "between", string> = {
  equal: "=",
  notEqual: "≠",
  less: "<",
  atMost: "≤",
  greater: ">",
  atLeast: "≥",
  between: "between",
};

/** Each comparison's name, as the column menu offers it. */
export const OP_NAMES: Record<CompareOp | "between", string> = {
  equal: "Equal to",
  notEqual: "Not equal to",
  less: "Less than",
  atMost: "At most",
  greater: "Greater than",
  atLeast: "At least",
  between: "Between",
};

/** A comparison for a chip: `> 100`, `≤ 2024-03-01`, or `between 1 and 5`. */
export function comparisonText(c: Comparison): string {
  if (c.op === "between") return `between ${scalarText(c.low)} and ${scalarText(c.high)}`;
  return `${OP_SIGNS[c.op]} ${scalarText(c.value)}`;
}
