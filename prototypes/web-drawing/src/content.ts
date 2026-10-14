// PROTOTYPE (#45): the huge Table Source, all in TS. Keys are Positions on
// both axes (like binnacle-memory's IdentitySource) and every value is
// synthesized from the Key, so nothing is stored per row. Values never enter
// wasm: the Grid only ever sees Keys.

export const HUGE_ROWS = 1e15;

export type Cell =
  | { kind: "text"; text: string; align: "left" | "right" }
  | { kind: "spark"; values: number[] };

/** What a painter asks of the Table Source, by Key. */
export interface Content {
  columnTitle(col: number): string;
  rowHeader(position: number): string;
  cell(row: number, col: number): Cell;
}

export const COLUMNS = [
  { title: "id", width: 150 },
  { title: "name", width: 130 },
  { title: "city", width: 120 },
  { title: "price", width: 100 },
  { title: "qty", width: 80 },
  { title: "trend", width: 160 },
  { title: "note", width: 220 },
];

const NAMES = ["Ada", "Brin", "Cato", "Dara", "Eno", "Fia", "Gus", "Hal", "Ines", "Jun", "Kit", "Lev", "Mira", "Nils", "Oona", "Pax"];
const SURNAMES = ["Marsh", "Keel", "Stern", "Bow", "Helm", "Mast", "Reef", "Tide", "Gale", "Cove"];
const CITIES = ["Lisbon", "Oslo", "Valparaiso", "Hobart", "Bergen", "Cadiz", "Halifax", "Mombasa", "Kochi", "Recife"];
const NOTES = [
  "rigging checked", "awaiting chandlery", "hull scraped, needs paint", "crew of four",
  "logbook missing pages", "under sail since dawn", "moored at the outer pier", "compass swung",
  "東京行き", "café crème au port",
];

/** A deterministic 32-bit hash of a Key (exact up to 2^53) and a salt. */
function hash(key: number, salt: number): number {
  let h = Math.imul((key % 4294967296) ^ salt, 0x9e3779b1);
  h ^= Math.floor(key / 4294967296);
  h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
  h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
  return (h ^ (h >>> 16)) >>> 0;
}

export function sparkValues(key: number): number[] {
  const out: number[] = [];
  let v = (hash(key, 99) % 1000) / 10;
  for (let i = 0; i < 24; i++) {
    v += ((hash(key, i) % 2001) - 1000) / 100;
    out.push(v);
  }
  return out;
}

export class HugeContent implements Content {
  /** Committed edits, by "row:col" Key pair. The prototype's only stored state. */
  edits = new Map<string, string>();

  columnTitle(col: number): string {
    return COLUMNS[col]?.title ?? `col ${col}`;
  }

  rowHeader(position: number): string {
    return (position + 1).toString();
  }

  text(row: number, col: number): string {
    const edited = this.edits.get(`${row}:${col}`);
    if (edited !== undefined) return edited;
    switch (col) {
      case 0: return "k" + row.toString(16).padStart(13, "0");
      case 1: return `${NAMES[hash(row, 1) % NAMES.length]} ${SURNAMES[hash(row, 2) % SURNAMES.length]}`;
      case 2: return CITIES[hash(row, 3) % CITIES.length];
      case 3: return ((hash(row, 4) % 1000000) / 100).toFixed(2);
      case 4: return (hash(row, 5) % 5000).toString();
      case 6: return NOTES[hash(row, 6) % NOTES.length];
      default: return "";
    }
  }

  cell(row: number, col: number): Cell {
    if (col === 5 && !this.edits.has(`${row}:${col}`)) return { kind: "spark", values: sparkValues(row) };
    return { kind: "text", text: this.text(row, col), align: col === 3 || col === 4 ? "right" : "left" };
  }
}
