// A stand-in for the browser's 2D canvas in node: every drawing call is
// recorded, text measures 7 px a character, and `document.createElement`
// hands out canvases backed by it.

export interface DrawCall {
  name: string;
  args: unknown[];
  /** The fill style in force when the call was made. */
  fill: unknown;
}

export function fakeContext(calls: DrawCall[] = []): CanvasRenderingContext2D {
  const state: Record<string | symbol, unknown> = {};
  return new Proxy(state, {
    get(target, prop) {
      if (prop in target) return target[prop];
      if (prop === "measureText") return (text: string) => ({ width: text.length * 7 });
      return (...args: unknown[]) => {
        calls.push({ name: String(prop), args, fill: target.fillStyle });
        if (prop === "createLinearGradient") return { addColorStop() {} };
      };
    },
    set(target, prop, value) {
      target[prop] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

/** Installs a `document` whose canvases draw into `fakeContext`, recording into `calls`. */
export function fakeDocument(calls: DrawCall[] = []): void {
  const canvas = () => ({ style: {}, width: 0, height: 0, getContext: () => fakeContext(calls) });
  (globalThis as { document?: unknown }).document = {
    createElement: (tag: string) => {
      if (tag !== "canvas") throw new Error(`fake document: no <${tag}>`);
      return canvas();
    },
  };
}
