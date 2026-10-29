// A widget without a DOM: the real wasm Grid and Feed run through the real
// frame, with a painter that only records what it was handed.

import type { Content } from "../src/content";
import { createFeed, type Feed } from "../src/feed";
import { runFrame, type FrameParts } from "../src/frame";
import type { AxisName } from "../src/codes";
import { createInitialSizes, type SizeChoices } from "../src/sizes";
import type { ScrollMapping } from "../src/scroller";
import type { Snapshot } from "../src/snapshot";
import type { FailedRequest, TableSource } from "../src/source";
import { bridge } from "../src/wasm";

export const ROW_HEIGHT = 20;
export const COL_WIDTH = 80;
export const HEADER = 24;
export const BAND = 40;

const still: ScrollMapping = { take: () => false, writeBack: () => {} };

export class Harness<RowId, ColId, V> {
  readonly feed: Feed<RowId, ColId>;
  readonly parts: FrameParts;
  snapshot!: Snapshot;
  content: Content | null = null;
  /** Whether the initial sizes computed `size` for `key`, as the widget asks for a layout. */
  readonly computed: (axis: AxisName, key: bigint, size: number) => boolean;
  /** Every failed request the feed reported, with its error's message. */
  readonly errors: { message: string; request: FailedRequest<RowId, ColId> }[] = [];

  constructor(
    source: TableSource<RowId, ColId, V>,
    readonly width = 500,
    readonly height = 300,
    sizes?: SizeChoices<RowId, ColId>,
  ) {
    this.feed = createFeed(source, {
      failed: (error, request) => this.errors.push({ message: (error as Error).message, request }),
      windowed: (axis, keys) => sizing?.arrived(axis, keys),
    });
    const counts = this.feed.counts();
    const b = bridge();
    const grid = new b.WebGrid(
      new b.WebAxis(counts.rows, ROW_HEIGHT).header(HEADER),
      new b.WebAxis(counts.cols, COL_WIDTH).header(BAND),
    );
    const sizing =
      sizes &&
      createInitialSizes(sizes as SizeChoices<unknown, unknown>, {
        grid,
        id: (axis, key) => (axis === "rows" ? this.feed.rowId(key) : this.feed.colId(key)),
        fit: fitLength,
      });
    this.computed = (axis, key, size) => sizing?.computed(axis, key, size) ?? false;
    this.feed.connect(grid);
    this.parts = {
      grid,
      scroll: still,
      feed: this.feed,
      painter: {
        element: null as unknown as HTMLElement,
        resize() {},
        paint: (_s, content) => {
          this.content = content;
          return false;
        },
      },
      viewport: () => ({ width: this.width, height: this.height }),
      onEffect: (e) => this.feed.answer(e),
      ...(sizing ? { settle: (s: Snapshot) => sizing.settle(s, this.feed.content(s)) } : {}),
    };
  }

  get grid() {
    return this.parts.grid;
  }

  /** The last Snapshot drawn, as the widget gives it to `restoreLayout`. */
  get shown(): Snapshot | undefined {
    return this.snapshot;
  }

  frame(): Snapshot {
    this.snapshot = runFrame(this.parts, false).snapshot;
    return this.snapshot;
  }

  /** Runs frames while they settle sizes, as the widget would, and returns the last Snapshot. */
  settled(): Snapshot {
    for (let i = 0; i < 4 && runFrame(this.parts, false).again; i++);
    return this.frame();
  }

  /** A click at (x, y), Effects answered as the widget would. */
  click(x: number, y: number, modifiers = 0): void {
    this.grid.pointer(0, x, y, 0, modifiers, 1);
    this.grid.pointer(2, x, y, 0, modifiers, 1);
    this.drain();
  }

  drain(): void {
    for (const e of this.grid.poll_effects() as Parameters<Feed["answer"]>[0][]) this.feed.answer(e);
  }

  /** The text of every visible cell, row by row. */
  values(): unknown[][] {
    const s = this.snapshot;
    return s.rows.strip.map((_, i) => s.cols.strip.map((_, j) => this.content?.value(i, j)));
  }

  /** Whether each visible cell shows its request failed, row by row. */
  failed(): boolean[][] {
    const s = this.snapshot;
    return s.rows.strip.map((_, i) => s.cols.strip.map((_, j) => this.content?.failed(i, j) ?? false));
  }

  destroy(): void {
    this.feed.destroy();
    this.grid.free();
  }
}

/** A stand-in for measuring: ten px per character of the widest visible value, once all of them are in. */
function fitLength(s: Snapshot, content: Content, key: bigint): number | null {
  const j = s.cols.strip.findIndex((i) => i.key === key);
  if (j < 0) return null;
  let width = 0;
  for (let i = 0; i < s.rows.strip.length; i++) {
    const value = content.value(i, j);
    if (value === undefined) return null;
    width = Math.max(width, 10 * String(value).length);
  }
  return width;
}
