// Seam 2: the wasm boundary. The decoder and the Effect union are checked
// against Rust's own view of the same frame, never against constants.

import { beforeAll, describe, expect, test } from "vitest";
import { NOTICE_CODE } from "../src/codes";
import { decodeSnapshot } from "../src/snapshot";
import { isEffect } from "../src/effects";
import { bridge } from "../src/wasm";
import { loadForTests } from "./wasm";

beforeAll(loadForTests);

interface DebugFrame {
  numbers: number[];
  keys: bigint[];
  dump: Record<string, unknown>;
}

describe("the decoded Snapshot equals Rust's dump of the same frame", () => {
  const scenarios = [
    "plain",
    "busy",
    "resizing",
    "band",
    "moving",
    "editing",
    "locating",
    "scrolled",
    "dragging",
    "sorted",
  ];

  test("the scenario list matches Rust's", () => {
    expect(bridge().debug_scenarios()).toEqual(scenarios);
  });

  test.each(scenarios)("%s", (name) => {
    const grid = bridge().WebGrid.debug_scenario(name);
    try {
      const frame = grid.debug_frame() as DebugFrame;
      const decoded = decodeSnapshot(new Float64Array(frame.numbers), new BigUint64Array(frame.keys));
      expect({ ...decoded, cells: Array.from(decoded.cells) }).toEqual(frame.dump);
    } finally {
      grid.free();
    }
  });
});

describe("Effects", () => {
  test("every Effect variant passes the union's guard", () => {
    const effects = bridge().debug_every_effect() as unknown[];
    const types = effects.map((e) => (e as { type: string }).type);
    expect(new Set(types).size).toBe(effects.length);
    for (const effect of effects) expect(isEffect(effect), JSON.stringify(effect, (_, v) => (typeof v === "bigint" ? `${v}n` : v))).toBe(true);
  });

  test("the guard rejects what is not an Effect", () => {
    expect(isEffect({ type: "Nope" })).toBe(false);
    expect(isEffect({ type: "FocusChanged", generation: 1, row: 1, col: 2n })).toBe(false);
    expect(isEffect({ type: "Locate", generation: 1, axis: "diagonal", key: 1n })).toBe(false);
    expect(isEffect(null)).toBe(false);
    const edit = { type: "CycleSort", axis: "rows", key: 1n, multi: false };
    const view = (spec: unknown, e: unknown = edit) => ({ type: "ViewRequested", generation: 1, spec, edit: e });
    const unsorted = { sort: [] };
    expect(isEffect(view({ rows: unsorted, cols: unsorted }))).toBe(true);
    expect(isEffect(view({ rows: unsorted }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: { sort: [{ key: 1, direction: "ascending" }] } }))).toBe(false);
    expect(isEffect(view({ rows: unsorted, cols: unsorted }, { ...edit, multi: 1 }))).toBe(false);
  });
});

describe("Change Notices", () => {
  test("each notice code names the same notice in Rust", () => {
    for (const [type, code] of Object.entries(NOTICE_CODE)) {
      expect(bridge().debug_notice(code, 1, 2).split(/[ ({]/)[0]).toBe(type);
    }
  });
});
