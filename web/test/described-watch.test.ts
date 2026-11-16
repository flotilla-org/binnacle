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

test("a watch asked for while the stream is open replaces it, and none of the old stream's bytes are fed", async () => {
  const ndjson = { "Content-Type": "application/x-ndjson" };
  const encode = (text: string) => new TextEncoder().encode(text);
  // `/one` sends `one`, then `late`, buffered, which a body still gives after an abort.
  const fetch = vi.fn(async (target: string) => {
    if (target !== "/one") return new Response("two\n", { headers: ndjson });
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(encode("one\n"));
        c.enqueue(encode("late\n"));
      },
    });
    return new Response(body, { headers: ndjson });
  });
  vi.stubGlobal("fetch", fetch);
  const fed: string[] = [];
  const requests = [{ target: "/one" }, { target: "/two" }];
  let ask = () => {};
  const t = {
    pollRequest: () => requests.shift(),
    onRequest(listener: () => void) {
      ask = listener;
      return () => {};
    },
    open() {},
    // The filter changes as the table takes the open stream's first message.
    feed(bytes: Uint8Array) {
      fed.push(new TextDecoder().decode(bytes));
      if (fed.length === 1) ask();
    },
    ended() {},
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  await runUntil(t, "waiting");
  expect(fed).toEqual(["one\n", "two\n"]);
});

/** A table whose request the test changes: `ask(target)` is the table asking for another watch. */
function askingTable(first: string, parts: { feed?(): void } = {}) {
  let next: { target: string } | undefined = { target: first };
  const listeners = new Set<() => void>();
  const t = {
    pollRequest() {
      const r = next;
      next = undefined;
      return r;
    },
    onRequest(listener: () => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    open() {},
    feed: () => parts.feed?.(),
    ended() {},
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  return {
    table: t,
    ask(target: string) {
      next = { target };
      for (const l of [...listeners]) l();
    },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

/** Starts the loop on `t`; the phases it reports. */
function start(t: DescribedTable) {
  const phases: WatchPhase[] = [];
  const w = watch(t, { headers: {}, onPhase: (p) => phases.push(p) });
  return { w, phases };
}

/** `/bad` answers 400; any other target is a rows stream. */
const fetchBadThenRows = () =>
  vi.fn(async (target: string) =>
    target === "/bad"
      ? new Response('{"message":"no"}', { status: 400 })
      : new Response("{}\n", { headers: { "Content-Type": "application/x-ndjson" } }),
  );

test("a 4xx stops the loop until the table asks for a different watch, which opens in connecting", async () => {
  const fetch = fetchBadThenRows();
  vi.stubGlobal("fetch", fetch);
  const { table: t, ask } = askingTable("/bad");
  const { w, phases } = start(t);
  await settle();
  expect(phases).toEqual(["connecting", "error"]);
  ask("/good");
  await settle();
  w.destroy();
  expect(fetch.mock.calls.map((c) => c[0])).toEqual(["/bad", "/good"]);
  expect(phases.slice(2, 4)).toEqual(["connecting", "open"]);
});

test("a 4xx asked for again with the same target fetches nothing", async () => {
  const fetch = fetchBadThenRows();
  vi.stubGlobal("fetch", fetch);
  const { table: t, ask } = askingTable("/bad");
  const { w, phases } = start(t);
  await settle();
  ask("/bad");
  await settle();
  w.destroy();
  expect(fetch).toHaveBeenCalledTimes(1);
  expect(phases).toEqual(["connecting", "error"]);
});

test("after the host's destroy, a new request fetches nothing", async () => {
  const fetch = fetchBadThenRows();
  vi.stubGlobal("fetch", fetch);
  const { table: t, ask } = askingTable("/bad");
  const { w } = start(t);
  await settle();
  w.destroy();
  ask("/good");
  await settle();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test("after a throwing sink, a new request fetches nothing", async () => {
  const fetch = fetchBadThenRows();
  vi.stubGlobal("fetch", fetch);
  vi.spyOn(console, "error").mockImplementation(() => {});
  const { table: t, ask } = askingTable("/good", {
    feed() {
      throw new Error("a listener threw");
    },
  });
  const { w, phases } = start(t);
  await settle();
  expect(phases).toEqual(["connecting", "open", "error"]);
  ask("/other");
  await settle();
  w.destroy();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test("a watch asked for while a 4xx's body is still being read opens", async () => {
  let finish = () => {};
  const fetch = vi.fn(async (target: string) => {
    if (target !== "/bad") return new Response("{}\n", { headers: { "Content-Type": "application/x-ndjson" } });
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        finish = () => {
          c.enqueue(new TextEncoder().encode("no"));
          c.close();
        };
      },
    });
    return new Response(body, { status: 400 });
  });
  vi.stubGlobal("fetch", fetch);
  const { table: t, ask } = askingTable("/bad");
  const { w, phases } = start(t);
  await settle();
  ask("/good");
  finish();
  await settle();
  w.destroy();
  expect(fetch.mock.calls.map((c) => c[0])).toEqual(["/bad", "/good"]);
  expect(phases).toEqual(["connecting", "connecting", "open", "waiting"]);
});
