// Key Interners: the user's own IDs to Keys and back, with tuple IDs matched
// by value through `idKey`.

import * as hegel from "@hegeldev/hegel";
import * as gs from "@hegeldev/hegel/generators";
import { expect, test } from "vitest";
import { createInterner } from "../src/interner";

type Pair = [string, number];
const pairKey = ([a, b]: Pair) => `${a.length}:${a}${b}`;

const strings = gs.text({ maxSize: 4 });
const pairs = gs.tuples(gs.text({ maxSize: 3 }), gs.integers({ minValue: -3, maxValue: 3 }));

test("a Key hands back the ID it was issued for, and equal IDs share a Key", () =>
  hegel.test((tc) => {
    const ids = tc.draw(gs.arrays(strings, { maxSize: 30 }));
    const interner = createInterner<string>();
    const keys = ids.map((id) => interner.key(id));
    ids.forEach((id, i) => {
      expect(interner.id(keys[i]!)).toBe(id);
      ids.forEach((other, j) => expect(keys[i] === keys[j]).toBe(id === other));
    });
  }));

test("strings, numbers and bigints are different IDs even when they print alike", () =>
  hegel.test((tc) => {
    const small = gs.integers({ minValue: -3, maxValue: 3 });
    const ids = tc.draw(
      gs.arrays(gs.oneOf<string | number | bigint>(small.map(String), small, small.map(BigInt)), { maxSize: 30 }),
    );
    const interner = createInterner<string | number | bigint>();
    const keys = ids.map((id) => interner.key(id));
    ids.forEach((id, i) => {
      expect(interner.id(keys[i]!)).toBe(id);
      ids.forEach((other, j) => expect(keys[i] === keys[j]).toBe(id === other));
    });
  }));

test("tuple IDs are matched by value through idKey, not by reference", () =>
  hegel.test((tc) => {
    const ids = tc.draw(gs.arrays(pairs, { maxSize: 30 }));
    const interner = createInterner<Pair>(pairKey);
    // Fresh copies: no two calls share an array.
    const keys = ids.map((id) => interner.key([id[0], id[1]]));
    ids.forEach((id, i) => {
      expect(interner.id(keys[i]!)).toEqual(id);
      ids.forEach((other, j) => expect(keys[i] === keys[j]).toBe(id[0] === other[0] && id[1] === other[1]));
    });
  }));

test("after a rebuild, an ID seen since keeps its Key and no Key is reused for another ID", () =>
  hegel.test((tc) => {
    const interner = createInterner<string>();
    const issued = new Map<bigint, string>();
    const before = new Map<string, bigint>();
    for (const id of tc.draw(gs.arrays(strings, { maxSize: 20 }))) {
      before.set(id, interner.key(id));
      issued.set(interner.key(id), id);
    }
    const resets = tc.draw(gs.integers({ minValue: 1, maxValue: 3 }));
    for (let r = 0; r < resets; r++) {
      interner.rebuild();
      for (const id of tc.draw(gs.arrays(strings, { maxSize: 20 }))) {
        const key = interner.key(id);
        if (r === 0 && before.has(id)) expect(key).toBe(before.get(id));
        const was = issued.get(key);
        if (was !== undefined) expect(was).toBe(id);
        issued.set(key, id);
        expect(interner.id(key)).toBe(id);
      }
    }
  }));

test("a rebuild forgets IDs not seen since the rebuild before it", () => {
  const interner = createInterner<string>();
  const a = interner.key("a");
  interner.rebuild();
  expect(interner.id(a)).toBe("a");
  interner.rebuild();
  expect(interner.id(a)).toBeUndefined();
  expect(interner.key("a")).not.toBe(a);
});

test("Keys a rebuild is told to keep keep their IDs through any number of rebuilds; the rest go after two", () =>
  hegel.test((tc) => {
    const interner = createInterner<string>();
    const ids = [...new Set(tc.draw(gs.arrays(strings, { minSize: 1, maxSize: 10 })))];
    const keys = new Map(ids.map((id) => [id, interner.key(id)]));
    const kept = new Set(tc.draw(gs.arrays(gs.sampledFrom(ids), { maxSize: ids.length })));
    const rebuilds = tc.draw(gs.integers({ minValue: 2, maxValue: 5 }));
    for (let r = 0; r < rebuilds; r++) interner.rebuild([...kept].map((id) => keys.get(id)!));
    for (const id of ids) {
      const key = keys.get(id)!;
      expect(interner.id(key)).toBe(kept.has(id) ? id : undefined);
      if (kept.has(id)) expect(interner.key(id)).toBe(key);
    }
  }));
