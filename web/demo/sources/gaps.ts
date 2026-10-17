// The gaps source: the grid feature-gap research as a master/detail page.
// One row per feature, first by demand times fit; every column sorts, and the
// toolbar filters by a status in either adapter and by kind. The panel
// beside the grid follows `onSelectionChange` by row ID, so it stays on its
// feature through a sort or a filter. Its product table is a second grid.

import { PAD } from "../../src/canvas/context";
import {
  byColumn,
  fromRows,
  GridWidget,
  textRenderer,
  type CellCanvas,
  type CellRenderer,
  type RowsSource,
  type Theme,
  type ThemeChoice,
} from "../../src";
import type { DemoSource } from "./demo-source";
import { ADAPTERS, features, keeps, KINDS, PRODUCTS, sample, SCALE, STATUSES, type Feature, type GapFilter } from "./gaps-data";

type Value = Feature[keyof Feature];

const ISSUES = "https://github.com/flotilla-org/binnacle/issues/";

/** Whether a Theme draws on a dark background. */
function dark(theme: Theme): boolean {
  const hex = /^#([0-9a-f]{6})$/i.exec(theme.body.background)?.[1];
  if (!hex) return false;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5;
}

/** A colour for each light and dark Theme. */
type Tone = readonly [light: string, dark: string];

const TONES: Record<string, Tone> = {
  shipped: ["#15803d", "#a6e3a1"],
  yes: ["#15803d", "#a6e3a1"],
  partial: ["#b45309", "#f9e2af"],
  planned: ["#1d4ed8", "#89b4fa"],
  plugin: ["#1d4ed8", "#89b4fa"],
  paid: ["#7e22ce", "#cba6f7"],
  missing: ["#be123c", "#f38ba8"],
};
const QUIET: Tone = ["#94a3b8", "#6c7086"];

const tone = (status: string): Tone => TONES[status] ?? QUIET;
const pick = (t: Tone, theme: Theme) => t[dark(theme) ? 1 : 0];

/** A status as a coloured dot and its word. */
const statusRenderer: CellRenderer<Value> = {
  draw(cell, value) {
    if (value == null) return;
    const { ctx } = cell;
    const r = 4;
    ctx.fillStyle = pick(tone(String(value)), cell.theme);
    ctx.beginPath();
    ctx.arc(PAD + r, cell.height / 2, r, 0, 2 * Math.PI);
    ctx.fill();
    ctx.save();
    ctx.translate(2 * r + 4, 0);
    cell.text(String(value));
    ctx.restore();
  },
  preferredWidth: (value, measure) => measure.text(String(value)) + 12,
  text: (value) => String(value ?? ""),
};

/** A score out of SCALE as a bar with its number after it. */
function scoreRenderer(colour: Tone): CellRenderer<Value> {
  const NUMBER = 18;
  return {
    draw(cell: CellCanvas, value) {
      const score = Number(value);
      const { ctx } = cell;
      const across = Math.max(0, cell.width - 2 * PAD - NUMBER - 6);
      const thick = 8;
      const y = (cell.height - thick) / 2;
      ctx.fillStyle = cell.theme.border;
      ctx.beginPath();
      ctx.roundRect(PAD, y, across, thick, thick / 2);
      ctx.fill();
      if (score > 0) {
        ctx.fillStyle = pick(colour, cell.theme);
        ctx.beginPath();
        ctx.roundRect(PAD, y, Math.max(thick, (score / SCALE) * across), thick, thick / 2);
        ctx.fill();
      }
      cell.text(String(score), "end");
    },
    preferredWidth: () => 120,
    text: (value) => `${String(value)} of ${SCALE}`,
  };
}

const DEMAND: Tone = ["#2563eb", "#89b4fa"];
const FIT: Tone = ["#0d9488", "#94e2d5"];
const productCount = textRenderer<Value>({ align: "end", format: (v) => `${String(v)} of ${PRODUCTS.length}` });

const renderers: Partial<Record<keyof Feature, CellRenderer<Value>>> = {
  ratatui: statusRenderer,
  web: statusRenderer,
  demand: scoreRenderer(DEMAND),
  fit: scoreRenderer(FIT),
  products: productCount,
};

interface Support {
  product: string;
  support: string;
}

const supportRenderer = statusRenderer as CellRenderer<unknown>;

/** Builds an element with a class and children. */
function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  e.append(...children);
  return e;
}

function chip(text: string, status?: string): HTMLElement {
  const c = el("span", status === undefined ? "chip" : "chip status", text);
  if (status !== undefined) c.dataset.status = status;
  return c;
}

function meter(label: string, score: number, kind: string): HTMLElement {
  const bar = el("span", "meter-bar");
  bar.style.setProperty("--fill", `${(score / SCALE) * 100}%`);
  return el("div", `meter ${kind}`, el("span", "meter-label", label), bar, el("span", "meter-value", `${score} of ${SCALE}`));
}

function link(href: string, text: string): HTMLAnchorElement {
  const a = el("a", "", text);
  a.href = href;
  a.target = "_blank";
  a.rel = "noopener noreferrer";
  return a;
}

/** The detail panel: a feature's description, binnacle's status, its products and its evidence. */
function createPanel(look: { theme: ThemeChoice; border: GridWidget["border"] }) {
  const aside = el("aside", "gap-detail");
  aside.setAttribute("aria-label", "Feature detail");
  aside.setAttribute("aria-live", "polite");
  const empty = el("p", "gap-empty", "Pick a feature in the table to read its research.");
  const body = el("div", "gap-body");
  const title = el("h2", "");
  const tags = el("div", "gap-tags");
  const meters = el("div", "gap-meters");
  const description = el("p", "gap-description");
  const statuses = el("dl", "gap-status");
  const note = el("p", "gap-note");
  const issue = el("p", "gap-issue");
  const table = el("div", "gap-products");
  const evidence = el("ol", "gap-evidence");
  body.append(
    title,
    tags,
    meters,
    description,
    el("h3", "", "binnacle"),
    statuses,
    note,
    issue,
    el("h3", "", "Products"),
    table,
    el("h3", "", "Evidence"),
    evidence,
  );
  aside.append(empty, body);
  if (sample) aside.prepend(el("p", "gap-sample", "Sample data: the research has not landed yet."));
  body.hidden = true;

  // Fits every product's row under the header band.
  table.style.height = `${30 + PRODUCTS.length * 24}px`;
  const support: Support[] = PRODUCTS.map((p) => ({ product: p.name, support: "" }));
  const source: RowsSource<Support> = fromRows(support, [
    { key: "product", label: "Product" },
    { key: "support", label: "Support", renderer: supportRenderer },
  ]);
  const grid = GridWidget.create(table, {
    source,
    label: "Product support",
    columnWidth: 150,
    theme: look.theme,
    border: look.border,
  });

  let shown: Feature | null = null;
  return {
    element: aside,
    grid,
    get feature() {
      return shown;
    },
    show(f: Feature) {
      if (f === shown) return;
      shown = f;
      empty.hidden = true;
      body.hidden = false;
      aside.dataset.feature = f.id;
      title.textContent = f.name;
      tags.replaceChildren(chip(f.kind), chip(f.category));
      meters.replaceChildren(meter("Demand", f.demand, "demand"), meter("Fit", f.fit, "fit"));
      description.textContent = f.description;
      statuses.replaceChildren(...ADAPTERS.flatMap((a) => [el("dt", "", a), el("dd", "", chip(f[a], f[a]))]));
      note.textContent = f.note;
      issue.replaceChildren(
        ...(f.issue === null
          ? ["No issue filed yet."]
          : [
              "Tracked in ",
              typeof f.issue === "number" ? link(`${ISSUES}${f.issue}`, `#${f.issue}`) : link(f.issue, f.issue),
              ".",
            ]),
      );
      PRODUCTS.forEach((p, i) => {
        support[i]!.support = f.support[p.id] ?? "unknown";
        source.changed(i);
      });
      evidence.replaceChildren(
        ...f.evidence.map((e) => el("li", "", link(e.url, e.title), el("span", "gap-host", new URL(e.url).host))),
      );
    },
    destroy() {
      void grid.then((g) => g.destroy());
      source.destroy();
    },
  };
}

/** The status and kind selects, writing the filter they choose. */
function createFilters(changed: (filter: GapFilter) => void) {
  const status = el("select", "");
  status.setAttribute("aria-label", "Filter by status");
  status.add(new Option("Any status", ""));
  for (const adapter of ADAPTERS) {
    const group = el("optgroup", "");
    group.label = adapter;
    for (const s of STATUSES) group.append(new Option(`${adapter}: ${s}`, `${adapter}:${s}`));
    status.append(group);
  }
  const kind = el("select", "");
  kind.setAttribute("aria-label", "Filter by kind");
  kind.add(new Option("Any kind", ""));
  for (const k of KINDS) kind.add(new Option(k, k));
  const read = (): GapFilter => {
    const [adapter, s] = status.value.split(":");
    const known = ADAPTERS.find((a) => a === adapter);
    return { status: known && s !== undefined ? { adapter: known, status: s } : null, kind: kind.value || null };
  };
  status.addEventListener("change", () => changed(read()));
  kind.addEventListener("change", () => changed(read()));
  return [status, kind] as const;
}

export const gaps: DemoSource = {
  name: "gaps",
  description: `${features.length} grid features: demand, fit and binnacle's status, with the evidence${sample ? " (sample data)" : ""}`,
  async mount(host, toolbar, look) {
    const source = fromRows(features as Feature[], [
      { key: "name", label: "Feature" },
      { key: "kind", label: "Kind" },
      { key: "category", label: "Category" },
      { key: "ratatui", label: "ratatui" },
      { key: "web", label: "web" },
      { key: "demand", label: "Demand" },
      { key: "fit", label: "Fit" },
      { key: "products", label: "Products" },
    ]);
    const filters = createFilters((filter) => source.filter((f) => keeps(f, filter)));
    toolbar.append(...filters);

    const layout = el("div", "gaps");
    const master = el("div", "gaps-grid");
    const panel = createPanel(look);
    layout.append(master, panel.element);
    host.append(layout);

    // Columns share the width the grid gets, so wide screens show whole names.
    const columns = 8;
    const columnWidth = Math.max(110, Math.floor((master.clientWidth - 48) / columns));
    const widget = await GridWidget.create(master, {
      source,
      columnWidth,
      renderer: byColumn<keyof Feature, Value>((col) => renderers[col]),
      ...look,
      onSelectionChange(change) {
        look.onSelectionChange(change);
        // Keeps the last feature when focus goes, so a filter that hides it leaves it readable.
        const id = change.focus?.rowId;
        if (id !== undefined) panel.show(features[id]!);
      },
    });
    const products = await panel.grid;
    return {
      get theme() {
        return widget.theme;
      },
      set theme(choice) {
        widget.theme = choice;
        products.theme = choice;
      },
      get border() {
        return widget.border;
      },
      set border(style) {
        widget.border = style;
        products.border = style;
      },
      destroy() {
        widget.destroy();
        panel.destroy();
        source.destroy();
        layout.remove();
        for (const f of filters) f.remove();
      },
    };
  },
};
