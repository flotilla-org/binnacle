// The dev server's `/tables` proxy forwards only to loopback servers and
// Unix sockets, so it is never an open proxy.

import { createServer, get, type IncomingMessage, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { expect, test } from "vitest";
import { forward, upstreamOf } from "../demo/tables-proxy";

const FALLBACK = "http://127.0.0.1:7777";

test("loopback servers and absolute Unix sockets are taken", () => {
  expect(upstreamOf(undefined, FALLBACK)).toEqual({ host: "127.0.0.1", port: 7777 });
  expect(upstreamOf("http://127.0.0.1:9000", FALLBACK)).toEqual({ host: "127.0.0.1", port: 9000 });
  expect(upstreamOf("http://localhost:9000", FALLBACK)).toEqual({ host: "localhost", port: 9000 });
  expect(upstreamOf("http://[::1]:9000", FALLBACK)).toEqual({ host: "::1", port: 9000 });
  expect(upstreamOf("unix:/run/flotilla.sock", FALLBACK)).toEqual({ socketPath: "/run/flotilla.sock" });
  expect(upstreamOf("unix:~/x.sock", FALLBACK, "/home/me")).toEqual({ socketPath: "/home/me/x.sock" });
});

test("anything else is refused", () => {
  for (const server of [
    "http://example.com",
    "http://example.com:80",
    "http://10.0.0.1:80",
    "http://10.0.0.1:7777",
    "http://example.com:7777",
    "http://127.0.0.2:7777",
    "https://127.0.0.1:9000",
    "http://127.0.0.1",
    "http://127.0.0.1:9000/elsewhere",
    "http://user@127.0.0.1:9000",
    "unix:relative.sock",
    "unix:",
    "127.0.0.1:9000",
  ]) {
    expect(upstreamOf(server, FALLBACK), server).toBeNull();
  }
});

/** Listens on a free loopback port; resolves to it. */
async function listen(server: Server): Promise<number> {
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  return (server.address() as AddressInfo).port;
}

/** A proxy in front of `upstream`, as the dev server runs it. */
async function proxied(upstream: Server): Promise<{ port: number; close(): void }> {
  const up = await listen(upstream);
  const proxy = createServer((req, res) => forward(req, res, `http://127.0.0.1:${up}`));
  const port = await listen(proxy);
  return {
    port,
    close() {
      proxy.closeAllConnections();
      proxy.close();
      upstream.closeAllConnections();
      upstream.close();
    },
  };
}

/** GETs `/tables/t/rows` through the proxy: the status, the body read, and how it ended. */
function watch(port: number): Promise<{ status: number; body: string; ended: "end" | "broken" }> {
  return new Promise((resolve, reject) => {
    get({ host: "127.0.0.1", port, path: "/tables/t/rows" }, (res: IncomingMessage) => {
      let body = "";
      res.setEncoding("utf8");
      res.on("data", (chunk: string) => (body += chunk));
      res.on("end", () => resolve({ status: res.statusCode ?? 0, body, ended: "end" }));
      res.on("error", () => resolve({ status: res.statusCode ?? 0, body, ended: "broken" }));
    }).on("error", reject);
  });
}

test("an answer that comes whole is passed on whole", async () => {
  const { port, close } = await proxied(
    createServer((_, res) => {
      res.setHeader("Content-Type", "application/x-ndjson");
      res.write("one\n");
      res.end("two\n");
    }),
  );
  try {
    expect(await watch(port)).toEqual({ status: 200, body: "one\ntwo\n", ended: "end" });
  } finally {
    close();
  }
});

test("an answer that breaks off breaks off the response, so the page sees its watch end", async () => {
  const { port, close } = await proxied(
    createServer((_, res) => {
      res.setHeader("Content-Type", "application/x-ndjson");
      res.flushHeaders();
      // The server dies mid-stream: one line, then the connection is cut.
      res.write("one\n", () => setTimeout(() => res.socket?.destroy(), 50));
    }),
  );
  try {
    expect(await watch(port)).toEqual({ status: 200, body: "one\n", ended: "broken" });
  } finally {
    close();
  }
}, 5_000);
