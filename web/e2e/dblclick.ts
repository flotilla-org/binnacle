// A double-click the widget counts as one. The widget counts presses itself
// (`src/clicks.ts`: the second within 500 ms of the first), and Playwright's
// `mouse.dblclick` makes no promise about the gap: on a loaded machine its
// two presses can reach the page more than 500 ms apart, which the widget
// rightly takes as two single clicks. So the page's own press times are
// checked with the widget's counter, and a missed double-click is tried again
// once the last press is too old to chain with the next.

import type { Page } from "@playwright/test";
import { createClickCounter } from "../src/clicks";

const TRIES = 5;

export async function dblclick(page: Page, x: number, y: number): Promise<void> {
  for (let i = 0; i < TRIES; i++) {
    await page.evaluate(() => {
      const w = window as unknown as { presses?: number[] };
      if (!w.presses) document.addEventListener("pointerdown", (e) => w.presses!.push(e.timeStamp), true);
      w.presses = [];
    });
    await page.mouse.dblclick(x, y);
    const presses = await page.evaluate(() => (window as unknown as { presses: number[] }).presses);
    const clicks = createClickCounter();
    const counts = presses.map((time) => clicks.press({ time, x, y, button: 0 }));
    if (counts.join() === "1,2") return;
    await page.waitForTimeout(600);
  }
  throw new Error(`no double-click at (${x}, ${y}) in ${TRIES} tries`);
}
