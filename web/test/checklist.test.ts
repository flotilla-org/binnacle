// The checklist's rules (#107): each tick writes the condition that is
// exact, and the ticks read back from the conditions are the ones the user
// made.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { describe, expect, test } from "vitest";
import {
  columnConditions,
  excluded,
  headTicked,
  replaced,
  tick,
  tickHead,
  ticked,
  type Shown,
  type Written,
} from "../src/checklist";
import type { Condition, ListedValue } from "../src/source";

const ID = "port";
const OTHER: Condition<string> = { id: "name", kind: { type: "OneOf", values: ["x"] }, not: false };

/** The conditions after writing `written` over the column's, the other column's untouched. */
function after(conditions: Condition<string>[], written: Written<string>): Condition<string>[] {
  const column = columnConditions(conditions, ID);
  return [...conditions.filter((c) => c.id !== ID), ...replaced(column, written)];
}

const whole = (values: ListedValue[]): Shown => ({ values, partial: false, search: "" });

const contains = (text: string, not = false): Condition<string> => ({
  id: ID,
  kind: { type: "Text", query: { text, caseSensitive: false, regex: false } },
  not,
});
const oneOf = (values: ListedValue[], not = false): Condition<string> => ({ id: ID, kind: { type: "OneOf", values }, not });

describe("the checklist", () => {
  test("whatever ticks and unticks a user makes, the ticks read back are theirs, and all ticked of a whole list leaves no OneOf", () =>
    hegel.test(
      (tc) => {
        const values: ListedValue[] = tc.draw(
          gs.arrays(gs.oneOf(gs.just(null), gs.sampledFrom(["", "Bergen", "bergen", "Oslo", "Cádiz", "7"])), {
            minSize: 1,
            maxSize: 6,
            unique: true,
          }),
        );
        const shown: Shown = { values, partial: tc.draw(gs.booleans()), search: "" };
        let conditions: Condition<string>[] = [OTHER];
        const model = new Set(values);
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 10 }));
        for (let i = 0; i < steps; i++) {
          const column = columnConditions(conditions, ID);
          if (tc.draw(gs.integers({ minValue: 0, maxValue: 4 })) === 0) {
            const on = !headTicked(column, shown);
            tc.note(`head ${on}`);
            conditions = after(conditions, tickHead(column, ID, on, shown));
            for (const v of values) on ? model.add(v) : model.delete(v);
          } else {
            const v = tc.draw(gs.sampledFrom(values));
            const on = !model.has(v);
            tc.note(`${on ? "tick" : "untick"} ${JSON.stringify(v)}`);
            conditions = after(conditions, tick(column, ID, v, on, shown));
            on ? model.add(v) : model.delete(v);
          }
          const now = columnConditions(conditions, ID);
          expect(values.filter((v) => ticked(now, v))).toEqual(values.filter((v) => model.has(v)));
          expect(headTicked(now, shown)).toBe(model.size === values.length);
          if (model.size === values.length && !shown.partial) expect(now.oneOf).toBeNull();
          expect(now.all.filter((c) => c.kind.type === "OneOf")).toHaveLength(now.oneOf ? 1 : 0);
          expect(now.oneOf?.not && now.oneOf.kind.type === "OneOf" && now.oneOf.kind.values.length === 0).toBeFalsy();
          expect(conditions).toContainEqual(OTHER);
        }
      },
      { testCases: 300 },
    ));

  test("unticking from all excludes just those values, so values that arrive later stay included", () => {
    const shown = whole(["Bergen", "Oslo", null]);
    let conditions: Condition<string>[] = [];
    conditions = after(conditions, tick(columnConditions(conditions, ID), ID, "Bergen", false, shown));
    conditions = after(conditions, tick(columnConditions(conditions, ID), ID, null, false, shown));
    expect(conditions).toEqual([oneOf(["Bergen", null], true)]);
    expect(ticked(columnConditions(conditions, ID), "Tromsø")).toBe(true);
  });

  test("ticking from none includes just those values, so values that arrive later stay excluded", () => {
    const shown = whole(["Bergen", "Oslo", null]);
    let conditions: Condition<string>[] = [];
    conditions = after(conditions, tickHead(columnConditions(conditions, ID), ID, false, shown));
    expect(conditions).toEqual([oneOf([])]);
    conditions = after(conditions, tick(columnConditions(conditions, ID), ID, "Oslo", true, shown));
    expect(conditions).toEqual([oneOf(["Oslo"])]);
    expect(ticked(columnConditions(conditions, ID), "Tromsø")).toBe(false);
  });

  test("ticking every value again removes the condition, but not while the list is capped or searched", () => {
    const values = ["Bergen", "Oslo"];
    const listed = [oneOf(["Bergen"])];
    expect(after(listed, tick(columnConditions(listed, ID), ID, "Oslo", true, whole(values)))).toEqual([]);
    const capped = { values, partial: true, search: "" };
    expect(after(listed, tick(columnConditions(listed, ID), ID, "Oslo", true, capped))).toEqual([
      oneOf(["Bergen", "Oslo"]),
    ]);
    const unlisted = [oneOf(["Oslo"], true)];
    expect(after(unlisted, tick(columnConditions(unlisted, ID), ID, "Oslo", true, capped))).toEqual([]);
  });

  test("select all matching writes a Text condition on the search, and later unticks AND a not OneOf onto it", () => {
    const searched: Shown = { values: ["Bergen", "Bergenhus"], partial: true, search: "berg" };
    let conditions: Condition<string>[] = [oneOf(["Oslo"])];
    expect(headTicked(columnConditions(conditions, ID), searched)).toBe(false);
    conditions = after(conditions, tickHead(columnConditions(conditions, ID), ID, true, searched));
    expect(conditions).toEqual([contains("berg")]);
    expect(headTicked(columnConditions(conditions, ID), searched)).toBe(true);
    conditions = after(conditions, tick(columnConditions(conditions, ID), ID, "Bergenhus", false, searched));
    expect(conditions).toEqual([contains("berg"), oneOf(["Bergenhus"], true)]);
    // Reopened without the search: the values the search rejects are unticked, and can't be ticked.
    const column = columnConditions(conditions, ID);
    const ticks = ["Bergen", "Bergenhus", "Oslo", null].map((v) => [ticked(column, v), excluded(column, v)]);
    expect(ticks).toEqual([
      [true, false],
      [false, false],
      [false, true],
      [false, true],
    ]);
    expect(tick(column, ID, "Oslo", true, whole(["Oslo"]))).toEqual({ text: column.text, oneOf: column.oneOf });
  });

  test("unticking select all matching unticks each value shown", () => {
    const searched: Shown = { values: ["Bergen", null], partial: false, search: "berg" };
    const conditions = [contains("berg")];
    const written = tickHead(columnConditions(conditions, ID), ID, false, searched);
    expect(after(conditions, written)).toEqual([contains("berg"), oneOf(["Bergen"], true)]);
  });

  test("a Text condition with an invalid pattern, which no source applies, excludes no value", () => {
    const query = { text: "(", caseSensitive: false, regex: true };
    for (const not of [false, true]) {
      const column = columnConditions([{ id: ID, kind: { type: "Text", query }, not }], ID);
      expect(["Bergen", null].map((v) => excluded(column, v))).toEqual([false, false]);
    }
  });

  test("a blank is its own value, apart from empty text", () => {
    const column = columnConditions([oneOf([null])], ID);
    expect([ticked(column, null), ticked(column, "")]).toEqual([true, false]);
  });
});
