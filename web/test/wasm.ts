import { readFileSync } from "node:fs";
import { loadBinnacle } from "../src/wasm";

/** Loads the real wasm build from disk; vitest runs in node, without fetch for file URLs. */
export function loadForTests(): Promise<void> {
  return loadBinnacle(readFileSync(new URL("../wasm/binnacle_wasm_bg.wasm", import.meta.url)));
}
