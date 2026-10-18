// The typing pause: typed text waits until typing stops for the pause, then
// goes; anything else (Enter, a toggle) sends what waits at once.

export interface Pause {
  /** Whether typed text waits to be sent. */
  readonly waiting: boolean;
  /** Text was typed: sends once `delay()` ms pass with no more typing. */
  typed(): void;
  /** Sends at once, dropping any wait. */
  now(): void;
  /** Drops any wait without sending. */
  cancel(): void;
}

export function createPause(delay: () => number, send: () => void): Pause {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  };
  const now = () => {
    cancel();
    send();
  };
  return {
    get waiting() {
      return timer !== null;
    },
    typed() {
      cancel();
      timer = setTimeout(now, delay());
    },
    now,
    cancel,
  };
}
