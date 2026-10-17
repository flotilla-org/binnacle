// The feature-gap research as the gaps demo reads it, apart from the widget so
// end-to-end tests can compute what they expect from the same data. The JSON
// is a copy of the research branch's file; `features` is the one place that
// knows its field names.

import research from "../data/grid-feature-gaps.json" with { type: "json" };

export type Adapter = "ratatui" | "web";
export const ADAPTERS: readonly Adapter[] = ["ratatui", "web"];

export interface Evidence {
  title: string;
  url: string;
}

export interface Product {
  id: string;
  name: string;
  url: string;
}

export interface Feature {
  id: string;
  name: string;
  kind: string;
  category: string;
  description: string;
  demand: number;
  fit: number;
  /** Demand times fit: the default order, highest first. */
  priority: number;
  ratatui: string;
  web: string;
  /** How many products have the feature in any form. */
  products: number;
  support: Record<string, string>;
  note: string;
  issue: number | string | null;
  evidence: Evidence[];
}

/** The scale demand and fit are scored on. */
export const SCALE = 5;

/** A product's support that does not count as having the feature. */
const ABSENT = new Set(["no", "none", "missing"]);

export const sample: boolean = research.sample === true;

export const PRODUCTS: readonly Product[] = Object.entries(research.products).map(([id, p]) => ({ id, ...p }));

/** Every feature, highest priority first (ties by name). */
export const features: readonly Feature[] = research.features
  .map(
    (f): Feature => ({
      id: f.id,
      name: f.name,
      kind: f.kind,
      category: f.category,
      description: f.description,
      demand: f.demand,
      fit: f.fit,
      priority: f.demand * f.fit,
      ratatui: f.binnacle.ratatui,
      web: f.binnacle.web,
      products: Object.values(f.products).filter((s) => !ABSENT.has(s)).length,
      support: f.products,
      note: f.binnacle.note,
      issue: f.binnacle.issue,
      evidence: f.evidence,
    }),
  )
  .sort((a, b) => b.priority - a.priority || a.name.localeCompare(b.name));

const distinct = (values: string[]) => [...new Set(values)].sort();

export const KINDS = distinct(features.map((f) => f.kind));
export const STATUSES = distinct(features.flatMap((f) => ADAPTERS.map((a) => f[a])));

/** The quick filter: a status in an adapter, and a kind; `null` matches any. */
export interface GapFilter {
  status: { adapter: Adapter; status: string } | null;
  kind: string | null;
}

export function keeps(f: Feature, filter: GapFilter): boolean {
  const { status, kind } = filter;
  return (status === null || f[status.adapter] === status.status) && (kind === null || f.kind === kind);
}
