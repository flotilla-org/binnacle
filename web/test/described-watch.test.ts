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
    onRequest: () => () => {},
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

const encode = (text: string) => new TextEncoder().encode(text);
const ndjson = { "Content-Type": "application/x-ndjson" };

/**
 * A table that asks for `/one`, then for `/two` in place of the stream when `ask` is called, keeping the text of
 * each `feed`; `onFeed` runs after each.
 */
function replacing(onFeed: (fed: string[], ask: () => void) => void) {
  const fed: string[] = [];
  const calls = { ended: 0 };
  const requests = [{ target: "/one" }, { target: "/two" }];
  let listener = () => {};
  const ask = () => listener();
  const stub = {
    pollRequest: () => requests.shift(),
    onRequest(l: () => void) {
      listener = l;
      return () => {};
    },
    open() {},
    feed(bytes: Uint8Array) {
      fed.push(new TextDecoder().decode(bytes));
      onFeed(fed, ask);
    },
    ended() {
      calls.ended++;
    },
    status: () => ({ synced: false }),
  };
  return { table: stub as unknown as DescribedTable, fed, calls };
}

/**
 * `/one` sends `one`, then `late` when read again, and errors once aborted if `aborts`; `/two` sends `two` and
 * ends.
 */
function serving(aborts: boolean) {
  return vi.fn(async (target: string, init?: RequestInit) => {
    if (target !== "/one") return new Response("two\n", { headers: ndjson });
    let reads = 0;
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(encode("one\n"));
        if (aborts) init?.signal?.addEventListener("abort", () => c.error(new DOMException("aborted", "AbortError")));
      },
      pull(c) {
        if (reads++ === 0) c.enqueue(encode("late\n"));
        return new Promise<void>(() => {});
      },
    });
    return new Response(body, { headers: ndjson });
  });
}

test("a watch asked for while the stream is open replaces it at once, and the old stream's end is no news", async () => {
  const fetch = serving(true);
  vi.stubGlobal("fetch", fetch);
  // The filter changes while the loop waits on the open stream.
  const { table: t, fed, calls } = replacing((fed, ask) => {
    if (fed.length === 1) setTimeout(ask, 5);
  });
  const { phases } = await runUntil(t, "waiting");
  expect(fetch.mock.calls.map(([target]) => target)).toEqual(["/one", "/two"]);
  expect(fed[fed.length - 1]).toBe("two\n");
  expect(phases).toEqual(["connecting", "open", "connecting", "open", "waiting"]);
  expect(calls.ended).toBe(1);
});

test("no bytes of a replaced stream are fed once the table asks for another watch", async () => {
  // A body that has bytes buffered still gives them after the abort.
  const fetch = serving(false);
  vi.stubGlobal("fetch", fetch);
  // The filter changes as the table takes a message of the open stream.
  const { table: t, fed } = replacing((fed, ask) => {
    if (fed.length === 1) ask();
  });
  await runUntil(t, "waiting");
  expect(fed).toEqual(["one\n", "two\n"]);
});
