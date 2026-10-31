// A fake table server per test, for the tests that change the table as
// they go: each spawns the `fake-tables` binary the shared server's
// webServer entry built, serving the crews fixture, and pushes script
// steps to it through its standard input.

import { execFileSync, spawn } from "node:child_process";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { test as base } from "@playwright/test";

/** A running fake: its base URL, and how to add a step to its script. */
export interface Fake {
  url: string;
  push(step: object): void;
}

const CREWS = "../crates/binnacle-described/tests/fixtures/crews.ndjson";

export const test = base.extend<{ fake: Fake }, { fakeBinary: string }>({
  fakeBinary: [
    async ({}, use) => {
      const metadata = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { encoding: "utf8" });
      await use(join((JSON.parse(metadata) as { target_directory: string }).target_directory, "debug", "fake-tables"));
    },
    { scope: "worker" },
  ],
  fake: async ({ fakeBinary }, use) => {
    const child = spawn(fakeBinary, ["--listen", "127.0.0.1:0", "--fixture", CREWS, "--stdin"], {
      stdio: ["pipe", "pipe", "inherit"],
    });
    const url = await new Promise<string>((resolve, reject) => {
      const lines = createInterface({ input: child.stdout! });
      lines.on("line", (line) => {
        const found = /^fake-tables listening on (http:\/\/\S+)$/.exec(line);
        if (found) resolve(found[1]!);
      });
      child.on("error", reject);
      child.on("exit", (code) => reject(new Error(`fake-tables exited with ${code}`)));
    });
    await use({ url, push: (step) => child.stdin!.write(`${JSON.stringify(step)}\n`) });
    child.kill();
  },
});

export { expect } from "@playwright/test";
