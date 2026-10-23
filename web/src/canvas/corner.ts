// The corner grid where the two Header Bands meet, in the Tiers layout: the
// values caption and the values pseudo-level's name on its top row, each
// column level's name right-aligned in its own tier with an arrow, and the
// row levels' names along its bottom row, one per level column in header
// form or joined with › in outline form.

import type { AxisName } from "../codes";
import type { Content } from "../content";
import type { Measure } from "../measure";
import type { CornerLevel, Rect, Snapshot } from "../snapshot";
import type { Theme } from "../theme";
import { PAD, clip, type PaintContext } from "./context";

/** A piece of text the corner draws: a level's name, the values caption, or a separator. */
export interface CornerText {
  kind: "level" | "caption" | "join";
  /** The axis whose level it names (`cols` for the caption). */
  axis: AxisName;
  /** The level it names, for `kind: "level"`. */
  level: CornerLevel | null;
  text: string;
  /** Where its text sits, the box a click on a level name lands in. */
  rect: Rect;
}

type Piece = Omit<CornerText, "rect">;

const GAP = 10;

/** The sort mark after a level name whose field sorts its axis. */
function sortMark(s: Snapshot, axis: AxisName, level: CornerLevel): string {
  if (level.field === null) return "";
  const sort = s.appliedView[axis].sort;
  const i = sort.findIndex((c) => c.key === level.field);
  if (i < 0) return "";
  const arrow = sort[i]!.direction === "ascending" ? " ▲" : " ▼";
  return sort.length > 1 ? `${arrow}${i + 1}` : arrow;
}

/**
 * Where each piece of text goes in the corner, in the Tiers layout. A row
 * of pieces runs left to right from its cell's start, or right-aligned to
 * its end; the painter clips each to its cell.
 */
export function layOutCorner(s: Snapshot, content: Content, measure: Measure, theme: Theme): CornerText[] {
  const lines = s.cols.corner;
  const columns = s.rows.corner;
  if (lines.length === 0 || columns.length === 0) return [];
  const font = theme.headerFont;
  const level = (axis: AxisName, l: CornerLevel): Piece => ({
    kind: "level",
    axis,
    level: l,
    text: content.levelName(axis, l.field) + sortMark(s, axis, l),
  });
  const join = (axis: AxisName, text: string): Piece => ({ kind: "join", axis, level: null, text });
  const out: CornerText[] = [];
  /** Lays `pieces` on line `t` within `[from, to)`: from its start, or ending at its end if `right`. */
  const lay = (t: number, pieces: Piece[], from: number, to: number, right: boolean) => {
    const { start: y, end } = lines[t]!.across;
    const widths = pieces.map((p) => measure.width(p.text, font));
    const total = widths.reduce((a, b) => a + b, 0);
    let x = right ? Math.max(from + PAD, to - PAD - total) : from + PAD;
    pieces.forEach((p, i) => {
      out.push({ ...p, rect: { x, y, width: widths[i]!, height: end - y } });
      x += widths[i]!;
    });
  };
  const bottom = lines.length - 1;
  const [left, right] = [columns[0]!.across.start, columns[columns.length - 1]!.across.end];

  // Column levels, right-aligned in their own tiers with an arrow; the
  // values pseudo-level's name goes on the top row instead.
  lines.forEach((line, t) => {
    const named = line.levels.filter((l) => l.field !== null);
    if (named.length === 0) return;
    const pieces = named.flatMap((l, i) => (i === 0 ? [level("cols", l)] : [join("cols", " › "), level("cols", l)]));
    lay(t, [...pieces, join("cols", " →")], left, right, true);
  });

  // Row levels along the bottom row: each in its level column, or joined in the outline column.
  for (const column of columns) {
    const pieces = column.levels.flatMap((l, i) => (i === 0 ? [level("rows", l)] : [join("rows", " › "), level("rows", l)]));
    if (pieces.length > 0) lay(bottom, pieces, column.across.start, column.across.end, false);
  }

  // The caption and the values pseudo-level's name, on the top row: at its
  // start above a column level, or after the row names when there is one row.
  const head: Piece[] = [];
  const caption = content.caption();
  if (caption !== null) head.push({ kind: "caption", axis: "cols", level: null, text: caption });
  const values = lines.flatMap((l) => l.levels).find((l) => l.field === null);
  if (values) {
    if (head.length > 0) head.push(join("cols", "  "));
    head.push(level("cols", values));
  }
  if (head.length > 0) {
    const y = lines[0]!.across.start;
    const onTop = out.filter((p) => p.rect.y === y);
    if (bottom === 0) {
      const after = onTop.reduce((x, p) => Math.max(x, p.rect.x + p.rect.width + GAP - PAD), left);
      lay(0, head, after, right, true);
    } else {
      const before = onTop.reduce((x, p) => Math.min(x, p.rect.x - GAP + PAD), right);
      lay(0, head, left, before, false);
    }
  }
  return out;
}

/** The level name at (x, y) among `texts`, if any. */
export function cornerLevelAt(texts: readonly CornerText[], x: number, y: number): CornerText | undefined {
  return texts.find(({ kind, rect: r }) => kind === "level" && x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height);
}

/** The corner: its background, hairlines between its tiers, and its names in the Tiers layout. */
export function paintCorner(p: PaintContext): void {
  const { ctx, theme, measure, left, top, s, pixel } = p;
  ctx.fillStyle = theme.header.background;
  ctx.fillRect(0, 0, left, top);
  ctx.save();
  clip(ctx, 0, 0, left, top);
  const lines = s.cols.corner;
  const bottom = lines[lines.length - 1];
  ctx.fillStyle = theme.handle;
  for (const line of lines.slice(0, -1)) ctx.fillRect(0, line.across.end - pixel, left, pixel);
  if (bottom) {
    for (const column of s.rows.corner.slice(0, -1)) {
      ctx.fillRect(column.across.end - pixel, bottom.across.start, pixel, bottom.across.end - bottom.across.start);
    }
  }
  ctx.textAlign = "left";
  for (const t of layOutCorner(s, p.content, measure, theme)) {
    const cell = s.rows.corner.find((c) => c.across.start <= t.rect.x && t.rect.x < c.across.end);
    // A row level's name keeps to its level column; the rest may cross the corner.
    const end = t.axis === "rows" && cell ? cell.across.end : left;
    ctx.save();
    clip(ctx, t.rect.x, t.rect.y, Math.max(0, Math.min(t.rect.width, end - t.rect.x)), t.rect.height);
    ctx.font = t.kind === "caption" ? `italic ${theme.headerFont}` : theme.headerFont;
    ctx.fillStyle =
      t.kind === "join" ? theme.handle : t.kind === "caption" ? theme.rowNumber.color : t.level?.field === null ? theme.sortMarker : theme.header.color;
    ctx.fillText(t.text, t.rect.x, t.rect.y + t.rect.height / 2);
    ctx.restore();
  }
  ctx.restore();
}
