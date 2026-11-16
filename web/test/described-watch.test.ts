// The described demo's watch loop: what stops it, and what it tries again.

import { afterEach, expect, test, vi } from "vitest";
import type { DescribedTable } from "../src";
import { watch, type WatchError, type WatchPhase } from "../demo/sources/described-watch";

/** A table that asks for its watch, and again after each end, and does what `open` and `feed` say. */
function table(parts: { open?(type: string): void; feed?(bytes: Uint8Array): void }) {
  const calls = { opened: 0, fed: 0, ended: 0 };
  let asking = true;
  const stub = {
    pollRequest() {
      if (!asking) return undefined;
      asking = false;
      return { kind: "watch", table: "t", target: "/tables/t/rows" };
    },
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
      asking = true;
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
  const requests = [{ kind: "watch", target: "/one" }];
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
      if (fed.length === 1) {
        requests.push({ kind: "watch", target: "/two" });
        ask();
      }
    },
    ended() {},
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  await runUntil(t, "waiting");
  expect(fed).toEqual(["one\n", "two\n"]);
});

test("the count read opens beside the watch, its bytes go only to the count read, and closing it aborts it", async () => {
  const ndjson = { "Content-Type": "application/x-ndjson" };
  const encode = (text: string) => new TextEncoder().encode(text);
  let aborted = false;
  // The watch sends `rows` and stays open; the count read sends `count` and stays open until aborted.
  const fetch = vi.fn(async (target: string, init?: RequestInit) => {
    const sent = target === "/count" ? "count\n" : "rows\n";
    const body = new ReadableStream<Uint8Array>({
      start(c) {
        c.enqueue(encode(sent));
        init?.signal?.addEventListener("abort", () => {
          if (target === "/count") aborted = true;
          c.error(new DOMException("aborted", "AbortError"));
        });
      },
    });
    return new Response(body, { headers: ndjson });
  });
  vi.stubGlobal("fetch", fetch);
  const fed: string[] = [];
  const counted: string[] = [];
  const requests: unknown[] = [
    { kind: "watch", target: "/rows" },
    { kind: "count", target: "/count" },
  ];
  let ask = () => {};
  const t = {
    pollRequest: () => requests.shift(),
    onRequest(listener: () => void) {
      ask = listener;
      return () => {};
    },
    open() {},
    feed: (bytes: Uint8Array) => void fed.push(new TextDecoder().decode(bytes)),
    ended() {},
    openCount() {},
    feedCount: (bytes: Uint8Array) => void counted.push(new TextDecoder().decode(bytes)),
    countEnded() {},
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  const w = watch(t, { headers: {}, onPhase() {} });
  await vi.waitFor(() => expect(counted).toEqual(["count\n"]));
  await vi.waitFor(() => expect(fed).toEqual(["rows\n"]));
  expect(fetch.mock.calls.map(([target]) => target)).toEqual(["/count", "/rows"]);
  requests.push({ kind: "closeCount" });
  ask();
  await vi.waitFor(() => expect(aborted).toBe(true));
  // The watch goes on: closing the count read is no news to it.
  expect(fetch).toHaveBeenCalledTimes(2);
  w.destroy();
});

test("a count read that ends is told to the table, which asks for it again after a wait", async () => {
  vi.stubGlobal("fetch", async (target: string) =>
    target === "/count"
      ? new Response("count\n", { headers: { "Content-Type": "application/x-ndjson" } })
      : new Promise<Response>(() => {}),
  );
  const calls = { countEnded: 0, opened: 0 };
  const requests: unknown[] = [
    { kind: "watch", target: "/rows" },
    { kind: "count", target: "/count" },
  ];
  const t = {
    pollRequest: () => requests.shift(),
    onRequest: () => () => {},
    openCount: () => void calls.opened++,
    feedCount() {},
    countEnded() {
      calls.countEnded++;
      requests.push({ kind: "count", target: "/count" });
    },
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  const w = watch(t, { headers: {}, onPhase() {} });
  await vi.waitFor(() => expect(calls.countEnded).toBe(1));
  expect(calls.opened).toBe(1);
  await vi.waitFor(() => expect(calls.opened).toBe(2), { timeout: 2000 });
  w.destroy();
});

test("a count read asked for again during its wait, with the watch's requests, opens only when the wait ends", async () => {
  const opened: number[] = [];
  vi.stubGlobal("fetch", async (target: string) => {
    if (target !== "/count") return new Promise<Response>(() => {});
    opened.push(Date.now());
    return new Response("count\n", { headers: { "Content-Type": "application/x-ndjson" } });
  });
  const requests: unknown[] = [
    { kind: "watch", target: "/rows" },
    { kind: "count", target: "/count" },
  ];
  let ask = () => {};
  const ended: number[] = [];
  const t = {
    pollRequest: () => requests.shift(),
    onRequest(listener: () => void) {
      ask = listener;
      return () => {};
    },
    openCount() {},
    feedCount() {},
    // The count read ended, and the table asks for it again; a watch event takes that request at once.
    countEnded() {
      ended.push(Date.now());
      requests.push({ kind: "count", target: "/count" });
      ask();
    },
    status: () => ({ synced: false }),
  } as unknown as DescribedTable;
  const w = watch(t, { headers: {}, onPhase() {} });
  await vi.waitFor(() => expect(opened.length).toBe(2), { timeout: 2000 });
  expect(opened[1]! - ended[0]!).toBeGreaterThanOrEqual(450);
  w.destroy();
});
