// The described source's watch loop: it opens the watch the Described Table
// asks for, feeds it each chunk of the body as it comes, and when the body
// ends tells the table, waits and opens it again. The wait starts at
// 500 ms and doubles to 5 s, starting again once the rows are synced; it is
// never shorter, so a stream that ends at once can't spin. A 4xx stops the
// loop; a 5xx or a network error tries again after the same wait.

import type { DescribedTable } from "../../src";

/** Where the loop is: opening the watch, reading it, waiting to reopen it, or stopped by an error. */
export type WatchPhase = "connecting" | "open" | "waiting" | "error";

/** Why the loop stopped: the HTTP status, and the server's message. */
export interface WatchError {
  status: number;
  message: string;
}

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

  const run = async () => {
    let request = table.pollRequest();
    while (!stopped) {
      if (request) {
        options.onPhase("connecting");
        controller = new AbortController();
        let opened = false;
        try {
          const response = await fetch(request.target, { headers: options.headers, signal: controller.signal });
          if (response.status >= 400 && response.status < 500) {
            options.onPhase("error", { status: response.status, message: await errorMessage(response) });
            return;
          }
          if (!response.ok || !response.body) throw new Error(`HTTP ${response.status}`);
          table.open(response.headers.get("Content-Type") ?? "");
          opened = true;
          options.onPhase("open");
          const reader = response.body.getReader();
          for (;;) {
            const { done, value } = await reader.read();
            if (done || stopped) break;
            table.feed(value);
            if (table.status().synced) wait = FIRST_WAIT;
          }
        } catch {
          // A network error, a 5xx or an abort: tried again below, unless stopped.
        }
        if (stopped) return;
        // The body ended (or broke off): the table asks for its watch again.
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
      controller?.abort();
      wake?.();
    },
  };
}
