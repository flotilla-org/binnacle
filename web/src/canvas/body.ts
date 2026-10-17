// The body: row backgrounds (zebra), then each cell through its Cell
// Renderer, translated to the cell's corner and clipped to it, or the
// loading marker while its Key or value has not arrived.

import type { Align, CellCanvas, CellStatus } from "../renderer";
import { cellState } from "../snapshot";
import { PAD, clip, type PaintContext } from "./context";

/** Paints the body; returns whether a renderer asked for another frame. */
export function paintBody(p: PaintContext): boolean {
  const { ctx, s, theme, left, top, right } = p;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.fillStyle = theme.body.background;
  ctx.fillRect(left, top, p.width - left, p.height - top);
  const cell = createCell(p);
  const picked = new Set(s.picked);
  const cols = s.cols.strip;
  s.rows.strip.forEach((row, i) => {
    if (theme.zebra && row.position % 2 === 1) {
      ctx.fillStyle = theme.zebra;
      ctx.fillRect(left, row.offset, right - left, row.size);
    }
    const rowId = p.content.id("rows", row);
    cols.forEach((col, j) => {
      const value = row.key === null || col.key === null ? undefined : p.content.value(i, j);
      const state = cellState(s, i, j);
      const status: CellStatus = {
        focused: state.focused,
        selected: state.selected,
        picked: row.key !== null && picked.has(row.key),
        editing: state.editing,
        loading: value === undefined,
        flashing: row.key === null || col.key === null ? 0 : p.flashes.amount(row.key, col.key, p.now),
      };
      ctx.save();
      ctx.translate(col.offset, row.offset);
      clip(ctx, 0, 0, col.size, row.size);
      cell.place(col.size, row.size, status);
      if (status.flashing > 0) {
        ctx.globalAlpha = status.flashing;
        ctx.fillStyle = theme.flash.background;
        ctx.fillRect(0, 0, col.size, row.size);
        ctx.globalAlpha = 1;
      }
      if (status.loading) cell.marker();
      else p.renderer(rowId, p.content.id("cols", col)).draw(cell, value);
      ctx.restore();
    });
  });
  ctx.restore();
  return cell.again;
}

/** The one CellCanvas a paint hands every renderer in turn. */
interface PaintedCell extends CellCanvas {
  width: number;
  height: number;
  /** Some renderer asked for another frame. */
  again: boolean;
  place(width: number, height: number, status: CellStatus): void;
  /** The loading marker. */
  marker(): void;
}

function createCell(p: PaintContext): PaintedCell {
  const { ctx, theme, measure } = p;
  let status: CellStatus;
  const write = (s: string, align: Align, color: string) => {
    const shown = measure.fit(s, theme.font, cell.width - 2 * PAD);
    if (shown === "") return;
    const room = cell.width - 2 * PAD - measure.width(shown, theme.font);
    const x = PAD + (align === "end" ? room : align === "center" ? room / 2 : 0);
    ctx.font = theme.font;
    ctx.fillStyle = color;
    ctx.fillText(shown, x, cell.height / 2);
  };
  const cell: PaintedCell = {
    ctx,
    width: 0,
    height: 0,
    get status(): Readonly<CellStatus> {
      return status;
    },
    theme,
    now: p.now,
    again: false,
    text(s: string, align: Align = "start") {
      write(s, align, status.flashing >= 0.5 ? theme.flash.color : theme.body.color);
    },
    requestFrame() {
      cell.again = true;
    },
    place(width: number, height: number, next: CellStatus) {
      cell.width = width;
      cell.height = height;
      status = next;
    },
    marker() {
      write("…", "start", theme.loading);
    },
  };
  return cell;
}
