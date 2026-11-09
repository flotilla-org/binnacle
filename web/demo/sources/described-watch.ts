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

import type { DescribedTable } from "../../src";

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

/** Opens `table`'s watch and keeps it open until destroyed. */
export function watch(table: DescribedTable, options: WatchOptions): Watch {
  let stopped = false;
  let controller: AbortController | null = null;
  let wake: (() => void) | null = null;
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
    options.onPhase("error", error);
  };

  let request = table.pollRequest();
  /** The table asked for its watch in place of the stream: abort it, and wake a wait. */
  const unrequest = table.onRequest(() => {
    request = table.pollRequest();
    controller?.abort();
    controller = null;
    wake?.();
  });

  const run = async () => {
    while (!stopped) {
      if (request) {
        options.onPhase("connecting");
        const target = request.target;
        request = undefined;
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
          request = table.pollRequest();
        }
      }
      options.onPhase("waiting");
      await sleep(wait);
      wait = Math.min(wait * 2, LAST_WAIT);
      if (!request) request = table.pollRequest();
    }
  };
  void run();

  return {
    destroy() {
      stopped = true;
      unrequest();
      controller?.abort();
      wake?.();
    },
  };
}
