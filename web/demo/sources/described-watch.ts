// The described source's watch loop: it opens the watch the Described Table
// asks for, feeds it each chunk of the body as it comes, and when the body
// ends tells the table, waits and opens it again. The wait starts at
// 500 ms and doubles to 5 s, starting again once the rows are synced; it is
// never shorter, so a stream that ends at once can't spin. A 5xx or a
// network error tries again after the same wait. A 4xx, a 200 that is not a
// rows stream, and an error from the table itself (a throwing sink or
// listener) stop the loop: trying again would only fail the same way. When
// a filter changes the `where` clauses the table sends, the table asks for
// its watch while the stream is open: the loop aborts the stream, feeds
// none of its bytes from then on, and opens the new watch at once.
//
// While the table sends `where` clauses, it asks for a count read beside the
// watch: a second loop opens it and feeds its bytes to the table's count
// read calls, never to `feed`, and aborts it when the table asks to close
// it. Its end is told to the table, which asks for it again, after the same
// waits. It never changes the phase: when it fails, the Totals stay
// unknown, and the status line shows the rows alone.

import type { DescribedTable, DescribedWatchRequest } from "../../src";

/** Where the loop is: opening the watch, reading it, waiting to reopen it, or stopped by an error. */
export type WatchPhase = "connecting" | "open" | "waiting" | "error";

/** Why the loop stopped: the HTTP status (none for an error of the table's own), and the message. */
export interface WatchError {
  status?: number;
  message: string;
}

/** How a status line says `error`. */
export const errorText = (e: WatchError): string =>
  e.status === undefined ? `error: ${e.message}` : `error ${e.status}: ${e.message}`;

const messageOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export interface WatchOptions {
  /** The headers sent with every request: the server, the encoding asked for and the token. */
  headers: Record<string, string>;
  /** Called as the phase changes. */
  onPhase(phase: WatchPhase, error?: WatchError): void;
}

export interface Watch {
  /** Aborts the watch and stops the loop. */
  destroy(): void;
}

const FIRST_WAIT = 500;
const LAST_WAIT = 5000;

/** The server's message for a failed request: its JSON `message`, else the body's text. */
export async function errorMessage(response: Response): Promise<string> {
  const text = await response.text().catch(() => "");
  try {
    const json = JSON.parse(text) as { message?: unknown };
    if (typeof json.message === "string") return json.message;
  } catch {
    // Not JSON: the text itself.
  }
  return text.trim() || response.statusText;
}

/** The count read's loop: `open` reads `target` in place of any read open, `close` aborts it. */
interface CountRead {
  open(target: string): void;
  close(): void;
}

/**
 * Reads the count read `table` asks for, feeding it to the table's count read calls. When a read ends or fails, the
 * table is told, and after a wait (doubling from 500 ms to 5 s, as the watch's, and starting again once a read
 * brings bytes), `poll` takes the read it asks for again. A 4xx or a body that is not a rows stream leaves it closed.
 */
function countRead(table: DescribedTable, headers: Record<string, string>, poll: () => void): CountRead {
  let controller: AbortController | null = null;
  let wait = FIRST_WAIT;
  const close = () => {
    controller?.abort();
    controller = null;
  };
  const read = async (target: string, mine: AbortController) => {
    const current = () => controller === mine;
    try {
      const response = await fetch(target, { headers, signal: mine.signal });
      if (!current()) return;
      if (response.status >= 400 && response.status < 500) return close();
      if (response.ok && response.body) {
        try {
          table.openCount(response.headers.get("Content-Type") ?? "");
        } catch {
          void response.body.cancel();
          return close();
        }
        const reader = response.body.getReader();
        for (;;) {
          const { done, value } = await reader.read();
          if (done || !current()) break;
          table.feedCount(value);
          wait = FIRST_WAIT;
        }
      }
    } catch (e) {
      if (!(e instanceof DOMException && e.name === "AbortError") && !(e instanceof TypeError)) console.error(e);
    }
    if (!current()) return;
    table.countEnded();
    await new Promise((resolve) => setTimeout(resolve, wait));
    wait = Math.min(wait * 2, LAST_WAIT);
    if (current()) poll();
  };
  return {
    open(target) {
      controller?.abort();
      const mine = new AbortController();
      controller = mine;
      void read(target, mine);
    },
    close() {
      close();
      wait = FIRST_WAIT;
    },
  };
}

/** Opens `table`'s watch and keeps it open until destroyed. */
export function watch(table: DescribedTable, options: WatchOptions): Watch {
  let stopped = false;
  let controller: AbortController | null = null;
  let wake: (() => void) | null = null;
  /** The table asked for another watch: the next wait is skipped. */
  let asked = false;
  let wait = FIRST_WAIT;

  const sleep = (ms: number) =>
    new Promise<void>((resolve) => {
      const timer = setTimeout(resolve, ms);
      wake = () => {
        clearTimeout(timer);
        resolve();
      };
    });

  /** Stops the loop with `error`. */
  const fail = (error: WatchError) => {
    stopped = true;
    count.close();
    options.onPhase("error", error);
  };

  let request: DescribedWatchRequest | undefined;
  /** Takes every request the table has: the watch's for the loop below, the count read's for its own. */
  const take = (): boolean => {
    let watched = false;
    for (let next = table.pollRequest(); next; next = table.pollRequest()) {
      switch (next.kind) {
        case "watch":
          request = next;
          watched = true;
          break;
        case "count":
          if (!stopped) count.open(next.target);
          break;
        case "closeCount":
          count.close();
          break;
      }
    }
    return watched;
  };
  /** Takes the table's requests; for a watch in place of the stream, aborts it and wakes a wait. */
  const ask = () => {
    if (!take()) return;
    controller?.abort();
    controller = null;
    asked = true;
    wake?.();
  };
  const count = countRead(table, options.headers, ask);
  take();
  const unrequest = table.onRequest(ask);

  const run = async () => {
    while (!stopped) {
      if (request) {
        options.onPhase("connecting");
        const target = request.target;
        request = undefined;
        asked = false;
        const mine = new AbortController();
        controller = mine;
        /** A newer watch replaced this one. */
        const replaced = () => controller !== mine;
        let opened = false;
        try {
          const response = await fetch(target, { headers: options.headers, signal: mine.signal });
          if (replaced()) continue;
          if (response.status >= 400 && response.status < 500) {
            return fail({ status: response.status, message: await errorMessage(response) });
          }
          if (response.ok && response.body) {
            try {
              table.open(response.headers.get("Content-Type") ?? "");
            } catch (e) {
              // Not a rows stream: not a table server, and it won't become one.
              void response.body.cancel();
              return fail({ status: response.status, message: messageOf(e) });
            }
            opened = true;
            options.onPhase("open");
            const reader = response.body.getReader();
            for (;;) {
              const { done, value } = await reader.read();
              if (done || stopped || replaced()) break;
              try {
                table.feed(value);
              } catch (e) {
                // A sink or listener threw: a bug, not the stream's end.
                console.error(e);
                mine.abort();
                return fail({ message: messageOf(e) });
              }
              if (table.status().synced) wait = FIRST_WAIT;
            }
          }
        } catch (e) {
          // The network failed, or the watch was aborted: tried again below, unless stopped.
          if (!(e instanceof DOMException && e.name === "AbortError") && !(e instanceof TypeError)) console.error(e);
        }
        if (stopped) return;
        // Replaced: the new watch opens at once, and this one's end is no news.
        if (replaced()) continue;
        // The body ended (or broke off), or a 5xx came: the table asks for its watch again.
        if (opened) {
          table.ended();
          take();
        }
      }
      options.onPhase("waiting");
      if (!asked) await sleep(wait);
      asked = false;
      wait = Math.min(wait * 2, LAST_WAIT);
      if (!request) take();
    }
  };
  void run();

  return {
    destroy() {
      stopped = true;
      unrequest();
      controller?.abort();
      count.close();
      wake?.();
    },
  };
}
