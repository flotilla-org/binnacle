/// <reference types="vitest/config" />
import { defineConfig } from "vite";

export default defineConfig({
  build: { target: "es2022" },
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
  },
});
