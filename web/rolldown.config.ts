// The package build: the two entries and the chunk they share into dist/,
// so both load one copy of the wasm glue. The wasm-bindgen output stays
// beside dist/ in wasm/, and `new URL("../wasm/...", import.meta.url)` is
// left for the consumer's bundler to emit as an asset.

import { defineConfig } from "rolldown";

export default defineConfig({
  input: { index: "src/index.ts", "react/index": "src/react/index.ts" },
  external: [/^react($|\/)/, /\/wasm\/binnacle_wasm\.js$/],
  output: {
    dir: "dist",
    format: "esm",
    // Every file sits at the depth of src/'s top level, where the wasm path resolves.
    chunkFileNames: "binnacle-[hash].js",
    cleanDir: true,
  },
});
