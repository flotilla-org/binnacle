// Seam 1: click counting. Browsers do not agree on `detail` for pointer
// events (Chrome reports 0), so the widget counts presses itself.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { expect, test } from "vitest";
import { createClickCounter, type ClickCounter, type Press } from "../src/clicks";

const button = gs.integers({ minValue: 0, maxValue: 4 });
const coordinate = gs.floats({ minValue: -1e4, maxValue: 1e4 });
const start = gs.floats({ minValue: 0, maxValue: 1e9 });

// Margins keep the properties' boundaries clear of rounding in `time + dt`
// and `x + dx`; the exact bounds have their own example test.
const EPSILON = 0.01;

interface Setup {
  counter: ClickCounter;
  interval: number;
  slop: number;
  /** A first press anywhere. */
  first(): Press;
  /** A press within the interval and the slop of `p`, with the same button. */
  near(p: Press): Press;
}

/** A counter with any interval and slop, and presses to give it. */
function setup(tc: hegel.TestCase): Setup {
  const interval = tc.draw(gs.integers({ minValue: 1, maxValue: 2000 }));
  const slop = tc.draw(gs.integers({ minValue: 0, maxValue: 20 }));
  const d = Math.max(0, slop - EPSILON);
  return {
    counter: createClickCounter({ interval, slop }),
    interval,
    slop,
    first: () => ({ time: tc.draw(start), x: tc.draw(coordinate), y: tc.draw(coordinate), button: tc.draw(button) }),
    near: (p) => ({
      time: p.time + tc.draw(gs.floats({ minValue: 0, maxValue: Math.max(0, interval - EPSILON) })),
      x: p.x + tc.draw(gs.floats({ minValue: -d, maxValue: d })),
      y: p.y + tc.draw(gs.floats({ minValue: -d, maxValue: d })),
      button: p.button,
    }),
  };
}

test("presses each close to the one before count up from one", () =>
  hegel.test((tc) => {
    const { counter, first, near } = setup(tc);
    const n = tc.draw(gs.integers({ minValue: 1, maxValue: 8 }));
    let p = first();
    for (let i = 1; i <= n; i++) {
      expect(counter.press(p)).toBe(i);
      p = near(p);
    }
  }));

test("a press after the interval starts again at one", () =>
  hegel.test((tc) => {
    const { counter, interval, first, near } = setup(tc);
    let p = first();
    const before = tc.draw(gs.integers({ minValue: 1, maxValue: 5 }));
    for (let i = 0; i < before; i++) counter.press((p = near(p)));
    const late = { ...p, time: p.time + interval + EPSILON + tc.draw(gs.floats({ minValue: 0, maxValue: 1e6 })) };
    expect(counter.press(late)).toBe(1);
  }));

test("a press that moved past the slop starts again at one", () =>
  hegel.test((tc) => {
    const { counter, slop, first, near } = setup(tc);
    const p = first();
    counter.press(p);
    const far = gs.oneOf(
      gs.floats({ minValue: slop + EPSILON, maxValue: 1e4 }),
      gs.floats({ minValue: -1e4, maxValue: -slop - EPSILON }),
    );
    const [dx, dy] = tc.draw(gs.oneOf(gs.tuples(far, gs.just(0)), gs.tuples(gs.just(0), far)));
    expect(counter.press({ ...near(p), x: p.x + dx, y: p.y + dy })).toBe(1);
  }));

test("a press with another button starts again at one", () =>
  hegel.test((tc) => {
    const { counter, first, near } = setup(tc);
    const p = first();
    counter.press(p);
    const other = tc.draw(button);
    tc.assume(other !== p.button);
    expect(counter.press({ ...near(p), button: other })).toBe(1);
  }));

test("a clock that went backwards starts again at one", () =>
  hegel.test((tc) => {
    const { counter, first } = setup(tc);
    const p = first();
    counter.press(p);
    const earlier = p.time - tc.draw(gs.floats({ minValue: EPSILON, maxValue: 1e6 }));
    expect(counter.press({ ...p, time: earlier })).toBe(1);
  }));

test("the interval is an exclusive bound and the slop an inclusive one", () => {
  const counter = createClickCounter({ interval: 500, slop: 4 });
  expect(counter.press({ time: 1000, x: 10, y: 10, button: 0 })).toBe(1);
  expect(counter.press({ time: 1499, x: 14, y: 6, button: 0 })).toBe(2);
  expect(counter.press({ time: 1999, x: 14, y: 6, button: 0 })).toBe(1);
});
