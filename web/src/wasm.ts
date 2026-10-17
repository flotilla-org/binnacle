// Loads the wasm build behind an async initialisation. The wasm API is
// internal to this package: only the widget and its helpers call it.

import init, * as generated from "../wasm/binnacle_wasm.js";

export type { WebAxis, WebGrid } from "../wasm/binnacle_wasm.js";
export type Bridge = typeof generated;

let loading: Promise<void> | undefined;
let loaded = false;

/**
 * Loads and instantiates the wasm module once; later calls share the first
 * load. `source` overrides where it comes from (bytes in tests, a URL when a
 * host serves the file itself).
 */
export function loadBinnacle(source?: BufferSource | URL): Promise<void> {
  loading ??= init({
    module_or_path: source ?? new URL("../wasm/binnacle_wasm_bg.wasm", import.meta.url),
  }).then(() => {
    loaded = true;
  });
  return loading;
}

/** The loaded wasm exports. Throws until `loadBinnacle` has resolved. */
export function bridge(): Bridge {
  if (!loaded) throw new Error("binnacle: await loadBinnacle() before creating a grid");
  return generated;
}
