// PROTOTYPE (#45): the framework-neutral widget. It owns a container, the
// requestAnimationFrame loop (idle unless something changed), the scroller,
// pointer input and the one persistent textarea that takes keys and IME.

import { decode, type Snapshot } from "./bridge";
import { COLUMNS, HugeContent } from "./content";
import type { Painter } from "./painter";
import { THEME, scrollbarGeometry } from "./painter";
import type { WebGrid } from "../wasm/pkg/binnacle_web_proto";

/** Sizer cap for the native scroller; Chrome tops out near 33M px, Firefox lower. */
const CAP = 10_000_000;
/** A scroll event moving further than this many viewports is a jump (ScrollTo). */
const JUMP = 1.5;

export type ScrollMode = "native" | "custom";

export interface Stats {
  last: number;
  times: number[];
  drawn: number;
  frames: number;
  effects: string[];
  scrollEvents: number;
  scrollMapped: { scroll: number; scrollTo: number };
}

export class Widget {
  readonly stats: Stats = {
    last: 0, times: [], drawn: 0, frames: 0, effects: [], scrollEvents: 0,
    scrollMapped: { scroll: 0, scrollTo: 0 },
  };
  snapshot: Snapshot | null = null;
  private raf = 0;
  private force = true;
  private dpr = window.devicePixelRatio || 1;
  private vw = 0;
  private vh = 0;
  private scroller?: HTMLDivElement;
  private sizer?: HTMLDivElement;
  private layer: HTMLDivElement;
  private ime: HTMLTextAreaElement;
  /** Sizer size and the scroll offsets the widget last wrote, per axis (rows, cols). */
  sizerSize: [number, number] = [0, 0];
  private lastSet: [number, number] = [0, 0];
  private editing: { row: number; col: number } | null = null;
  /** The focused cell's Keys as the last drained FocusChanged named them. */
  private focus: [number, number] | null = null;
  private composing = false;
  private clicks = { at: 0, x: 0, y: 0, n: 0 };
  private drag: { axis: number; start: number; at: number } | null = null;
  onFrame: () => void = () => {};

  constructor(
    host: HTMLElement,
    readonly painter: Painter,
    readonly scroll: ScrollMode,
    readonly grid: WebGrid,
    readonly content: HugeContent,
    /** Custom scrollbar thickness (variant C), 0 otherwise. */
    private bar = 0,
  ) {
    COLUMNS.forEach((c, i) => grid.set_size(1, i, c.width));
    this.layer = document.createElement("div");
    this.ime = document.createElement("textarea");
    this.ime.className = "ime";
    this.ime.setAttribute("aria-label", "grid input");
    this.ime.spellcheck = false;
    this.layer.append(painter.element, this.ime);
    if (scroll === "native") {
      this.scroller = document.createElement("div");
      this.scroller.className = "scroller";
      this.sizer = document.createElement("div");
      this.sizer.className = "sizer";
      this.layer.className = "layer";
      this.sizer.append(this.layer);
      this.scroller.append(this.sizer);
      host.append(this.scroller);
      this.scroller.addEventListener("scroll", () => { this.stats.scrollEvents++; if (this.consumeScroll()) this.schedule(); });
    } else {
      this.layer.className = "custom";
      host.append(this.layer);
      this.layer.addEventListener("wheel", (e) => this.onWheel(e), { passive: false });
    }
    this.layer.addEventListener("pointerdown", (e) => this.onPointer(e, 0));
    this.layer.addEventListener("pointermove", (e) => this.onPointer(e, 1));
    this.layer.addEventListener("pointerup", (e) => this.onPointer(e, 2));
    this.layer.addEventListener("pointerleave", () => { this.grid.pointer(1, -1, -1, 0, 0, 0); this.schedule(); });
    this.ime.addEventListener("keydown", (e) => this.onKey(e));
    this.ime.addEventListener("input", (e) => this.onInput(e as InputEvent));
    this.ime.addEventListener("compositionstart", (e) => this.onComposition(e));
    this.ime.addEventListener("compositionupdate", (e) => this.log(`compositionupdate "${e.data}"`));
    this.ime.addEventListener("compositionend", (e) => { this.composing = false; this.log(`compositionend "${e.data}"`); });
    new ResizeObserver(() => { this.force = true; this.schedule(); }).observe(host);
    this.watchDpr();
    this.ime.focus({ preventScroll: true });
    this.schedule();
  }

  private watchDpr() {
    const mq = matchMedia(`(resolution: ${this.dpr}dppx)`);
    mq.addEventListener("change", () => {
      this.dpr = window.devicePixelRatio || 1;
      this.force = true;
      this.schedule();
      this.watchDpr();
    }, { once: true });
  }

  log(line: string) {
    this.stats.effects.unshift(line);
    this.stats.effects.length = Math.min(this.stats.effects.length, 10);
  }

  schedule() {
    if (!this.raf) this.raf = requestAnimationFrame(() => this.frame());
  }

  /** The Grid's viewport in CSS px: the scroller's client box, or the area less the drawn scrollbars. */
  private measure(): [number, number, number, number] {
    if (this.scroller) {
      const w = this.scroller.clientWidth, h = this.scroller.clientHeight;
      return [w, h, w, h];
    }
    const r = this.layer.getBoundingClientRect();
    return [r.width - this.bar, r.height - this.bar, r.width, r.height];
  }

  private frame() {
    this.raf = 0;
    const t0 = performance.now();
    this.stats.frames++;
    this.pump();
    // A scroll offset changed since the last scroll event (script, or input
    // racing the frame) must go in before the writeback below overwrites it.
    if (this.scroller) this.consumeScroll();
    const [w, h, pw, ph] = this.measure();
    if (w !== this.vw || h !== this.vh || this.force) {
      this.vw = w;
      this.vh = h;
      this.painter.resize(pw, ph, this.dpr);
      this.layer.style.width = `${pw}px`;
      this.layer.style.height = `${ph}px`;
    }
    this.fill(w, h);
    let s = decode(this.grid.snapshot());
    // Fit the row-number band to the rows shown, as the ratatui demo does on
    // the huge table; a change lays the frame out again.
    const fit = this.rowBandFit(s);
    if (fit !== s.axes[1].header && s.mode !== "ResizingBand") {
      this.grid.set_header(1, fit);
      this.fill(w, h);
      s = decode(this.grid.snapshot());
      s.damage.layout = true;
    }
    this.snapshot = s;
    const d = s.damage;
    if (d.layout || d.selection || d.positions || this.force) {
      this.painter.paint(s, this.content);
      // Force style and layout now so DOM painting cost shows in the frame time.
      void this.painter.element.offsetWidth;
      this.stats.drawn++;
      this.stats.last = performance.now() - t0;
      if (this.stats.drawn > 1) this.stats.times.push(this.stats.last);
      if (this.stats.times.length > 300) this.stats.times.shift();
    }
    this.force = false;
    this.placeIme(s);
    this.layer.style.cursor = s.cursor;
    if (this.scroller) this.syncScroller(s);
    this.onFrame();
    if (s.animating) this.schedule();
  }

  /** begin_frame, then answer the Windows it asks for. */
  private fill(w: number, h: number) {
    const req = this.grid.begin_frame(w, h);
    for (let a = 0; a < 2; a++) {
      const start = req[a * 2], end = req[a * 2 + 1];
      if (Number.isNaN(start)) continue;
      // The identity source: Keys are Positions. A real source may answer later.
      const keys = new Float64Array(end - start);
      for (let i = 0; i < keys.length; i++) keys[i] = start + i;
      this.grid.set_window(a, start, keys);
    }
  }

  private measurer = document.createElement("canvas").getContext("2d")!;

  private rowBandFit(s: Snapshot): number {
    this.measurer.font = THEME.font;
    let widest = 0;
    for (const row of s.rows) widest = Math.max(widest, this.measurer.measureText(this.content.rowHeader(row.position)).width);
    return Math.ceil(widest) + 20;
  }

  /** Drains Effects and answers the ones this host acts on. */
  private pump() {
    const effects = JSON.parse(this.grid.poll_effects()) as Array<Record<string, any>>;
    for (const e of effects) {
      this.log(JSON.stringify(e));
      switch (e.t) {
        case "FocusChanged":
          // Track focus in drain order: the last Snapshot can lag keys typed
          // faster than frames, and TextTyped names no cell.
          this.focus = [e.row, e.col];
          break;
        case "EditRequested":
        case "Activated":
          if (this.editing) break;
          this.startEdit(e.row, e.col, this.composing ? this.ime.value : this.content.text(e.row, e.col), !this.composing);
          break;
        case "TextTyped":
          if (this.focus && !this.editing) this.startEdit(this.focus[0], this.focus[1], this.ime.value, false);
          break;
        case "EditCancelled":
          this.editing = null;
          this.ime.value = "";
          this.ime.classList.remove("editing");
          break;
      }
    }
  }


  private startEdit(row: number, col: number, text: string, selectAll: boolean) {
    this.grid.edit_started(row, col);
    this.editing = { row, col };
    if (!this.composing) this.ime.value = text;
    this.ime.classList.add("editing");
    if (selectAll) this.ime.select();
    this.log(`edit_started(${row}, ${col})`);
  }

  private endEdit(commit: boolean) {
    if (!this.editing) return;
    if (commit) {
      this.content.edits.set(`${this.editing.row}:${this.editing.col}`, this.ime.value);
      (this.painter as { version?: number }).version = ((this.painter as { version?: number }).version ?? 0) + 1;
    }
    this.grid.edit_ended(commit);
    this.log(`edit_ended(${commit ? "Commit" : "Cancel"})`);
    this.editing = null;
    this.ime.value = "";
    this.ime.classList.remove("editing");
    this.force = true;
    this.schedule();
  }

  /** The textarea sits on the focused cell (transparent) or on the editor rect (visible). */
  private placeIme(s: Snapshot) {
    const r = (this.editing && s.editorRect) || s.focusRect || { x: 0, y: 0, width: 1, height: 1 };
    const st = this.ime.style;
    st.transform = `translate(${r.x}px, ${r.y}px)`;
    st.width = `${Math.max(r.width, this.editing ? 140 : 1)}px`;
    st.height = `${r.height}px`;
  }

  // ---- scrolling ----

  /** Glide's hybrid mapping: small moves are pixel deltas, jumps and the ends are fractions. */
  private consumeScroll(): boolean {
    const sc = this.scroller!;
    const now = [sc.scrollTop, sc.scrollLeft];
    const view = [sc.clientHeight, sc.clientWidth];
    let dx = 0, dy = 0, changed = false;
    for (let a = 0; a < 2; a++) {
      const cur = now[a];
      if (cur === this.lastSet[a]) continue;
      changed = true;
      const max = this.sizerSize[a] - view[a];
      const delta = cur - this.lastSet[a];
      if (cur <= 0 || cur >= max - 1 || Math.abs(delta) > view[a] * JUMP) {
        this.grid.scroll_to(a, max > 0 ? Math.min(1, Math.max(0, cur / max)) : 0);
        this.stats.scrollMapped.scrollTo++;
      } else if (a === 0) dy = delta;
      else dx = delta;
      this.lastSet[a] = cur;
    }
    if (dx || dy) {
      this.grid.scroll(dx, dy);
      this.stats.scrollMapped.scroll++;
    }
    return changed;
  }

  /** After a frame: size the sizer from the Thumb and write the thumb back to the scroll offsets. */
  private syncScroller(s: Snapshot) {
    const sc = this.scroller!;
    const view = [sc.clientHeight, sc.clientWidth];
    for (let a = 0; a < 2; a++) {
      const { size } = s.axes[a].thumb;
      const header = s.axes[a].header;
      const estimate = size >= 1 ? view[a] : header + (view[a] - header) / size;
      const sizer = Math.round(Math.min(CAP, estimate));
      if (sizer !== this.sizerSize[a]) {
        this.sizerSize[a] = sizer;
        if (a === 0) this.sizer!.style.height = `${sizer}px`;
        else this.sizer!.style.width = `${sizer}px`;
      }
    }
    for (let a = 0; a < 2; a++) {
      const max = this.sizerSize[a] - view[a];
      const target = Math.round(s.axes[a].thumb.fraction * Math.max(0, max));
      const cur = a === 0 ? sc.scrollTop : sc.scrollLeft;
      if (Math.abs(target - cur) >= 1) {
        if (a === 0) sc.scrollTop = target;
        else sc.scrollLeft = target;
      }
      this.lastSet[a] = a === 0 ? sc.scrollTop : sc.scrollLeft;
    }
  }

  private onWheel(e: WheelEvent) {
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 24 : e.deltaMode === 2 ? this.vh : 1;
    let dx = e.deltaX * unit, dy = e.deltaY * unit;
    if (e.shiftKey && !dx) { dx = dy; dy = 0; }
    this.grid.scroll(dx, dy);
    this.stats.scrollEvents++;
    this.stats.scrollMapped.scroll++;
    this.schedule();
  }

  // ---- pointer ----

  private onPointer(e: PointerEvent, kind: 0 | 1 | 2) {
    const r = this.layer.getBoundingClientRect();
    const x = e.clientX - r.left, y = e.clientY - r.top;
    if (this.bar && this.snapshot && this.customBar(e, kind, x, y)) return;
    if (e.target === this.ime && this.editing) return;
    const mods = (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0) | (e.metaKey ? 8 : 0);
    let clicks = 1;
    if (kind === 0) {
      const c = this.clicks;
      const near = Math.abs(x - c.x) < 4 && Math.abs(y - c.y) < 4;
      c.n = near && e.timeStamp - c.at < 400 ? c.n + 1 : 1;
      Object.assign(c, { at: e.timeStamp, x, y });
      clicks = c.n;
      e.preventDefault();
      if (this.editing) this.endEdit(true);
      this.layer.setPointerCapture(e.pointerId);
      this.ime.focus({ preventScroll: true });
    }
    const button = e.button === 1 ? 1 : e.button === 2 ? 2 : 0;
    this.grid.pointer(kind, x, y, button, mods, clicks);
    this.schedule();
  }

  /** Variant C: presses and drags on the drawn scrollbars. Returns whether it took the event. */
  private customBar(e: PointerEvent, kind: number, x: number, y: number): boolean {
    const s = this.snapshot!;
    const r = this.layer.getBoundingClientRect();
    const g = scrollbarGeometry(s, r.width, r.height, this.bar);
    if (kind === 0) {
      const axis = x >= g.v.x ? 0 : y >= g.h.y ? 1 : -1;
      if (axis < 0) return false;
      e.preventDefault();
      const bar = axis === 0 ? g.v : g.h;
      const along = axis === 0 ? y : x;
      if (along >= bar.at && along <= bar.at + bar.thumb) {
        this.drag = { axis, start: along, at: bar.at };
        this.layer.setPointerCapture(e.pointerId);
      } else {
        // Track click: page towards the press.
        const page = (axis === 0 ? this.vh - s.axes[0].header : this.vw - s.axes[1].header) * Math.sign(along - bar.at);
        this.grid.scroll(axis === 1 ? page : 0, axis === 0 ? page : 0);
        this.schedule();
      }
      return true;
    }
    if (!this.drag) return false;
    const bar = this.drag.axis === 0 ? g.v : g.h;
    const along = this.drag.axis === 0 ? y : x;
    const travel = bar.track - bar.thumb;
    const fraction = travel > 0 ? (this.drag.at + along - this.drag.start - bar.start) / travel : 0;
    this.grid.scroll_to(this.drag.axis, Math.min(1, Math.max(0, fraction)));
    this.stats.scrollMapped.scrollTo++;
    if (kind === 2) this.drag = null;
    this.schedule();
    return true;
  }

  // ---- keys and IME ----

  private onKey(e: KeyboardEvent) {
    if (e.isComposing || e.key === "Process" || e.altKey) return;
    if (this.editing) {
      if (e.key === "Enter") { e.preventDefault(); this.endEdit(true); }
      else if (e.key === "Escape") { e.preventDefault(); this.endEdit(false); }
      return;
    }
    const g = this.grid;
    const ext = e.shiftKey;
    const edge = e.ctrlKey || e.metaKey;
    let handled = true;
    switch (e.key) {
      case "ArrowDown": g.move_focus(0, edge ? 2 : 0, true, ext); break;
      case "ArrowUp": g.move_focus(0, edge ? 2 : 0, false, ext); break;
      case "ArrowRight": g.move_focus(1, edge ? 2 : 0, true, ext); break;
      case "ArrowLeft": g.move_focus(1, edge ? 2 : 0, false, ext); break;
      case "PageDown": g.move_focus(0, 1, true, ext); break;
      case "PageUp": g.move_focus(0, 1, false, ext); break;
      case "Home": g.move_focus(edge ? 0 : 1, 2, false, ext); break;
      case "End": g.move_focus(edge ? 0 : 1, 2, true, ext); break;
      case "Tab": g.move_focus(1, 0, !e.shiftKey, false); break;
      case "Enter": g.activate(); break;
      case "F2": g.begin_edit(); break;
      case "Escape": g.cancel(); break;
      default:
        if ((e.ctrlKey || e.metaKey) && e.key === "a") g.select_all();
        else handled = false;
    }
    if (handled) {
      e.preventDefault();
      // Drain now, not at the next frame: a key typed in between would
      // otherwise land as TypeText before the edit opens.
      this.pump();
      this.schedule();
    }
  }

  /** A printable key typed into the transparent textarea outside an edit: TypeText. */
  private onInput(e: InputEvent) {
    if (this.editing || this.composing || e.isComposing) return;
    this.log(`input "${e.data}" -> TypeText`);
    this.grid.type_text(e.data ?? this.ime.value);
    this.pump();
    this.schedule();
  }

  /** IME composition starting outside an edit asks for one, keeping the composition running. */
  private onComposition(e: CompositionEvent) {
    this.composing = true;
    this.log(`compositionstart "${e.data}"`);
    if (!this.editing) {
      this.grid.begin_edit();
      this.log("-> BeginEdit");
      this.pump();
      this.schedule();
    }
  }
}
