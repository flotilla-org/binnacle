// The checklist's rules (#107): each tick writes the condition that is
// exact, and the ticks read back from the conditions are the ones the user
// made. The property mirrors the Rust port's
// (crates/binnacle-ratatui/tests/checklist.rs), so the two can't drift.

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

/** Whether `condition` keeps an item whose value is `value`: Text as a contains ignoring case, which never keeps a blank; OneOf exactly. */
function modelKeeps(condition: Condition<string>, value: ListedValue): boolean {
  const kind = condition.kind;
  if (kind.type === "Compare" || kind.type === "Native") throw new Error("only Text and OneOf conditions are drawn");
  const matched =
    kind.type === "OneOf"
      ? kind.values.includes(value)
      : value !== null && value.toLowerCase().includes(kind.query.text.toLowerCase());
  return matched !== condition.not;
}

const whole = (values: ListedValue[]): Shown => ({ values, partial: false, search: "" });

const contains = (text: string, not = false): Condition<string> => ({
  id: ID,
  kind: { type: "Text", query: { text, caseSensitive: false, regex: false } },
  not,
});
const oneOf = (values: ListedValue[], not = false): Condition<string> => ({ id: ID, kind: { type: "OneOf", values }, not });

describe("the checklist", () => {
  test("whatever the user ticks, unticks, searches and selects all matching, over a whole or a capped list (which leaves a value out), the conditions keep exactly the values left ticked", () =>
    hegel.test(
      (tc) => {
        const candidates: ListedValue[] = [null, "", "Bergen", "bergen", "Oslo", "Cádiz", "7"];
        const universe = candidates.filter(() => tc.draw(gs.booleans()));
        if (universe.length === 0) universe.push(tc.draw(gs.sampledFrom(candidates)));
        const partial = tc.draw(gs.booleans());
        // A capped list leaves out a value the rows hold: the last text.
        const unlisted = partial ? universe.filter((v) => v !== null).at(-1) : undefined;
        const has = (text: string, search: string) => text.toLowerCase().includes(search.toLowerCase());
        let conditions: Condition<string>[] = [OTHER];
        // The model: the values ticked, and the search "select all matching" kept.
        let model = new Set(universe);
        let text: string | null = null;
        let search = "";
        const excluded = (v: ListedValue) => text !== null && (v === null || !has(v, text));
        const steps = tc.draw(gs.integers({ minValue: 1, maxValue: 20 }));
        for (let i = 0; i < steps; i++) {
          if (tc.draw(gs.integers({ minValue: 0, maxValue: 3 })) === 0) {
            search = tc.draw(gs.sampledFrom(["", "berg", "o", "e", "7", "z"]));
            tc.note(`search ${JSON.stringify(search)}`);
          }
          // The values listed, the blank first, as the menu shows them.
          const values = [
            ...(search === "" && universe.includes(null) ? [null] : []),
            ...universe.filter((v) => v !== null && v !== unlisted && (search === "" || has(v, search))),
          ];
          const shown: Shown = { values, partial, search };
          const column = columnConditions(conditions, ID);
          let changed = true;
          if (tc.draw(gs.integers({ minValue: 0, maxValue: 4 })) === 0) {
            const on = !headTicked(column, shown);
            tc.note(`head ${on}`);
            conditions = after(conditions, tickHead(column, ID, on, shown));
            if (on && search !== "") text = search;
            if (on) model = new Set(universe.filter((v) => !excluded(v)));
            else if (search === "") model.clear();
            else for (const v of values) model.delete(v);
          } else if (values.length > 0) {
            const v = tc.draw(gs.sampledFrom(values));
            const on = !ticked(column, v);
            tc.note(`${on ? "tick" : "untick"} ${JSON.stringify(v)}`);
            conditions = after(conditions, tick(column, ID, v, on, shown));
            if (model.has(v)) model.delete(v);
            else if (excluded(v)) changed = false;
            else model.add(v);
          } else {
            continue;
          }
          const now = columnConditions(conditions, ID);
          expect(conditions).toContainEqual(OTHER);
          expect(universe.filter((v) => now.all.every((c) => modelKeeps(c, v)))).toEqual(universe.filter((v) => model.has(v)));
          const oneOfs = now.all.filter((c) => c.kind.type === "OneOf");
          expect(oneOfs.length).toBeLessThanOrEqual(1);
          expect(oneOfs.some((c) => c.not && c.kind.type === "OneOf" && c.kind.values.length === 0)).toBe(false);
          if (search === "") {
            const open = values.filter((v) => !excluded(v));
            const all = open.length > 0 && open.every((v) => model.has(v));
            expect(headTicked(now, shown)).toBe(all);
            if (all && !partial && changed) expect(oneOfs).toEqual([]);
          }
        }
      },
      { testCases: 1000 },
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
