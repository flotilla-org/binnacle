// The feature-gap research as the gaps demo reads it, apart from the widget so
// end-to-end tests can compute what they expect from the same data. The JSON
// is a copy of `docs/research/data/grid-feature-gaps.json` from the research
// branch (described in `docs/research/04-grid-feature-gaps.md` there); this
// module is the one place that knows its field names.

import research from "../data/grid-feature-gaps.json" with { type: "json" };

export type Adapter = "core" | "ratatui" | "web";
export const ADAPTERS: readonly Adapter[] = ["core", "ratatui", "web"];

/** binnacle's status for a feature in one adapter. */
export type Status = "has" | "partial" | "missing" | "out-of-scope" | "n/a";
/** Most ready first: the order the binnacle column sorts in. */
export const STATUSES: readonly Status[] = ["has", "partial", "missing", "out-of-scope", "n/a"];

/** A product's support for a feature; `unknown` where the product was not surveyed for it. */
export type Support = "core" | "plugin" | "paid" | "partial" | "no" | "unknown";

export interface Evidence {
  kind: string;
  url: string;
  signal: string;
}

export interface Product {
  id: string;
  name: string;
}

export interface Feature {
  id: string;
  name: string;
  kind: string;
  category: string;
  description: string;
  /** 0 to 4. */
  demand: number;
  /** 0 to 4. */
  fit: number;
  /** Demand times fit, 0 to 16: the default order, highest first. */
  priority: number;
  core: Status;
  ratatui: Status;
  web: Status;
  /**
   * The three statuses as one sortable code, a letter per adapter in
   * `ADAPTERS` order, `a` for `has` through `e` for `n/a`: most ready first.
   */
  binnacle: string;
  gap: boolean;
  /** How many surveyed products have the feature in any form. */
  products: number;
  surveyed: number;
  support: Record<string, Support>;
  productNotes: Record<string, string>;
  note: string;
  /** Issue references as the research writes them, like `#78, #80`. */
  issue: string | null;
  scope: string | null;
  evidence: Evidence[];
  notes: string;
}

/** The top of the demand and fit scale. */
export const SCALE = 4;

/** The surveyed products, in the research's order, with the names its note gives them. */
export const PRODUCTS: readonly Product[] = [
  { id: "aggrid", name: "AG Grid" },
  { id: "muix", name: "MUI X Data Grid" },
  { id: "glide", name: "Glide Data Grid" },
  { id: "tanstack", name: "TanStack Table" },
  { id: "handsontable", name: "Handsontable" },
  { id: "tabulator", name: "Tabulator" },
  { id: "revogrid", name: "RevoGrid" },
  { id: "perspective", name: "FINOS Perspective" },
  { id: "rdg", name: "react-data-grid" },
  { id: "datatables", name: "DataTables" },
  { id: "excel", name: "Microsoft Excel" },
  { id: "sheets", name: "Google Sheets" },
  { id: "visidata", name: "VisiData" },
  { id: "ratatui_table", name: "ratatui Table" },
  { id: "tui_csv", name: "csvlens" },
  { id: "k9s", name: "k9s" },
  { id: "qt", name: "Qt QTableView" },
  { id: "egui", name: "egui table" },
  { id: "datasette", name: "Datasette" },
  { id: "tad", name: "Tad" },
];

/** The letter a status takes in `Feature.binnacle`. */
export const statusCode = (s: Status): string => "abcde"[STATUSES.indexOf(s)]!;
export const codeStatus = (code: string): Status => STATUSES["abcde".indexOf(code)] ?? "n/a";

/** Every feature, highest demand times fit first (ties by name). */
export const features: readonly Feature[] = research
  .map(
    (f): Feature => ({
      id: f.id,
      name: f.name,
      kind: f.kind,
      category: f.category,
      description: f.description,
      demand: f.demand,
      fit: f.fit,
      priority: f.demand_x_fit,
      core: f.status_core as Status,
      ratatui: f.status_ratatui as Status,
      web: f.status_web as Status,
      binnacle: [f.status_core, f.status_ratatui, f.status_web].map((s) => statusCode(s as Status)).join(""),
      gap: f.is_gap,
      products: f.products_with,
      surveyed: f.products_surveyed,
      support: f.products as Record<string, Support>,
      productNotes: f.product_notes as Record<string, string>,
      note: f.binnacle_note,
      issue: f.binnacle_issue,
      scope: f.scope_ref,
      evidence: f.evidence,
      notes: f.notes,
    }),
  )
  .sort((a, b) => b.priority - a.priority || (a.name < b.name ? -1 : 1));

const distinct = (values: string[]) => [...new Set(values)].sort();

export const KINDS = distinct(features.map((f) => f.kind));

/** The quick filter's status choice: gaps, anything out of scope, or one status in one adapter. */
export type StatusFilter = { type: "gap" } | { type: "out-of-scope" } | { type: "is"; adapter: Adapter; status: Status };

export interface GapFilter {
  status: StatusFilter | null;
  kind: string | null;
}

function statusKeeps(f: Feature, s: StatusFilter): boolean {
  switch (s.type) {
    case "gap":
      return f.gap;
    case "out-of-scope":
      return ADAPTERS.some((a) => f[a] === "out-of-scope");
    case "is":
      return f[s.adapter] === s.status;
  }
}

export function keeps(f: Feature, filter: GapFilter): boolean {
  return (filter.status === null || statusKeeps(f, filter.status)) && (filter.kind === null || f.kind === filter.kind);
}
