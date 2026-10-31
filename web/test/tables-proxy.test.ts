// The dev server's `/tables` proxy forwards only to loopback servers and
// Unix sockets, so it is never an open proxy.

import { expect, test } from "vitest";
import { upstreamOf } from "../demo/tables-proxy";

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
