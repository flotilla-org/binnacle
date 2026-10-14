// PROTOTYPE (#45): the one interface both painters implement. A painter owns
// an element the widget places, and draws a Snapshot plus a Content lookup
// by Key. It never talks to the Grid.

import type { Snapshot } from "./bridge";
import type { Content } from "./content";

export interface Painter {
  readonly name: string;
  /** What the widget mounts; sized by `resize`. */
  readonly element: HTMLElement;
  /** CSS px size of the drawing surface and the device pixel ratio. */
  resize(width: number, height: number, dpr: number): void;
  paint(snapshot: Snapshot, content: Content): void;
}

/** Options the widget passes to painters that also draw chrome. */
export interface Chrome {
  /** Custom scrollbars drawn from the Thumb (variant C), thickness in px. */
  scrollbar: number;
}

export const THEME = {
  font: "13px system-ui, -apple-system, sans-serif",
  headerFont: "600 12px system-ui, -apple-system, sans-serif",
  bg: "#ffffff",
  alt: "#f8fafc",
  line: "#e2e8f0",
  headerBg: "#f1f5f9",
  headerHover: "#e2e8f0",
  headerSelected: "#dbeafe",
  headerText: "#334155",
  text: "#0f172a",
  loading: "#94a3b8",
  selection: "rgba(37, 99, 235, 0.13)",
  focus: "#2563eb",
  spark: "#059669",
  guide: "#2563eb",
  bar: "rgba(15, 23, 42, 0.06)",
  thumb: "rgba(15, 23, 42, 0.35)",
};

/** Where the custom scrollbars and their thumbs sit, in CSS px. */
export function scrollbarGeometry(s: Snapshot, width: number, height: number, t: number) {
  const bar = (axisIndex: number, length: number) => {
    const { fraction, size } = s.axes[axisIndex].thumb;
    const header = s.axes[axisIndex].header;
    const track = Math.max(0, length - header);
    const thumb = Math.min(track, Math.max(24, track * Math.min(1, size)));
    return { start: header, track, thumb, at: header + fraction * (track - thumb), visible: size < 1 };
  };
  return {
    v: { x: width - t, ...bar(0, height - t) },
    h: { y: height - t, ...bar(1, width - t) },
  };
}

/** Whether the row and column are wholly inside the selection range across the other axis. */
export function headerSelected(s: Snapshot, axisIndex: number, position: number): boolean {
  const r = s.selectionRange;
  if (!r) return false;
  const [mine, across] = axisIndex === 0 ? [r.rows, r.cols] : [r.cols, r.rows];
  const count = s.axes[1 - axisIndex].count;
  return across[0] === 0 && across[1] === count && position >= mine[0] && position < mine[1];
}
