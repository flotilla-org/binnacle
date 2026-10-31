// The dev server's way to a table server: the demo page always fetches
// same-origin `/tables...`, and this plugin forwards it to the server the
// `X-Binnacle-Server` header names (else `fallback`), streaming the answer
// back as it comes. A browser can't reach a Unix socket, and `fake-tables`
// sends no CORS headers. Only loopback and Unix sockets are allowed: the
// dev server must never be an open proxy. A `unix:` server may name any
// socket the user can reach. Vite's CORS lets any local origin
// (`localhost` or `127.0.0.1`, any port) through, so script on any local
// origin can set the header; the plugin runs in the dev server alone
// (`apply: "serve"`).

import { request, type IncomingMessage, type ServerResponse } from "node:http";
import { homedir } from "node:os";
import { join } from "node:path";
import { pipeline } from "node:stream";
import type { Plugin } from "vite";

/** Where a request goes: a loopback host and port, or a Unix socket. */
export type Upstream = { host: string; port: number } | { socketPath: string };

const LOOPBACK: Record<string, string> = { "127.0.0.1": "127.0.0.1", localhost: "localhost", "[::1]": "::1" };

/** Why a server is refused, as the page shows it. */
export const BAD_SERVER = "server must be loopback or unix:";

/**
 * The upstream `header` names, else `fallback`: `http://127.0.0.1:<port>`,
 * `http://localhost:<port>`, `http://[::1]:<port>`, or `unix:<absolute
 * path>` (a leading `~/` is `home`). Anything else is `null`.
 */
export function upstreamOf(header: string | undefined, fallback: string, home: string = homedir()): Upstream | null {
  const server = header?.trim() || fallback;
  if (server.startsWith("unix:")) {
    const path = server.slice("unix:".length);
    const socketPath = path.startsWith("~/") ? join(home, path.slice(2)) : path;
    return socketPath.startsWith("/") ? { socketPath } : null;
  }
  let url: URL;
  try {
    url = new URL(server);
  } catch {
    return null;
  }
  const host = LOOPBACK[url.hostname];
  const bare = url.username === "" && url.password === "" && url.pathname === "/" && url.search === "" && url.hash === "";
  if (url.protocol !== "http:" || host === undefined || url.port === "" || !bare) return null;
  return { host, port: Number(url.port) };
}

const isTables = (path: string) => path === "/tables" || path.startsWith("/tables/") || path.startsWith("/tables?");

function refuse(res: ServerResponse, status: number, code: string, message: string): void {
  res.statusCode = status;
  res.setHeader("Content-Type", "application/json");
  res.end(JSON.stringify({ code, message }));
}

/**
 * Forwards one request under `/tables`, streaming the answer back
 * unbuffered. An answer that breaks off (the server died mid-stream) breaks
 * off the response too, so the page sees its watch end.
 */
export function forward(req: IncomingMessage, res: ServerResponse, fallback: string): void {
  const named = req.headers["x-binnacle-server"];
  const upstream = upstreamOf(typeof named === "string" ? named : undefined, fallback);
  if (!upstream) return refuse(res, 400, "bad_server", BAD_SERVER);
  const headers: Record<string, string> = {
    host: "socketPath" in upstream ? "localhost" : `${upstream.host}:${upstream.port}`,
  };
  for (const name of ["accept", "authorization"]) {
    const value = req.headers[name];
    if (typeof value === "string") headers[name] = value;
  }
  const out = request({ ...upstream, method: req.method ?? "GET", path: req.url ?? "/tables", headers }, (answer) => {
    res.statusCode = answer.statusCode ?? 502;
    const type = answer.headers["content-type"];
    if (type) res.setHeader("Content-Type", type);
    res.setHeader("Cache-Control", "no-store");
    res.flushHeaders();
    pipeline(answer, res, (e) => {
      if (e) res.destroy();
    });
  });
  out.on("error", (e) => {
    if (res.headersSent) res.destroy();
    else refuse(res, 502, "upstream", e.message);
  });
  // The browser closed the watch: drop the upstream one too. After a normal
  // end this destroys a request already done, which does nothing.
  res.on("close", () => out.destroy());
  req.pipe(out);
}

/** The dev server's `/tables` proxy; `fallback` is the server used without an `X-Binnacle-Server` header. */
export function tablesProxy(fallback: string): Plugin {
  return {
    name: "binnacle-tables-proxy",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (isTables(req.url ?? "")) forward(req, res, fallback);
        else next();
      });
    },
  };
}
