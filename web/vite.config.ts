/// <reference types="vitest/config" />
import { defineConfig, searchForWorkspaceRoot } from "vite";
import { tablesProxy } from "./demo/tables-proxy";

export default defineConfig({
  build: { target: "es2022" },
  // The described source's table server: `pnpm fake-tables` by default.
  plugins: [tablesProxy(process.env.BINNACLE_FAKE_TABLES ?? "http://127.0.0.1:7777")],
  // The Values page reads the shared fixture beside the Rust formatter.
  server: { fs: { allow: [searchForWorkspaceRoot(process.cwd()), "../crates/binnacle-format/fixtures"] } },
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
  },
});
