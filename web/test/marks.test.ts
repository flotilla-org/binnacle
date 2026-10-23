// When a toggled group's disclosure mark draws pending: only once its
// toggle has waited 150 ms unanswered, so a fast source never flickers.

import { expect, test } from "vitest";
import { createPendingMarks, MARK_DELAY } from "../src/canvas/pending";
import type { Snapshot, StripItem } from "../src/snapshot";

/** A Snapshot whose rows show Group Items keyed `keys`, those in `pending` with a toggle unanswered. */
const frame = (keys: bigint[], pending: bigint[]): Snapshot => {
  const strip: StripItem[] = keys.map((key, position) => ({
    position,
    key,
    offset: position * 24,
    size: 24,
    outline: { kind: "group", depth: 0, open: "open", ancestors: [], group: null },
    togglePending: pending.includes(key),
  }));
  return { rows: { strip, spans: [] } } as unknown as Snapshot;
};

test("a mark draws pending only once its toggle has waited 150 ms, and not once answered", () => {
  const marks = createPendingMarks();
  marks.seen(frame([1n, 2n], []), 0);
  expect(marks.waiting).toBe(false);
  marks.seen(frame([1n, 2n], [2n]), 10);
  expect(marks.waiting).toBe(true);
  expect(marks.pending(2n, 10 + MARK_DELAY - 1)).toBe(false);
  marks.seen(frame([1n, 2n], [2n]), 10 + MARK_DELAY);
  expect(marks.pending(2n, 10 + MARK_DELAY)).toBe(true);
  expect(marks.pending(1n, 10 + MARK_DELAY)).toBe(false);
  // Answered: no longer pending, and nothing to wait for.
  marks.seen(frame([1n, 2n], []), 400);
  expect(marks.pending(2n, 400)).toBe(false);
  expect(marks.waiting).toBe(false);
});

test("a toggle answered and made again waits its own 150 ms", () => {
  const marks = createPendingMarks();
  marks.seen(frame([1n], [1n]), 0);
  marks.seen(frame([1n], []), 100);
  marks.seen(frame([1n], [1n]), 200);
  expect(marks.pending(1n, 200 + MARK_DELAY - 1)).toBe(false);
  expect(marks.pending(1n, 200 + MARK_DELAY)).toBe(true);
  expect(marks.pending(null, 1000)).toBe(false);
});
