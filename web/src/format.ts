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
  /** Decimals for every Number in the column, usually `fewestDecimals` of its numbers: a whole number from 0 to 255, or the formatter throws on its first call. */
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

/** A zone: its offset from UTC, in seconds east, at `at` (ms since the Unix epoch). */
export type Zone = (at: number) => number;

/** A zone with one offset at every instant: `fixedZone(0)` is UTC. */
export function fixedZone(offset: number): Zone {
  return () => offset;
}

/** The clock and zone the host injects. */
export interface FormatClock {
  /** Now, ms since the Unix epoch. */
  now: number;
  /** The zone dates draw in, asked for each date's own offset; the runtime's local zone by default. */
  zone?: Zone;
}

/** A column's formatter. Call its methods once the wasm build has loaded (`loadBinnacle`). */
export interface ColumnFormatter {
  /** The text drawn at `clock`. */
  format(value: TypedValue, clock: FormatClock): Formatted;
  /** The stable text, for search, filters, copy and assistive tech: dates absolute, so it never goes stale. */
  stable(value: TypedValue, clock: FormatClock): Formatted;
  /** Whether the column's dates draw relative to now, so its cells redraw as the clock moves. */
  readonly relative: boolean;
}

/** A column's formatter, its wasm side made on first use. */
export function columnFormatter(column: ColumnFormat = {}): ColumnFormatter {
  let wasm: WebColumnFormat | undefined;
  const get = () => (wasm ??= new (bridge().WebColumnFormat)(column.unit, column.role, column.options));
  return {
    format: (value, clock) => get().format(value, clock.now, clock.zone) as Formatted,
    stable: (value, clock) => get().formatStable(value, clock.now, clock.zone) as Formatted,
    get relative() {
      return get().relative;
    },
  };
}

/** The fewest decimals, at most 3, that show every finite number in `numbers`: a column's `decimals`. */
export function fewestDecimals(numbers: readonly number[]): number {
  return bridge().fewest_decimals(Float64Array.from(numbers));
}
