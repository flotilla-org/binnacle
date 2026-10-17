// The records demo's rows, apart from the widget so end-to-end tests can
// compute what they expect from the same data.

export interface RecordRow {
  id: number;
  name: string;
  /** Missing for every seventh record, to show nulls sorting last. */
  weight: number | null;
  crew: number;
  /** Missing for every eleventh record. */
  port: string | null;
}

export const RECORDS = 5000;
const NAMES = ["anchor", "bosun", "capstan", "davit", "ensign", "fathom", "galley", "hawser"];
const PORTS = ["Aberdeen", "Bergen", "Cádiz", "Dunedin", "Esbjerg", "Funchal", "Gdańsk", "Halifax", "Île d'Yeu"];

function generate(i: number): RecordRow {
  // A cheap scramble so the columns are not already in order.
  const h = Number((BigInt(i) * 0x9e3779b97f4a7c15n) % (1n << 64n) >> 40n);
  return {
    id: i,
    name: `${NAMES[h % 8]} ${h % 97}`,
    weight: i % 7 === 0 ? null : (h % 10_000) / 10,
    crew: h % 13,
    port: i % 11 === 0 ? null : PORTS[(h >> 4) % PORTS.length]!,
  };
}

export const rows = Array.from({ length: RECORDS }, (_, i) => generate(i));

/** The filter box's rule: the name or the port contains `text`, ignoring case. */
export function matches(row: RecordRow, text: string): boolean {
  const t = text.trim().toLowerCase();
  return row.name.toLowerCase().includes(t) || (row.port?.toLowerCase().includes(t) ?? false);
}
