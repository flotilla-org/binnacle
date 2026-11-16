// A null group reads (null), dim as a null cell is; an empty one reads
// (empty), in the ordinary colour. The same two labels name the values in a
// Distinct Values list and a chip.

import { beforeAll, expect, test } from "vitest";
import { light } from "../src";
import { createCanvasPainter } from "../src/canvas/painter";
import { listedText } from "../src/filterbar";
import { EMPTY_LABEL, NULL_LABEL, valueLabel } from "../src/labels";
import { fromRows } from "../src/rows";
import { fakeDocument, type DrawCall } from "./canvas";
import { AXIS_CODE } from "../src/codes";
import { HEADER, Harness } from "./harness";
import { loadForTests } from "./wasm";

const calls: DrawCall[] = [];

beforeAll(async () => {
  await loadForTests();
  fakeDocument(calls);
});

interface Rec {
  s: string | null;
  n: number;
}
/** A row Header Band wide enough for the labels. */
const WIDE = 160;
const COLUMNS = [{ key: "s" }, { key: "n" }] as const;

test("a null and an empty string read apart where a value is named", () => {
  expect([NULL_LABEL, EMPTY_LABEL]).toEqual(["(null)", "(empty)"]);
  expect([null, "", "a"].map(valueLabel)).toEqual(["(null)", "(empty)", "a"]);
  expect(listedText([null, "", "Bergen"])).toBe("(null), (empty), Bergen");
});

test("a null group and an empty one side by side: (null) dim, (empty) not", () => {
  const rows: Rec[] = [
    { s: null, n: 1 },
    { s: "", n: 2 },
    { s: "", n: 3 },
    { s: "a", n: 4 },
  ];
  const source = fromRows(rows, COLUMNS);
  const h = new Harness(source);
  try {
    h.frame();
    h.grid.set_header(AXIS_CODE.cols, WIDE);
    h.grid.group_by(0, h.feed.colKey("s"));
    h.drain();
    const s = h.frame();
    const painter = createCanvasPainter(light);
    painter.resize(h.width, h.height, 1);
    calls.length = 0;
    painter.paint(s, h.feed.content(s));
    const band = calls
      .filter((c) => c.name === "fillText" && (c.args[1] as number) < WIDE && (c.args[2] as number) > HEADER)
      .map((c) => ({ text: c.args[0] as string, fill: c.fill }));
    const at = (text: string) => band.find((b) => b.text.startsWith(text));
    expect(band.map((b) => b.text)).toEqual(expect.arrayContaining(["(null) (1)", "(empty) (2)", "a (1)"]));
    expect(at("(null)")?.fill).toBe(light.dim);
    expect(at("(empty)")?.fill).not.toBe(light.dim);
    expect(at("a (")?.fill).not.toBe(light.dim);
  } finally {
    h.destroy();
  }
});
