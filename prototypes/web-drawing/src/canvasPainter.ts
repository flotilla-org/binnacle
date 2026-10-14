// PROTOTYPE (#45): draws a Snapshot onto one high-DPI canvas, every frame
// from scratch (no blit-on-scroll, no per-cell damage).

import type { Snapshot } from "./bridge";
import type { Content } from "./content";
import { THEME, headerSelected, scrollbarGeometry, type Chrome, type Painter } from "./painter";

export class CanvasPainter implements Painter {
  readonly name = "canvas";
  readonly element: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private w = 0;
  private h = 0;
  private dpr = 1;

  constructor(private chrome: Chrome) {
    this.element = document.createElement("canvas");
    this.element.style.display = "block";
    this.ctx = this.element.getContext("2d", { alpha: false })!;
  }

  resize(width: number, height: number, dpr: number) {
    this.w = width;
    this.h = height;
    this.dpr = dpr;
    this.element.width = Math.round(width * dpr);
    this.element.height = Math.round(height * dpr);
    this.element.style.width = `${width}px`;
    this.element.style.height = `${height}px`;
  }

  paint(s: Snapshot, content: Content) {
    const c = this.ctx;
    c.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    c.textBaseline = "middle";
    const top = s.axes[0].header; // column header band height
    const left = s.axes[1].header; // row header band width
    const right = s.axes[1].extent;
    const bottom = s.axes[0].extent;
    c.fillStyle = THEME.bg;
    c.fillRect(0, 0, this.w, this.h);

    // Body: Gap lines show through between cell backgrounds.
    c.save();
    c.beginPath();
    c.rect(left, top, right - left, bottom - top);
    c.clip();
    const last = s.cols[s.cols.length - 1];
    const bodyRight = last ? last.offset + last.size : left;
    c.fillStyle = THEME.line;
    for (const row of s.rows) c.fillRect(left, row.offset + row.size, bodyRight - left, s.axes[0].gap);
    for (const col of s.cols) c.fillRect(col.offset + col.size, top, s.axes[1].gap, bottom - top);
    c.font = THEME.font;
    for (const row of s.rows) {
      if (row.position % 2 === 1) {
        c.fillStyle = THEME.alt;
        c.fillRect(left, row.offset, bodyRight - left, row.size);
      }
      for (const col of s.cols) {
        if (row.key === null || col.key === null) {
          c.fillStyle = THEME.loading;
          c.fillText("…", col.offset + 6, row.offset + row.size / 2);
          continue;
        }
        const cell = content.cell(row.key, col.key);
        if (cell.kind === "spark") {
          drawSpark(c, cell.values, col.offset + 4, row.offset + 3, col.size - 8, row.size - 6);
          continue;
        }
        c.save();
        c.beginPath();
        c.rect(col.offset, row.offset, col.size, row.size);
        c.clip();
        c.fillStyle = THEME.text;
        if (cell.align === "right") {
          c.textAlign = "right";
          c.fillText(cell.text, col.offset + col.size - 6, row.offset + row.size / 2);
          c.textAlign = "left";
        } else {
          c.fillText(cell.text, col.offset + 6, row.offset + row.size / 2);
        }
        c.restore();
      }
    }
    c.fillStyle = THEME.selection;
    for (const r of s.selectionRects) c.fillRect(r.x, r.y, r.width, r.height);
    const f = s.focusRect;
    if (f && s.mode !== "Editing") {
      c.strokeStyle = THEME.focus;
      c.lineWidth = 2;
      c.strokeRect(f.x + 1, f.y + 1, f.width - 2, f.height - 2);
    }
    c.restore();

    // Column headers.
    c.save();
    c.beginPath();
    c.rect(left, 0, right - left, top);
    c.clip();
    c.font = THEME.headerFont;
    for (const col of s.cols) {
      const hovered = s.hoveredHeader?.axis === 1 && s.hoveredHeader.position === col.position;
      c.fillStyle = headerSelected(s, 1, col.position) ? THEME.headerSelected : hovered ? THEME.headerHover : THEME.headerBg;
      c.fillRect(col.offset, 0, col.size, top);
      c.fillStyle = THEME.line;
      c.fillRect(col.offset + col.size, 0, s.axes[1].gap, top);
      c.fillStyle = THEME.headerText;
      c.fillText(col.key === null ? "…" : content.columnTitle(col.key), col.offset + 6, top / 2);
    }
    c.restore();

    // Row headers.
    c.save();
    c.beginPath();
    c.rect(0, top, left, bottom - top);
    c.clip();
    c.font = THEME.font;
    c.textAlign = "right";
    for (const row of s.rows) {
      c.fillStyle = headerSelected(s, 0, row.position) ? THEME.headerSelected : THEME.headerBg;
      c.fillRect(0, row.offset, left, row.size + s.axes[0].gap);
      c.fillStyle = THEME.headerText;
      c.fillText(content.rowHeader(row.position), left - 8, row.offset + row.size / 2);
    }
    c.textAlign = "left";
    c.restore();
    c.fillStyle = THEME.headerBg;
    c.fillRect(0, 0, left, top);
    c.fillStyle = THEME.line;
    c.fillRect(0, top - 1, right, 1);
    c.fillRect(left - 1, 0, 1, bottom);

    const g = s.resizeGuide;
    if (g) {
      c.fillStyle = THEME.guide;
      if (g.axis === 1) c.fillRect(g.offset - 1, 0, 2, bottom);
      else c.fillRect(0, g.offset - 1, right, 2);
    }

    const t = this.chrome.scrollbar;
    if (t > 0) {
      const sb = scrollbarGeometry(s, this.w, this.h, t);
      c.fillStyle = THEME.bar;
      c.fillRect(sb.v.x, 0, t, this.h);
      c.fillRect(0, sb.h.y, this.w, t);
      c.fillStyle = THEME.thumb;
      if (sb.v.visible) roundRect(c, sb.v.x + 2, sb.v.at, t - 4, sb.v.thumb);
      if (sb.h.visible) roundRect(c, sb.h.at, sb.h.y + 2, sb.h.thumb, t - 4);
    }
  }
}

function roundRect(c: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
  c.beginPath();
  c.roundRect(x, y, w, h, Math.min(w, h) / 2);
  c.fill();
}

function drawSpark(c: CanvasRenderingContext2D, v: number[], x: number, y: number, w: number, h: number) {
  let min = Infinity, max = -Infinity;
  for (const n of v) { if (n < min) min = n; if (n > max) max = n; }
  const span = max - min || 1;
  c.beginPath();
  for (let i = 0; i < v.length; i++) {
    const px = x + (i / (v.length - 1)) * w;
    const py = y + h - ((v[i] - min) / span) * h;
    if (i === 0) c.moveTo(px, py); else c.lineTo(px, py);
  }
  c.strokeStyle = THEME.spark;
  c.lineWidth = 1.25;
  c.stroke();
}

