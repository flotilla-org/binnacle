// The one place that tells a value from a Promise (the graphql-js
// `PromiseOrValue` pattern). A value is applied at once, in the same frame;
// a Promise is applied when it lands, then `landed` schedules a frame.
// Every request carries its own AbortSignal, aborted as soon as its answer
// is no longer wanted: a Change Notice (for answers stamped with an epoch),
// the feed superseding it, or the widget going away. An aborted request's
// answer is dropped even if the source answers anyway.

import type { MaybePromise } from "./source";

export interface SettleOptions {
  /** Drop the answer if a Change Notice bumps the epoch before it lands. */
  fresh?: boolean;
  /** Runs for an answer not applied: aborted (at once), dropped, or a rejected Promise. */
  otherwise?(): void;
}

/** A request in flight, or already answered. */
export interface Request {
  readonly signal: AbortSignal;
  /** Still in flight: neither applied, nor dropped, nor failed. */
  readonly pending: boolean;
  /** The answer is no longer wanted: aborts the signal and runs `otherwise` now. No-op once settled. */
  abort(): void;
}

export interface Settler {
  /** Bumped on every Change Notice. */
  readonly epoch: number;
  /** A Change Notice: aborts every `fresh` request in flight. */
  bump(): void;
  /** Asks with a fresh AbortSignal and applies the answer as `options` say. */
  settle<T>(
    ask: (signal: AbortSignal) => MaybePromise<T>,
    apply: (value: T) => void,
    options?: SettleOptions,
  ): Request;
  /** Aborts everything in flight; nothing lands afterwards and no `otherwise` runs. */
  close(): void;
}

function isPromise<T>(answer: MaybePromise<T>): answer is Promise<T> {
  return typeof (answer as { then?: unknown } | null)?.then === "function";
}

export function createSettler(landed: () => void, failed: (error: unknown) => void): Settler {
  let epoch = 0;
  let closed = false;
  const inFlight = new Set<{ fresh: boolean; abort(): void; controller: AbortController }>();
  return {
    get epoch() {
      return epoch;
    },
    bump() {
      epoch++;
      for (const r of [...inFlight]) if (r.fresh) r.abort();
    },
    settle(ask, apply, options = {}) {
      const controller = new AbortController();
      const answer = ask(controller.signal);
      if (!isPromise(answer)) {
        apply(answer);
        return { signal: controller.signal, pending: false, abort() {} };
      }
      const entry = {
        fresh: options.fresh ?? false,
        controller,
        abort() {
          if (!inFlight.delete(entry)) return;
          controller.abort();
          options.otherwise?.();
        },
      };
      inFlight.add(entry);
      answer.then(
        (value) => {
          if (!inFlight.delete(entry)) return;
          apply(value);
          landed();
        },
        (error: unknown) => {
          if (!inFlight.delete(entry)) return;
          options.otherwise?.();
          failed(error);
          landed();
        },
      );
      return {
        signal: controller.signal,
        get pending() {
          return inFlight.has(entry);
        },
        abort: () => entry.abort(),
      };
    },
    close() {
      if (closed) return;
      closed = true;
      const all = [...inFlight];
      inFlight.clear();
      for (const r of all) r.controller.abort();
    },
  };
}
