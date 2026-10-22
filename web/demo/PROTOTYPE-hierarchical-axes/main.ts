// PROTOTYPE (#186), throwaway: a grouped and a pivoted table, to see how
// hierarchical axes look and feel on the web. The grid is a fake drawn as one
// CSS grid with sticky pinned parts, and the Table Source is a fake that
// recomputes items, outlines and spans from the View Spec's `grouping` part
// on every change: none of #181 to #185 is in the core yet. Look and feel are
// borrowed from the web adapter's Themes and column menu; none of their
// structure is. The state pane on the right shows the spec and the focused
// item's outline after every action.

import { catppuccinMocha, light, mono, type Theme } from "../../src/theme";

// ---------------------------------------------------------------- the fake data

type Field = "vessel" | "region" | "port" | "quarter" | "cargo" | "weight" | "crew";
const FIELDS: Field[] = ["vessel", "region", "port", "quarter", "cargo", "weight", "crew"];
const NUMERIC = new Set<Field>(["weight", "crew"]);

interface Ship { id: number; vessel: string; region: string; port: string; quarter: string; cargo: string; weight: number; crew: number }

const REGIONS: Record<string, string[]> = {
  North: ["Leith", "Aberdeen", "Whitby", "Lerwick"],
  South: ["Falmouth", "Plymouth", "Southampton"],
  East: ["Hull", "Harwich", "Felixstowe"],
  West: ["Bristol", "Swansea", "Liverpool"],
};
const VESSELS = [
  "Cormorant", "Petrel", "Gannet", "Kittiwake", "Shearwater", "Fulmar", "Guillemot", "Razorbill", "Puffin", "Skua",
  "Tern", "Merganser", "Eider", "Scoter", "Dunlin", "Curlew", "Whimbrel", "Plover", "Sanderling", "Turnstone",
];
const CARGO = ["grain", "coal", "timber", "containers", "fish"];

function rng(seed: number) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const SHIPS: Ship[] = (() => {
  const r = rng(186);
  const pick = <T>(xs: T[]) => xs[Math.floor(r() * xs.length)]!;
  const regions = Object.keys(REGIONS);
  return Array.from({ length: 420 }, (_, id) => {
    const region = pick(regions);
    return {
      id, vessel: pick(VESSELS), region, port: pick(REGIONS[region]!), quarter: `Q${1 + Math.floor(r() * 4)}`,
      cargo: pick(CARGO), weight: 10 + Math.floor(r() * 50), crew: 4 + Math.floor(r() * 10),
    };
  });
})();

// ---------------------------------------------------------------- the View Spec's grouping part

type Agg = "sum" | "count" | "avg" | "min" | "max";
interface Value { field: Field | null; agg: Agg }
interface Expand { depth: number; except: Set<string> }
type Form = "outline" | "header";
interface Grouping {
  rowLevels: Field[];
  colLevels: Field[];
  values: Value[];
  rowForm: Form;
  expand: { rows: Expand; cols: Expand };
}

const preset = (name: "grouped" | "pivot"): Grouping =>
  name === "grouped"
    ? {
        rowLevels: ["region", "port"], colLevels: [],
        values: [{ field: "weight", agg: "sum" }, { field: "crew", agg: "sum" }, { field: null, agg: "count" }],
        rowForm: "outline", expand: { rows: { depth: 2, except: new Set(["g:region=North", "g:region=East/port=Hull"]) }, cols: { depth: 99, except: new Set() } },
      }
    : {
        rowLevels: ["region", "port"], colLevels: ["quarter"],
        values: [{ field: "weight", agg: "sum" }, { field: null, agg: "count" }],
        rowForm: "outline", expand: { rows: { depth: 1, except: new Set() }, cols: { depth: 99, except: new Set() } },
      };

const params = new URL(location.href).searchParams;
let presetName: "grouped" | "pivot" = params.get("preset") === "pivot" ? "pivot" : "grouped";
let g: Grouping = preset(presetName);
if (params.get("form") === "header") g.rowForm = "header";
let cornerLayout: "tiers" | "compact" = params.get("corner") === "compact" ? "compact" : "tiers";
let pinThree = params.get("pin") === "3";
let themeName = params.get("theme") ?? "light";
/** Each pending toggle: the Group Key and the open state it asks for. */
const pending = new Map<string, { open: boolean; timer: number }>();
let bulkPending: string | null = null;
let lastAction = "Loaded";
const LATENCY = 450;

const valueName = (v: Value) => (v.field ? `${v.agg}(${v.field})` : "count");
const pathStr = (p: [Field, string][]) => p.map(([f, v]) => `${f}=${v}`).join("/");

function aggregate(ships: Ship[], v: Value): number | null {
  if (v.agg === "count") return ships.length;
  if (!ships.length || !v.field) return null;
  const xs = ships.map((s) => s[v.field!] as number);
  switch (v.agg) {
    case "sum": return xs.reduce((a, b) => a + b, 0);
    case "avg": return Math.round((xs.reduce((a, b) => a + b, 0) / xs.length) * 10) / 10;
    case "min": return Math.min(...xs);
    case "max": return Math.max(...xs);
  }
}
const fmt = (n: number | null) => (n == null ? "" : n.toLocaleString("en-GB"));

function distinct(ships: Ship[], f: Field): string[] {
  const vs = [...new Set(ships.map((s) => String(s[f])))];
  return NUMERIC.has(f) ? vs.sort((a, b) => +a - +b) : vs.sort();
}

const isOpen = (e: Expand, depth: number, key: string) => (depth < e.depth) !== e.except.has(key);

// ---------------------------------------------------------------- the fake source: items and outlines

type Kind = "leaf" | "group" | "subtotal" | "grand";
type OpenState = "open" | "closed" | "none";
/** What a Window answer would carry per item: the outline from #183. */
interface Item {
  key: string;
  kind: Kind;
  depth: number;
  path: [Field, string][];
  /** One Group Key per level the item sits under, outermost first. */
  ancestors: string[];
  parent: string | null;
  open: OpenState;
  ships: Ship[];
  ship?: Ship;
  /** Pivot columns only: which value. */
  value?: number;
}

const pivot = () => g.colLevels.length > 0;

function rowItems(): Item[] {
  const out: Item[] = [];
  const header = g.rowForm === "header" && cornerLayout !== "compact";
  const n = g.rowLevels.length;
  const walk = (ships: Ship[], level: number, path: [Field, string][], ancestors: string[]) => {
    const f = g.rowLevels[level]!;
    for (const v of distinct(ships, f)) {
      const sub = ships.filter((s) => String(s[f]) === v);
      const p: [Field, string][] = [...path, [f, v]];
      const key = `g:${pathStr(p)}`;
      const anc = [...ancestors, key];
      const canOpen = level < n - 1 || !pivot();
      const open: OpenState = canOpen ? (isOpen(g.expand.rows, level, key) ? "open" : "closed") : "none";
      const parent = ancestors.at(-1) ?? null;
      // In header form a group has no item of its own: its span stands for it.
      if (!header || open === "none") out.push({ key, kind: "group", depth: level, path: p, ancestors: anc, parent, open, ships: sub });
      if (open === "open") {
        if (level < n - 1) walk(sub, level + 1, p, anc);
        else for (const s of sub) out.push({ key: `l:${s.id}`, kind: "leaf", depth: level + 1, path: p, ancestors: anc, parent: key, open: "none", ships: [s], ship: s });
      }
      if (open !== "none" && (open === "open" || header))
        out.push({ key: `s:${pathStr(p)}`, kind: "subtotal", depth: level + 1, path: p, ancestors: anc, parent: key, open, ships: sub });
    }
  };
  if (n) walk(SHIPS, 0, [], []);
  else for (const s of SHIPS) out.push({ key: `l:${s.id}`, kind: "leaf", depth: 0, path: [], ancestors: [], parent: null, open: "none", ships: [s], ship: s });
  out.push({ key: "t:rows", kind: "grand", depth: 0, path: [], ancestors: [], parent: null, open: "none", ships: SHIPS });
  return out;
}

function colItems(): Item[] {
  if (!pivot()) {
    // The base columns, less those the row levels already show in the band.
    return FIELDS.filter((f) => !g.rowLevels.includes(f)).map((f) => ({
      key: `c:${f}`, kind: "leaf" as Kind, depth: 0, path: [[f, ""]] as [Field, string][], ancestors: [], parent: null, open: "none" as OpenState, ships: [],
    }));
  }
  const out: Item[] = [];
  const n = g.colLevels.length;
  const values = (base: Omit<Item, "key" | "value">, tag: string) =>
    g.values.forEach((v, i) => out.push({ ...base, key: `${tag}|${valueName(v)}`, value: i }));
  const walk = (ships: Ship[], level: number, path: [Field, string][], ancestors: string[]) => {
    const f = g.colLevels[level]!;
    for (const v of distinct(ships, f)) {
      const sub = ships.filter((s) => String(s[f]) === v);
      const p: [Field, string][] = [...path, [f, v]];
      const key = `g:${pathStr(p)}`;
      const anc = [...ancestors, key];
      const parent = ancestors.at(-1) ?? null;
      if (level < n - 1) {
        const open = isOpen(g.expand.cols, level, key);
        if (open) walk(sub, level + 1, p, anc);
        values({ kind: "subtotal", depth: level + 1, path: p, ancestors: anc, parent: key, open: open ? "open" : "closed", ships: sub }, `s:${pathStr(p)}`);
      } else values({ kind: "leaf", depth: level + 1, path: p, ancestors: anc, parent, open: "none", ships: sub }, `p:${pathStr(p)}`);
    }
  };
  walk(SHIPS, 0, [], []);
  values({ kind: "grand", depth: 0, path: [], ancestors: [], parent: null, open: "none", ships: SHIPS }, "t:cols");
  return out;
}

// ---------------------------------------------------------------- themes, with the prototype's Summary Item roles

interface SummaryRoles {
  groupItem: { color: string; background: string };
  subtotal: { color: string; background: string };
  grandTotal: { color: string; background: string; rule: string; cross: string };
  disclosure: string;
}
const THEMES: Record<string, [Theme, SummaryRoles]> = {
  light: [light, {
    groupItem: { color: "#0f172a", background: "#eef2f7" },
    subtotal: { color: "#1e40af", background: "#f5f8fc" },
    grandTotal: { color: "#0f172a", background: "#e2e8f0", rule: "#64748b", cross: "#cbd5e1" },
    disclosure: "#475569",
  }],
  "catppuccin mocha": [catppuccinMocha, {
    groupItem: { color: "#cdd6f4", background: "#313244" },
    subtotal: { color: "#74c7ec", background: "#181825" },
    grandTotal: { color: "#f9e2af", background: "#45475a", rule: "#6c7086", cross: "#585b70" },
    disclosure: "#89b4fa",
  }],
  mono: [mono, {
    groupItem: { color: "#000000", background: "#e6e6e6" },
    subtotal: { color: "#000000", background: "#f5f5f5" },
    grandTotal: { color: "#000000", background: "#d4d4d4", rule: "#000000", cross: "#bfbfbf" },
    disclosure: "#000000",
  }],
};

function applyTheme() {
  const [t, s] = THEMES[themeName] ?? THEMES.light!;
  const vars: Record<string, string> = {
    font: t.font, hfont: t.headerFont, "body-fg": t.body.color, "body-bg": t.body.background,
    "hdr-fg": t.header.color, "hdr-bg": t.header.background, "hdr-hover": t.headerHovered.background,
    "rn-fg": t.rowNumber.color, "rn-bg": t.rowNumber.background, border: t.border, handle: t.handle,
    focus: t.focus.color, sort: t.sortMarker, loading: t.loading, "pinned-edge": t.handle,
    "group-fg": s.groupItem.color, "group-bg": s.groupItem.background, "sub-fg": s.subtotal.color, "sub-bg": s.subtotal.background,
    "grand-fg": s.grandTotal.color, "grand-bg": s.grandTotal.background, "grand-rule": s.grandTotal.rule, "grand-cross": s.grandTotal.cross,
    disclosure: s.disclosure, "row-h": `${ROW_H}px`,
  };
  for (const [k, v] of Object.entries(vars)) document.documentElement.style.setProperty(`--${k}`, v);
}

// ---------------------------------------------------------------- layout

const ROW_H = 26;
const TIER_H = 28;
const OUTLINE_W = 230;
const LEVEL_W = 140;
const BASE_W: Record<Field, number> = { vessel: 120, region: 90, port: 110, quarter: 76, cargo: 100, weight: 90, crew: 70 };
const PIVOT_W = 108;

interface Layout {
  rows: Item[];
  cols: Item[];
  /** Columns in drawn order with their region. */
  drawn: { item: Item; region: "lead" | "scroll" | "trail" }[];
  band: number;
  bandW: number[];
  tiers: number;
  /** Header rows: the tiers, plus Excel's "Column labels" row in the compact corner. */
  headRows: number;
  compactExtra: boolean;
  header: boolean;
}
let L: Layout;

function layout(): Layout {
  const rows = rowItems();
  const cols = colItems();
  const header = g.rowForm === "header" && cornerLayout !== "compact" && g.rowLevels.length > 0;
  const band = header ? g.rowLevels.length : 1;
  const bandW = header ? g.rowLevels.map(() => LEVEL_W) : [OUTLINE_W];
  const trailKeys = pivot() ? cols.filter((c) => c.kind === "grand").map((c) => c.key) : [];
  const scrollable = cols.filter((c) => !trailKeys.includes(c.key));
  const leadKeys = pinThree ? scrollable.slice(0, 3).map((c) => c.key) : [];
  const drawn = [
    ...scrollable.filter((c) => leadKeys.includes(c.key)).map((item) => ({ item, region: "lead" as const })),
    ...scrollable.filter((c) => !leadKeys.includes(c.key)).map((item) => ({ item, region: "scroll" as const })),
    ...cols.filter((c) => trailKeys.includes(c.key)).map((item) => ({ item, region: "trail" as const })),
  ];
  const tiers = pivot() ? g.colLevels.length + (g.values.length > 1 ? 1 : 0) : 1;
  const compactExtra = cornerLayout === "compact" && pivot();
  return { rows, cols, drawn, band, bandW, tiers, headRows: tiers + (compactExtra ? 1 : 0), compactExtra, header };
}

const colW = (c: Item) => (pivot() ? PIVOT_W : BASE_W[c.path[0]![0]]);

// ---------------------------------------------------------------- focus

/** Focus by Key on each axis; a band column is `b:<level>` (`b:0` is the outline column). */
let focus: { row: string; col: string; ancestors: string[]; kind: Kind } = { row: "g:region=East/port=Felixstowe", col: "b:0", ancestors: [], kind: "group" };

function fixFocus() {
  if (!L.rows.some((r) => r.key === focus.row)) {
    // The focused item went inside a collapsed group: move to its Group
    // Item, or in header form to the Subtotal left under the span (#184).
    const keys = [...focus.ancestors].reverse();
    const hit = keys.map((k) => (L.header ? `s:${k.slice(2)}` : k)).find((k) => L.rows.some((r) => r.key === k))
      ?? (L.header ? keys.find((k) => L.rows.some((r) => r.key === k)) : undefined);
    focus.row = hit ?? L.rows[0]!.key;
  }
  const isBand = focus.col.startsWith("b:");
  if (isBand && +focus.col.slice(2) >= L.band) focus.col = `b:${L.band - 1}`;
  if (!isBand && !L.drawn.some((d) => d.item.key === focus.col)) focus.col = L.drawn[0]?.item.key ?? "b:0";
  const r = L.rows.find((r) => r.key === focus.row)!;
  focus.ancestors = r.ancestors;
  focus.kind = r.kind;
}

const colOrder = () => [...L.bandW.map((_, i) => `b:${i}`), ...L.drawn.map((d) => d.item.key)];

/** The Group Key a band cell stands for, in header form: the row's group at that level. */
function bandGroup(row: Item, level: number): string | null {
  if (!L.header) return row.kind === "group" ? row.key : null;
  return row.ancestors[level] ?? null;
}
/** The id a band cell merges by: rows sharing it at a level are one span (by Key, never by label). */
function bandId(row: Item, level: number): string {
  if (row.kind === "grand") return "t";
  const anc = row.ancestors[level];
  if (!anc) return `x:${row.key}`;
  if (row.kind === "subtotal" && level === row.depth - 1 && row.open === "closed") return anc;
  if (row.kind === "subtotal" && level >= row.depth) return `x:${row.key}`;
  return anc;
}

// ---------------------------------------------------------------- toggles, with a pending state

function groupDepth(key: string) {
  return key.slice(2).split("/").length - 1;
}

function currentOpen(axis: "rows" | "cols", key: string) {
  return isOpen(g.expand[axis], groupDepth(key), key);
}

/** Asks the fake source to open or close one group; it answers after LATENCY. A second toggle supersedes the first. */
function toggle(axis: "rows" | "cols", key: string, want?: boolean) {
  const p = pending.get(key);
  const target = want ?? (p ? !p.open : !currentOpen(axis, key));
  if (p) clearTimeout(p.timer);
  if (target === currentOpen(axis, key)) {
    pending.delete(key);
    lastAction = `Toggle ${key}: superseded, nothing to do`;
    render();
    return;
  }
  const timer = window.setTimeout(() => {
    pending.delete(key);
    const e = g.expand[axis];
    if (e.except.has(key)) e.except.delete(key); else e.except.add(key);
    lastAction = `Source answered: ${key} ${target ? "Expanded" : "Collapsed"}`;
    render();
  }, LATENCY);
  pending.set(key, { open: target, timer });
  lastAction = `View Edit ${target ? "Expand" : "Collapse"}(${key}): pending`;
  render();
}

function bulk(label: string, apply: () => void) {
  bulkPending = label;
  lastAction = `${label}: pending`;
  render();
  window.setTimeout(() => {
    bulkPending = null;
    apply();
    lastAction = `Source answered ${label} (Refiltered)`;
    render();
  }, LATENCY);
}

/** Every group under `key` (itself included), from the data. */
function descendants(key: string): string[] {
  const path = key.slice(2).split("/").map((s) => s.split("=") as [Field, string]);
  const ships = SHIPS.filter((s) => path.every(([f, v]) => String(s[f]) === v));
  const out: string[] = [key];
  const walk = (sub: Ship[], level: number, p: [Field, string][]) => {
    if (level >= g.rowLevels.length - (pivot() ? 1 : 0)) return;
    const f = g.rowLevels[level]!;
    for (const v of distinct(sub, f)) {
      const q: [Field, string][] = [...p, [f, v]];
      out.push(`g:${pathStr(q)}`);
      walk(sub.filter((s) => String(s[f]) === v), level + 1, q);
    }
  };
  walk(ships, path.length, path);
  return out;
}

function openAllUnder(key: string) {
  bulk(`Expand everything under ${key}`, () => {
    const e = g.expand.rows;
    for (const k of descendants(key)) {
      if (groupDepth(k) >= g.rowLevels.length - (pivot() ? 1 : 0)) continue;
      if (groupDepth(k) < e.depth) e.except.delete(k); else e.except.add(k);
    }
  });
}

// ---------------------------------------------------------------- keys

function focusedGroup(): string | null {
  const row = L.rows.find((r) => r.key === focus.row)!;
  if (focus.col.startsWith("b:")) return bandGroup(row, +focus.col.slice(2));
  return row.kind === "group" ? row.key : row.parent;
}

function move(dr: number, dc: number) {
  const order = colOrder();
  let ci = order.indexOf(focus.col);
  ci = Math.max(0, Math.min(order.length - 1, ci + dc));
  focus.col = order[ci]!;
  let ri = L.rows.findIndex((r) => r.key === focus.row);
  if (dr) {
    // In a band column a span is one cell: step over it.
    const lvl = focus.col.startsWith("b:") && L.header ? +focus.col.slice(2) : -1;
    const id = (i: number) => (lvl >= 0 ? bandId(L.rows[i]!, lvl) : String(i));
    const start = id(ri);
    do ri += dr; while (ri > 0 && ri < L.rows.length - 1 && id(ri) === start);
    ri = Math.max(0, Math.min(L.rows.length - 1, ri));
    if (lvl >= 0 && dr < 0) while (ri > 0 && id(ri - 1) === id(ri)) ri--;
  }
  focus.row = L.rows[ri]!.key;
}

const canOpen = (key: string) => groupDepth(key) < g.rowLevels.length - (pivot() ? 1 : 0);
/** The open state a band cell's group shows, or "none" when the cell is not a group's. */
function cellState(row: Item, level: number): { key: string | null; state: OpenState } {
  const gk = bandGroup(row, level);
  if (!gk) return { key: null, state: "none" };
  if (L.header ? bandId(row, level) !== gk : row.kind !== "group") return { key: null, state: "none" };
  if (!canOpen(gk)) return { key: gk, state: "none" };
  // A pending toggle counts as answered, so a second Left steps out rather than superseding.
  const p = pending.get(gk);
  return { key: gk, state: (p ? p.open : currentOpen("rows", gk)) ? "open" : "closed" };
}

/** Right opens a closed group, or steps in when it is open; elsewhere it moves right. */
function right() {
  const row = L.rows.find((r) => r.key === focus.row)!;
  if (!focus.col.startsWith("b:")) return move(0, 1);
  const { key, state } = cellState(row, +focus.col.slice(2));
  if (key && state === "closed") return toggle("rows", key);
  if (key && state === "open" && !L.header) {
    // Steps in: to the group's first child.
    const i = L.rows.findIndex((r) => r.key === row.key);
    focus.row = L.rows[i + 1]!.key;
    return;
  }
  move(0, 1);
}

/** Left closes an open group, or steps out to its parent. */
function left() {
  const row = L.rows.find((r) => r.key === focus.row)!;
  if (!focus.col.startsWith("b:")) return move(0, -1);
  const level = +focus.col.slice(2);
  const { key, state } = cellState(row, level);
  if (key && state === "open") return toggle("rows", key);
  if (L.header) {
    if (level > 0) {
      focus.col = `b:${level - 1}`;
      const id = bandId(row, level - 1);
      focus.row = L.rows.find((r) => bandId(r, level - 1) === id)!.key;
    }
    return;
  }
  if (row.parent && L.rows.some((r) => r.key === row.parent)) focus.row = row.parent;
}

const vp = document.querySelector<HTMLElement>("#viewport")!;

vp.addEventListener("keydown", (e) => {
  if (menuOpen) return;
  let handled = true;
  switch (e.key) {
    case "ArrowDown": move(1, 0); break;
    case "ArrowUp": move(-1, 0); break;
    case "ArrowRight": right(); break;
    case "ArrowLeft": left(); break;
    case "PageDown": move(Math.floor(vp.clientHeight / ROW_H) - 3, 0); break;
    case "PageUp": move(-(Math.floor(vp.clientHeight / ROW_H) - 3), 0); break;
    case "Home": focus.row = L.rows[0]!.key; break;
    case "End": focus.row = L.rows.at(-1)!.key; break;
    case "Enter": { const k = focusedGroup(); if (k && canOpen(k)) toggle("rows", k); break; }
    case "*": { const k = focusedGroup(); if (k) openAllUnder(k); break; }
    case "0": case "1": case "2": case "3": {
      const n = +e.key;
      bulk(n ? `ExpandTo(${n})` : "Collapse all (ExpandTo(0))", () => { g.expand.rows = { depth: n, except: new Set() }; });
      break;
    }
    case "m": case "ContextMenu": openMenuFor(focus.col, null); break;
    default: handled = false;
  }
  if (handled) {
    e.preventDefault();
    render();
    scrollFocusIntoView();
  }
});

// ---------------------------------------------------------------- the grouping menu

let menuOpen = false;
const overlay = document.querySelector<HTMLElement>("#overlay")!;

interface MenuItem { label: string; hint?: string; run?: () => void; disabled?: boolean }

function changeGrouping(label: string, edit: () => void) {
  // Changing the levels changes paths, so expand exceptions no longer fit: pruned (#185).
  edit();
  g.expand.rows.except.clear();
  g.expand.cols.except.clear();
  lastAction = `View Edit: ${label}`;
  render();
}

function fieldMenu(f: Field): MenuItem[] {
  const items: MenuItem[] = [];
  const used = g.rowLevels.includes(f) || g.colLevels.includes(f);
  items.push({ label: `Group rows by ${f}`, disabled: used, run: () => changeGrouping(`group rows by ${f}`, () => g.rowLevels.push(f)) });
  items.push({ label: `Add ${f} as a column level`, disabled: used || NUMERIC.has(f), run: () => changeGrouping(`add column level ${f}`, () => g.colLevels.push(f)) });
  const aggs: Agg[] = NUMERIC.has(f) ? ["sum", "avg", "min", "max"] : [];
  for (const a of aggs) {
    const has = g.values.some((v) => v.field === f && v.agg === a);
    items.push({ label: `Add a value: ${a}(${f})`, disabled: has, run: () => changeGrouping(`add value ${a}(${f})`, () => g.values.push({ field: f, agg: a })) });
  }
  if (!g.values.some((v) => v.agg === "count")) items.push({ label: "Add a value: count", run: () => changeGrouping("add value count", () => g.values.push({ field: null, agg: "count" })) });
  return items;
}

function levelMenu(axis: "rows" | "cols", f: Field): MenuItem[] {
  const levels = axis === "rows" ? g.rowLevels : g.colLevels;
  const other = axis === "rows" ? g.colLevels : g.rowLevels;
  const i = levels.indexOf(f);
  return [
    { label: `Remove ${f} level`, run: () => changeGrouping(`remove level ${f}`, () => levels.splice(i, 1)) },
    { label: `Move ${f} to ${axis === "rows" ? "columns" : "rows"}`, run: () => changeGrouping(`move ${f}`, () => { levels.splice(i, 1); other.push(f); }) },
    ...(axis === "rows"
      ? [{ label: `Collapse to ${f}`, hint: `ExpandTo(${i})`, run: () => bulk(`ExpandTo(${i})`, () => { g.expand.rows = { depth: i, except: new Set() }; }) },
         { label: `Expand through ${f}`, hint: `ExpandTo(${i + 1})`, run: () => bulk(`ExpandTo(${i + 1})`, () => { g.expand.rows = { depth: i + 1, except: new Set() }; }) }]
      : []),
  ];
}

function valuesMenu(): MenuItem[] {
  return [
    ...g.values.map((v, i) => ({ label: `Remove value ${valueName(v)}`, disabled: g.values.length < 2, run: () => changeGrouping(`remove value ${valueName(v)}`, () => g.values.splice(i, 1)) })),
    ...(["weight", "crew"] as Field[]).flatMap((f) => fieldMenu(f).filter((m) => m.label.startsWith("Add a value"))),
  ];
}

/** The menu for a column (or band column) Key, or a corner level. */
function openMenuFor(colKey: string, at: { x: number; y: number } | null, level?: { axis: "rows" | "cols" | "values"; field?: Field }) {
  let title = "";
  let items: MenuItem[] = [];
  if (level) {
    if (level.axis === "values") { title = "Values"; items = valuesMenu(); }
    else { title = `${level.axis === "rows" ? "Row" : "Column"} level: ${level.field}`; items = levelMenu(level.axis, level.field!); }
  } else if (colKey.startsWith("c:")) {
    const f = colKey.slice(2) as Field;
    title = `Column: ${f}`;
    items = fieldMenu(f);
  } else if (colKey.startsWith("b:")) {
    const f = L.header ? g.rowLevels[+colKey.slice(2)] : undefined;
    if (f) { title = `Row level: ${f}`; items = levelMenu("rows", f); }
    else { title = "Row levels"; items = g.rowLevels.flatMap((f) => levelMenu("rows", f).slice(0, 1)); }
  } else {
    const col = L.cols.find((c) => c.key === colKey)!;
    const f = col.path.at(-1)?.[0];
    title = `Pivot column: ${colKey}`;
    items = [...(f ? levelMenu("cols", f) : []), ...valuesMenu()];
  }
  const unused = FIELDS.filter((f) => !NUMERIC.has(f) && !g.rowLevels.includes(f) && !g.colLevels.includes(f));
  if (unused.length && !colKey.startsWith("c:")) {
    items.push({ label: "" });
    for (const f of unused) {
      items.push({ label: `Group rows by ${f}`, run: () => changeGrouping(`group rows by ${f}`, () => g.rowLevels.push(f)) });
      items.push({ label: `Add ${f} as a column level`, run: () => changeGrouping(`add column level ${f}`, () => g.colLevels.push(f)) });
    }
  }
  if (!colKey.startsWith("b:") && !level) {
    items.push({ label: "" });
    items.push({ label: pinThree ? "Unpin columns" : "Pin the first three columns left", run: () => { pinThree = !pinThree; render(); } });
  }
  const pos = at ?? (() => {
    const el = vp.querySelector<HTMLElement>(`[data-hcol="${CSS.escape(colKey)}"]`) ?? vp;
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.bottom };
  })();
  const menu = document.createElement("div");
  menu.className = "menu";
  menu.style.left = `${Math.min(pos.x, innerWidth - 260)}px`;
  menu.style.top = `${Math.min(pos.y, innerHeight - 40 - items.length * 28)}px`;
  menu.innerHTML = `<div class="title">${esc(title)}</div>`;
  const buttons: HTMLButtonElement[] = [];
  for (const it of items) {
    if (!it.label) { menu.append(document.createElement("hr")); continue; }
    const b = document.createElement("button");
    b.innerHTML = `${esc(it.label)}${it.hint ? `<span class="sub">${esc(it.hint)}</span>` : ""}`;
    b.disabled = !!it.disabled;
    b.onclick = () => { closeMenu(); it.run?.(); };
    menu.append(b);
    if (!it.disabled) buttons.push(b);
  }
  overlay.replaceChildren(menu);
  menuOpen = true;
  let hot = 0;
  const mark = () => buttons.forEach((b, i) => b.classList.toggle("hot", i === hot));
  mark();
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeMenu();
    else if (e.key === "ArrowDown") { hot = (hot + 1) % buttons.length; mark(); }
    else if (e.key === "ArrowUp") { hot = (hot - 1 + buttons.length) % buttons.length; mark(); }
    else if (e.key === "Enter") buttons[hot]?.click();
    else return;
    e.preventDefault();
  };
  const onDown = (e: MouseEvent) => { if (!menu.contains(e.target as Node)) closeMenu(); };
  document.addEventListener("keydown", onKey, true);
  setTimeout(() => document.addEventListener("mousedown", onDown, true));
  closeMenu = () => {
    overlay.replaceChildren();
    menuOpen = false;
    document.removeEventListener("keydown", onKey, true);
    document.removeEventListener("mousedown", onDown, true);
    closeMenu = () => {};
    vp.focus();
  };
}
let closeMenu = () => {};

// ---------------------------------------------------------------- drawing

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]!);

function markHtml(axis: "rows" | "cols", key: string, state: OpenState) {
  if (state === "none") return `<span class="mark none"></span>`;
  const p = pending.get(key);
  if (p) return `<span class="mark pending" data-toggle="${axis}|${esc(key)}" title="Waiting for the source">${p.open ? "▾" : "▸"}</span>`;
  return `<span class="mark" data-toggle="${axis}|${esc(key)}">${state === "open" ? "▾" : "▸"}</span>`;
}

const countOf = (it: Item) => (g.values.some((v) => v.agg === "count") ? `<span class="count">${it.ships.length}</span>` : "");

function render() {
  L = layout();
  fixFocus();
  const { rows, drawn, band, bandW, headRows, tiers } = L;
  const bodyW = drawn.map((d) => colW(d.item));
  const bandTotal = bandW.reduce((a, b) => a + b, 0);
  const leadW: number[] = [];
  let acc = bandTotal;
  drawn.forEach((d, i) => { leadW.push(acc); if (d.region === "lead") acc += bodyW[i]!; });
  const leadEnd = acc;
  const trailRight: number[] = new Array(drawn.length).fill(0);
  acc = 0;
  for (let i = drawn.length - 1; i >= 0; i--) if (drawn[i]!.region === "trail") { trailRight[i] = acc; acc += bodyW[i]!; }
  const lastLead = drawn.map((d) => d.region).lastIndexOf("lead");
  const firstTrail = drawn.findIndex((d) => d.region === "trail");

  const grid = document.createElement("div");
  grid.className = "grid";
  grid.style.gridTemplateColumns = [...bandW, ...bodyW].map((w) => `${w}px`).join(" ");
  grid.style.gridTemplateRows = `repeat(${headRows}, ${TIER_H}px) repeat(${rows.length}, ${ROW_H}px)`;
  const html: string[] = [];
  const cell = (r: number, c: number, h: number, w: number, cls: string, style: string, inner: string, attrs = "") =>
    html.push(`<div class="c ${cls}" style="grid-area:${r + 1}/${c + 1}/span ${h}/span ${w};${style}" ${attrs}>${inner}</div>`);
  const colPin = (i: number, last = i) => {
    const d = drawn[i]!;
    if (d.region === "lead") return { cls: `pin-lead${last === lastLead ? " pin-edge-lead" : ""}`, style: `left:${leadW[i]}px;` };
    if (d.region === "trail") return { cls: `pin-trail${i === firstTrail ? " pin-edge-trail" : ""}`, style: `right:${trailRight[last]}px;` };
    return { cls: "", style: "" };
  };

  // The column Header Band: tiers, spans merged by ancestor Key, broken at pinned edges.
  const top = (t: number) => `top:${t * TIER_H}px;`;
  const off = L.compactExtra ? 1 : 0;
  if (L.compactExtra) {
    html.push(`<div class="c h" style="grid-area:1/${band + 1}/span 1/span ${drawn.length};${top(0)}"><span class="lbl" style="position:sticky;left:${bandTotal}px">Column labels: ${g.colLevels.join(" › ")}</span></div>`);
  }
  for (let t = 0; t < tiers; t++) {
    let i = 0;
    while (i < drawn.length) {
      const c = drawn[i]!.item;
      const isLevel = pivot() && t < g.colLevels.length;
      const pathLen = c.kind === "subtotal" ? c.depth : c.path.length;
      if (isLevel && pathLen <= t && c.kind !== "leaf") {
        // A subtotal or grand total column: one cell down the tiers its path lacks.
        if (pathLen < t) { i++; continue; }
      }
      let id: string, label: string, mark = "", vspan = 1, extra = "";
      if (!pivot()) { id = c.key; label = c.path[0]![0]; }
      else if (isLevel && pathLen > t) { id = c.ancestors[t]!; label = c.path[t]![1]; }
      else if (isLevel) {
        id = `${c.kind}:${c.ancestors.at(-1) ?? "t"}`;
        label = c.kind === "grand" ? "All" : `${c.path.at(-1)![1]} total`;
        vspan = g.colLevels.length - t;
      } else { id = c.key; label = valueName(g.values[c.value!]!); }
      let j = i + 1;
      while (j < drawn.length && drawn[j]!.region === drawn[i]!.region && headerId(drawn[j]!.item, t) === id) j++;
      if (isLevel && pathLen > t && t < g.colLevels.length - 1) {
        const open = c.kind === "subtotal" && c.depth === t + 1 ? c.open : currentOpen("cols", id) ? "open" : "closed";
        mark = markHtml("cols", id, open);
      }
      const pin = colPin(i, j - 1);
      const kindCls = c.kind === "grand" ? " col-grand" : c.kind === "subtotal" && (pathLen <= t || !isLevel) ? " col-sub" : "";
      const span = j - i > 1 || vspan > 1;
      const numCls = !isLevel && pivot() ? " num" : !pivot() && NUMERIC.has(c.path[0]![0]) ? " num" : "";
      const labelLeft = drawn[i]!.region === "scroll" ? leadEnd : bandTotal;
      if (span) extra = `--label-left:${labelLeft}px;`;
      cell(off + t, band + i, vspan, j - i, `h${span ? " span" : ""}${numCls}${kindCls} ${pin.cls}${pin.cls ? " pinned" : ""}`,
        `${top(off + t)}${pin.style}${extra}`,
        `<span class="lbl">${mark}${esc(label)}</span>${t === tiers - 1 || vspan > 1 ? `<button class="hbtn" data-menu="${esc(c.key)}">▾</button>` : ""}`,
        `data-hcol="${esc(c.key)}"`);
      i = j;
    }
  }

  // The corner grid where the band columns meet the tiers.
  drawCorner(html, band, headRows, tiers);

  // The body and the row Header Band.
  const gtIndex = rows.length - 1;
  const focusRowIdx = rows.findIndex((r) => r.key === focus.row);
  rows.forEach((row, ri) => {
    const r = headRows + ri;
    const kcls = row.kind === "group" && row.open !== "none" ? "k-group" : row.kind === "subtotal" ? "k-sub" : row.kind === "grand" ? "k-grand" : "";
    const bottom = ri === gtIndex ? " pin-bottom" : "";
    // Band cells.
    if (!L.header) {
      const indent = row.kind === "grand" ? 0 : row.depth * 16;
      let inner = "";
      if (row.kind === "group") inner = `${markHtml("rows", row.key, row.open)}<span class="lbl">${esc(row.path.at(-1)![1])}</span>${countOf(row)}`;
      else if (row.kind === "subtotal") inner = `<span class="mark none"></span><span class="lbl">${esc(row.path.at(-1)![1])} total</span>`;
      else if (row.kind === "grand") inner = `<span class="lbl">Grand total</span>${countOf(row)}`;
      else inner = `<span class="mark none"></span><span class="lbl" style="color:var(--rn-fg)">#${row.ship!.id}</span>`;
      const f = focus.col === "b:0" && ri === focusRowIdx ? " focus" : "";
      cell(r, 0, 1, 1, `b ${kcls}${bottom}${f}`, `left:0;padding-left:${8 + indent}px;`, inner, `data-row="${ri}" data-col="b:0"`);
    } else {
      for (let lvl = 0; lvl < band; lvl++) {
        const id = bandId(row, lvl);
        if (ri > 0 && ri !== gtIndex && bandId(rows[ri - 1]!, lvl) === id && rows[ri - 1]!.kind !== "grand") continue;
        if (lvl > 0 && bandId(row, lvl - 1) === id) continue;
        let h = 1;
        while (ri + h < gtIndex && bandId(rows[ri + h]!, lvl) === id) h++;
        let w = 1;
        while (lvl + w < band && bandId(row, lvl + w) === id) w++;
        let inner: string;
        let cls = "";
        if (row.kind === "grand") { inner = `<span class="lbl">Grand total</span>${countOf(row)}`; cls = "k-grand"; }
        else if (id.startsWith("x:")) { inner = `<span class="lbl">${esc(row.path.at(-1)![1])} total</span>`; cls = "k-sub"; }
        else {
          const gk = id;
          const state: OpenState = !canOpen(gk) ? "none" : currentOpen("rows", gk) ? "open" : "closed";
          const label = gk.slice(2).split("/").at(-1)!.split("=")[1]!;
          inner = `<span class="lbl">${markHtml("rows", gk, state)}${esc(label)}${state === "closed" ? `<span class="count">total</span>` : ""}</span>`;
          cls = h > 1 ? "vspan" : "";
          if (state === "closed") cls += " k-sub";
        }
        const focused = focus.col.startsWith("b:") && +focus.col.slice(2) >= lvl && +focus.col.slice(2) < lvl + w && focusRowIdx >= ri && focusRowIdx < ri + h ? " focus" : "";
        const left = bandW.slice(0, lvl).reduce((a, b) => a + b, 0);
        cell(r, lvl, h, w, `b ${cls}${bottom}${focused}`, `left:${left}px;--label-top:${headRows * TIER_H}px;`, inner, `data-row="${ri}" data-col="b:${lvl}"`);
      }
    }
    // Body cells.
    drawn.forEach((d, i) => {
      const c = d.item;
      let text = "";
      let num = false;
      if (!pivot()) {
        const f = c.path[0]![0];
        num = NUMERIC.has(f);
        if (row.kind === "leaf") text = String(row.ship![f]);
        else {
          const v = g.values.find((v) => v.field === f);
          // Excel: an open Group Item with a Subtotal after it stays blank.
          const blank = row.kind === "group" && row.open === "open";
          if (v && !blank) text = fmt(aggregate(row.ships, v));
        }
      } else {
        num = true;
        const blank = row.kind === "group" && row.open === "open";
        if (!blank) {
          const ships = row.ships.filter((s) => c.path.every(([f, v]) => String(s[f]) === v));
          text = fmt(aggregate(ships, g.values[c.value!]!));
        }
      }
      const pin = colPin(i);
      const ccls = c.kind === "grand" && pivot() ? " col-grand" : c.kind === "subtotal" ? " col-sub" : "";
      const f = focus.col === c.key && ri === focusRowIdx ? " focus" : "";
      cell(r, band + i, 1, 1, `${num ? "num " : ""}${kcls}${ccls}${bottom} ${pin.cls}${f}`, pin.style, esc(text), `data-row="${ri}" data-col="${esc(c.key)}"`);
    });
  });

  grid.innerHTML = html.join("");
  vp.replaceChildren(grid);
  vp.style.setProperty("--label-left", `${bandTotal}px`);
  renderHarness();
  renderState();
}

function headerId(c: Item, t: number): string {
  if (!pivot()) return c.key;
  if (t < g.colLevels.length) {
    const pathLen = c.kind === "subtotal" ? c.depth : c.path.length;
    if (pathLen > t) return c.ancestors[t]!;
    return `${c.kind}:${c.ancestors.at(-1) ?? "t"}`;
  }
  return c.key;
}

function drawCorner(html: string[], band: number, headRows: number, tiers: number) {
  const lvl = (axis: "rows" | "cols" | "values", f: string) =>
    `<span class="lvl${axis === "values" ? " values" : ""}" data-level="${axis}|${f}">${esc(axis === "values" ? "Σ values" : f)}</span>`;
  const caption = pivot() ? (g.values.length > 1 ? g.values.map(valueName).join(", ") : valueName(g.values[0]!)) : "";
  const leftOf = (c: number) => L.bandW.slice(0, c).reduce((a, b) => a + b, 0);
  const put = (r: number, c: number, w: number, cls: string, inner: string) =>
    html.push(`<div class="c h corner ${cls}" style="grid-area:${r + 1}/${c + 1}/span 1/span ${w};top:${r * TIER_H}px;left:${leftOf(c)}px">${inner}</div>`);
  if (cornerLayout === "compact") {
    // Excel's compact form: the values caption top-left, "Row labels" over the one indented column.
    for (let r = 0; r < headRows; r++) {
      if (r === 0 && pivot()) put(r, 0, band, "", `<span class="cap">${esc(g.values.length > 1 ? "Σ Values" : `Sum of ${g.values[0]!.field ?? "count"}`)}</span>`);
      else if (r === headRows - 1) put(r, 0, band, "", `<span class="lvl" data-level="rows|*">Row labels ▾</span><span class="spacer"></span>${pivot() && g.values.length > 1 ? `<span class="cap">Values</span>` : ""}`);
      else put(r, 0, band, "empty", "");
    }
    return;
  }
  // Tiers: column level names right-aligned in their tier rows, row level
  // names along the bottom row, the values caption on top.
  const rowNames = L.header
    ? g.rowLevels.map((f) => lvl("rows", f))
    : [g.rowLevels.map((f) => lvl("rows", f)).join(`<span class="sep">›</span>`)];
  for (let t = 0; t < tiers; t++) {
    const colName = !pivot() ? "" : t < g.colLevels.length ? `${lvl("cols", g.colLevels[t]!)}<span class="sep">→</span>` : `${lvl("values", "")}<span class="sep">→</span>`;
    const cap = t === 0 && caption ? `<span class="cap">${esc(caption)}</span>` : "";
    if (t < tiers - 1) {
      if (band > 1) {
        put(t, 0, band - 1, "", cap);
        put(t, band - 1, 1, "right", colName);
      } else put(t, 0, 1, "", `${cap}<span class="spacer"></span>${colName}`);
    } else {
      // The bottom row: row level names, and the bottom tier's own name at the right.
      if (L.header) rowNames.forEach((n, c) => put(t, c, 1, "", c === band - 1 ? `${n}${t === 0 ? cap : ""}<span class="spacer"></span>${colName}` : n));
      else put(t, 0, 1, "", `${rowNames[0]}${t === 0 && cap ? ` ${cap}` : ""}<span class="spacer"></span>${colName}`);
    }
  }
}

function scrollFocusIntoView() {
  const el = vp.querySelector<HTMLElement>(".focus");
  if (!el) return;
  const head = L.headRows * TIER_H;
  const r = el.getBoundingClientRect();
  const v = vp.getBoundingClientRect();
  const foot = L.rows.at(-1)!.key === focus.row ? 0 : ROW_H;
  if (r.top < v.top + head) vp.scrollTop -= v.top + head - r.top;
  else if (r.bottom > v.bottom - foot - 14) vp.scrollTop += r.bottom - (v.bottom - foot - 14);
  if (!focus.col.startsWith("b:")) {
    const bandW = L.bandW.reduce((a, b) => a + b, 0);
    if (r.left < v.left + bandW) vp.scrollLeft -= v.left + bandW - r.left;
    else if (r.right > v.right - 120) vp.scrollLeft += r.right - (v.right - 120);
  }
}

// ---------------------------------------------------------------- pointer

vp.addEventListener("click", (e) => {
  const t = e.target as HTMLElement;
  const tog = t.closest<HTMLElement>("[data-toggle]");
  if (tog) {
    const [axis, key] = tog.dataset.toggle!.split("|") as ["rows" | "cols", string];
    toggle(axis, key);
    return;
  }
  const mb = t.closest<HTMLElement>("[data-menu]");
  if (mb) { const r = mb.getBoundingClientRect(); openMenuFor(mb.dataset.menu!, { x: r.left, y: r.bottom }); return; }
  const lv = t.closest<HTMLElement>("[data-level]");
  if (lv) {
    const [axis, f] = lv.dataset.level!.split("|") as ["rows" | "cols" | "values", string];
    const r = lv.getBoundingClientRect();
    openMenuFor(axis === "rows" && f === "*" ? "b:0" : "", { x: r.left, y: r.bottom }, f === "*" ? undefined : { axis, field: f as Field });
    return;
  }
  const c = t.closest<HTMLElement>("[data-row]");
  if (c) {
    focus.row = L.rows[+c.dataset.row!]!.key;
    focus.col = c.dataset.col!;
    lastAction = "Click";
    render();
  }
});
vp.addEventListener("contextmenu", (e) => {
  const h = (e.target as HTMLElement).closest<HTMLElement>("[data-hcol]");
  if (!h) return;
  e.preventDefault();
  openMenuFor(h.dataset.hcol!, { x: e.clientX, y: e.clientY });
});

// ---------------------------------------------------------------- harness and state

const harness = document.querySelector<HTMLElement>("#harness")!;
function renderHarness() {
  const btn = (on: boolean, label: string, act: string, disabled = false) =>
    `<button class="${on ? "on" : ""}" data-act="${act}" ${disabled ? "disabled" : ""}>${label}</button>`;
  harness.innerHTML = `<b>PROTOTYPE #186</b>
    <span class="grp">Preset ${btn(presetName === "grouped", "Grouped", "preset:grouped")}${btn(presetName === "pivot", "Pivot", "preset:pivot")}</span>
    <span class="grp">Rows ${btn(g.rowForm === "outline", "Outline form", "form:outline", cornerLayout === "compact")}${btn(g.rowForm === "header", "Header form", "form:header", cornerLayout === "compact")}</span>
    <span class="grp">Corner ${btn(cornerLayout === "tiers", "Tiers", "corner:tiers")}${btn(cornerLayout === "compact", "Compact (Excel)", "corner:compact")}</span>
    <span class="grp">${btn(pinThree, "Pin 3 columns left", "pin")}</span>
    <span class="grp">Theme <select data-act="theme">${Object.keys(THEMES).map((n) => `<option ${n === themeName ? "selected" : ""}>${n}</option>`).join("")}</select></span>`;
}
harness.addEventListener("click", (e) => {
  const act = (e.target as HTMLElement).closest<HTMLElement>("button")?.dataset.act;
  if (!act) return;
  const [k, v] = act.split(":");
  if (k === "preset") { presetName = v as typeof presetName; const form = g.rowForm; g = preset(presetName); g.rowForm = form; }
  if (k === "form") g.rowForm = v as Form;
  if (k === "corner") cornerLayout = v as typeof cornerLayout;
  if (k === "pin") pinThree = !pinThree;
  lastAction = `Harness: ${act}`;
  render();
  vp.focus();
});
harness.addEventListener("change", (e) => {
  themeName = (e.target as HTMLSelectElement).value;
  applyTheme();
  render();
  vp.focus();
});

const statePane = document.querySelector<HTMLElement>("#state")!;
const status = document.querySelector<HTMLElement>("#status")!;
function renderState() {
  const row = L.rows.find((r) => r.key === focus.row)!;
  const ex = (e: Expand) => `{ depth: ${e.depth === 99 ? '"all"' : e.depth}, except: [${[...e.except].map((k) => `\n      "${k}"`).join(",")}${e.except.size ? "\n    " : ""}] }`;
  statePane.textContent = `grouping: {
  rowLevels: [${g.rowLevels.map((f) => `"${f}"`).join(", ")}],   // subtotal: after
  colLevels: [${g.colLevels.map((f) => `"${f}"`).join(", ")}],
  values: [${g.values.map(valueName).join(", ")}],
  rowForm: "${L.header ? "header" : "outline"}", colForm: "header",
  grandTotal: { rows: "after" (pinned), cols: "after" (pinned) },
  expand: {
    rows: ${ex(g.expand.rows)},
    cols: ${ex(g.expand.cols)},
  },
}
corner: ${cornerLayout}

focus: ${focus.row} × ${focus.col}
outline of the focused row: {
  kind: ${row.kind},
  depth: ${row.depth},
  parent: ${row.parent ?? "none"},
  open: ${row.open},
  ancestors: [${row.ancestors.join(", ")}],
  field: ${row.kind === "group" || row.kind === "subtotal" ? row.path.at(-1)![0] : "-"},
  value: ${row.kind === "group" || row.kind === "subtotal" ? row.path.at(-1)![1] : "-"},
}

pending: ${[...pending].map(([k, p]) => `${p.open ? "Expand" : "Collapse"}(${k})`).join(", ") || (bulkPending ?? "none")}
items: ${L.rows.length} rows × ${L.cols.length} columns
last: ${lastAction}`;
  status.textContent = `${row.kind === "grand" ? "Grand total" : row.kind === "leaf" ? `Shipment #${row.ship!.id}` : `${row.kind === "group" ? "Group" : "Subtotal"} ${row.path.map(([, v]) => v).join(" › ")}`} · ${focus.col}  ·  → open / step in   ← close / step out   * open all under   0-3 expand to level   Enter toggle   m menu   ·  ${lastAction}`;
}

applyTheme();
render();
vp.focus();
