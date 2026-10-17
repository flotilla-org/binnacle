// A widget without a DOM: the real wasm Grid and Feed run through the real
// frame, with a painter that only records what it was handed.

import type { Content } from "../src/content";
import { createFeed, type Feed } from "../src/feed";
import { runFrame, type FrameParts } from "../src/frame";
import type { ScrollMapping } from "../src/scroller";
import type { Snapshot } from "../src/snapshot";
import type { TableSource } from "../src/source";
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

  constructor(
    source: TableSource<RowId, ColId, V>,
    readonly width = 500,
    readonly height = 300,
  ) {
    this.feed = createFeed(source);
    const counts = this.feed.counts();
    const b = bridge();
    const grid = new b.WebGrid(
      new b.WebAxis(counts.rows, ROW_HEIGHT).header(HEADER),
      new b.WebAxis(counts.cols, COL_WIDTH).header(BAND),
    );
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
    };
  }

  get grid() {
    return this.parts.grid;
  }

  frame(): Snapshot {
    this.snapshot = runFrame(this.parts, false).snapshot;
    return this.snapshot;
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

  destroy(): void {
    this.feed.destroy();
    this.grid.free();
  }
}
