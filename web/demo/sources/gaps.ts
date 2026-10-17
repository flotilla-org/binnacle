// The gaps source: the grid feature-gap research as a master/detail page.
// One row per feature, first by demand times fit; every column sorts, and the
// toolbar filters by status and kind. The panel beside the grid follows
// `onSelectionChange` by row ID, so it stays on its feature through a sort or
// a filter. Its product table is a second grid.

import { PAD } from "../../src/canvas/context";
import {
  fromRows,
  GridWidget,
  textRenderer,
  type CellCanvas,
  type CellRenderer,
  type InitialSize,
  type RowsSource,
  type Theme,
  type ThemeChoice,
} from "../../src";
import type { DemoSource } from "./demo-source";
import {
  ADAPTERS,
  codeStatus,
  features,
  keeps,
  KINDS,
  PRODUCTS,
  SCALE,
  STATUSES,
  type Feature,
  type GapFilter,
  type Status,
  type Support,
} from "./gaps-data";

type Value = Feature[keyof Feature];

const ISSUES = "https://github.com/flotilla-org/binnacle/issues/";

/** Whether a Theme draws on a dark background. */
function dark(theme: Theme): boolean {
  const hex = /^#([0-9a-f]{6})$/i.exec(theme.body.background)?.[1];
  if (!hex) return false;
  const [r, g, b] = [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b < 0.5;
}

/** A colour for light Themes and one for dark. */
type Tone = readonly [light: string, dark: string];

const GREEN: Tone = ["#15803d", "#a6e3a1"];
const AMBER: Tone = ["#b45309", "#f9e2af"];
const RED: Tone = ["#be123c", "#f38ba8"];
const BLUE: Tone = ["#1d4ed8", "#89b4fa"];
const VIOLET: Tone = ["#7e22ce", "#cba6f7"];
const GREY: Tone = ["#94a3b8", "#7f849c"];

const pick = (t: Tone, theme: Theme) => t[dark(theme) ? 1 : 0];

/** How a mark draws: filled, half filled, a ring, a struck ring, or a dash. */
type Shape = "fill" | "half" | "ring" | "struck" | "dash";

const STATUS_MARK: Record<Status, [Shape, Tone]> = {
  has: ["fill", GREEN],
  partial: ["half", AMBER],
  missing: ["ring", RED],
  "out-of-scope": ["struck", GREY],
  "n/a": ["dash", GREY],
};

const SUPPORT_MARK: Record<Support, [Shape, Tone]> = {
  core: ["fill", GREEN],
  plugin: ["fill", BLUE],
  paid: ["fill", VIOLET],
  partial: ["half", AMBER],
  no: ["ring", RED],
  unknown: ["dash", GREY],
};

const R = 4.5;

/** Draws a mark centred at (x, y). */
function mark(ctx: CanvasRenderingContext2D, x: number, y: number, shape: Shape, colour: string): void {
  ctx.fillStyle = colour;
  ctx.strokeStyle = colour;
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  switch (shape) {
    case "fill":
      ctx.arc(x, y, R, 0, 2 * Math.PI);
      ctx.fill();
      return;
    case "half":
      ctx.arc(x, y, R - 0.75, 0, 2 * Math.PI);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(x, y, R, Math.PI / 2, (3 * Math.PI) / 2);
      ctx.fill();
      return;
    case "ring":
      ctx.arc(x, y, R - 0.75, 0, 2 * Math.PI);
      ctx.stroke();
      return;
    case "struck":
      ctx.arc(x, y, R - 0.75, 0, 2 * Math.PI);
      ctx.moveTo(x - R * 0.7, y + R * 0.7);
      ctx.lineTo(x + R * 0.7, y - R * 0.7);
      ctx.stroke();
      return;
    case "dash":
      ctx.moveTo(x - R, y);
      ctx.lineTo(x + R, y);
      ctx.stroke();
  }
}

/** The three adapters' statuses as marks in a row, core then ratatui then web. */
const binnacleRenderer: CellRenderer<Value> = {
  draw(cell, value) {
    const code = String(value);
    const step = 3 * R + 6;
    [...code].forEach((c, i) => {
      const [shape, tone] = STATUS_MARK[codeStatus(c)];
      mark(cell.ctx, PAD + R + i * step, cell.height / 2, shape, pick(tone, cell.theme));
    });
  },
  preferredWidth: () => 2 * PAD + 9 * R + 12,
  text: (value) => [...String(value)].map((c, i) => `${ADAPTERS[i]} ${codeStatus(c)}`).join(", "),
};

/** A support level as a mark and its word; `unknown` faint, so it reads apart from `no`. */
const supportRenderer: CellRenderer<unknown> = {
  draw(cell, value) {
    const support = String(value) as Support;
    const [shape, tone] = SUPPORT_MARK[support] ?? SUPPORT_MARK.unknown;
    const { ctx } = cell;
    mark(ctx, PAD + R, cell.height / 2, shape, pick(tone, cell.theme));
    ctx.save();
    if (support === "unknown") ctx.globalAlpha = 0.55;
    ctx.translate(2 * R + 6, 0);
    cell.text(support);
    ctx.restore();
  },
  text: (value) => String(value),
};

const number = new Intl.NumberFormat("en", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/** A score from 0 to SCALE as a bar over a track ticked at each whole step, with the number after it. */
function scoreRenderer(colour: Tone): CellRenderer<Value> {
  const NUMBER = 34;
  return {
    draw(cell: CellCanvas, value) {
      const score = Number(value);
      const { ctx, theme } = cell;
      const across = Math.max(0, cell.width - 2 * PAD - NUMBER);
      const thick = 8;
      const y = (cell.height - thick) / 2;
      ctx.fillStyle = theme.border;
      ctx.beginPath();
      ctx.roundRect(PAD, y, across, thick, thick / 2);
      ctx.fill();
      if (score > 0) {
        ctx.fillStyle = pick(colour, theme);
        ctx.beginPath();
        ctx.roundRect(PAD, y, Math.max(thick, (score / SCALE) * across), thick, thick / 2);
        ctx.fill();
      }
      // Ticks cut through bar and track alike, so a score just past a step reads as past it.
      ctx.fillStyle = theme.body.background;
      for (let t = 1; t < SCALE; t++) ctx.fillRect(Math.round(PAD + (t / SCALE) * across) - 1, y - 2, 2, thick + 4);
      cell.text(number.format(score), "end");
    },
    preferredWidth: () => 140,
    text: (value) => `${number.format(Number(value))} of ${SCALE}`,
  };
}

const DEMAND: Tone = ["#2563eb", "#89b4fa"];
const FIT: Tone = ["#0d9488", "#94e2d5"];

/** The grid's columns: the Feature row's key, header text, starting width and renderer. */
const COLUMNS: { key: keyof Feature; label: string; width: InitialSize; renderer?: CellRenderer<Value> }[] = [
  { key: "name", label: "Feature", width: "fill" },
  { key: "kind", label: "Kind", width: "fit" },
  { key: "category", label: "Category", width: "fit" },
  { key: "binnacle", label: "core · ratatui · web", width: "fit", renderer: binnacleRenderer },
  { key: "demand", label: "Demand", width: 150, renderer: scoreRenderer(DEMAND) },
  { key: "fit", label: "Fit", width: 150, renderer: scoreRenderer(FIT) },
  { key: "products", label: "Products", width: "fit", renderer: textRenderer<Value>({ align: "end" }) },
];

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
  return el("div", `meter ${kind}`, el("span", "meter-label", label), bar, el("span", "meter-value", number.format(score)));
}

function link(href: string, ...text: (Node | string)[]): HTMLAnchorElement {
  const a = el("a", "", ...text);
  a.href = href;
  a.target = "_blank";
  a.rel = "noopener noreferrer";
  return a;
}

/** `#78, #80` as links to the issues, keeping the text between them. */
function issueLinks(refs: string): (Node | string)[] {
  return refs.split(/(#\d+)/).filter(Boolean).map((part) => (/^#\d+$/.test(part) ? link(`${ISSUES}${part.slice(1)}`, part) : part));
}

/** A row of the product table: a product and its support for the feature shown. */
interface ProductRow {
  product: string;
  support: Support;
}

/** The detail panel: a feature's description, binnacle's status, its products and its evidence. */
function createPanel(look: { theme: ThemeChoice; border: GridWidget["border"] }) {
  const aside = el("aside", "gap-detail");
  aside.setAttribute("aria-label", "Feature detail");
  const empty = el("p", "gap-empty", "Pick a feature in the table to read its research.");
  const body = el("div", "gap-body");
  const title = el("h2", "");
  const tags = el("div", "gap-tags");
  const meters = el("div", "gap-meters");
  const description = el("p", "gap-description");
  const statuses = el("dl", "gap-status");
  const note = el("p", "gap-note");
  const refs = el("p", "gap-refs");
  const surveyed = el("span", "gap-count");
  const table = el("div", "gap-products");
  const productNotes = el("ul", "gap-product-notes");
  const evidence = el("ol", "gap-evidence");
  const research = el("p", "gap-research");
  body.append(
    title,
    tags,
    meters,
    description,
    el("h3", "", "binnacle"),
    statuses,
    note,
    refs,
    el("h3", "", "Products ", surveyed),
    table,
    productNotes,
    el("h3", "", "Evidence"),
    evidence,
    research,
  );
  aside.append(empty, body);
  body.hidden = true;

  // Fits every product's row under the header band.
  table.style.height = `${30 + PRODUCTS.length * 24}px`;
  const rows: ProductRow[] = PRODUCTS.map((p) => ({ product: p.name, support: "unknown" }));
  const source: RowsSource<ProductRow> = fromRows(rows, [
    { key: "product", label: "Product" },
    { key: "support", label: "Support", renderer: supportRenderer },
  ]);
  const grid = GridWidget.create(table, {
    source,
    label: "Product support",
    columnWidth: 150,
    initialColumnWidth: (col) => (col === "support" ? "fill" : "fit"),
    theme: look.theme,
    border: look.border,
  });

  let shown: Feature | null = null;
  return {
    element: aside,
    grid,
    show(f: Feature) {
      if (f === shown) return;
      shown = f;
      empty.hidden = true;
      body.hidden = false;
      aside.dataset.feature = f.id;
      title.textContent = f.name;
      tags.replaceChildren(chip(f.kind), chip(f.category), ...(f.gap ? [chip("gap", "gap")] : []));
      meters.replaceChildren(
        meter("Demand", f.demand, "demand"),
        meter("Fit", f.fit, "fit"),
        el("div", "meter-total", `Demand × fit ${number.format(f.priority)} of ${SCALE * SCALE}`),
      );
      description.textContent = f.description;
      statuses.replaceChildren(...ADAPTERS.flatMap((a) => [el("dt", "", a), el("dd", "", chip(f[a], f[a]))]));
      note.textContent = f.note;
      refs.replaceChildren(
        ...(f.issue ? [el("span", "", "Issues ", ...issueLinks(f.issue))] : []),
        ...(f.scope ? [el("span", "gap-scope", `Scope: ${f.scope}`)] : []),
      );
      refs.hidden = !f.issue && !f.scope;
      surveyed.textContent = `${f.products} of ${f.surveyed} surveyed have it`;
      PRODUCTS.forEach((p, i) => {
        rows[i]!.support = f.support[p.id] ?? "unknown";
        source.changed(i);
      });
      productNotes.replaceChildren(
        ...PRODUCTS.filter((p) => f.productNotes[p.id]).map((p) => el("li", "", el("b", "", p.name), ` ${f.productNotes[p.id]}`)),
      );
      evidence.replaceChildren(
        ...f.evidence.map((e) => el("li", "", link(e.url, e.signal), el("span", "gap-host", `${e.kind} · ${new URL(e.url).host}`))),
      );
      research.textContent = f.notes;
    },
    destroy() {
      void grid.then((g) => g.destroy());
      source.destroy();
    },
  };
}

/** The status and kind selects, reporting the filter they choose. */
function createFilters(changed: (filter: GapFilter) => void) {
  const status = el("select", "");
  status.setAttribute("aria-label", "Filter by status");
  status.add(new Option("Any status", ""));
  status.add(new Option("Gaps: missing or partial", "gap"));
  status.add(new Option("Out of scope anywhere", "out-of-scope"));
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
    const v = status.value;
    const [adapter, s] = v.split(":");
    const a = ADAPTERS.find((x) => x === adapter);
    const st = STATUSES.find((x) => x === s);
    return {
      status: v === "gap" || v === "out-of-scope" ? { type: v } : a && st ? { type: "is", adapter: a, status: st } : null,
      kind: kind.value || null,
    };
  };
  status.addEventListener("change", () => changed(read()));
  kind.addEventListener("change", () => changed(read()));
  return [status, kind] as const;
}

export const gaps: DemoSource = {
  name: "gaps",
  description: `${features.length} grid features: demand, fit and binnacle's status, with the evidence`,
  async mount(host, toolbar, look) {
    const source = fromRows(
      features as Feature[],
      COLUMNS.map(({ key, label }) => ({ key, label })),
    );
    const filters = createFilters((filter) => source.filter((f) => keeps(f, filter)));
    toolbar.append(...filters);

    const layout = el("div", "gaps");
    const master = el("div", "gaps-grid");
    const panel = createPanel(look);
    layout.append(master, panel.element);
    host.append(layout);

    const plain = textRenderer<Value>();
    const renderers = new Map(COLUMNS.map((c) => [c.key, c.renderer ?? plain]));
    const widths = new Map(COLUMNS.map((c) => [c.key, c.width]));
    const widget = await GridWidget.create(master, {
      source,
      columnWidth: 200,
      initialColumnWidth: (col) => widths.get(col),
      // A list of features: focus picks a whole row, as in a list view.
      selectionUnit: "rows",
      renderer: (_row, col) => renderers.get(col) ?? plain,
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
