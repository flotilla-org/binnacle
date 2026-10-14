// PROTOTYPE (#45): three drawing variants of the huge source, switchable via
// `?variant=` and the floating bar. Throwaway; see ../NOTES.md.
//
//   A  canvas painter + capped native scroller (Glide's hybrid mapping)
//   B  DOM painter (elements reused by Key) + the same scroller
//   C  canvas painter + custom scrollbar drawn from the Thumb, wheel intercepted

import init, { WebGrid } from "../wasm/pkg/binnacle_web_proto";
import { CanvasPainter } from "./canvasPainter";
import { COLUMNS, HUGE_ROWS, HugeContent } from "./content";
import { DomPainter } from "./domPainter";
import type { Painter } from "./painter";
import { Widget, type ScrollMode } from "./widget";

const VARIANTS: Record<string, { name: string; painter: () => Painter; scroll: ScrollMode; bar: number }> = {
  A: { name: "canvas + native scroller", painter: () => new CanvasPainter({ scrollbar: 0 }), scroll: "native", bar: 0 },
  B: { name: "DOM + native scroller", painter: () => new DomPainter(), scroll: "native", bar: 0 },
  C: { name: "canvas + drawn scrollbar", painter: () => new CanvasPainter({ scrollbar: 12 }), scroll: "custom", bar: 12 },
};
const KEYS = Object.keys(VARIANTS);

await init();
const params = new URLSearchParams(location.search);
const variant = KEYS.includes(params.get("variant") ?? "") ? params.get("variant")! : "A";
const v = VARIANTS[variant];

const grid = new WebGrid(HUGE_ROWS, 24, 28, COLUMNS.length, 120, 132, 1);
const widget = new Widget(document.getElementById("host")!, v.painter(), v.scroll, grid, new HugeContent(), v.bar);
/** Scripted scroll for frame times: `step` px per animation frame, through the variant's own input path. */
async function bench(frames = 240, step = 37) {
  const raf = () => new Promise((r) => requestAnimationFrame(r));
  const sc = document.querySelector<HTMLElement>(".scroller");
  const layer = document.querySelector<HTMLElement>(".custom");
  widget.stats.times.length = 0;
  const t0 = performance.now();
  for (let i = 0; i < frames; i++) {
    if (sc) sc.scrollTop += step;
    else layer!.dispatchEvent(new WheelEvent("wheel", { deltaY: step, cancelable: true }));
    await raf();
  }
  const wall = performance.now() - t0;
  const xs = [...widget.stats.times].sort((a, b) => a - b);
  return {
    variant, fps: Math.round((frames / wall) * 1000), drawn: xs.length,
    median: +xs[xs.length >> 1].toFixed(2), p95: +xs[Math.floor(xs.length * 0.95)].toFixed(2), max: +xs[xs.length - 1].toFixed(2),
    anchor: widget.snapshot!.axes[0].anchor, nodes: document.getElementsByTagName("*").length,
  };
}
(window as any).__proto = { widget, grid, variant, bench };

// ---- the always-visible state overlay ----
const overlay = document.getElementById("overlay")!;
const p95 = (xs: number[]) => {
  if (!xs.length) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))];
};
const fmt = (n: number) => n.toLocaleString("en-US", { maximumFractionDigits: 3 });
let pending = false;
widget.onFrame = () => {
  if (pending) return;
  pending = true;
  // Update the overlay after the frame so its DOM work stays out of the timing.
  setTimeout(() => {
    pending = false;
    const s = widget.snapshot;
    if (!s) return;
    const st = widget.stats;
    const [r, c] = s.axes;
    const sc = document.querySelector<HTMLElement>(".scroller");
    const lines = [
      `variant      ${variant}: ${v.name}`,
      `frame ms     last ${st.last.toFixed(2)}  p95 ${p95(st.times).toFixed(2)}  (n=${st.times.length})`,
      `frames       drawn ${st.drawn} / run ${st.frames}`,
      `DOM nodes    ${document.getElementsByTagName("*").length}`,
      `scroll in    events ${st.scrollEvents}  Scroll ${st.scrollMapped.scroll}  ScrollTo ${st.scrollMapped.scrollTo}`,
      `anchor rows  pos ${fmt(r.anchor.position)} + ${r.anchor.offset.toFixed(1)}px`,
      `anchor cols  pos ${c.anchor.position} + ${c.anchor.offset.toFixed(1)}px`,
      `thumb rows   frac ${r.thumb.fraction.toFixed(9)} size ${r.thumb.size.toExponential(2)}`,
      `thumb cols   frac ${c.thumb.fraction.toFixed(4)} size ${c.thumb.size.toFixed(3)}`,
      sc
        ? `scrollTop    ${sc.scrollTop} / sizer ${widget.sizerSize[0]} (max ${widget.sizerSize[0] - sc.clientHeight})`
        : `scrollTop    n/a (custom scrollbar)`,
      `focus        ${r.focus ? `row pos ${fmt(r.focus.position)} key ${r.focus.key}` : "none"}${c.focus ? `, col pos ${c.focus.position} key ${c.focus.key}` : ""}`,
      `mode         ${s.mode}${s.animating ? " (animating)" : ""}`,
      `strips       ${s.rows.length} rows x ${s.cols.length} cols, gen ${s.generation}`,
      `effects (newest first)`,
      ...st.effects.map((e) => "  " + e),
    ];
    overlay.textContent = lines.join("\n");
  }, 0);
};

// ---- the floating variant switcher ----
const bar = document.getElementById("switcher")!;
const go = (step: number) => {
  const next = KEYS[(KEYS.indexOf(variant) + step + KEYS.length) % KEYS.length];
  params.set("variant", next);
  location.search = params.toString();
};
const prev = document.createElement("button");
prev.textContent = "←";
prev.onclick = () => go(-1);
const label = document.createElement("span");
label.textContent = `${variant}: ${v.name}`;
const next = document.createElement("button");
next.textContent = "→";
next.onclick = () => go(1);
bar.append(prev, label, next);
// Arrow keys drive the grid through the textarea; Alt+arrows switch variants.
window.addEventListener("keydown", (e) => {
  if (e.altKey && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
    e.preventDefault();
    go(e.key === "ArrowLeft" ? -1 : 1);
  }
});
