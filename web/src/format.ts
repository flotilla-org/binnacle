// Typed Values and the shared formatter: the same text, alignment and style
// hints for a value, unit and role as every other binnacle host, from the
// Rust formatter through wasm.

import type { WebColumnFormat } from "../wasm/binnacle_wasm.js";
import { bridge } from "./wasm";

/** The kinds of Typed Value, as the formatter names them. */
export type ValueType =
  | "null"
  | "bool"
  | "int"
  | "uint"
  | "number"
  | "text"
  | "bytes"
  | "datetime"
  | "category"
  | "nested"
  | "invalid";

/**
 * A Typed Value as TypeScript writes it: `null` (or `undefined`), a boolean,
 * a number (a Number), a bigint (an Int, or a UInt past `i64`), a string (a
 * Text), a `Date` or `{ instant }` in nanoseconds since the Unix epoch (a
 * DateTime), bytes, a category, a JSON tree, or a value the source could
 * not read as the kind its column expects.
 */
export type TypedValue =
  | null
  | undefined
  | boolean
  | number
  | bigint
  | string
  | Date
  | Uint8Array
  | { instant: bigint }
  | { category: string; label?: string }
  | { nested: unknown }
  | { invalid: string; expected: ValueType };

/** A column's choices; the defaults follow its role. */
export interface FormatOptions {
  /** Relative (`3m ago`) or absolute (`2026-10-29 14:03`) dates; by default a `time` column's are relative. */
  time?: "relative" | "absolute";
  /** Decimals for every Number in the column, usually `fewestDecimals` of its numbers. */
  decimals?: number;
  /** What a null shows in place of `∅`. */
  nullGlyph?: string;
  /** Plain-text mode, for output with no styles to see: a null is `-`. */
  plain?: boolean;
}

/** How a column's values format: its unit (a UCUM code), its role (`"time"`, or one this version does not know) and options. */
export interface ColumnFormat {
  unit?: string;
  role?: string;
  options?: FormatOptions;
}

/** Style hints, as bit flags in `Formatted.style`. */
export const DIM = 1;
export const WARNING = 2;
export const PENDING = 4;

export interface Formatted {
  /** One line, never a line break. */
  text: string;
  align: "start" | "end";
  /** `DIM`, `WARNING` and `PENDING`, or'd. */
  style: number;
}

/** The clock and zone the host injects. */
export interface FormatClock {
  /** Now, ms since the Unix epoch. */
  now: number;
  /** The local zone, seconds east of UTC. */
  utcOffset: number;
}

/** A column's formatter. Call it once the wasm build has loaded (`loadBinnacle`). */
export function columnFormatter(column: ColumnFormat = {}): (value: TypedValue, clock: FormatClock) => Formatted {
  let format: WebColumnFormat | undefined;
  return (value, clock) => {
    format ??= new (bridge().WebColumnFormat)(column.unit, column.role, column.options);
    return format.format(value, clock.now, clock.utcOffset) as Formatted;
  };
}

/** The fewest decimals, at most 3, that show every finite number in `numbers`: a column's `decimals`. */
export function fewestDecimals(numbers: readonly number[]): number {
  return bridge().fewest_decimals(Float64Array.from(numbers));
}

/** The browser's zone at `at` (ms since the Unix epoch), seconds east of UTC. */
export function localOffset(at: number): number {
  return -new Date(at).getTimezoneOffset() * 60;
}
