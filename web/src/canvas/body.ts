// The body: row backgrounds (zebra), then each cell through its Cell
// Renderer over its find mark (the current match ringed), translated to the cell's corner and clipped to it, or the
// loading marker while its Key or value has not arrived, or the error marker
// once the Table Source failed the request for it. Cells of an unmatched row
// or column are dimmed under a wash. An open Group Item whose Subtotal is
// placed draws no figures: they are the Subtotal's. A Summary Item's column
// (a pivot's All column) is tinted as its row would be, where no Summary
// Item's row tints it.

import type { Align, CellCanvas, CellStatus } from "../renderer";
import { cellState, itemSpan } from "../snapshot";
import { PAD, clip, clipCell, summaryStyle, type PaintContext } from "./context";

/** Paints the body; returns whether a renderer asked for another frame. */
export function paintBody(p: PaintContext): boolean {
  const { ctx, s, theme, left, top, right } = p;
  ctx.save();
  clip(ctx, left, top, p.width - left, p.height - top);
  ctx.fillStyle = theme.body.background;
  ctx.fillRect(left, top, p.width - left, p.height - top);
  const cell = createCell(p);
  const pickedRows = new Set(s.picked.rows);
  const pickedCols = new Set(s.picked.cols);
  const cols = s.cols.strip;
  const colRoles = cols.map((col) => summaryStyle(theme, col.outline));
  const levels = s.appliedView.grouping?.rowLevels ?? [];
  s.rows.strip.forEach((row, i) => {
    const level = row.outline?.kind === "group" && row.outline.open === "open" ? levels[row.outline.depth] : undefined;
    const figureless = level !== undefined && "field" in level && level.subtotal !== "off";
    const role = summaryStyle(theme, row.outline);
    const fill = role?.background ?? (row.position % 2 === 1 ? theme.zebra : null);
    if (fill) {
      const span = itemSpan(s.rows, i);
      const y = Math.max(row.offset, span.start);
      ctx.fillStyle = fill;
      ctx.fillRect(left, y, right - left, Math.min(row.offset + row.size, span.end) - y);
    }
    const rowId = p.content.id("rows", row);
    const rowUnmatched = p.content.unmatched("rows", i);
    cols.forEach((col, j) => {
      const value = row.key === null || col.key === null ? undefined : p.content.value(i, j);
      const state = cellState(s, i, j);
      const failed = p.content.failed(i, j);
      const status: CellStatus = {
        focused: state.focused,
        selected: state.selected,
        picked: (row.key !== null && pickedRows.has(row.key)) || (col.key !== null && pickedCols.has(col.key)),
        editing: state.editing,
        loading: value === undefined && !failed && !figureless,
        flashing: row.key === null || col.key === null ? 0 : p.flashes.amount(row.key, col.key, p.now),
      };
      ctx.save();
      clipCell(p, i, j);
      ctx.translate(col.offset, row.offset);
      clip(ctx, 0, 0, col.size, row.size);
      cell.place(col.size, row.size, status);
      const colRole = colRoles[j];
      if (colRole && !role) {
        ctx.fillStyle = colRole.background;
        ctx.fillRect(0, 0, col.size, row.size);
      }
      if (state.currentMatch || p.content.marked(i, j)) {
        ctx.fillStyle = state.currentMatch ? theme.findCurrent.background : theme.findMatch.background;
        ctx.fillRect(0, 0, col.size, row.size);
      }
      if (state.currentMatch) {
        const line = theme.focus.width;
        ctx.strokeStyle = theme.findCurrent.color;
        ctx.lineWidth = line;
        ctx.strokeRect(line / 2, line / 2, col.size - line, row.size - line);
      }
      if (status.flashing > 0) {
        ctx.globalAlpha = status.flashing;
        ctx.fillStyle = theme.flash.background;
        ctx.fillRect(0, 0, col.size, row.size);
        ctx.globalAlpha = 1;
      }
      if (failed) cell.marker("⚠", theme.error);
      else if (status.loading) cell.marker("…", theme.loading);
      else if (!figureless) p.renderer(rowId, p.content.id("cols", col)).draw(cell, value);
      if (rowUnmatched || p.content.unmatched("cols", j)) {
        ctx.fillStyle = theme.unmatched;
        ctx.fillRect(0, 0, col.size, row.size);
      }
      ctx.restore();
    });
  });
  ctx.restore();
  return cell.again;
}

/** Writes `message` centred in the body, cut to fit. */
export function paintEmpty(p: PaintContext, message: string): void {
  const { ctx, theme, measure, left, top } = p;
  const shown = measure.fit(message, theme.font, p.width - left - 2 * PAD);
  if (shown === "") return;
  ctx.font = theme.font;
  ctx.fillStyle = theme.loading;
  ctx.fillText(shown, left + (p.width - left - measure.width(shown, theme.font)) / 2, top + (p.height - top) / 2);
}

/** The one CellCanvas a paint hands every renderer in turn. */
interface PaintedCell extends CellCanvas {
  width: number;
  height: number;
  /** Some renderer asked for another frame. */
  again: boolean;
  place(width: number, height: number, status: CellStatus): void;
  /** A marker in place of the value: loading or failed. */
  marker(glyph: string, color: string): void;
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
    marker(glyph, color) {
      write(glyph, "start", color);
    },
  };
  return cell;
}
