// The shared fixture of Typed Values (crates/binnacle-format/fixtures), read
// as TypeScript writes Typed Values. The Rust tests, vitest through wasm,
// the ratatui values example and the Values page all format it alike.

import raw from "../../../crates/binnacle-format/fixtures/values.json?raw";
import { fewestDecimals, type ColumnFormat, type FormatOptions, type TypedValue, type ValueType } from "../../src";

export interface FixtureColumn extends ColumnFormat {
  name: string;
}

export interface ValuesFixture {
  /** The time the expected text was formatted at, ms since the Unix epoch. */
  now: number;
  /** The zone it was formatted in, seconds east of UTC. */
  utcOffset: number;
  columns: FixtureColumn[];
  /** One value a column, by column name. */
  rows: Record<string, TypedValue>[];
}

type Tagged = null | Record<string, unknown>;

/** An RFC 3339 instant in nanoseconds, its fraction kept whole. */
function instant(text: string): bigint {
  const fraction = /\.(\d+)/.exec(text)?.[1] ?? "";
  const ms = Date.parse(text.replace(/\.\d+/, ""));
  return BigInt(ms) * 1_000_000n + BigInt(fraction.padEnd(9, "0").slice(0, 9));
}

/** A tagged fixture value as a `TypedValue`. */
export function typed(tagged: Tagged): TypedValue {
  if (tagged === null) return null;
  if ("bool" in tagged) return tagged.bool as boolean;
  if ("int" in tagged) return BigInt(tagged.int as number | string);
  if ("uint" in tagged) return BigInt(tagged.uint as number | string);
  if ("number" in tagged) {
    const n = tagged.number as number | string;
    return n === "inf" ? Infinity : n === "-inf" ? -Infinity : Number(n);
  }
  if ("text" in tagged) return tagged.text as string;
  if ("bytes" in tagged) {
    const hex = tagged.bytes as string;
    return Uint8Array.from({ length: hex.length / 2 }, (_, i) => parseInt(hex.slice(2 * i, 2 * i + 2), 16));
  }
  if ("datetime" in tagged) return { instant: instant(tagged.datetime as string) };
  if ("category" in tagged) {
    const label = tagged.label as string | undefined;
    return label === undefined ? { category: tagged.category as string } : { category: tagged.category as string, label };
  }
  if ("nested" in tagged) return { nested: tagged.nested };
  if ("invalid" in tagged) return { invalid: tagged.invalid as string, expected: tagged.expected as ValueType };
  throw new Error(`no known tag in ${JSON.stringify(tagged)}`);
}

interface RawFixture {
  now: string;
  utcOffset: number;
  columns: { name: string; unit?: string; role?: string; options?: FormatOptions }[];
  rows: Record<string, Tagged>[];
}

/**
 * The fixture, each column's decimals its options' or else the fewest that
 * show its Numbers. Call it once the wasm build has loaded.
 */
export function valuesFixture(): ValuesFixture {
  const fixture = JSON.parse(raw) as RawFixture;
  const rows = fixture.rows.map((row) =>
    Object.fromEntries(fixture.columns.map((c) => [c.name, typed(row[c.name] ?? null)])),
  );
  const columns = fixture.columns.map((c): FixtureColumn => {
    const numbers = rows.map((r) => r[c.name]).filter((v): v is number => typeof v === "number");
    const options = { ...c.options, decimals: c.options?.decimals ?? fewestDecimals(numbers) };
    return { ...c, options };
  });
  return { now: Date.parse(fixture.now), utcOffset: fixture.utcOffset, columns, rows };
}
