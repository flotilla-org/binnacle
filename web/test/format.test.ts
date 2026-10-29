// The shared formatter through wasm: the shared fixture formats to the same
// expected text as in Rust, any Typed Value formats to one line, and
// `valueRenderer` draws the style hints in the Theme's colours and asks for
// a frame a second while it shows a relative time.

import { readFileSync } from "node:fs";
import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { beforeAll, describe, expect, test } from "vitest";
import { valuesFixture } from "../demo/sources/values-data";
import { columnFormatter, DIM, fewestDecimals, fixedZone, light, valueRenderer, WARNING, type TypedValue } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { fakeDocument, type DrawCall } from "./canvas";
import { Harness } from "./harness";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

const NOW = Date.parse("2026-10-29T13:00:00Z");

test("the shared fixture formats to the expected text the Rust tests check", () => {
  const fixture = valuesFixture();
  const clock = { now: fixture.now, zone: fixedZone(fixture.utcOffset) };
  const lines = fixture.rows.flatMap((row, r) =>
    fixture.columns.map((column) => {
      const f = columnFormatter(column).format(row[column.name], clock);
      const style = [
        [1, "dim"],
        [2, "warning"],
        [4, "pending"],
      ]
        .filter(([bit]) => f.style & (bit as number))
        .map(([, name]) => name);
      return `${r}\t${column.name}\t${f.text}\t${f.align}\t${style.join("+") || "-"}\n`;
    }),
  );
  const expected = readFileSync(new URL("../../crates/binnacle-format/fixtures/values.expected.txt", import.meta.url), "utf8");
  expect(lines.join("")).toBe(expected);
});

test("each kind TypeScript writes reaches the formatter as its Typed Value", () => {
  const clock = { now: NOW, zone: fixedZone(0) };
  const formatter = columnFormatter();
  const plain = (v: TypedValue, c: typeof clock) => formatter.format(v, c);
  const text = (v: TypedValue) => plain(v, clock).text;
  expect(plain(null, clock)).toEqual({ text: "∅", align: "start", style: DIM });
  expect(text(undefined)).toBe("∅");
  expect(text(true)).toBe("✓");
  expect(text(2.5)).toBe("2.5");
  expect(text(-(2n ** 63n))).toBe("-9223372036854775808");
  expect(text(2n ** 64n - 1n)).toBe("18446744073709551615");
  expect(plain(2n ** 64n, clock)).toEqual({ text: "18446744073709551616", align: "start", style: WARNING });
  expect(text("Bergen")).toBe("Bergen");
  expect(text(new Date(NOW))).toBe("2026-10-29 13:00");
  expect(text({ instant: BigInt(NOW) * 1_000_000n })).toBe("2026-10-29 13:00");
  expect(plain(new Date(Number.NaN), clock)).toEqual({ text: "Invalid Date", align: "start", style: WARNING });
  expect(text(Uint8Array.of(0x3f, 0xa9, 0xc2, 1))).toBe("4 B 3fa9c2…");
  expect(text({ category: "ok", label: "Healthy" })).toBe("Healthy");
  expect(text({ category: "ok" })).toBe("ok");
  expect(text({ nested: { b: [1], a: null } })).toBe('{"a":null,"b":[1]}');
  expect(plain({ invalid: "12x", expected: "int" }, clock)).toEqual({ text: "12x", align: "start", style: WARNING });
  const one = (column: Parameters<typeof columnFormatter>[0], v: TypedValue) => columnFormatter(column).format(v, clock).text;
  expect(one({ unit: "By" }, 1536n)).toBe("1.5 KiB");
  expect(one({ unit: "widgets" }, 12)).toBe("12 widgets");
  expect(one({ options: { plain: true } }, null)).toBe("-");
  expect(one({ options: { decimals: 2 } }, 7)).toBe("7.00");
  expect(() => one({ options: { decimals: -1 } }, 7)).toThrow();
  expect(fewestDecimals([1, 2.5, 0.125])).toBe(3);
});

test("a time-role value moves on with the injected clock alone", () => {
  const seen = columnFormatter({ role: "time" });
  const value = new Date(NOW);
  const at = (ms: number) => seen.format(value, { now: NOW + ms, zone: fixedZone(0) }).text;
  expect(at(0)).toBe("just now");
  expect(at(59_000)).toBe("just now");
  expect(at(60_000)).toBe("1m ago");
  const absolute = columnFormatter({ role: "time", options: { time: "absolute" } });
  expect(absolute.format(value, { now: NOW + 60_000, zone: fixedZone(3600) }).text).toBe("2026-10-29 14:00");
});

test("each date takes its own offset from the zone, and the runtime's local zone by default", () => {
  const summerStart = Date.parse("2026-03-29T01:00:00Z");
  const summerEnd = Date.parse("2026-10-25T01:00:00Z");
  const london = (at: number) => (at >= summerStart && at < summerEnd ? 3600 : 0);
  const dates = columnFormatter();
  const shown = (iso: string) => dates.format(new Date(iso), { now: NOW, zone: london }).text;
  expect(shown("2026-07-01T12:00:00Z")).toBe("2026-07-01 13:00");
  expect(shown("2026-10-29T12:00:00Z")).toBe("2026-10-29 12:00");
  const july = new Date("2026-07-01T12:00:00Z");
  const local = (at: number) => -new Date(at).getTimezoneOffset() * 60;
  expect(dates.format(july, { now: NOW }).text).toBe(dates.format(july, { now: NOW, zone: local }).text);
});

test("stable text has every date absolute and is otherwise the formatted text", () => {
  const clock = { now: NOW + 3 * 60_000, zone: fixedZone(3600) };
  const seen = columnFormatter({ role: "time" });
  expect(seen.relative).toBe(true);
  expect(seen.format(new Date(NOW), clock).text).toBe("3m ago");
  expect(seen.stable(new Date(NOW), clock)).toEqual({ text: "2026-10-29 14:00", align: "end", style: 0 });
  expect(seen.stable(null, clock)).toEqual(seen.format(null, clock));
  expect(columnFormatter().relative).toBe(false);
  expect(columnFormatter({ options: { time: "relative" } }).relative).toBe(true);
  const renderer = valueRenderer<TypedValue>({ role: "time", clock: () => clock });
  expect(renderer.text!(new Date(NOW))).toBe("2026-10-29 14:00");
});

const typedValues: gs.Generator<TypedValue> = gs.oneOf<TypedValue>(
  gs.just(null),
  gs.booleans(),
  gs.floats(),
  gs.bigIntegers({ minValue: -(2n ** 70n), maxValue: 2n ** 70n }),
  gs.text(),
  gs.arrays(gs.sampledFrom(["a", "\n", "\r", "\t", "\u2028", "é"]), { maxSize: 8 }).map((cs) => cs.join("")),
  gs.binary({ maxSize: 40 }),
  gs.integers({ minValue: -8.64e15, maxValue: 8.64e15 }).map((ms) => new Date(ms)),
  gs.tuples(gs.text(), gs.optional(gs.text())).map(([category, label]) => (label == null ? { category } : { category, label })),
  gs.text().map((s) => ({ nested: { s, n: [1, s] } })),
  gs.text().map((invalid) => ({ invalid, expected: "int" as const })),
);

test("any Typed Value formats to one line, whatever the column", () =>
  hegel.test((tc) => {
    const value = tc.draw(typedValues);
    const unit = tc.draw(gs.sampledFrom([undefined, "By", "s", "ms", "%", "1", "\nodd"]));
    const role = tc.draw(gs.sampledFrom([undefined, "time", "other"]));
    const format = columnFormatter({ ...(unit ? { unit } : {}), ...(role ? { role } : {}) });
    const now = tc.draw(gs.floats({ allowNan: false, allowInfinity: false }));
    const clock = { now, zone: fixedZone(tc.draw(gs.integers({ minValue: -86_400, maxValue: 86_400 }))) };
    for (const f of [format.format(value, clock), format.stable(value, clock)]) {
      expect(f.text).not.toMatch(/[\n\r\u2028\u2029]/);
    }
  }));

describe("valueRenderer through the painter", () => {
  const COLUMNS: { value: TypedValue; role?: string }[] = [
    { value: null },
    { value: { invalid: "12x", expected: "int" } },
    { value: "Bergen" },
    { value: new Date(NOW), role: "time" },
  ];

  function paint(columns: typeof COLUMNS) {
    const calls: DrawCall[] = [];
    fakeDocument(calls);
    const h = new Harness<number, number, TypedValue>({
      rows: { identity: "position", count: () => 1, window: (s, e) => [0].slice(s, e) },
      cols: { identity: "position", count: () => columns.length, window: (s, e) => columns.map((_, i) => i).slice(s, e) },
      cells: (rs, cs) => rs.map(() => cs.map((c) => columns[c]!.value)),
    });
    h.frame();
    const s = h.frame();
    const painter = createCanvasPainter(light);
    painter.resize(h.width, h.height, 1);
    const clock = () => ({ now: NOW, zone: fixedZone(0) });
    const renderers = columns.map((c) => valueRenderer<TypedValue>({ ...(c.role ? { role: c.role } : {}), clock }));
    painter.renderer = (_row, col) => renderers[col as number]!;
    calls.length = 0;
    const again = painter.paint(s, h.feed.content(s));
    const texts = calls.filter((c) => c.name === "fillText").map((c) => ({ text: c.args[0], fill: c.fill }));
    return { calls, texts, again, due: painter.due };
  }

  test("a null draws dim, an invalid value in the warning colour with a mark, text in the body colour", () => {
    const { calls, texts } = paint(COLUMNS.slice(0, 3));
    expect(texts).toContainEqual({ text: "∅", fill: light.dim });
    expect(texts).toContainEqual({ text: "12x", fill: light.warning });
    expect(texts).toContainEqual({ text: "Bergen", fill: light.body.color });
    expect(calls.filter((c) => c.name === "fill" && c.fill === light.warning)).toHaveLength(1);
  });

  test("a relative time asks for a frame a second on, not every frame", () => {
    const shown = paint(COLUMNS);
    expect(shown.texts).toContainEqual({ text: "just now", fill: light.body.color });
    expect(shown.again).toBe(false);
    expect(shown.due).toBeGreaterThan(performance.now());
    expect(shown.due).toBeLessThanOrEqual(performance.now() + 1000);
    // A time-role column showing no date has nothing to move on.
    expect(paint([...COLUMNS.slice(0, 3), { value: null, role: "time" }]).due).toBe(Infinity);
  });
});
