// The one place that tells a value from a Promise (the graphql-js
// `PromiseOrValue` pattern). A value is applied at once, in the same frame;
// a Promise is applied when it lands, then `landed` schedules a frame.
// Answers stamped with an epoch are dropped if a Change Notice bumped the
// epoch meanwhile.

import type { MaybePromise } from "./source";

export interface SettleOptions {
  fresh?: boolean;
  otherwise?(): void;
}

export interface Settler {
  /** Bumped on every Change Notice. */
  readonly epoch: number;
  bump(): void;
  /**
   * Applies `answer`. With `fresh`, a late answer from an older epoch is
   * dropped. `otherwise` runs for an answer not applied: dropped, or a
   * rejected Promise.
   */
  settle<T>(answer: MaybePromise<T>, apply: (value: T) => void, options?: SettleOptions): void;
}

function isPromise<T>(answer: MaybePromise<T>): answer is Promise<T> {
  return typeof (answer as { then?: unknown } | null)?.then === "function";
}

export function createSettler(landed: () => void, failed: (error: unknown) => void): Settler {
  let epoch = 0;
  return {
    get epoch() {
      return epoch;
    },
    bump() {
      epoch++;
    },
    settle(answer, apply, options = {}) {
      if (!isPromise(answer)) {
        apply(answer);
        return;
      }
      const stamp = epoch;
      answer.then(
        (value) => {
          if (options.fresh && stamp !== epoch) options.otherwise?.();
          else apply(value);
          landed();
        },
        (error: unknown) => {
          options.otherwise?.();
          failed(error);
          landed();
        },
      );
    },
  };
}
