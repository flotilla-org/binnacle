// How the wrapper treats GridWidget's options, by kind rather than by name,
// so options the widget gains pass straight through: functions are called
// through the latest props, the live options are set on the widget, and any
// other option is read at creation.

const LIVE = new Set(["theme", "border"]);

type Options = object;

const entries = (o: Options): [string, unknown][] => Object.entries(o);

/** Whether `next` needs a new widget rather than the one created from `created`. */
export function recreates(created: Options, next: Options): boolean {
  const before = new Map(entries(created));
  const after = new Map(entries(next));
  const keys = new Set([...before.keys(), ...after.keys()]);
  for (const key of keys) {
    const a = before.get(key);
    const b = after.get(key);
    // A callback added or removed can change what the widget does (no onCommit means read-only).
    if ((a === undefined) !== (b === undefined)) return true;
    if (LIVE.has(key) || typeof b === "function") continue;
    if (a !== b) return true;
  }
  return false;
}

/** The options to create a widget with: each callback calls the latest props' one. */
export function throughLatest<O extends Options>(latest: { readonly current: O }): O {
  const out: Record<string, unknown> = {};
  for (const [key, value] of entries(latest.current)) {
    out[key] =
      typeof value === "function" && !LIVE.has(key)
        ? (...args: unknown[]) => {
            const f: unknown = (latest.current as Record<string, unknown>)[key];
            return typeof f === "function" ? f(...args) : undefined;
          }
        : value;
  }
  return out as O;
}

/** The live options set in `options`. */
export function live(options: Options): [string, unknown][] {
  return entries(options).filter(([key, value]) => LIVE.has(key) && value !== undefined);
}
