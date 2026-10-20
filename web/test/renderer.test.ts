// Cell Renderers as a widget user sees them: choosing one per cell through
// the column- and row-driven adapters, and drawing through the painter.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { afterEach, beforeAll, describe, expect, test, vi } from "vitest";
import {
  barRenderer,
  byColumn,
  byRow,
  fromRows,
  light,
  plainText,
  sparklineRenderer,
  textRenderer,
  type CellRenderer,
  type RendererChoice,
  type TableSource,
} from "../src";
import { answerAutoSize } from "../src/autosize";
import { MARKED } from "../src/canvas/bands";
import { PAD } from "../src/canvas/context";
import { createCanvasPainter } from "../src/canvas/painter";
import { AXIS_CODE, HEADER_CLICK } from "../src/codes";
import { FLASH_MS } from "../src/flash";
import { fakeDocument } from "./canvas";
import { COL_WIDTH, HEADER, Harness, ROW_HEIGHT } from "./harness";
import { loadForTests } from "./wasm";

const named = (name: string): CellRenderer<unknown> => ({ draw() {}, text: () => name });
const ids = gs.text();
/** An ID that is `special` about half the time, any other ID otherwise. */
const idNear = (special: string) => gs.oneOf(gs.just(special), ids);

test("byColumn picks by the column whatever the row, and falls back to text", () =>
  hegel.test((tc) => {
    const special = tc.draw(ids);
    const price = named("price");
    const choose = byColumn((col: string) => (col === special ? price : undefined));
    const row = tc.draw(gs.integers());
    const col = tc.draw(idNear(special));
    expect(choose(row, col)).toBe(col === special ? price : plainText);
  }));

test("byRow picks by the row whatever the column, and falls back to text", () =>
  hegel.test((tc) => {
    const special = tc.draw(ids);
    const totals = named("totals");
    const choose = byRow((row: string) => (row === special ? totals : undefined));
    const row = tc.draw(idNear(special));
    const col = tc.draw(gs.integers());
    expect(choose(row, col)).toBe(row === special ? totals : plainText);
  }));

describe("painting through Cell Renderers", () => {
  beforeAll(async () => {
    await loadForTests();
    fakeDocument();
  });

  type Seen = { row: string; col: string; value: string; width: number; height: number };

  function paintOnce(renderer: (seen: Seen[]) => RendererChoice<string, string, string>) {
    const rows = ["a", "b", "c"];
    const cols = ["x", "y"];
    const source: TableSource<string, string, string> = {
      rows: { count: () => rows.length, window: (s, e) => rows.slice(s, e) },
      cols: { count: () => cols.length, window: (s, e) => cols.slice(s, e) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
    };
    const h = new Harness(source);
    h.frame();
    const s = h.frame();
    const painter = createCanvasPainter(light);
    painter.resize(h.width, h.height, 1);
    const seen: Seen[] = [];
    painter.renderer = renderer(seen) as RendererChoice<unknown, unknown, unknown>;
    const again = painter.paint(s, h.feed.content(s));
    return { seen, again };
  }

  test("every visible cell is drawn by the renderer chosen for its IDs, at the cell's size", () => {
    const { seen, again } = paintOnce((seen) => (row, col) => ({
      draw: (cell, value) => seen.push({ row, col, value, width: cell.width, height: cell.height }),
    }));
    expect(seen).toEqual(
      ["a", "b", "c"].flatMap((row) =>
        ["x", "y"].map((col) => ({ row, col, value: `${row}${col}`, width: COL_WIDTH, height: ROW_HEIGHT })),
      ),
    );
    expect(again).toBe(false);
  });

  test("a renderer that requests a frame makes the paint ask for another", () => {
    const { again } = paintOnce(() => (row) => ({
      draw: (cell) => {
        if (row === "b") cell.requestFrame();
      },
    }));
    expect(again).toBe(true);
  });
});

describe("value-change flashes", () => {
  beforeAll(async () => {
    await loadForTests();
    fakeDocument();
  });
  afterEach(() => vi.useRealTimers());

  test("a marked cell is lit fully, fades, and stops asking for frames once dark", () => {
    vi.useFakeTimers({ toFake: ["performance"] });
    const h = new Harness<number, number, string>({
      rows: { identity: "position", count: () => 3, window: (s, e) => [0, 1, 2].slice(s, e) },
      cols: { identity: "position", count: () => 2, window: (s, e) => [0, 1].slice(s, e) },
      cells: (rs, cs) => rs.map((r) => cs.map((c) => `${r}${c}`)),
    });
    h.frame();
    const s = h.frame();
    const painter = createCanvasPainter(light);
    painter.resize(h.width, h.height, 1);
    const lit = new Map<string, number>();
    painter.renderer = (row, col) => ({ draw: (cell) => lit.set(`${String(row)},${String(col)}`, cell.status.flashing) });
    const paint = () => painter.paint(s, h.feed.content(s));

    painter.flashes.mark(1n, 0n, performance.now());
    painter.flashes.mark(2n, null, performance.now());
    expect(paint()).toBe(true);
    expect(Object.fromEntries(lit)).toEqual({ "0,0": 0, "0,1": 0, "1,0": 1, "1,1": 0, "2,0": 1, "2,1": 1 });

    vi.advanceTimersByTime(FLASH_MS / 2);
    expect(paint()).toBe(true);
    expect(lit.get("1,0")).toBeCloseTo(0.5);

    vi.advanceTimersByTime(FLASH_MS / 2);
    expect(paint()).toBe(false);
    expect([...lit.values()].every((v) => v === 0)).toBe(true);
  });
});

describe("the built-in renderers", () => {
  test("text formats numbers through an Intl.NumberFormat and leaves other values as they print", () => {
    const price = textRenderer({ format: new Intl.NumberFormat("en", { minimumFractionDigits: 2 }) });
    expect([1234.5, 7n, "n/a", null].map((v) => price.text!(v))).toEqual(["1,234.50", "7.00", "n/a", ""]);
  });

  test("a sparkline reads as its latest value, its direction and its range", () => {
    const trend = sparklineRenderer();
    expect(trend.text!([4, 1, 3.456])).toBe("3.46, falling, 1 to 4 over 3 values");
    expect(trend.text!([2, NaN, 5])).toBe("5, rising, 2 to 5 over 2 values");
    expect(trend.text!([7])).toBe("7");
    expect(trend.text!([])).toBe("no values");
  });

  test("a bar reads as its fraction in percent, clamped", () => {
    const load = barRenderer();
    expect([0.425, 0, 1.7, -2, NaN].map((v) => load.text!(v))).toEqual(["43%", "0%", "100%", "0%", "no value"]);
  });
});

describe("auto-size", () => {
  beforeAll(loadForTests);

  test("a column fits the widest preferred width among its visible cells, each from its own renderer", () => {
    const rows = ["a", "b", "c"];
    const h = new Harness<string, string, number>({
      rows: { count: () => rows.length, window: (s, e) => rows.slice(s, e) },
      cols: { count: () => 2, window: (s, e) => ["x", "y"].slice(s, e) },
      cells: (rs, cs) => rs.map((_, i) => cs.map(() => 50 + 40 * i)),
    });
    h.frame();
    const s = h.frame();
    // Row b's renderer asks for its value; the others for half of it.
    const renderer: RendererChoice<string, string, number> = (row) => ({
      draw() {},
      preferredWidth: (v) => (row === "b" ? v : v / 2),
    });
    const measure = { width: () => 0, fit: (t: string) => t, clear() {} };
    const key = s.cols.strip[0]!.key!;
    answerAutoSize({ type: "AutoSizeRequested", generation: s.generation, axis: "cols", key }, s, h.feed.content(s), {
      grid: h.grid,
      measure,
      theme: light,
      headerHeight: 24,
      renderer: renderer as RendererChoice<unknown, unknown, unknown>,
    });
    expect(h.frame().cols.strip[0]!.size).toBe(90);
  });

  test("the row header band fits its labels, with room for a sort marker while the columns sort by a row shown", () => {
    const source = fromRows(
      [
        { a: 2, b: 1 },
        { a: 1, b: 2 },
      ],
      [{ key: "a" }, { key: "b" }],
    );
    const h = new Harness(source);
    try {
      const measure = { width: (t: string) => 10 * t.length, fit: (t: string) => t, clear() {} };
      const fit = () => {
        const s = h.frame();
        answerAutoSize({ type: "BandAutoSizeRequested", generation: s.generation, axis: "cols" }, s, h.feed.content(s), {
          grid: h.grid,
          measure,
          theme: light,
          headerHeight: 24,
          renderer: () => plainText,
        });
        return h.frame().cols.header;
      };
      h.frame();
      // Labels "1" and "2".
      expect(fit()).toBe(10 + 2 * PAD);
      h.grid.set_header_click(AXIS_CODE.rows, HEADER_CLICK.sort);
      h.click(5, HEADER + ROW_HEIGHT / 2);
      h.frame();
      expect(h.snapshot.rows.headers[0]!.sort.applied).not.toBeNull();
      expect(fit()).toBe(10 + 2 * PAD + MARKED);
    } finally {
      h.destroy();
      source.destroy();
    }
  });
});

test("fromRows columns carry their renderers, plain text where they name none", async () => {
  await loadForTests();
  const price = textRenderer<number>({ align: "end" });
  const source = fromRows(
    [
      { ticker: "A", price: 1 },
      { ticker: "B", price: 2 },
    ],
    [{ key: "ticker" }, { key: "price", renderer: price }],
  );
  expect(source.renderer?.(1, "price")).toBe(price);
  expect(source.renderer?.(0, "ticker")).toBe(plainText);
  source.destroy();
});
