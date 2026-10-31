// The described demo's watch loop: what stops it, and what it tries again.

import { afterEach, expect, test, vi } from "vitest";
import type { DescribedTable } from "../src";
import { watch, type WatchError, type WatchPhase } from "../demo/sources/described-watch";

/** A table that asks for one watch and does what `open` and `feed` say. */
function table(parts: { open?(type: string): void; feed?(bytes: Uint8Array): void }) {
  const calls = { opened: 0, fed: 0, ended: 0 };
  const stub = {
    pollRequest: () => ({ table: "t", target: "/tables/t/rows" }),
    open(type: string) {
      calls.opened++;
      parts.open?.(type);
    },
    feed(bytes: Uint8Array) {
      calls.fed++;
      parts.feed?.(bytes);
    },
    ended() {
      calls.ended++;
    },
    status: () => ({ synced: false }),
  };
  return { table: stub as unknown as DescribedTable, calls };
}

/** Runs the loop until it reports `phase`; the phases it went through, and the error. */
async function runUntil(t: DescribedTable, phase: WatchPhase) {
  const phases: WatchPhase[] = [];
  let error: WatchError | undefined;
  let done!: () => void;
  const reached = new Promise<void>((resolve) => (done = resolve));
  const w = watch(t, {
    headers: {},
    onPhase(next, e) {
      phases.push(next);
      error = e;
      if (next === phase) done();
    },
  });
  await reached;
  // Give a loop that wrongly carried on the time to show it.
  await new Promise((resolve) => setTimeout(resolve, 20));
  w.destroy();
  return { phases, error };
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

test("a 200 that is not a rows stream stops the loop with the reason, and is not tried again", async () => {
  const fetch = vi.fn(async () => new Response("<html>", { headers: { "Content-Type": "text/html" } }));
  vi.stubGlobal("fetch", fetch);
  const { table: t, calls } = table({
    open(type) {
      throw new Error(`no rows stream is ${type}`);
    },
  });
  const { phases, error } = await runUntil(t, "error");
  expect(phases).toEqual(["connecting", "error"]);
  expect(error).toEqual({ status: 200, message: "no rows stream is text/html" });
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(calls.ended).toBe(0);
});

test("an error from the table's feed stops the loop and is logged, not taken for the stream's end", async () => {
  vi.stubGlobal("fetch", async () => new Response("{}\n", { headers: { "Content-Type": "application/x-ndjson" } }));
  const logged = vi.spyOn(console, "error").mockImplementation(() => {});
  const { table: t, calls } = table({
    feed() {
      throw new Error("a listener threw");
    },
  });
  const { phases, error } = await runUntil(t, "error");
  expect(phases).toEqual(["connecting", "open", "error"]);
  expect(error).toEqual({ message: "a listener threw" });
  expect(logged).toHaveBeenCalledWith(new Error("a listener threw"));
  expect(calls).toEqual({ opened: 1, fed: 1, ended: 0 });
});

test("a 5xx is tried again after a wait", async () => {
  const fetch = vi.fn(async () => new Response("down", { status: 503 }));
  vi.stubGlobal("fetch", fetch);
  const { table: t } = table({});
  const { phases } = await runUntil(t, "waiting");
  expect(phases).toEqual(["connecting", "waiting"]);
});

test("a stream that ends is told to the table, which asks again", async () => {
  vi.stubGlobal("fetch", async () => new Response("{}\n", { headers: { "Content-Type": "application/x-ndjson" } }));
  const { table: t, calls } = table({});
  const { phases } = await runUntil(t, "waiting");
  expect(phases).toEqual(["connecting", "open", "waiting"]);
  expect(calls).toEqual({ opened: 1, fed: 1, ended: 1 });
});
