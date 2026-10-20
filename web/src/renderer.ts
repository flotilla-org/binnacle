// The Cell Renderer contract: how one cell draws on the canvas, chosen per
// cell by `renderer(rowId, colId)`. The painter fills the cell's background
// before `draw` and lays the selection and focus ring over it after.

import { PAD } from "./canvas/context";
import type { Measure } from "./measure";
import type { Theme } from "./theme";

export interface CellRenderer<V> {
  /** Draws `value` into the cell. Not called while the value is loading: the painter draws its own marker. */
  draw(cell: CellCanvas, value: V): void;
  /** The width the cell would like, CSS px with padding: what a column's auto-size fits. Defaults to its `text` as plain text. */
  preferredWidth?(value: V, measure: CellMeasure): number;
  /** The cell's text for assistive technology; `String(value)` by default. */
  text?(value: V): string;
}

/** What the Grid and the host say about the cell being drawn. */
export interface CellStatus {
  focused: boolean;
  /** Inside the selected range. */
  selected: boolean;
  /** In a picked row or column. */
  picked: boolean;
  editing: boolean;
  /** The value is not there yet. Renderers are not called for such cells today, so this is false in `draw`. */
  loading: boolean;
  /** How lit the host's value-change flash is, from 1 when marked fading to 0 (not flashing). */
  flashing: number;
}

/**
 * One cell to draw into. Valid only during `draw`: the painter reuses it for
 * the next cell.
 */
export interface CellCanvas {
  /** Translated so (0, 0) is the cell's top-left corner, even when that is scrolled out of view, and clipped to its visible part. */
  readonly ctx: CanvasRenderingContext2D;
  /** The whole cell, visible or not, CSS px. */
  readonly width: number;
  readonly height: number;
  readonly status: Readonly<CellStatus>;
  readonly theme: Theme;
  /** The frame's time, `performance.now()` milliseconds. */
  readonly now: number;
  /** Writes `s` across the middle of the cell in the Theme's font and colour, padded, ellipsised to fit. */
  text(s: string, align?: Align): void;
  /** Asks for another frame after this one, to animate. Ask again each frame for as long as it should run. */
  requestFrame(): void;
}

export type Align = "start" | "end" | "center";

/** Measurement for `preferredWidth`, cached across frames. */
export interface CellMeasure {
  readonly theme: Theme;
  /** The width `CellCanvas.text` needs to show `s` whole, padding included. */
  text(s: string): number;
  /** The width of `s` in `font` (the Theme's body font by default), without padding. */
  width(s: string, font?: string): number;
}

/** Chooses the renderer for a cell, by the Table Source's own IDs. Return shared renderers; do not create one per call. */
export type RendererChoice<RowId, ColId, V> = (rowId: RowId, colId: ColId) => CellRenderer<V>;

/** Draws `String(value)`, nothing for `null`: the default renderer. */
export const plainText: CellRenderer<unknown> = {
  draw(cell, value) {
    if (value != null) cell.text(String(value));
  },
  text: (value) => (value == null ? "" : String(value)),
};

/** Chooses by column; `undefined` draws plain text. */
export function byColumn<ColId, V = unknown>(
  pick: (colId: ColId) => CellRenderer<V> | undefined,
): RendererChoice<unknown, ColId, V> {
  return (_rowId, colId) => pick(colId) ?? (plainText as CellRenderer<V>);
}

/** Chooses by row, as a transposed table wants; `undefined` draws plain text. */
export function byRow<RowId, V = unknown>(
  pick: (rowId: RowId) => CellRenderer<V> | undefined,
): RendererChoice<RowId, unknown, V> {
  return (rowId) => pick(rowId) ?? (plainText as CellRenderer<V>);
}

/** The text a renderer gives a value, for assistive technology and for measuring. */
export function cellText<V>(renderer: CellRenderer<V>, value: V): string {
  return renderer.text ? renderer.text(value) : value == null ? "" : String(value);
}

/** A `CellMeasure` over the painter's cached measurement. */
export function cellMeasure(measure: Measure, theme: Theme): CellMeasure {
  return {
    theme,
    text: (s) => (s === "" ? 0 : measure.width(s, theme.font) + 2 * PAD),
    width: (s, font = theme.font) => measure.width(s, font),
  };
}

/** The width a cell would like: its renderer's own answer, or its text's. */
export function preferredWidth<V>(renderer: CellRenderer<V>, value: V, measure: CellMeasure): number {
  return renderer.preferredWidth ? renderer.preferredWidth(value, measure) : measure.text(cellText(renderer, value));
}
