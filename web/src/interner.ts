// Key Interners: the widget's map from the user's own IDs to the Grid's
// Keys and back. Primitive IDs are their own canonical form; composite ones
// go through the axis's `idKey`. Keys are issued in order of first sight and
// never reused, so two IDs share a Key only if their canonical forms are
// equal.

export type Primitive = string | number | bigint;

export interface KeyInterner<Id> {
  /** The Key for `id`, issuing the next one if it is new. */
  key(id: Id): bigint;
  /** The ID a Key was issued for, if the interner still knows it. */
  id(key: bigint): Id | undefined;
  /**
   * Starts a new generation after a Reset. IDs seen in the generation before
   * keep their Keys when they come back (focus and Size Overrides follow
   * them); IDs not seen for two rebuilds are forgotten, so memory follows
   * what the Table Source still shows rather than everything it ever did,
   * except the IDs of the Keys in `keep` (pinned items and sized ones, say),
   * which last while they are kept.
   */
  rebuild(keep?: Iterable<bigint>): void;
}

interface Generation<Id> {
  keys: Map<Primitive, bigint>;
  ids: Map<bigint, Id>;
}

const generation = <Id>(): Generation<Id> => ({ keys: new Map(), ids: new Map() });

export function createInterner<Id>(idKey?: (id: Id) => Primitive): KeyInterner<Id> {
  const canonical = idKey ?? ((id: Id) => id as Primitive);
  let current = generation<Id>();
  let previous = generation<Id>();
  let next = 0n;
  return {
    key(id) {
      const c = canonical(id);
      let key = current.keys.get(c);
      if (key !== undefined) return key;
      key = previous.keys.get(c) ?? next++;
      current.keys.set(c, key);
      current.ids.set(key, id);
      return key;
    },
    id: (key) => current.ids.get(key) ?? previous.ids.get(key),
    rebuild(keep = []) {
      const kept = generation<Id>();
      for (const key of keep) {
        const id = current.ids.get(key) ?? previous.ids.get(key);
        if (id === undefined) continue;
        kept.keys.set(canonical(id), key);
        kept.ids.set(key, id);
      }
      previous = current;
      current = kept;
    },
  };
}

/** The interner for `identity: "position"`: the Key is the Position. */
export const positionInterner: KeyInterner<number> = {
  key: (position) => BigInt(position),
  id: (key) => Number(key),
  rebuild() {},
};
