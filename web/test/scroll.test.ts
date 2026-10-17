// The hybrid scroll mapping, through the frame the widget runs: a fake
// scroller element (clamping like a browser's) in front of the real wasm Grid.
// Each property runs on either axis: rows scroll down, columns across.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, expect, test } from "vitest";
import type { AxisName } from "../src/codes";
import { createFeed } from "../src/feed";
import { runFrame, type FrameParts } from "../src/frame";
import { createScrollMapping, type ScrollPort } from "../src/scroller";
import type { Snapshot } from "../src/snapshot";
import type { TableSource } from "../src/source";
import { bridge } from "../src/wasm";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

/** A scroller element as far as the mapping sees it: offsets clamp to the sizer, in whole px. */
class FakeScroller implements ScrollPort {
  private top = 0;
  private left = 0;
  private height: number;
  private width: number;

  constructor(
    readonly clientWidth: number,
    readonly clientHeight: number,
  ) {
    this.width = clientWidth;
    this.height = clientHeight;
  }

  get scrollTop() {
    return this.top;
  }
  set scrollTop(v: number) {
    this.top = Math.round(Math.min(Math.max(0, v), this.height - this.clientHeight));
  }
  get scrollLeft() {
    return this.left;
  }
  set scrollLeft(v: number) {
    this.left = Math.round(Math.min(Math.max(0, v), this.width - this.clientWidth));
  }
  setSize(width: number, height: number) {
    this.width = Math.max(width, this.clientWidth);
    this.height = Math.max(height, this.clientHeight);
    this.scrollTop = this.top;
    this.scrollLeft = this.left;
  }
}

function range(start: number, end: number): number[] {
  return Array.from({ length: end - start }, (_, i) => start + i);
}

function numbered(rows: number, cols: number): TableSource<number, number, string> {
  return {
    rows: { identity: "position", count: () => rows, window: range },
    cols: { identity: "position", count: () => cols, window: range },
    cells: (rs, cs) => rs.map((r) => cs.map((c) => `r${r}c${c}`)),
  };
}

/** A grid scrolled along `axis`, with that axis's offset and travel at hand. */
class Harness {
  constructor(
    readonly axis: AxisName,
    readonly count: number,
    readonly port: FakeScroller,
    readonly parts: FrameParts,
  ) {}

  static draw(tc: hegel.TestCase, counts = anyCount): Harness {
    const axis = tc.draw(gs.sampledFrom<AxisName>(["rows", "cols"]));
    const count = tc.draw(counts);
    const size = tc.draw(gs.integers({ minValue: 1, maxValue: 400 }));
    const header = tc.draw(gs.integers({ minValue: 0, maxValue: 80 }));
    const width = tc.draw(gs.integers({ minValue: 120, maxValue: 2000 }));
    const height = tc.draw(gs.integers({ minValue: 120, maxValue: 1400 }));
    const b = bridge();
    const scrolled = new b.WebAxis(count, size).header(header).min_size(Math.min(size, 24));
    const other = () => new b.WebAxis(4, 60).header(30);
    const grid = axis === "rows" ? new b.WebGrid(scrolled, other()) : new b.WebGrid(other(), scrolled);
    const port = new FakeScroller(width, height);
    const source = axis === "rows" ? numbered(count, 4) : numbered(4, count);
    return new Harness(axis, count, port, {
      grid,
      scroll: createScrollMapping(port),
      feed: createFeed(source),
      painter: null,
      viewport: () => ({ width, height }),
      onEffect: () => {},
    });
  }

  frame(): Snapshot {
    return runFrame(this.parts, false).snapshot;
  }

  get offset(): number {
    return this.axis === "rows" ? this.port.scrollTop : this.port.scrollLeft;
  }

  set offset(v: number) {
    if (this.axis === "rows") this.port.scrollTop = v;
    else this.port.scrollLeft = v;
  }

  /** The largest offset the sizer allows. */
  max(): number {
    const was = this.offset;
    this.offset = Number.MAX_SAFE_INTEGER;
    const max = this.offset;
    this.offset = was;
    return max;
  }

  /** Where the axis is scrolled to, comparable across frames: Position, then offset. */
  place(s: Snapshot): [number, number] {
    return [s[this.axis].anchor.position, s[this.axis].anchor.offset];
  }
}

function before(a: [number, number], b: [number, number]): boolean {
  return a[0] < b[0] || (a[0] === b[0] && a[1] < b[1]);
}

/** Small tables and huge ones, up to the largest count that crosses exactly. */
const anyCount = gs.oneOf(
  gs.integers({ minValue: 1, maxValue: 5000 }),
  gs.integers({ minValue: 1, maxValue: Number.MAX_SAFE_INTEGER }),
);

/** An offset anywhere, or a small step from either end. */
function anywhere(max: number) {
  const near = gs.integers({ minValue: 0, maxValue: Math.min(max, 2000) });
  return gs.oneOf(gs.integers({ minValue: 0, maxValue: max }), near, near.map((d) => max - d));
}

test("scrolling forward never moves the view back", () =>
  hegel.test((tc) => {
    const h = Harness.draw(tc);
    let place = h.place(h.frame());
    let fraction = 0;
    const deltas = tc.draw(
      gs.arrays(gs.oneOf(gs.integers({ minValue: 1, maxValue: 300 }), gs.integers({ minValue: 1, maxValue: 2e7 })), {
        maxSize: 20,
      }),
    );
    for (const d of deltas) {
      h.offset += d;
      // Sometimes a scroll event takes the offset in before the frame does.
      if (tc.draw(gs.booleans())) h.parts.scroll.take(h.parts.grid);
      const s = h.frame();
      tc.note(`+${d}: anchor ${h.place(s)} thumb ${s[h.axis].thumb.fraction}`);
      expect(before(h.place(s), place)).toBe(false);
      expect(s[h.axis].thumb.fraction).toBeGreaterThanOrEqual(fraction);
      place = h.place(s);
      fraction = s[h.axis].thumb.fraction;
    }
  }));

test("dragging to either end reaches the first and last items", () =>
  hegel.test((tc) => {
    const h = Harness.draw(tc);
    h.frame();
    h.offset = tc.draw(anywhere(h.max()));
    h.frame();
    h.offset = h.max();
    let s = h.frame();
    expect(s[h.axis].strip.at(-1)?.position).toBe(h.count - 1);
    // A table that fits has no travel; one that overflows sits at its end.
    expect(s[h.axis].thumb.fraction).toBe(s[h.axis].thumb.size < 1 ? 1 : 0);
    expect(h.offset).toBe(h.max());
    h.offset = 0;
    s = h.frame();
    expect(h.place(s)).toEqual([0, 0]);
    expect(h.offset).toBe(0);
  }));

test("a scroll offset that changed before the frame is never overwritten unread", () =>
  hegel.test((tc) => {
    const h = Harness.draw(tc, gs.integers({ minValue: 10_000, maxValue: Number.MAX_SAFE_INTEGER }));
    h.frame();
    // Start in the middle, then move any distance short of either end.
    const half = Math.round(h.max() / 2);
    h.offset = half;
    const start = h.place(h.frame());
    const d = tc.draw(
      gs.oneOf(gs.integers({ minValue: -(half - 2), maxValue: -1 }), gs.integers({ minValue: 1, maxValue: half - 2 })),
    );
    h.offset += d;
    const end = h.place(h.frame());
    tc.note(`moved ${d}: ${start} -> ${end}`);
    expect(d > 0 ? before(start, end) : before(end, start)).toBe(true);
  }));

test("between the ends, the offset is never pinned to an end", () =>
  hegel.test((tc) => {
    const h = Harness.draw(tc);
    h.frame();
    const max = h.max();
    h.offset = tc.draw(anywhere(max));
    h.frame();
    // A small move from wherever the write-back left the offset.
    h.offset += tc.draw(gs.integers({ minValue: -300, maxValue: 300 }));
    const { fraction } = h.frame()[h.axis].thumb;
    tc.note(`fraction ${fraction}, offset ${h.offset} of ${max}`);
    if (fraction > 0 && fraction < 1) {
      expect(h.offset).toBeGreaterThan(0);
      expect(h.offset).toBeLessThan(max);
    }
  }));
