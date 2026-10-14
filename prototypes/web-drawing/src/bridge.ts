// PROTOTYPE (#45): decodes the flat Snapshot array from wasm/src/lib.rs into
// plain objects a painter reads. Keep the offsets in step with that file.

export interface StripItem {
  position: number;
  key: number | null; // null while the Table Source has not filled it
  offset: number;
  size: number;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface AxisView {
  count: number;
  extent: number;
  /** Thickness of the band leading this axis (rows: column header height). */
  header: number;
  gap: number;
  anchor: { position: number; offset: number };
  thumb: { fraction: number; size: number };
  focus: { position: number; key: number | null } | null;
}

export const MODES = ["Idle", "DraggingSelection", "Resizing", "ResizingBand", "MovingHeaders", "Editing"];
export const CURSORS = ["default", "col-resize", "row-resize", "grabbing"];

export interface Snapshot {
  generation: number;
  damage: { layout: boolean; selection: boolean; positions: boolean };
  rows: StripItem[];
  cols: StripItem[];
  axes: [AxisView, AxisView];
  focusRect: Rect | null;
  editorRect: Rect | null;
  selectionRects: Rect[];
  selectionRange: { rows: [number, number]; cols: [number, number] } | null;
  cursor: string;
  mode: string;
  capturing: boolean;
  resizeGuide: { axis: number; offset: number } | null;
  dropIndicator: { axis: number; offset: number } | null;
  hoveredHeader: { axis: number; position: number } | null;
  animating: boolean;
}

const HEAD = 48;
const nn = (v: number) => (Number.isNaN(v) ? null : v);
const rect = (f: Float64Array, i: number): Rect | null =>
  Number.isNaN(f[i]) ? null : { x: f[i], y: f[i + 1], width: f[i + 2], height: f[i + 3] };

export function decode(f: Float64Array): Snapshot {
  const axes = [0, 1].map((i) => ({
    count: f[2 + i],
    extent: f[4 + i],
    header: f[6 + i],
    gap: f[8 + i],
    anchor: { position: f[10 + i * 2], offset: f[11 + i * 2] },
    thumb: { fraction: f[14 + i * 2], size: f[15 + i * 2] },
    focus: Number.isNaN(f[18 + i * 2]) ? null : { position: f[18 + i * 2], key: nn(f[19 + i * 2]) },
  })) as [AxisView, AxisView];
  const nRows = f[40], nCols = f[41], nSel = f[42];
  const items = (start: number, n: number) => {
    const out: StripItem[] = new Array(n);
    for (let j = 0; j < n; j++) {
      const i = start + j * 4;
      out[j] = { position: f[i], key: nn(f[i + 1]), offset: f[i + 2], size: f[i + 3] };
    }
    return out;
  };
  const rows = items(HEAD, nRows);
  const cols = items(HEAD + nRows * 4, nCols);
  const selectionRects: Rect[] = [];
  for (let j = 0, i = HEAD + (nRows + nCols) * 4; j < nSel; j++, i += 4) selectionRects.push(rect(f, i)!);
  const d = f[1];
  return {
    generation: f[0],
    damage: { layout: (d & 1) !== 0, selection: (d & 2) !== 0, positions: (d & 4) !== 0 },
    rows,
    cols,
    axes,
    focusRect: rect(f, 22),
    editorRect: rect(f, 26),
    selectionRects,
    selectionRange: Number.isNaN(f[43]) ? null : { rows: [f[43], f[44]], cols: [f[45], f[46]] },
    cursor: CURSORS[f[30]],
    mode: MODES[f[31]],
    capturing: f[32] === 1,
    resizeGuide: Number.isNaN(f[33]) ? null : { axis: f[33], offset: f[34] },
    dropIndicator: Number.isNaN(f[35]) ? null : { axis: f[35], offset: f[36] },
    hoveredHeader: Number.isNaN(f[37]) ? null : { axis: f[37], position: f[38] },
    animating: f[39] === 1,
  };
}

/** Whether the cell at row/col is focused, from the Snapshot's focus Positions and Keys. */
export function isFocused(s: Snapshot, row: StripItem, col: StripItem): boolean {
  const fr = s.axes[0].focus, fc = s.axes[1].focus;
  return !!fr && !!fc && fr.key !== null && fc.key !== null &&
    fr.position === row.position && fr.key === row.key &&
    fc.position === col.position && fc.key === col.key;
}
