/// <reference types="vitest/config" />
import { defineConfig, searchForWorkspaceRoot } from "vite";

export default defineConfig({
  build: { target: "es2022" },
  // The Values page reads the shared fixture beside the Rust formatter.
  server: { fs: { allow: [searchForWorkspaceRoot(process.cwd()), "../crates/binnacle-format/fixtures"] } },
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
  },
});
