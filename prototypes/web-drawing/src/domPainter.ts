// PROTOTYPE (#45): draws a Snapshot as absolutely placed elements for the
// visible cells only, reusing each element by its (row Key, col Key) across
// frames the way AG Grid keys rows by node. An element whose Key pair left
// the Strips goes to a pool and is handed to the next new pair.

import { isFocused, type Snapshot } from "./bridge";
import type { Cell, Content } from "./content";
import { headerSelected, type Painter } from "./painter";

const SVG = "http://www.w3.org/2000/svg";

interface Slot {
  el: HTMLDivElement;
  /** The Key pair (or header id) and edit version it last showed. */
  shows: string;
  x: number;
  y: number;
  w: number;
  h: number;
  cls: string;
}

/** Elements keyed by a string id, reused across frames. */
class Keyed {
  private live = new Map<string, Slot>();
  private next = new Map<string, Slot>();
  private pool: Slot[] = [];

  constructor(private parent: HTMLElement, private make: () => HTMLDivElement) {}

  /** The slot for `id` this frame, and whether its content must be (re)written. */
  take(id: string): [Slot, boolean] {
    let slot = this.live.get(id);
    if (slot) {
      this.live.delete(id);
      this.next.set(id, slot);
      return [slot, false];
    }
    slot = this.pool.pop();
    if (!slot) {
      const el = this.make();
      this.parent.appendChild(el);
      slot = { el, shows: "", x: NaN, y: NaN, w: NaN, h: NaN, cls: "" };
    }
    slot.el.style.display = "";
    this.next.set(id, slot);
    return [slot, true];
  }

  /** Ends the frame: every slot not taken goes to the pool, hidden. */
  end() {
    for (const slot of this.live.values()) {
      slot.el.style.display = "none";
      slot.shows = "";
      this.pool.push(slot);
    }
    this.live = this.next;
    this.next = new Map();
  }
}

function place(slot: Slot, x: number, y: number, w: number, h: number) {
  if (slot.x !== x || slot.y !== y) {
    slot.el.style.transform = `translate(${x}px, ${y}px)`;
    slot.x = x;
    slot.y = y;
  }
  if (slot.w !== w) { slot.el.style.width = `${w}px`; slot.w = w; }
  if (slot.h !== h) { slot.el.style.height = `${h}px`; slot.h = h; }
}

function setClass(slot: Slot, cls: string) {
  if (slot.cls !== cls) { slot.el.className = cls; slot.cls = cls; }
}

function div(cls: string): HTMLDivElement {
  const el = document.createElement("div");
  el.className = cls;
  return el;
}

export class DomPainter implements Painter {
  readonly name = "dom";
  readonly element: HTMLDivElement;
  private body: HTMLDivElement;
  private colBand: HTMLDivElement;
  private rowBand: HTMLDivElement;
  private corner: HTMLDivElement;
  private overlay: HTMLDivElement;
  private cells: Keyed;
  private sparks: Keyed;
  private colHeaders: Keyed;
  private rowHeaders: Keyed;
  private fills: HTMLDivElement[] = [];
  private focus: HTMLDivElement;
  private guide: HTMLDivElement;
  /** Bumped by the widget when the Table Source's values change (an edit). */
  version = 0;

  constructor() {
    this.element = div("dom-grid");
    this.body = div("dom-body");
    this.colBand = div("dom-colband");
    this.rowBand = div("dom-rowband");
    this.corner = div("dom-corner");
    this.overlay = div("dom-overlay");
    this.focus = div("dom-focus");
    this.guide = div("dom-guide");
    this.overlay.append(this.focus, this.guide);
    this.element.append(this.body, this.colBand, this.rowBand, this.corner, this.overlay);
    this.cells = new Keyed(this.body, () => div("c"));
    this.sparks = new Keyed(this.body, () => {
      const el = div("c");
      const svg = document.createElementNS(SVG, "svg");
      svg.setAttribute("preserveAspectRatio", "none");
      svg.setAttribute("viewBox", "0 0 100 100");
      svg.appendChild(document.createElementNS(SVG, "polyline"));
      el.appendChild(svg);
      return el;
    });
    this.colHeaders = new Keyed(this.colBand, () => div("h"));
    this.rowHeaders = new Keyed(this.rowBand, () => div("rh"));
  }

  resize(width: number, height: number, _dpr: number) {
    this.element.style.width = `${width}px`;
    this.element.style.height = `${height}px`;
  }

  paint(s: Snapshot, content: Content) {
    const top = s.axes[0].header;
    const left = s.axes[1].header;
    const right = s.axes[1].extent;
    const bottom = s.axes[0].extent;
    const v = this.version;
    // Bands and the body clip their children; children use viewport offsets
    // shifted by the band's own origin.
    setBox(this.body, left, top, right - left, bottom - top);
    setBox(this.colBand, left, 0, right - left, top);
    setBox(this.rowBand, 0, top, left, bottom - top);
    setBox(this.corner, 0, 0, left, top);
    setBox(this.overlay, left, top, right - left, bottom - top);

    for (const row of s.rows) {
      const alt = row.position % 2 === 1 ? " alt" : "";
      for (const col of s.cols) {
        const x = col.offset - left, y = row.offset - top;
        if (row.key === null || col.key === null) {
          const [slot, fresh] = this.cells.take(`p${row.position}:${col.position}`);
          if (fresh || slot.shows !== "…") { slot.el.textContent = "…"; slot.shows = "…"; }
          setClass(slot, "c loading" + alt);
          place(slot, x, y, col.size, row.size);
          continue;
        }
        const id = `${row.key}:${col.key}`;
        const cell = content.cell(row.key, col.key);
        const shows = `${id}@${v}`;
        const pool = cell.kind === "spark" ? this.sparks : this.cells;
        const [slot] = pool.take(id);
        if (slot.shows !== shows) { write(slot.el, cell); slot.shows = shows; }
        const focused = isFocused(s, row, col) && s.mode !== "Editing";
        setClass(slot, "c" + alt + (cell.kind === "text" && cell.align === "right" ? " num" : "") + (focused ? " focused" : ""));
        place(slot, x, y, col.size, row.size);
      }
    }
    this.cells.end();
    this.sparks.end();

    for (const col of s.cols) {
      const [slot] = this.colHeaders.take(`${col.key ?? "p" + col.position}`);
      const title = col.key === null ? "…" : content.columnTitle(col.key);
      if (slot.shows !== title) { slot.el.textContent = title; slot.shows = title; }
      const hovered = s.hoveredHeader?.axis === 1 && s.hoveredHeader.position === col.position;
      setClass(slot, "h" + (headerSelected(s, 1, col.position) ? " sel" : hovered ? " hover" : ""));
      place(slot, col.offset - left, 0, col.size, top);
    }
    this.colHeaders.end();
    for (const row of s.rows) {
      const [slot] = this.rowHeaders.take(`${row.key ?? "p" + row.position}`);
      const label = content.rowHeader(row.position);
      if (slot.shows !== label) { slot.el.textContent = label; slot.shows = label; }
      setClass(slot, "rh" + (headerSelected(s, 0, row.position) ? " sel" : ""));
      place(slot, 0, row.offset - top, left, row.size);
    }
    this.rowHeaders.end();

    // Selection fills: a handful of divs, created on demand.
    while (this.fills.length < s.selectionRects.length) {
      const el = div("dom-fill");
      this.overlay.appendChild(el);
      this.fills.push(el);
    }
    this.fills.forEach((el, i) => {
      const r = s.selectionRects[i];
      if (!r) { el.style.display = "none"; return; }
      el.style.display = "";
      setBox(el, r.x - left, r.y - top, r.width, r.height);
    });
    const f = s.focusRect;
    this.focus.style.display = f && s.mode !== "Editing" ? "" : "none";
    if (f) setBox(this.focus, f.x - left, f.y - top, f.width, f.height);
    const g = s.resizeGuide;
    this.guide.style.display = g ? "" : "none";
    if (g) {
      if (g.axis === 1) setBox(this.guide, g.offset - left - 1, -top, 2, bottom);
      else setBox(this.guide, -left, g.offset - top - 1, right, 2);
    }
  }
}

function setBox(el: HTMLElement, x: number, y: number, w: number, h: number) {
  el.style.transform = `translate(${x}px, ${y}px)`;
  el.style.width = `${w}px`;
  el.style.height = `${h}px`;
}

function write(el: HTMLDivElement, cell: Cell) {
  if (cell.kind === "text") {
    el.textContent = cell.text;
    return;
  }
  let min = Infinity, max = -Infinity;
  for (const n of cell.values) { if (n < min) min = n; if (n > max) max = n; }
  const span = max - min || 1;
  const pts = cell.values
    .map((n, i) => `${((i / (cell.values.length - 1)) * 100).toFixed(1)},${(95 - ((n - min) / span) * 90).toFixed(1)}`)
    .join(" ");
  el.querySelector("polyline")!.setAttribute("points", pts);
}

