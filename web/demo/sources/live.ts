// The live source: 200 tickers whose prices and loads move four times a
// second, like the ratatui demo's `--live`. The price is right-aligned, its
// recent history a sparkline, the load a bar, and changed cells flash. Each
// tick tells the widget through Change Notices, so a sort by price or load
// holds as the values move. A hand-written Table Source, renderers by column.

import {
  barRenderer,
  byColumn,
  GridWidget,
  sparklineRenderer,
  textRenderer,
  type AxisSink,
  type CellRenderer,
  type SortSpec,
  type TableSource,
} from "../../src";
import type { DemoSource } from "./demo-source";

const TICKERS = 200;
const HISTORY = 24;
const TICK_MS = 250;
const NAMES = ["anchor", "bosun", "capstan", "davit", "ensign", "fathom", "galley", "hawser"];
const COLUMNS = ["ticker", "price", "trend", "load"] as const;

type Column = (typeof COLUMNS)[number];
type Value = string | number | readonly number[];

interface Ticker {
  name: string;
  price: number;
  /** Recent prices, oldest first, ending with `price`. */
  history: number[];
  /** Between 0 and 1. */
  load: number;
}

/** Numbers in 0..1 from a fixed seed (mulberry32), so runs repeat. */
function random(seed: number): () => number {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 2 ** 32;
  };
}

const value = (t: Ticker, col: Column): Value =>
  col === "ticker" ? t.name : col === "price" ? t.price : col === "trend" ? t.history : t.load;

function compare(a: Ticker, b: Ticker, spec: SortSpec<Column>): number {
  for (const { id, direction } of spec) {
    const [x, y] = [value(a, id), value(b, id)];
    const order = typeof x === "string" ? x.localeCompare(y as string) : (x as number) - (y as number);
    if (order !== 0) return direction === "ascending" ? order : -order;
  }
  return 0;
}

const renderers: Partial<Record<Column, CellRenderer<Value>>> = {
  price: textRenderer<Value>({
    align: "end",
    format: new Intl.NumberFormat("en", { minimumFractionDigits: 2, maximumFractionDigits: 2 }),
  }),
  trend: sparklineRenderer<Value>(),
  load: barRenderer<Value>(),
};

export const live: DemoSource = {
  name: "live",
  description: `${TICKERS} tickers ticking: sparklines, bars and flashes`,
  async mount(host, toolbar, look) {
    const next = random(0x2545f491);
    const tickers: Ticker[] = Array.from({ length: TICKERS }, (_, i) => {
      // A made-up past, so the sparklines are full from the start.
      const history = [10 + next() * 990];
      while (history.length < HISTORY) history.push(Math.max(0.01, history.at(-1)! * (1 + (next() - 0.5) * 0.04)));
      return { name: `${NAMES[i % 8]!.toUpperCase()} ${Math.floor(i / 8)}`, price: history.at(-1)!, history, load: next() };
    });
    /** Row IDs (indices into `tickers`) in the order shown. */
    let order = tickers.map((_, i) => i);
    let spec: SortSpec<Column> = [];
    const sinks = new Set<AxisSink<number>>();
    const resort = () => {
      order = tickers.map((_, i) => i);
      if (spec.length) order.sort((a, b) => compare(tickers[a]!, tickers[b]!, spec) || a - b);
    };

    const source: TableSource<number, Column, Value> = {
      rows: {
        count: () => order.length,
        window: (start, end) => order.slice(start, end),
        sort(next) {
          spec = next;
          resort();
          for (const s of sinks) s.notify({ type: "Reordered" });
          return spec;
        },
        sortable: (by) => by !== "trend",
        locate: (id) => ({ type: "Found", position: order.indexOf(id) }),
      },
      cols: { count: () => COLUMNS.length, window: (start, end) => COLUMNS.slice(start, end) },
      cells: (rows, cols) => rows.map((r) => cols.map((c) => value(tickers[r]!, c))),
      attach(sink) {
        sinks.add(sink.rows);
        return () => sinks.delete(sink.rows);
      },
    };

    const widget = await GridWidget.create(host, {
      source,
      renderer: byColumn((col: Column) => renderers[col]),
      columnWidth: 140,
      ...look,
    });

    /** Moves about a quarter of the tickers and tells the widget what changed. */
    const tick = () => {
      const moved: number[] = [];
      tickers.forEach((t, i) => {
        if (next() >= 0.25) return;
        const step = next() - 0.5;
        t.price = Math.max(0.01, t.price * (1 + step * 0.04));
        t.history.push(t.price);
        if (t.history.length > HISTORY) t.history.shift();
        t.load = Math.min(1, Math.max(0, t.load + (next() - 0.5) * 0.2));
        moved.push(i);
      });
      // A ContentChanged over the whole table is one cheap block for 200
      // rows, and leaves nothing stale whatever was painted last.
      for (const s of sinks) s.notify({ type: "ContentChanged", start: 0, end: order.length });
      if (spec.length) {
        const was = order.join();
        resort();
        if (order.join() !== was) for (const s of sinks) s.notify({ type: "Reordered" });
      }
      for (const i of moved) {
        widget.flash(i, "price");
        widget.flash(i, "load");
      }
    };
    let timer = setInterval(tick, TICK_MS);

    const pause = document.createElement("button");
    pause.type = "button";
    pause.textContent = "Pause";
    pause.setAttribute("aria-pressed", "false");
    pause.addEventListener("click", () => {
      const paused = pause.getAttribute("aria-pressed") !== "true";
      pause.setAttribute("aria-pressed", String(paused));
      pause.textContent = paused ? "Resume" : "Pause";
      clearInterval(timer);
      if (!paused) timer = setInterval(tick, TICK_MS);
    });
    toolbar.append(pause);

    return {
      get theme() {
        return widget.theme;
      },
      set theme(choice) {
        widget.theme = choice;
      },
      get border() {
        return widget.border;
      },
      set border(style) {
        widget.border = style;
      },
      destroy() {
        clearInterval(timer);
        widget.destroy();
        pause.remove();
      },
    };
  },
};
